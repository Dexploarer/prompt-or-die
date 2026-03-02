# CLAUDE.md — Prompt or Die Engine

## Build & Run

```bash
# Build entire workspace
cargo build

# Run desktop PoC (pure simulation, no rendering)
cargo run --bin prompt-or-die

# Run dedicated server (tokio async)
cargo run --bin pod-server

# Tests
cargo test                    # all workspace tests
cargo test -p pod-core        # single crate

# Check / lint
cargo check
cargo clippy --workspace
```

### Server Environment Variables

| Variable | Default | Description |
|---|---|---|
| `POD_BIND_ADDRESS` | `0.0.0.0:7777` | Server bind address |
| `POD_TICK_RATE` | `60` | Target tick rate (Hz) |
| `POD_MAX_CLIENTS` | `32` | Max concurrent clients |
| `POD_WORLD_SEED` | `42` | Deterministic world seed |
| `POD_MAP_NAME` | `default` | Map to load |

## Architecture

**Agent-native 2D game engine** — humans and AI agents operate through the same pipeline with identical constraints. No agent type gets special treatment.

### Workspace Layout

```
crates/
  pod-core       # ECS world, tick loop, agent trait, actions, observations, events
  pod-physics    # Rapier2D integration with bidirectional ECS sync
  pod-spatial    # R-tree index, uniform grid, NavMesh, A* pathfinding, raycasting
  pod-agents     # LlmAgent, ScriptedAgent (BT/FSM), NeuralAgent
  pod-scripting  # Lua 5.4 VM (mlua) with sandbox
  pod-render     # wgpu (native) / PixiJS bridge (web)
  pod-net        # QUIC (native, bincode) / WebSocket (web, JSON), server-authoritative
  pod-animation  # Keyframe clips, state machines, tweening, blending
  pod-scene      # Scene graph, prefabs, asset pipeline, state stack, save/load
apps/
  pod-desktop    # Desktop binary — simulation PoC with WandererAgent
  pod-server     # Dedicated server binary — tokio async game loop
```

### Tick Pipeline (60 TPS, deterministic)

Every tick executes 5 phases in order (`crates/pod-core/src/tick.rs`):

1. **Build Observations** — perception queries (vision range/FOV, hearing) produce `Observation` per agent
2. **Deliver & Collect Decisions** — agents receive observations, return `Vec<Action>`
3. **Validate & Execute Actions** — `AgentConstraints` enforced identically for all agent types (3 actions/tick, cooldowns), then actions applied
4. **Physics/Movement** — velocity integration, collision resolution
5. **Flush Events** — event bus broadcasts to listeners

### Core Patterns

- **ECS**: `hecs` — entities are bags of components, systems are free functions over queries
- **Determinism**: `ChaCha8Rng` seeded per world, used for all game randomness
- **Agent trait** (`pod-core/src/agent.rs`): `observe(&mut self, obs)` → `decide(&mut self) -> Vec<Action>` — all agents implement this
- **AgentType enum**: Human, LlmAgent, NeuralAgent, ScriptedNpc, System
- **Actions**: Move, Stop, Rotate, LookAt, Attack, AttackTarget, UseAbility, Interact, Speak, Signal, Idle, Spawn
- **Observations**: serializable to structured text via `to_agent_prompt()` for LLM consumption
- **EntityBuilder** (`world.spawn_at(x, y).with_color(...).with_label(...).build()`) for fluent entity creation

### Key Constants

- `TICKS_PER_SECOND = 60` (pod-core)
- `actions_per_tick = 3` (AgentConstraints default)
- `attack_cooldown = 30 ticks` (0.5s)

### Cross-Platform

- **Native**: wgpu + winit rendering, QUIC networking (quinn), full physics
- **Web (wasm32)**: wgpu web + PixiJS bridge, WebSocket networking, JSON serialization
- Conditional compilation via `#[cfg(not(target_arch = "wasm32"))]` and `#[cfg(target_arch = "wasm32")]`
