//! Luau/Lua virtual machine host for entity scripting

use mlua::{Function, Lua, Table, Value};
use std::collections::HashMap;

use crate::sandbox::{apply_sandbox, SandboxConfig};

/// The script virtual machine — owns and executes scripts
pub struct ScriptVm {
    lua: Lua,
    /// Script sources for re-execution with custom environments
    /// We store sources instead of compiled functions because Lua 5.4
    /// per-function environments must be set at load time via set_environment()
    scripts: HashMap<String, String>,
    _config: SandboxConfig,
}

impl ScriptVm {
    /// Create a new sandboxed Lua state
    pub fn new(config: SandboxConfig) -> mlua::Result<Self> {
        let lua = Lua::new();
        apply_sandbox(&lua)?;
        Ok(Self {
            lua,
            scripts: HashMap::new(),
            _config: config,
        })
    }

    /// Create with default sandbox config
    pub fn default() -> mlua::Result<Self> {
        Self::new(SandboxConfig::default())
    }

    /// Load and validate a script (stored as source for later execution with custom environments)
    pub fn load_script(&mut self, name: impl Into<String>, source: &str) -> mlua::Result<()> {
        let name_str = name.into();

        // Validate syntax by attempting to load without executing
        self.lua.load(source).into_function()?;

        // Store the source for later execution with per-script environments
        self.scripts.insert(name_str, source.to_string());
        Ok(())
    }

    /// Remove a cached script
    pub fn unload_script(&mut self, name: &str) {
        self.scripts.remove(name);
    }

    /// Call a hook function in a script (e.g., on_tick, on_spawn)
    ///
    /// In Lua 5.4, function environments use upvalues and are set at load time.
    /// Each script execution gets its own environment table via set_environment().
    pub fn call_hook(
        &self,
        script_name: &str,
        hook_name: &str,
        context: Table,
    ) -> mlua::Result<Value> {
        let source = self
            .scripts
            .get(script_name)
            .ok_or_else(|| mlua::Error::external("Script not loaded"))?;

        // Create a new environment for this script execution
        let script_env = self.lua.create_table()?;

        // Set _G to this environment for backwards compatibility
        script_env.set("_G", script_env.clone())?;

        // Copy safe globals from the sandboxed state
        let globals = self.lua.globals();
        for key in [
            "math",
            "string",
            "table",
            "tonumber",
            "tostring",
            "type",
            "pairs",
            "ipairs",
            "next",
            "setmetatable",
            "getmetatable",
        ] {
            if let Ok(value) = globals.get::<Value>(key) {
                script_env.set(key, value)?;
            }
        }

        // Set the API context (entity, world, events, etc.)
        script_env.set("entity", context.clone())?;
        script_env.set("world", context.get::<Table>("world")?)?;
        script_env.set("events", context.get::<Table>("events")?)?;
        script_env.set("time", context.get::<Table>("time")?)?;
        script_env.set("math_api", context.get::<Table>("math_api")?)?;
        script_env.set("log", context.get::<Table>("log")?)?;

        // Load the script source with the custom environment
        // In Lua 5.4, set_environment() sets the _ENV upvalue for the chunk
        let function = self
            .lua
            .load(source.as_str())
            .set_environment(script_env.clone())
            .into_function()?;

        // Execute the function to define its hooks in the environment
        function.call::<()>(())?;

        // Now look up the hook function in the script's environment
        let hook_fn = match script_env.get::<Value>(hook_name)? {
            Value::Function(function) => function,
            _ => {
                return Err(mlua::Error::external(format!(
                    "Hook '{}' not found",
                    hook_name
                )));
            }
        };

        // Call the hook with the context
        hook_fn.call::<Value>(context)
    }

    /// Get the Lua state (for advanced use)
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Create a new sandbox table with the provided API context
    pub fn create_context(&self) -> mlua::Result<Table> {
        self.lua.create_table()
    }

    /// Register a Rust function in the script's global namespace
    pub fn register_function(&self, name: &str, f: Function) -> mlua::Result<()> {
        self.lua.globals().set(name, f)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_creation() {
        let vm = ScriptVm::default().expect("VM creation failed");
        assert_eq!(vm.scripts.len(), 0);
    }

    #[test]
    fn test_script_loading() {
        let mut vm = ScriptVm::default().expect("VM creation failed");
        let result = vm.load_script("test", "local x = 42; function get_value() return x end");
        assert!(result.is_ok());
        assert_eq!(vm.scripts.len(), 1);
    }

    #[test]
    fn test_script_syntax_error() {
        let mut vm = ScriptVm::default().expect("VM creation failed");
        let result = vm.load_script("bad", "this is not valid lua ][");
        assert!(result.is_err());
    }

    #[test]
    fn test_script_unload() {
        let mut vm = ScriptVm::default().expect("VM creation failed");
        vm.load_script("test", "local x = 42").expect("load failed");
        assert_eq!(vm.scripts.len(), 1);
        vm.unload_script("test");
        assert_eq!(vm.scripts.len(), 0);
    }
}
