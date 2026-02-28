# pod-scripting Architecture

Complete architectural overview of the scripting system.

## System Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    Game Engine (pod-core)                   │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐   │
│  │            World Tick Loop                           │   │
│  │  - Process entities                                  │   │
│  │  - Call lifecycle hooks                              │   │
│  │  - Emit events                                       │   │
│  └──────────────┬─────────────────────────────────────┘   │
│                 │                                          │
│                 ▼                                          │
│  ┌──────────────────────────────────────────────────────┐   │
│  │         Script VM (pod-scripting)                    │   │
│  │                                                      │   │
│  │  ┌────────────────────────────────────────────────┐  │   │
│  │  │ ScriptVm                                       │  │   │
│  │  │ - Owns Lua 5.4 state                           │  │   │
│  │  │ - Manages script cache                         │  │   │
│  │  │ - Calls hooks (on_tick, on_damage, etc)        │  │   │
│  │  └────────────────────────────────────────────────┘  │   │
│  │                                                      │   │
│  │  ┌────────────────────────────────────────────────┐  │   │
│  │  │ Sandbox                                        │  │   │
│  │  │ - Removes unsafe modules (os, io, debug)       │  │   │
│  │  │ - Enforces memory limits (1MB default)         │  │   │
│  │  │ - Enforces instruction limits (10k default)    │  │   │
│  │  └────────────────────────────────────────────────┘  │   │
│  │                                                      │   │
│  │  ┌────────────────────────────────────────────────┐  │   │
│  │  │ Script API Tables (Lua Globals)                │  │   │
│  │  │ ┌────────────────────────────────────────────┐ │  │   │
│  │  │ │ entity.*                                   │ │  │   │
│  │  │ │ - get_position() -> {x, y}                │ │  │   │
│  │  │ │ - set_position(x, y)                      │ │  │   │
│  │  │ │ - get_rotation() -> number                │ │  │   │
│  │  │ │ - get_velocity() -> {x, y}                │ │  │   │
│  │  │ │ - get_health() -> {current, max}          │ │  │   │
│  │  │ └────────────────────────────────────────────┘ │  │   │
│  │  │ ┌────────────────────────────────────────────┐ │  │   │
│  │  │ │ world.*                                    │ │  │   │
│  │  │ │ - spawn(type, x, y) -> id                 │ │  │   │
│  │  │ │ - destroy(id)                             │ │  │   │
│  │  │ │ - find_nearest(x, y, r, tag) -> [...]    │ │  │   │
│  │  │ └────────────────────────────────────────────┘ │  │   │
│  │  │ ┌────────────────────────────────────────────┐ │  │   │
│  │  │ │ events.*                                   │ │  │   │
│  │  │ │ - emit(name, data)                        │ │  │   │
│  │  │ └────────────────────────────────────────────┘ │  │   │
│  │  │ ┌────────────────────────────────────────────┐ │  │   │
│  │  │ │ time.*                                     │ │  │   │
│  │  │ │ - tick() -> u64                           │ │  │   │
│  │  │ │ - dt() -> f32                             │ │  │   │
│  │  │ └────────────────────────────────────────────┘ │  │   │
│  │  │ ┌────────────────────────────────────────────┐ │  │   │
│  │  │ │ math_api.*                                 │ │  │   │
│  │  │ │ - distance(x1, y1, x2, y2) -> f32        │ │  │   │
│  │  │ │ - direction(x1, y1, x2, y2) -> {x, y}   │ │  │   │
│  │  │ │ - random() -> f32                         │ │  │   │
│  │  │ └────────────────────────────────────────────┘ │  │   │
│  │  │ ┌────────────────────────────────────────────┐ │  │   │
│  │  │ │ log.*                                      │ │  │   │
│  │  │ │ - info(msg)                               │ │  │   │
│  │  │ │ - warn(msg)                               │ │  │   │
│  │  │ │ - error(msg)                              │ │  │   │
│  │  │ └────────────────────────────────────────────┘ │  │   │
│  │  └────────────────────────────────────────────────┘  │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Module Layout

```
pod-scripting/
├── Cargo.toml                 # Dependencies
├── README.md                  # User documentation
├── IMPLEMENTATION.md          # Technical implementation details
├── ARCHITECTURE.md            # This file
├── EXAMPLES.md                # Example scripts
└── src/
    ├── lib.rs                 # Module root, exports
    ├── vm.rs                  # ScriptVm - Lua state manager
    ├── api.rs                 # ScriptContext, build_api
    └── sandbox.rs             # SandboxConfig, apply_sandbox
```

## Data Flow

### Script Loading

```
load_script(name, source)
    ↓
Compile Lua source → Result<Function>
    ↓
Cache in ScriptVm::scripts HashMap
    ↓
Ready for execution
```

### Hook Execution

```
call_hook(script_name, hook_name, context)
    ↓
Retrieve cached script function
    ↓
Create isolated Lua environment
    ↓
Set up API tables (entity, world, events, etc)
    ↓
Execute script to define functions
    ↓
Call hook_name function with context
    ↓
Return Value or catch error
```

### Error Handling Chain

```
Script Error (syntax, runtime, etc)
    ↓
Catch in mlua::Error
    ↓
Log via log::error!()
    ↓
Return Err to caller
    ↓
Caller decides: retry, disable script, or continue
```

## Type System

### ScriptVm

```rust
pub struct ScriptVm {
    lua: Lua,                          // The Lua 5.4 state
    scripts: HashMap<String, Function>, // Compiled & cached scripts
    config: SandboxConfig,             // Security settings
}
```

### ScriptContext

```rust
pub struct ScriptContext {
    pub entity_id: u64,      // Which entity this context is for
    pub position: Vec2,      // Entity position
    pub rotation: f32,       // Entity rotation in radians
    pub velocity: Vec2,      // Entity velocity
    pub health: (f32, f32),  // (current, max) health
    pub tick: u64,           // Current world tick
    pub dt: f32,             // Delta time since last frame
}
```

### SandboxConfig

```rust
pub struct SandboxConfig {
    pub memory_limit: usize,      // Default: 1MB
    pub instruction_limit: u32,   // Default: 10,000
}
```

## Integration with pod-core

The scripting system connects to engine components:

### Components Used

```
Script (component)
    ├── source: String        (asset key, e.g., "player_ai")
    └── enabled: bool

Transform (component)
    ├── position: Vec2
    ├── rotation: f32
    └── scale: Vec2

Velocity (component)
    ├── linear: Vec2
    └── angular: f32

Health (component)
    ├── current: f32
    ├── max: f32
    ├── armor: f32
    └── invulnerable: bool

EventBus (system)
    └── Used for script-emitted events
```

### Integration Points

1. **Script Loading** — Triggered by Script component presence
2. **Tick Hook** — Called each frame in main game loop
3. **Collision Hook** — Called by physics system (pod-physics)
4. **Damage Hook** — Called by damage system
5. **Event Emission** — Scripts emit through EventBus

## Execution Model

### Per-Script Isolation

Each script execution happens in its own Lua environment:

```lua
-- Script 1 & 2 don't interfere
Script 1: local my_var = 42
Script 2: local my_var = 99  -- Different variable
```

Each environment gets:
- Safe globals (math, string, table, etc)
- API tables (entity, world, events, etc)
- Fresh state (no pollution between calls)

### Resource Limits

Memory and instruction limits are configurable:

```
Memory:      1MB per script instance
Instructions: 10,000 per hook call
```

When limits are exceeded:
- Script is terminated
- Error is logged
- Execution returns Err
- Entity/world state is preserved

## Security Model

### What Scripts CAN'T Do

```
❌ File I/O (os, io modules removed)
❌ Code loading (require, loadfile removed)
❌ Environment introspection (debug module removed)
❌ Memory access (rawget, rawset removed)
❌ Infinite loops (instruction limit)
❌ Memory exhaustion (memory limit)
```

### What Scripts CAN Do

```
✓ Read entity state (position, health, velocity)
✓ Modify own entity's transform
✓ Query nearby entities
✓ Emit events
✓ Use math utilities
✓ Log messages
✓ Use standard Lua libraries (math, string, table, pairs, ipairs)
```

## Performance Characteristics

### Compilation Phase

- **First load**: ~1-5ms (depends on script size)
- **Subsequent loads**: Cached, instant

### Execution Phase

- **Hook call overhead**: ~0.1-0.2ms
- **Script execution**: Depends on script complexity
- **Typical hook**: 0.5-2ms

### Memory

- **VM overhead**: ~500KB (Lua 5.4 state)
- **Per script**: ~10KB average (cached bytecode)
- **Per execution**: Limited by SandboxConfig

## Example Integration Code

### In the Game Engine

```rust
use pod_scripting::{ScriptVm, ScriptContext, build_api};

// Initialize VM once at startup
let mut script_vm = ScriptVm::default()?;

// Load scripts at level load time
for (name, source) in load_scripts_from_disk() {
    script_vm.load_script(&name, &source)?;
}

// In main tick loop, for each entity with Script component:
if let Some(Script { source, enabled }) = get_script_component(entity) {
    if !enabled {
        continue;
    }

    // Build context
    let mut ctx = ScriptContext::new(entity.id);
    ctx.position = get_component::<Transform>(entity).position;
    ctx.rotation = get_component::<Transform>(entity).rotation;
    ctx.velocity = get_component::<Velocity>(entity).linear;

    let health = get_component::<Health>(entity);
    ctx.health = (health.current, health.max);
    ctx.tick = world.current_tick;
    ctx.dt = world.delta_time;

    // Build API
    let api = build_api(script_vm.lua(), &ctx)?;

    // Call hook
    match script_vm.call_hook(&source, "on_tick", api) {
        Ok(_) => {},
        Err(e) => {
            log::error!("Script error in entity {}: {}", entity.id, e);
            // Continue, entity still ticks
        }
    }

    // Handle position changes requested by script
    // (script called entity.set_position(), we apply it here)
    if let Some(new_pos) = ctx.requested_position {
        set_component::<Transform>(entity, Transform {
            position: new_pos,
            ..get_component(entity)
        });
    }
}
```

## Testing Strategy

### Unit Tests

- VM creation and initialization
- Script loading with various syntaxes
- API table building
- Sandbox restrictions

### Integration Tests

- Script calling hooks
- Error handling and recovery
- Context data passing
- Event emission

### Performance Tests

- Script compilation time
- Hook execution time
- Memory usage under load
- Instruction limit enforcement

## Future Extensions

### Considered Additions (Not in v1)

1. **Coroutines** — For long-running tasks split across frames
2. **Debugging Hooks** — Breakpoints, watches, profiling
3. **Hot Reload** — Update scripts without engine restart
4. **Bytecode Caching** — Faster loading of large scripts
5. **Custom Allocator** — Better memory tracking
6. **Networking Sandbox** — Block socket access
7. **Async Hooks** — Scripts that span multiple frames
8. **Script Signals** — Kill/pause running scripts

## Conclusion

The pod-scripting system provides a secure, efficient way to add dynamic behavior to entities without requiring game engine recompilation. By using a sandboxed Lua VM with carefully controlled APIs, scripts can implement complex AI and behavior while remaining safe, fast, and easy to debug.
