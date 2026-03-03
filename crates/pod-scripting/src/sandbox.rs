//! Script sandbox — security and resource limits

use mlua::Lua;

/// Sandbox configuration for script execution
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Maximum memory per script instance (bytes)
    pub memory_limit: usize,
    /// Maximum instruction count per call
    pub instruction_limit: u32,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            memory_limit: 1024 * 1024,           // 1MB
            instruction_limit: 10000,             // 10k instructions
        }
    }
}

/// Apply sandbox restrictions to a Lua state
pub fn apply_sandbox(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();

    // Remove dangerous standard library modules
    globals.set("os", mlua::Value::Nil)?;
    globals.set("io", mlua::Value::Nil)?;
    globals.set("loadfile", mlua::Value::Nil)?;
    globals.set("dofile", mlua::Value::Nil)?;
    globals.set("require", mlua::Value::Nil)?;
    globals.set("debug", mlua::Value::Nil)?;
    globals.set("package", mlua::Value::Nil)?;

    // Remove unsafe table functions
    if let mlua::Value::Table(table) = globals.get::<mlua::Value>("table")? {
        table.set("load", mlua::Value::Nil)?;
        table.set("loadstring", mlua::Value::Nil)?;
    }

    // In Lua 5.4, per-function environments use _ENV upvalues and are set at load time.
    // getfenv/setfenv (Lua 5.1) don't exist in Lua 5.4, but we remove them if present.
    // Also remove debug introspection and raw table access to prevent sandbox escape.
    globals.set("getfenv", mlua::Value::Nil)?;
    globals.set("setfenv", mlua::Value::Nil)?;
    globals.set("rawget", mlua::Value::Nil)?;
    globals.set("rawset", mlua::Value::Nil)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_blocks_os_module() {
        let lua = Lua::new();
        apply_sandbox(&lua).unwrap();

        // Verify os is removed
        assert!(lua
            .load("return os")
            .eval::<Option<mlua::Table>>()
            .is_ok());
        let result: mlua::Value = lua.load("return os").eval().unwrap();
        assert!(result.is_nil());
    }
}
