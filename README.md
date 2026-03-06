# Prompt or Die

Prompt or Die is an open-source game platform for building games where autonomous AI agents and human players are first-class participants in the same world. The runtime is written in Rust, uses a deterministic ECS core, supports native and browser clients, and is being extended toward a full 2D, 2.5D, and 3D authoring stack.

## What exists today

- Deterministic ECS runtime in `pod-core` with a shared agent pipeline: Observe -> Decide -> Validate -> Execute -> Broadcast
- Native and browser rendering surfaces in `pod-render`, including mixed 2D/2.5D/3D frame extraction
- A real browser-side Three.js client in `apps/pod-web` that consumes the WebGPU frame contract
- Scene, prefab, save/load, and state-stack authoring in `pod-scene`
- Dedicated editor shell in `pod-editor`
- Direct-connect networking plus SpacetimeDB integration in `pod-net` and `pod-stdb`
- Asset processing, animation, scripting, spatial queries, and physics support across the workspace

## Quick start

```bash
cargo build --workspace
cargo run --bin prompt-or-die
cargo run --bin pod-server
cargo test --workspace
cargo check --workspace

cd apps/pod-web
bun install
bun run dev
```

## Workspace map

```text
crates/
  pod-core       Deterministic ECS world, tick loop, agent contract, actions, observations, events
  pod-render     Native wgpu renderer and browser bridge
  pod-scene      Scenes, prefabs, save/load, typed bindings, streaming
  pod-net        QUIC/WebSocket transport and SpacetimeDB-aware clients
  pod-stdb       SpacetimeDB tables, reducers, events, and client wrapper
  pod-agents     LLM, neural, scripted, and hybrid agent implementations
  pod-editor     Visual editor shell and authoring panels
  pod-assets     Asset import, processing, caching, and procedural generation
  pod-animation  Keyframes, tweening, blending, and state machines
  pod-physics    Physics integration
  pod-spatial    Pathfinding, raycasts, and spatial queries
  pod-scripting  Lua scripting API and sandbox
apps/
  pod-desktop    Desktop runtime and local simulation entry point
  pod-web        Browser-side Three.js/WebGPU client and bridge demo
  pod-server     Dedicated authoritative server
specs/
  Product and subsystem requirements
docs/
  Public architecture and integration guides
```

## Platform docs

- [Architecture Overview](docs/architecture.md)
- [Plugin Model](docs/plugin-model.md)
- [Agent Integration Contract](docs/agent-integration-contract.md)

## Current status

The project has completed its deterministic core, networking, rendering baseline, editor scaffold, scene-system foundations, and the first real browser-side Three.js/WebGPU client. The next major layers are public platform hardening, import/shipping workflows, and a formal plugin lifecycle.
