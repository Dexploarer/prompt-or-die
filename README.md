# Prompt or Die

Prompt or Die is an open-source game platform for building games where autonomous AI agents and human players are first-class participants in the same world. The runtime is written in Rust, uses a deterministic ECS core, supports native and browser clients, and is being extended toward a full 2D, 2.5D, and 3D authoring stack.

## What exists today

- Deterministic ECS runtime in `pod-core` with a shared agent pipeline: Observe -> Decide -> Validate -> Execute -> Broadcast
- Native and browser rendering surfaces in `pod-render`, including mixed 2D/2.5D/3D frame extraction
- A real browser-side Three.js client in `apps/pod-web` that consumes the WebGPU frame contract
- Headless multi-world tournament and evaluation entrypoint in `apps/pod-headless`
- Scene, prefab, save/load, and state-stack authoring in `pod-scene`
- Dedicated editor shell in `pod-editor`
- Direct-connect networking plus SpacetimeDB integration in `pod-net` and `pod-stdb`
- Asset processing, content-addressed source staging, animation, scripting, spatial queries, and physics support across the workspace

## Quick start

```bash
cargo build --workspace
cargo run --bin prompt-or-die
cargo run --bin pod-headless -- --profile ci-smoke
cargo run --bin pod-headless -- --profile ci-smoke --dataset-output /tmp/pod-headless-dataset.json
cargo run --bin pod-headless -- --profile ci-smoke --topology-output /tmp/pod-headless-topology.json
cargo run -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input /tmp/pod-headless-topology.json --fail-on-checks
cargo run -q -p pod-agents --example controller_parity_benchmark -- --fail-on-checks
bun ./scripts/run_shard_target_snapshot.ts --label 2026-03
bun ./scripts/compare_moat_snapshots.ts --baseline docs/benchmark-snapshots/2026-03-shard-target.json --candidate docs/benchmark-snapshots/2026-03-shard-target.json --output /tmp/pod-benchmark-snapshot-comparison.json
cargo run --bin pod-server
cargo test --workspace
cargo check --workspace

cd apps/pod-web
bun install
bun run dev
```

## Stage an asset

`pod-assets` now exposes a concrete staged-import entrypoint for supported authoring files:

```bash
cargo run -p pod-assets --example stage_import -- --output-root artifacts/staged-assets path/to/asset.glb
```

The command prints the content-addressed asset id, detected format, canonical source path, and staged output path. It preserves source extensions for staged `gltf/glb/jpeg/ktx2/svg` artifacts and serializes supported scene formats to `.scene.json`. Use `--json` to emit a machine-readable import list.

To assemble and materialize a runtime handoff from staged imports, pass a bundle spec and `--materialize-runtime`:

```bash
cargo run -p pod-assets --example stage_import -- --json --materialize-runtime --output-root artifacts/staged-assets --base-dir apps/pod-web --bundle-spec apps/pod-web/artifacts/staged-assets/pod-runtime-bundle-spec.json apps/pod-web/artifacts/source-assets/meshes/adventurer-avatar.glb
```

That keeps runtime bundle assembly and runtime-public asset materialization in `pod-assets` instead of duplicating the contract and filesystem copies inside app-specific scripts. The current `pod-web` sample lane keeps human-inspectable `.gltf` sidecars in `artifacts/source-assets` while staging `.glb` mesh sources for the shipped browser path. The runtime bundle spec can now also reference optional staged `.ktx2` texture sidecars for compressed browser texture variants, explicit mesh LOD variants, and checked-in meshopt-compressed `.meshopt.glb` mesh variants; bundle validation rejects conflicting output paths, non-`ktx2` compressed texture declarations, and malformed compressed mesh records. In `pod-web`, any matching `artifacts/source-assets/textures/<asset-id>.ktx2` sidecar is folded into that shared bundle spec automatically, and the sample sync emits LOD `.glb` mesh variants, checked-in ring `.ktx2` fixtures, and checked-in `.meshopt.glb` mesh fixtures through the same contract, so `.glb` meshes plus optional KTX2 sprite sidecars and meshopt mesh variants are the explicit browser-facing contract instead of app-local conventions.
When a consumer like `apps/pod-web` reads the emitted bundle manifest, it can now project staged `compressed_variant.runtime_path` values directly into app-level `ktx2Path` loader metadata and staged compressed mesh variants into `meshoptLods` / `runtime.compressedVariants` instead of hand-maintaining parallel asset maps. The sample runtime budget report also makes the selection truth explicit: runtime prefers whichever variant wins the budget report, the shipped ring sprites default to `.ktx2`, and the shipped sample meshes default to `meshopt` when the checked-in compressed fixtures beat the source `.glb` outputs.

Malformed bundle specs fail deterministically in the shared pipeline:
- duplicate runtime output paths are rejected before materialization
- compressed sprite sidecars must stage a real `.ktx2` source
- compressed mesh variants must resolve to staged glTF/GLB scene imports
- app-side sync scripts fail fast if `stage_import --json` does not return a valid bundle manifest payload

On the browser side, `window.podRender.getStats()` now exposes both `runtimePerf` and `mainThreadPerf` counters. `runtimePerf` tracks render-thread warmup time, stable-vs-slow frame counts, stable-frame percentage, and slowest frame time; `mainThreadPerf` tracks time-to-first-submission plus average/slowest main-thread frame-submission cost. The same stats surface also exposes average and slowest geometry/sprite load times, so frame and asset-load drift stay visible in artifacts instead of living in ad hoc profiling only. Stats also report the requested render-thread mode and any explicit worker fallback reason (`missing-worker-constructor`, `missing-offscreen-canvas`, `missing-canvas-transfer-control`) so Phase 5 comparisons stay honest when worker prerequisites are not available. Worker routes now also coalesce outbound frame submissions until the render worker replies, skip the duplicate post-init surface sync that previously re-sent unchanged canvas metrics immediately after initialization, attribute submission traffic by `frame`, `control`, and `resize` under `mainThreadPerf.byKind`, batch same-turn telemetry/world-event control updates into one worker post before the next frame, and enforce zero `control`/`resize` worker-route chatter on the local-sandbox smoke gate. For artifact-grade sampling outside the smoke suite, `apps/pod-web/scripts/measure-render-routes.ts` now emits `apps/pod-web/artifacts/render-route-measurements.json`, `bun run measure:render-routes:check` fails on deterministic browser invariants (completed asset loads plus worker chatter) while still recording stability and asset-load timing booleans in the artifact, and `scripts/run_moat_benchmarks.ts` includes that report under `browserRouteMeasurements`. The standard browser asset gate is now `cd apps/pod-web && bun run verify:assets`, which reruns `sync:assets` and fails if the committed generated asset trees drift.

Phase 6 transport visibility is now benchmarked too. Direct-connect shard transport summaries carry bounded snapshot and delta metrics in addition to the existing queue/recovery counters: full snapshot count and bytes, recovery snapshot bytes, delta message count and bytes, delta entity churn (`updated` / `destroyed`), peak pending queue depth, and per-client queue-pressure incident counts. The compact gameplay connection line intentionally stays short, while the richer transport rollup remains available in the browser debug panel so networking regressions can be inspected without polluting the main HUD.
That transport slice is now backed by both regression tests and a dedicated moat artifact: `cargo run -p pod-net --example transport_benchmark_suite -- --profile shard-target --fail-on-checks` runs deterministic in-process delta, recovery, resume, and degraded-path scenarios, publishes shard-target byte and queue-depth baselines, and `scripts/run_moat_benchmarks.ts` includes that JSON under `transportMeasurements` alongside the core, browser, and creator benchmark surfaces.

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
  Public architecture and integration guides
```

## Platform docs

- [Architecture Overview](docs/architecture.md)
- [Plugin Model](docs/plugin-model.md)
- [Agent Integration Contract](docs/agent-integration-contract.md)
- [Competitive Matrix](docs/competitive-matrix.md)
- [Moat Gates](docs/moat-gates.md)
- [Benchmark Suite](docs/benchmark-suite.md)
- [Reference Bootstrap](docs/reference-bootstrap.md)
- [Bootstrap Showcase Research](docs/bootstrap-showcase-research.md)

## Current status

The project has completed its deterministic core, networking, rendering baseline, editor scaffold, scene-system foundations, and the first real browser-side Three.js/WebGPU client. The next major layers are public platform hardening, import/shipping workflows, and a formal plugin lifecycle. `apps/pod-headless` is now the main non-UI proving ground for the agent roadmap: it can run deterministic multi-world scenarios, export reward-aware datasets, bind runtime agents to admitted teams per world, roll projected cross-world effects into applied target-world state summaries, resolve authored quest lines per world so alternate-reality objective shifts show up as explicit progression instead of raw counters, emit evaluation summaries for controller mix plus world-level quest/effect progress, emit `world_quest_bindings` plus a `topology_parity` summary in the main report, and export a shared `RemoteTopologyBundle` artifact that now has a real SpacetimeDB publication surface via `remote_topology_document` plus `publish_remote_topology_document`. `pod-agents` now also ships a curated cross-controller parity harness in `crates/pod-agents/src/controller_harness.rs`, with `crates/pod-agents/examples/controller_parity_benchmark.rs` exposing a cheap local benchmark for scripted, LLM, hybrid, and neural agents that publishes common validity, objective, encounter, latency, tool-call, and parity metrics. That topology layer is no longer headless-private: `pod-core` now owns `assign_roster_to_world_teams(...)`, `build_world_admission_summary(...)`, `build_world_quest_bindings(...)`, `build_remote_topology_bundle(...)`, `RemoteTopologyParitySummary`, and `build_remote_topology_parity_summary(...)`, so app, benchmark, and remote runtime surfaces share one topology assembly/consistency contract, including admitted team-slot assignment under `RemoteTopologyBundle.world_admissions`. That same parity signal is now part of the moat benchmark contract too: `scripts/run_moat_benchmarks.ts` records it as `headlessTopology`, fails on parity drift, and `scripts/publish_moat_snapshots.ts` preserves the headless summary in committed shard-target benchmark snapshots. Meanwhile, `pod-stdb` and `pod-net::SpacetimeDBClient` can ingest authority-published topology rows, keep snapshot metadata aligned with the decoded topology, expose resolved world-admission summaries alongside quest/evaluation state, forward the source document through the debug stream for inspection, drive generated-mode topology updates through a lightweight `GeneratedRuntimeBridge` hook seam for focused tests, a deterministic command-driven `GeneratedBindingRuntime` / `GeneratedBindingEndpoint` path for moat and CI parity coverage, and now a real generated SDK-backed `GeneratedSdkRuntime` installed through `install_generated_sdk_runtime(...)` when live generated bindings are available. `crates/pod-net/examples/topology_feed_benchmark_suite.rs` can now opt into that live path with `--generated-sdk-host`, `--generated-sdk-auth-token`, and `--generated-sdk-timeout-ms`; the repo has already produced successful local live artifacts at `artifacts/topology-feed-live-local.json` and `artifacts/topology-feed-live-shard-local.json`, the first committed monthly shard-target benchmark snapshot now lives at `docs/benchmark-snapshots/2026-03-shard-target.json`, `scripts/run_shard_target_snapshot.ts` now wraps the full local shard-target capture/publication flow into one reproducible command, and `scripts/compare_moat_snapshots.ts` now turns those published monthly snapshots into a structured regression/improvement report instead of manual diffing.
