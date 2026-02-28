# pod-scripting

A secure Luau/Lua 5.4 virtual machine for entity scripting in the Prompt or Die game engine.

## Overview

This crate embeds a sandboxed Lua 5.4 VM that allows dynamic entity behavior without requiring recompilation. Scripts can implement lifecycle hooks and interact with the game world through a safe, limited API.

## Quick Start

```rust
use pod_scripting::ScriptVm;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the VM
    let mut vm = ScriptVm::default()?;

    // Load a script
    let script = r#"
        function on_spawn(entity)
            log.info("Entity spawned!")
        end

        function on_tick(entity, dt)
            local pos = entity.get_position()
            log.info("Position: " .. pos.x .. ", " .. pos.y)
        end
    "#;

    vm.load_script("my_entity", script)?;

    Ok(())
}
```

## Script API

### Entity Functions

Access and modify the entity's state:

```lua
entity.get_position()       -- returns {x, y}
entity.set_position(x, y)   -- set entity position (validated)
entity.get_rotation()       -- returns rotation in radians
entity.get_velocity()       -- returns {x, y}
entity.get_health()         -- returns {current, max}
```

### World Functions

Interact with the game world:

```lua
entity_id = world.spawn("enemy", 100, 200)           -- create entity
world.destroy(entity_id)                             -- remove entity
nearby = world.find_nearest(x, y, radius, "enemy")   -- find entities
```

### Event Functions

Emit custom events:

```lua
events.emit("custom_event", {value = 42})
```

### Time Functions

Access timing information:

```lua
current_tick = time.tick()    -- current tick number
delta_time = time.dt()        -- time since last frame
```

### Math Functions

Utility math functions:

```lua
distance = math_api.distance(x1, y1, x2, y2)
direction = math_api.direction(x1, y1, x2, y2)  -- normalized {x, y}
random_value = math_api.random()                 -- 0.0 to 1.0
```

### Logging Functions

Log messages:

```lua
log.info("Information message")
log.warn("Warning message")
log.error("Error message")
```

## Lifecycle Hooks

Scripts can implement these hooks (all optional):

```lua
function on_spawn(entity)
    -- Called when entity is created
end

function on_tick(entity, dt)
    -- Called every frame
    -- dt is delta time in seconds
end

function on_collision(entity, other)
    -- Called when entity collides with another
    -- other has id, x, y, etc.
end

function on_damage(entity, amount, source)
    -- Called when entity takes damage
    -- source is the damaging entity ID or nil
end

function on_destroy(entity)
    -- Called before entity is destroyed
end
```

## Security

The VM is sandboxed to prevent:

- **File I/O**: No filesystem access (os, io removed)
- **Code loading**: No dynamic code loading (require, loadfile removed)
- **Environment introspection**: No debug access (debug module removed)
- **Unsafe operations**: No raw memory access (rawget, rawset removed)
- **Resource exhaustion**: Memory and instruction limits enforced

### Configuration

Customize security limits:

```rust
use pod_scripting::{ScriptVm, SandboxConfig};

let config = SandboxConfig {
    memory_limit: 2 * 1024 * 1024,  // 2MB
    instruction_limit: 50000,        // 50k instructions
};

let vm = ScriptVm::new(config)?;
```

## Error Handling

All errors are caught and logged. Scripts never cause panics:

```rust
match vm.load_script("my_script", source) {
    Ok(()) => log::info!("Script loaded"),
    Err(e) => log::error!("Script error: {}", e),
}

match vm.call_hook("my_script", "on_tick", context) {
    Ok(_) => {},
    Err(e) => log::error!("Hook error: {}", e),
}
```

## Module Structure

- **`vm.rs`** — `ScriptVm`: manages Lua state and script execution
- **`api.rs`** — `ScriptContext` and API building for scripts
- **`sandbox.rs`** — Security configuration and restrictions
- **`lib.rs`** — Public exports and module documentation

## Integration with pod-core

The scripting system integrates with core engine types:

- **`Transform`** — Position, rotation, scale exposed to scripts
- **`Velocity`** — Linear and angular velocity exposed to scripts
- **`Health`** — Current, max HP, armor exposed to scripts
- **`Script`** — Component specifying script asset and enabled state
- **`EventBus`** — Events emitted by scripts are broadcast

## Performance

- Scripts are compiled once and cached
- Each execution runs in a fresh context (isolation)
- Deterministic RNG for reproducibility
- Memory and instruction limits prevent runaway scripts
- Estimated overhead: ~1-2ms per hook call

## Testing

Run the test suite:

```bash
cargo test -p pod-scripting
```

Tests verify:
- VM initialization
- Script loading and compilation
- API building
- Error handling
- Sandbox restrictions

## Future Enhancements

Possible additions:
- Hot script reloading
- Coroutines for long tasks
- Debugging hooks and profiling
- Bytecode caching
- Custom error callbacks
- Networking restrictions
