# Prompt or Die

Prompt or Die is an open-source game platform for building deterministic,
authoritative worlds where autonomous AI agents and human players operate under
the same gameplay rules. The runtime is written in Rust, supports native and
browser clients, and is being extended toward a full 2D, 2.5D, and 3D
authoring stack.

Current release posture: `v0.1.0-alpha.2`, an early testing alpha. Expect
breaking changes across runtime contracts, tooling, and benchmark/report
surfaces while the platform hardens.

## Start here

- [Documentation Hub](docs/README.md)
- [Reference Bootstrap](docs/reference-bootstrap.md)
- [Architecture Overview](docs/architecture.md)
- [Benchmark Suite](docs/benchmark-suite.md)

## Quick start

### Build and run the main surfaces

```bash
cargo build --workspace
cargo run --bin prompt-or-die
cargo run --bin pod-server

cd apps/pod-web
bun install
bun run dev
```

### Run the main proof surfaces

```bash
cargo run --bin pod-headless -- --profile ci-smoke
cargo run --bin pod-headless -- --profile ci-smoke --dataset-output /tmp/pod-headless-dataset.json --topology-output /tmp/pod-headless-topology.json
cargo run -q -p pod-agents --example controller_parity_benchmark -- --fail-on-checks
cargo run -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input /tmp/pod-headless-topology.json --fail-on-checks

cargo test --workspace
cargo check --workspace
```

### Publish the weekly shard-target snapshot

```bash
bun ./scripts/run_shard_target_snapshot.ts --label 2026-W11
```

For retained history, snapshot comparison, and benchmark interpretation, see
[Benchmark Suite](docs/benchmark-suite.md) and
[Benchmark Snapshot History](docs/benchmark-snapshots/README.md).

### Stage an asset

```bash
cargo run -p pod-assets --example stage_import -- --output-root artifacts/staged-assets path/to/asset.glb
```

For bundle specs, `--materialize-runtime`, KTX2/meshopt variants, runtime
selection rules, and browser asset verification, see
[Asset Pipeline](docs/asset-pipeline.md).

## What exists today

- Deterministic ECS runtime in `pod-core` with one shared agent pipeline:
  `Observe -> Decide -> Validate -> Execute -> Broadcast`
- Native and browser rendering surfaces in `pod-render`
- A real browser-side Three.js client in `apps/pod-web`
- Headless multi-world tournament and evaluation entrypoint in
  `apps/pod-headless`
- Scene, prefab, save/load, and state-stack authoring in `pod-scene`
- Direct-connect networking plus SpacetimeDB integration in `pod-net` and
  `pod-stdb`
- Asset processing, animation, scripting, spatial, and physics support across
  the workspace

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
  pod-headless   Headless multi-world tournament and evaluation runner
  pod-web        Browser-side Three.js/WebGPU client and bridge demo
  pod-server     Dedicated authoritative server
specs/
  Product and subsystem requirements
docs/
  Public architecture, workflow, benchmark, and release guides
```

## Documentation

- [Documentation Hub](docs/README.md)
- [Architecture Overview](docs/architecture.md)
- [Plugin Model](docs/plugin-model.md)
- [Agent Integration Contract](docs/agent-integration-contract.md)
- [Agent Runtime Audit](docs/agent-runtime-audit.md)
- [Multi-World Agent Topology](docs/multi-world-agent-topology.md)
- [Asset Pipeline](docs/asset-pipeline.md)
- [Reference Bootstrap](docs/reference-bootstrap.md)
- [Benchmark Suite](docs/benchmark-suite.md)
- [Benchmark Snapshot History](docs/benchmark-snapshots/README.md)
- [Competitive Matrix](docs/competitive-matrix.md)
- [Moat Gates](docs/moat-gates.md)
- [Bootstrap Showcase Research](docs/bootstrap-showcase-research.md)

## Current status

The repo already has the deterministic core, browser client, headless
multi-world runner, shared topology contract, controller parity harness, and
weekly shard-target benchmark workflow in place. The next major layers are
public platform hardening, import/shipping polish, and a formal plugin/app
lifecycle.

## Release history

- [v0.1.0-alpha.2](docs/releases/0.1.0-alpha.2.md)
- [v0.1.0-alpha.1](docs/releases/0.1.0-alpha.1.md)
- [Migration: Wave 1 reconnect protocol + StDB connection mode](docs/migrations/2026-03-03-wave1-reconnect-and-stdb-mode.md)
