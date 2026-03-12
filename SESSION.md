# Session State

**Current Phase**: Phase 8 - CI and Regression Gates
**Current Stage**: Implementation
**Last Checkpoint**: `b790e13b`
**Planning Docs**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md), [IMPLEMENTATION_PLAN.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PLAN.md), [progress.md](/Users/home/Desktop/prompt-or-die/progress.md)

---

## Phase 1: Browser Asset Fast Path ✅
**Completed**: 2026-03-11
**Summary**: Promoted `.glb` to the shipped browser path, kept `.gltf` sidecars, and fixed multi-mesh scene extraction.

## Phase 2: Asset Load Telemetry and Budgets ✅
**Completed**: 2026-03-11
**Summary**: Added geometry/sprite load counters plus average/slowest timings, surfaced them through renderer stats and the compact HUD line, and revalidated with targeted tests, full Bun tests, build, and browser smoke.

## Phase 3: Creator Import and Precompile Lane ✅
**Type**: Infrastructure
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Audit the current import boundary in `crates/pod-assets`
- [x] Materialize content-addressed artifacts for supported non-scene imports instead of only indexing metadata
- [x] Preserve `.gltf` / `.glb` source extensions during staging so the import lane stops pretending `.gltf` was already precompiled to `.glb`
- [x] Add a concrete command surface via `cargo run -p pod-assets --example stage_import -- ...`
- [x] Define the staged-source to runtime-precompile handoff for pod-web via `artifacts/source-assets`, `artifacts/staged-assets`, and `pod-staged-asset-manifest.json`
- [x] Extend staged imports to the current sample SVG texture sources so meshes and sprites share the same import boundary
- [x] Add machine-readable `--json` output to the staged-import CLI so pod-web can consume it programmatically
- [x] Move bundle-manifest assembly into `pod-assets` so `sync-assets.mjs` only emits the sample source set and runtime mapping spec
- [x] Keep `sync-assets.mjs` as the sample geometry generator, but move canonical runtime-public asset materialization into `pod-assets`
- [x] Validate the new handoff through `bun run sync:assets`, browser build, and browser smoke
- [x] Add deterministic validation that runtime-precompile output stays aligned with staged imports
- [x] Define explicit failure handling and optional staged `.ktx2` compression sidecars for the runtime bundle contract
- [x] Project available compressed sidecars into app-level runtime manifests without manual duplication
- [x] Express the full browser-facing contract in the sample lane by folding any authored `.ktx2` sprite sidecars into the shared runtime bundle spec automatically

**Next Action**: Start Phase 5 by profiling worker vs main-thread asset warmup and frame stability now that the shipped manifest prefers both meshopt-compressed geometry and KTX2 textures by budget.

**Key Files**:
- [crates/pod-assets/src/lib.rs](/Users/home/Desktop/prompt-or-die/crates/pod-assets/src/lib.rs)
- [crates/pod-assets/examples/stage_import.rs](/Users/home/Desktop/prompt-or-die/crates/pod-assets/examples/stage_import.rs)
- [apps/pod-web/scripts/sync-assets.mjs](/Users/home/Desktop/prompt-or-die/apps/pod-web/scripts/sync-assets.mjs)
- [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Known Issues**:
- The shared pipeline now ships checked-in `.ktx2` ring fixtures and `.meshopt.glb` mesh fixtures, and the runtime prefers both by budget; the next remaining bottleneck is how quickly those optimized assets warm on worker vs main-thread routes.
- Sample geometry generation intentionally remains in `apps/pod-web/scripts/sync-assets.mjs`; the next work should measure runtime behavior, not move that sample-authoring step again.

## Phase 4: Compression, LOD, and Shipping Discipline ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Add explicit mesh LOD outputs to the shared staged-to-runtime bundle contract and sample lane
- [x] Teach the shipped manifest to declare per-LOD runtime metadata and size/load estimates
- [x] Ensure `pod-web` chooses explicit LOD and compressed sprite variants deterministically from manifest metadata
- [x] Add a generated runtime budget report plus tests that enforce monotonic LOD reductions
- [x] Add real `.ktx2` ring fixtures to the sample lane and surface them in the shipped manifest plus runtime budget report
- [x] Add a genuinely smaller supercompressed sample so `.ktx2` becomes the default choice where it is actually faster
- [x] Add shared meshopt-compressed mesh fixtures plus runtime bundle/manifest metadata so shipped geometry can also prefer a compressed fast path by budget

**Key Files**:
- [crates/pod-assets/src/lib.rs](/Users/home/Desktop/prompt-or-die/crates/pod-assets/src/lib.rs)
- [apps/pod-web/scripts/sync-assets.mjs](/Users/home/Desktop/prompt-or-die/apps/pod-web/scripts/sync-assets.mjs)
- [apps/pod-web/src/assets.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/assets.ts)
- [apps/pod-web/artifacts/staged-assets/pod-runtime-budget-report.json](/Users/home/Desktop/prompt-or-die/apps/pod-web/artifacts/staged-assets/pod-runtime-budget-report.json)

**Known Issues**:
- The shipped sample set now prefers compressed assets on both fronts: `.ktx2` for ring sprites and `.meshopt.glb` for meshes where the compressed fixtures beat the source `.glb` outputs by budget.
- Browser smoke is deterministic across the optimized routes, but Phase 5 still needs explicit worker/main-thread perf counters so regressions surface as data, not only as pass/fail route checks.

## Phase 5: Render Worker and Main-Thread Relief ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Added explicit runtime warmup/frame-stability counters to `pod-web` renderer stats (`runtimePerf.warmupMs`, frame budget, stable/slow counts, stable-frame percentage, slowest frame)
- [x] Surfaced the new counters in the compact runtime HUD and added deterministic unit coverage
- [x] Extended browser smoke so both main-thread and worker local-sandbox routes must publish non-empty warmup/frame-stability stats after movement and asset warmup
- [x] Added explicit `mainThreadPerf` submission counters plus requested-vs-actual render-thread fallback metadata to `pod-web` runtime stats
- [x] Documented worker fallback reasons and tightened smoke to assert submission-counter consistency on both render routes
- [x] Coalesced worker-route frame submissions on the main thread so only the newest pending frame is posted while a worker render is still in flight
- [x] Removed the duplicate post-init worker `resize` sync by treating the init surface metrics as authoritative until the canvas actually changes
- [x] Added deterministic runtime tests for both worker hot-path reductions and revalidated build plus browser smoke
- [x] Added per-kind main-thread submission buckets (`frame`, `control`, `resize`) so worker-route traffic can be attributed without losing the existing aggregate counters
- [x] Tightened worker smoke to verify bucket reconciliation and stabilized the paused showcase screenshot gate by advancing time explicitly until the authored route reports ready
- [x] Batched worker-route world-event and telemetry traffic into a combined control message so multiple same-turn control updates collapse into one worker post
- [x] Preserved worker-frame ordering by flushing queued control state before the next frame submission and revalidated the serialized browser smoke harness
- [x] Turned the worker local-sandbox smoke into an explicit chatter regression gate by requiring `mainThreadPerf.byKind.control` and `resize` submissions to remain at zero on the worker route
- [x] Revalidated those ceilings with targeted Playwright sampling plus the full serialized smoke suite
- [x] Added deterministic `runtimePerf` frame-stability floors to the local-sandbox smoke route so Phase 5 now asserts worker/main-thread frame quality, not just submission chatter
- [x] Added `apps/pod-web/scripts/measure-render-routes.ts` and threaded its JSON output into `scripts/run_moat_benchmarks.ts` as `browserRouteMeasurements`

**Key Files**:
- [apps/pod-web/src/renderer.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/renderer.ts)
- [apps/pod-web/src/render-runtime.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/render-runtime.ts)
- [apps/pod-web/src/render-worker.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/render-worker.ts)
- [apps/pod-web/src/hud.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/hud.ts)
- [apps/pod-web/src/render-runtime-gates.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/render-runtime-gates.ts)
- [apps/pod-web/tests/worker-input.e2e.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/tests/worker-input.e2e.ts)
- [apps/pod-web/tests/showcase-visual.e2e.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/tests/showcase-visual.e2e.ts)
- [apps/pod-web/scripts/measure-render-routes.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/scripts/measure-render-routes.ts)
- [apps/pod-web/playwright.config.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/playwright.config.ts)
- [scripts/run_moat_benchmarks.ts](/Users/home/Desktop/prompt-or-die/scripts/run_moat_benchmarks.ts)
- [docs/benchmark-suite.md](/Users/home/Desktop/prompt-or-die/docs/benchmark-suite.md)
- [crates/pod-net/src/server.rs](/Users/home/Desktop/prompt-or-die/crates/pod-net/src/server.rs)
- [crates/pod-core/src/ops.rs](/Users/home/Desktop/prompt-or-die/crates/pod-core/src/ops.rs)
- [apps/pod-web/src/contracts.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/contracts.ts)

## Phase 6: Transport and Snapshot Performance Surface ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Added bounded shard/client transport metrics for full snapshot bytes, recovery bytes, delta bytes, delta entity churn, peak queue depth, and per-client queue-pressure incidents
- [x] Threaded those counters through `apps/pod-web` debug transport summaries while leaving the gameplay HUD compact
- [x] Added degraded-network coverage in `apps/pod-web/src/direct-connect.test.ts` so stale-authority backlog saturation now forces reconnect instead of local recovery once the heartbeat watchdog says authority is stale
- [x] Added `pod-net` server coverage for `ClientMessage::RequestFullSnapshot` and reconnect-token session resume in `crates/pod-net/src/server.rs`, proving recovery and resume flows preserve and increment the transport counters deterministically

**Key Files**:
- [apps/pod-web/src/direct-connect.test.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/direct-connect.test.ts)
- [crates/pod-net/src/server.rs](/Users/home/Desktop/prompt-or-die/crates/pod-net/src/server.rs)
- [crates/pod-core/src/ops.rs](/Users/home/Desktop/prompt-or-die/crates/pod-core/src/ops.rs)
- [apps/pod-web/src/contracts.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/contracts.ts)
- [apps/pod-web/src/hud.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/hud.ts)

**Known Issues**:
- The transport counters are now inspectable and regression-tested, but they still live in targeted tests and debug summaries rather than a dedicated moat benchmark lane.

## Phase 7: Plugin and Runtime Boundary Hardening ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Expanded [docs/plugin-model.md](/Users/home/Desktop/prompt-or-die/docs/plugin-model.md) into an explicit seam map that names the current contract surfaces integrators should depend on now
- [x] Updated [docs/architecture.md](/Users/home/Desktop/prompt-or-die/docs/architecture.md) to distinguish exported crate seams from app composition roots like [`main.ts`](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/main.ts)
- [x] Revalidated documented seams at the crate boundary across `pod-scene`, `pod-assets`, `pod-net`, and `pod-web`
- [x] Identified the remaining missing lifecycle/registration hooks that still force integrators into app bootstrap code

**Next Action**:
- Start Phase 8 by promoting the current asset-sync and benchmark checks into routine local/CI command surfaces.

## Phase 8: CI and Regression Gates 🔄
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)
