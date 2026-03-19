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
- [CLI Surface](docs/cli-surface.md)
- [Reference Bootstrap](docs/reference-bootstrap.md)
- [Architecture Overview](docs/architecture.md)
- [Benchmark Suite](docs/benchmark-suite.md)
- [Rust SDK Boundary](docs/rust-sdk-boundary.md)

## Quick start

Discover the supported platform command surface from one entrypoint:

```bash
bun ./scripts/pod.ts list
bun ./scripts/pod.ts shell
bun ./scripts/pod.ts runtime server --dry-run
bun ./scripts/pod.ts web dev
bun ./scripts/pod.ts show pod-server --json
```

For attached terminal workflows, use `bun ./scripts/pod.ts shell`. For
interactive machine sessions, use newline-delimited JSON request/event objects
through `bun ./scripts/pod.ts shell --agent`.

```bash
printf '{"type":"builtin","requestId":"1","name":"context"}\n{"type":"builtin","requestId":"2","name":"exit"}\n' | bun ./scripts/pod.ts shell --agent
```

For an attached terminal workflow, use the interactive shell:

```bash
bun ./scripts/pod.ts shell
bun ./scripts/pod.ts shell --agent
bun ./scripts/pod.ts export events --format toon
bun ./scripts/pod.ts export world --format json
```

`pod shell` is the human-friendly attached terminal experience. `pod shell --agent`
is the machine-friendly interactive shell mode for long-lived autonomous agent
sessions. One-shot automation should keep using `list/show/env/command/run --json`.
TOON is reserved for the large LLM-facing export surfaces under
`pod export world|events|multiverse --format toon`.

### Build and run the main surfaces

```bash
bun ./scripts/pod.ts workspace build
bun ./scripts/pod.ts runtime desktop
bun ./scripts/pod.ts runtime server

cd apps/pod-web
bun install
cd ../..

bun ./scripts/pod.ts web dev
```

If you want to stay inside one terminal and inspect supported commands as you
go, use `bun ./scripts/pod.ts shell`. For machine-driven interactive sessions,
use `bun ./scripts/pod.ts shell --agent`.

### Run the main proof surfaces

```bash
bun ./scripts/pod.ts runtime headless -- --profile ci-smoke
bun ./scripts/pod.ts runtime headless -- --profile ci-smoke --dataset-output /tmp/pod-headless-dataset.json --topology-output /tmp/pod-headless-topology.json
bun ./scripts/pod.ts benchmark controller-parity
bun ./scripts/pod.ts benchmark toon-exports -- --fail-on-checks
bun ./scripts/benchmark_toon_exports.ts --profile extensive --output artifacts/toon-export-benchmark-extensive.json --html-output artifacts/toon-export-benchmark-extensive.html --markdown-output artifacts/toon-export-benchmark-extensive.md --charts-dir artifacts/toon-export-benchmark-extensive-charts --fail-on-checks
bun ./scripts/pod.ts benchmark topology-feed -- --topology-input /tmp/pod-headless-topology.json --fail-on-checks
bun ./scripts/pod.ts workspace test
bun ./scripts/pod.ts workspace check
```

### Publish the weekly shard-target snapshot

```bash
bun ./scripts/pod.ts history run-shard-target-snapshot -- --label 2026-W11
```

For retained history, snapshot comparison, and benchmark interpretation, see
[Benchmark Suite](docs/benchmark-suite.md) and
[Benchmark Snapshot History](docs/benchmark-snapshots/README.md).

### Stage an asset

```bash
bun ./scripts/pod.ts assets stage-import -- --output-root artifacts/staged-assets path/to/asset.glb
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
- [CLI Surface](docs/cli-surface.md)
- [Architecture Overview](docs/architecture.md)
- [Plugin Model](docs/plugin-model.md)
- [Agent Integration Contract](docs/agent-integration-contract.md)
- [Agent Runtime Audit](docs/agent-runtime-audit.md)
- [Multi-World Agent Topology](docs/multi-world-agent-topology.md)
- [Asset Pipeline](docs/asset-pipeline.md)
- [Reference Bootstrap](docs/reference-bootstrap.md)
- [Benchmark Suite](docs/benchmark-suite.md)
- [Platform Stabilization](docs/platform-stabilization.md)
- [Rust SDK Boundary](docs/rust-sdk-boundary.md)
- [Benchmark Snapshot History](docs/benchmark-snapshots/README.md)
- [Competitive Matrix](docs/competitive-matrix.md)
- [Moat Gates](docs/moat-gates.md)
- [Bootstrap Showcase Research](docs/bootstrap-showcase-research.md)

## Planning route

- Active execution checklist: [IMPLEMENTATION_PHASES.md](IMPLEMENTATION_PHASES.md)
- Historical implementation log: [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md)
- Current session tracker: [SESSION.md](SESSION.md)
- Delivery recap: [progress.md](progress.md)

## Current status

The repo already has the deterministic core, browser client, headless
multi-world runner, shared topology contract, controller parity harness, and
weekly shard-target benchmark workflow in place. The next major layers are
public platform hardening, import/shipping polish, and a formal plugin/app
lifecycle. The current hardening contract for benchmark tiers, planning-doc
ownership, shipping profiles, authz policy scope, and SDK boundaries lives in
[docs/platform-stabilization.md](docs/platform-stabilization.md). The repo-owned
Rust SDK hookup boundary now lives in
[docs/rust-sdk-boundary.md](docs/rust-sdk-boundary.md).

## Release history

- [v0.1.0-alpha.2](docs/releases/0.1.0-alpha.2.md)
- [v0.1.0-alpha.1](docs/releases/0.1.0-alpha.1.md)
- [Migration: Wave 1 reconnect protocol + StDB connection mode](docs/migrations/2026-03-03-wave1-reconnect-and-stdb-mode.md)
