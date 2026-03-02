//! # pod-scripting — Luau/Lua VM for entity scripting
//!
//! Embeds a secure Luau virtual machine for dynamic entity behavior.
//! Scripts are sandboxed and have resource limits.
//!
//! ## Architecture
//!
//! - **VM** (`vm.rs`): Manages Lua state, script compilation, and execution
//! - **API** (`api.rs`): Exposes entity, world, events, and math functions to scripts
//! - **Sandbox** (`sandbox.rs`): Security restrictions and resource limits
//!
//! ## Usage
//!
//! ```no_run
//! use pod_scripting::{ScriptVm, ScriptContext};
//!
//! let mut vm = ScriptVm::default().expect("VM init failed");
//!
//! // Load a script
//! let script = r#"
//!     function on_tick(entity, dt)
//!         log.info("Entity ticked")
//!     end
//! "#;
//! vm.load_script("my_script", script).expect("Script load failed");
//!
//! // Call a hook
//! let ctx = ScriptContext::new(42);
//! // ... setup context fields ...
//! // vm.call_hook("my_script", "on_tick", context).expect("Hook call failed");
//! ```
//!
//! ## Script Hooks
//!
//! Scripts can implement lifecycle hooks:
//! - `on_spawn(entity)` — when entity is created
//! - `on_tick(entity, dt)` — every frame
//! - `on_collision(entity, other)` — physics collision
//! - `on_damage(entity, amount, source)` — damage event
//! - `on_destroy(entity)` — before destruction

pub mod vm;
pub mod api;
pub mod sandbox;

pub use vm::ScriptVm;
pub use api::{ScriptContext, build_api};
pub use sandbox::SandboxConfig;

/// Initialize the scripting module
pub fn init() {
    log::info!("{} initialized (Lua 5.4 VM with sandbox)", env!("CARGO_PKG_NAME"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_init() {
        // Just verify that the module can be initialized
        init();
    }

    #[test]
    fn test_vm_and_api_together() {
        use mlua::Lua;

        let lua = Lua::new();
        let ctx = ScriptContext::new(1);

        // Build API and verify no errors
        let _api = build_api(&lua, &ctx).expect("API build failed");
    }
}
