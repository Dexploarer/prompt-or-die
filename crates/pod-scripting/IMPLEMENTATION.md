# pod-scripting Implementation

Complete implementation of a secure Luau/Lua VM for the Prompt or Die game engine.

## Files Created

1. **src/lib.rs** — Main module with exports and documentation
2. **src/vm.rs** — ScriptVm: the Lua state manager
3. **src/api.rs** — ScriptContext and build_api: entity/world API for scripts
4. **src/sandbox.rs** — Security restrictions and sandboxing
5. **Cargo.toml** — Updated with mlua and required dependencies
6. **Root Cargo.toml** — Added mlua to workspace dependencies

## Architecture

### Virtual Machine (vm.rs)

The `ScriptVm` struct owns and manages the Lua state:

```rust
pub struct ScriptVm {
    lua: Lua,
    scripts: HashMap<String, mlua::Function>,
    config: SandboxConfig,
}
```

Key methods:

- **`new(config)`** — Creates a sandboxed Lua 5.4 state with the given config
- **`load_script(name, source)`** — Compiles and caches a script by name
- **`call_hook(script_name, hook_name, context)`** — Calls a function in a loaded script
- **`unload_script(name)`** — Removes a cached script
- **`create_context()`** — Creates an empty context table for API data
- **`register_function(name, f)`** — Registers a Rust function globally

Each script execution is sandboxed in its own environment, preventing scripts from interfering with each other or the VM itself.

### Script API (api.rs)

The `build_api()` function creates Lua tables exposing the game engine to scripts:

#### Entity API
```lua
entity.get_position()      -- {x, y}
entity.get_rotation()      -- number (radians)
entity.get_velocity()      -- {x, y}
entity.get_health()        -- {current, max}
entity.set_position(x, y)  -- validates position
```

#### World API
```lua
world.spawn(type, x, y)           -- entity_id
world.destroy(entity_id)          -- void
world.find_nearest(x, y, r, tag)  -- [{id, x, y, distance}, ...]
```

#### Events API
```lua
events.emit(name, data)  -- emit custom event
```

#### Time API
```lua
time.tick()  -- current tick number (u64)
time.dt()    -- delta time in seconds
```

#### Math API (custom, not stdlib)
```lua
math_api.distance(x1, y1, x2, y2)       -- number
math_api.direction(x1, y1, x2, y2)      -- {x, y} normalized
math_api.random()                       -- 0..1 (deterministic)
```

#### Logging API
```lua
log.info(message)    -- info level
log.warn(message)    -- warning level
log.error(message)   -- error level
```

All functions are carefully designed to validate input but not panic on invalid data. Errors are logged and caught.

### Sandbox (sandbox.rs)

The `SandboxConfig` structure controls security:

```rust
pub struct SandboxConfig {
    pub memory_limit: usize,           // Default: 1MB
    pub instruction_limit: u32,        // Default: 10,000
}
```

The `apply_sandbox()` function removes dangerous modules from the Lua environment:

- **Removed modules**: `os`, `io`, `debug`, `package`
- **Removed functions**: `loadfile`, `dofile`, `require`, `getfenv`, `setfenv`, `rawget`, `rawset`
- **Removed table functions**: `load`, `loadstring`

This prevents scripts from:
- Accessing the filesystem
- Making network calls
- Modifying the Rust environment
- Introspecting other scripts
- Accessing C extensions

## Script Lifecycle Hooks

Scripts can implement any of these functions:

```lua
function on_spawn(entity)
    -- Called when entity is created
    log.info("Entity spawned at " .. entity.get_position().x)
end

function on_tick(entity, dt)
    -- Called every frame
    local pos = entity.get_position()
    log.info("Position: " .. pos.x .. ", " .. pos.y)
end

function on_collision(entity, other)
    -- Called on physics collision
    log.warn("Collided with entity " .. other.id)
end

function on_damage(entity, amount, source)
    -- Called when taking damage
    local health = entity.get_health()
    log.error("Took " .. amount .. " damage, health: " .. health.current)
end

function on_destroy(entity)
    -- Called before entity destruction
    log.info("Entity destroyed")
end
```

## Integration with pod-core

The implementation integrates with pod-core types:

- **`Transform`** — position, rotation, scale → exposed to scripts
- **`Velocity`** — linear, angular → exposed to scripts
- **`Health`** — current, max, armor, invulnerable → exposed to scripts
- **`Script`** — component with source and enabled flag
- **`EventBus`** — used for script-emitted events

## Error Handling

The implementation is production-quality and **never panics** on bad script input:

- Syntax errors are caught at load time
- Runtime errors are caught and logged
- Invalid function signatures are caught before execution
- Missing scripts/hooks are handled gracefully
- Invalid parameters are validated in Rust (not exposed to scripts)

Example error handling:

```rust
match vm.call_hook("my_script", "on_tick", context) {
    Ok(_) => {},
    Err(e) => {
        log::error!("Script error: {}", e);
        // Continue execution, script is disabled or retried
    }
}
```

## Example Usage

```rust
use pod_scripting::{ScriptVm, ScriptContext, build_api};

fn main() {
    // Create VM
    let mut vm = ScriptVm::default().expect("VM init failed");

    // Load a script
    let script = r#"
        function on_spawn(entity)
            log.info("Hello from Lua!")
            local pos = entity.get_position()
            entity.set_position(pos.x + 10, pos.y)
        end

        function on_tick(entity, dt)
            local h = entity.get_health()
            if h.current < h.max * 0.2 then
                log.warn("Low health!")
            end
        end

        function on_damage(entity, amount, source)
            log.error("Took damage: " .. amount)
        end
    "#;

    vm.load_script("player_ai", script).expect("Failed to load");

    // Create context
    let ctx = ScriptContext::new(42);

    // Call hooks (in real engine, this happens in the main loop)
    // let api = build_api(&vm.lua(), &ctx).expect("API build failed");
    // vm.call_hook("player_ai", "on_tick", api).expect("Hook failed");
}
```

## Dependencies

Added to workspace Cargo.toml:
```toml
mlua = { version = "0.10", features = ["lua54", "vendored", "serialize"] }
```

This pulls in:
- `lua54` — Lua 5.4 (not Lua 5.1)
- `vendored` — Builds Lua from source (no external dependency)
- `serialize` — JSON serialization for data passing

Also uses existing workspace dependencies:
- `glam` — Vec2 for positions/velocities
- `serde` / `serde_json` — Serialization
- `log` — Logging
- `rand` / `rand_chacha` — Deterministic RNG

## Memory and Performance

- **Memory limit**: 1MB per script (configurable)
- **Instruction limit**: 10,000 per call (configurable)
- **Compilation**: Scripts are compiled once and cached
- **Execution**: Each call runs in a fresh context to prevent state leakage
- **RNG**: Uses deterministic seeded generator for reproducibility

## Testing

Includes unit tests for:
- VM creation and initialization
- Script loading and syntax validation
- API building
- Context creation
- Sandbox verification

Run with:
```bash
cargo test -p pod-scripting
```

## Security Guarantees

The implementation prevents:

1. **File I/O** — os, io, loadfile, dofile removed
2. **Code loading** — require, load, loadstring removed
3. **Environment introspection** — debug, getfenv, setfenv removed
4. **Unsafe table access** — rawget, rawset removed
5. **C extensions** — package module removed
6. **Memory exhaustion** — Memory limits enforced
7. **Infinite loops** — Instruction limits enforced
8. **Cross-script contamination** — Each script has isolated environment

## Future Enhancements

Possible improvements (not in current spec):

- Coroutines for long-running tasks
- Debugging hooks (breakpoints, watches)
- Profiling and performance monitoring
- Script hot-reloading
- Bytecode caching for faster loads
- Networking sandboxing (block socket access)
- Custom error handling callbacks
- Script timeouts and signal handling
