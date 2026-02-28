# AGENTS.md — Prompt or Die Operational Guide

## Project Identity
**Prompt or Die** (POD) — An open-source game studio platform for building games where autonomous AI agents are first-class citizens alongside human players. Built on Rust + SpacetimeDB 2.0.

## Build & Test

```bash
# Build everything
cargo build --workspace

# Run desktop PoC
cargo run --bin prompt-or-die

# Run dedicated server
cargo run --bin pod-server

# All tests
cargo test --workspace

# Single crate
cargo test -p pod-core

# Check + lint
cargo check --workspace
cargo clippy --workspace -- -D warnings
```

## Architecture Rules

### ECS Pattern (hecs)
- Entities are bags of components; systems are free functions over queries
- Never store entity references inside components — use `Entity` IDs
- All game state must be serializable for SpacetimeDB persistence

### Agent Pipeline (sacrosanct)
Every agent type goes through the SAME pipeline with IDENTICAL constraints:
```
Observe → Decide → Validate → Execute → Broadcast
```
No agent type (Human, LLM, Neural, Scripted, System) gets special treatment.

### SpacetimeDB Integration
- Game state lives in SpacetimeDB **tables**
- Game logic (tick, actions, physics) runs as **reducers** (atomic transactions)
- Clients subscribe to **SQL queries** for real-time state sync
- **Event tables** for transient data (observations, chat, combat events)
- Rust modules compile to WASM for SpacetimeDB deployment

### Determinism
- `ChaCha8Rng` seeded per world for ALL randomness
- Fixed tick rate (60 TPS) — no frame-rate dependency
- Reducer transactions are atomic (all-or-nothing)

### Cross-Platform
- Native: wgpu + winit, QUIC (quinn)
- Web (wasm32): wgpu web + PixiJS bridge, WebSocket
- Use `#[cfg(not(target_arch = "wasm32"))]` / `#[cfg(target_arch = "wasm32")]`

## Workspace Layout

```
crates/
  pod-core       # ECS, tick loop, agent trait, actions, observations, events
  pod-physics    # Rapier2D integration
  pod-spatial    # R-tree, NavMesh, A* pathfinding, raycasting
  pod-agents     # LlmAgent, ScriptedAgent, NeuralAgent
  pod-scripting  # Lua 5.4 VM (mlua)
  pod-render     # wgpu 2D/3D rendering
  pod-net        # Networking (QUIC/WebSocket + SpacetimeDB)
  pod-animation  # Keyframe clips, state machines, tweening
  pod-scene      # Scene graph, prefabs, asset pipeline, state stack
  pod-stdb       # SpacetimeDB 2.0 integration layer [NEW]
  pod-assets     # Asset generation & construction pipeline [NEW]
  pod-editor     # Game maker / visual editor [NEW]
apps/
  pod-desktop    # Desktop binary
  pod-server     # Dedicated server (tokio async)
specs/           # Requirement specifications
```

## Conventions

- **One task per iteration** — focus, complete, commit
- **cargo check before commit** — never commit broken code
- **Test what you build** — write tests alongside implementation
- **Update IMPLEMENTATION_PLAN.md** — mark done, add discovered tasks
- **Don't assume** — search codebase before implementing; avoid duplicates
- **Full implementations** — no placeholders, no stubs, no TODOs in committed code
- **Document the why** — comments explain reasoning, not mechanics
