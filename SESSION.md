# Session State

**Current Phase**: Phase 13 - Remote Agent Topology on SpacetimeDB
**Current Stage**: In Progress
**Last Checkpoint**: `e21bc6fc`
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
- The moat suite now has published shard-target transport byte and queue-depth baselines, but it still lacks committed historical shard-target snapshots for transport/browser drift review across monthly runs.

## Phase 7: Plugin and Runtime Boundary Hardening ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Expanded [docs/plugin-model.md](/Users/home/Desktop/prompt-or-die/docs/plugin-model.md) into an explicit seam map that names the current contract surfaces integrators should depend on now
- [x] Updated [docs/architecture.md](/Users/home/Desktop/prompt-or-die/docs/architecture.md) to distinguish exported crate seams from app composition roots like [`main.ts`](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/main.ts)
- [x] Revalidated documented seams at the crate boundary across `pod-scene`, `pod-assets`, `pod-net`, and `pod-web`
- [x] Identified the remaining missing lifecycle/registration hooks that still force integrators into app bootstrap code

**Next Action**:
- Publish committed shard-target moat snapshots for `transportMeasurements` and `browserRouteMeasurements` so drift can be reviewed historically instead of only through pass/fail gates.

## Phase 8: CI and Regression Gates ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Added `apps/pod-web/scripts/verify-generated-assets.ts` plus `bun run verify:assets`, so local/CI validation now reruns `sync:assets` and fails if the committed generated source/staged/runtime asset trees drift.
- [x] Extended the shared render-route gate surface with minimum completed-asset-load counts plus average/slowest geometry and sprite load ceilings in `apps/pod-web/src/render-runtime-gates.ts`, and enforced them in both `apps/pod-web/tests/worker-input.e2e.ts` and `apps/pod-web/scripts/measure-render-routes.ts`.
- [x] Added `bun run measure:render-routes:check`, which records `artifacts/render-route-measurements.json` and fails when shared frame-quality, worker-chatter, or asset-load thresholds regress.
- [x] Promoted both new gates into standard command surfaces by wiring them into `.github/workflows/ci.yml` and `scripts/run_moat_benchmarks.ts`, and CI now uploads `apps/pod-web/artifacts/render-route-measurements.json` plus `apps/pod-web/artifacts/staged-assets/pod-runtime-budget-report.json`.

**Key Files**:
- [apps/pod-web/scripts/verify-generated-assets.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/scripts/verify-generated-assets.ts)
- [apps/pod-web/scripts/measure-render-routes.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/scripts/measure-render-routes.ts)
- [apps/pod-web/tests/worker-input.e2e.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/tests/worker-input.e2e.ts)
- [apps/pod-web/src/render-runtime-gates.ts](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/render-runtime-gates.ts)
- [.github/workflows/ci.yml](/Users/home/Desktop/prompt-or-die/.github/workflows/ci.yml)
- [scripts/run_moat_benchmarks.ts](/Users/home/Desktop/prompt-or-die/scripts/run_moat_benchmarks.ts)

**Known Issues**:
- The browser asset/render-route gates and transport baselines are now routine, but the repo still lacks committed shard-target benchmark snapshots for drift review across monthly runs.

## Phase 9: Agent Runtime Audit and Contract Alignment ✅
**Completed**: 2026-03-12
**Summary**: Audited the live agent source of truth in `pod-core` and `pod-agents`, corrected the public agent integration contract to match the actual runtime surface, and published a grounded agent runtime audit that identifies the neural stack as the current weak point.

**Key Files**:
- [docs/agent-integration-contract.md](/Users/home/Desktop/prompt-or-die/docs/agent-integration-contract.md)
- [docs/agent-runtime-audit.md](/Users/home/Desktop/prompt-or-die/docs/agent-runtime-audit.md)
- [crates/pod-core/src/agent.rs](/Users/home/Desktop/prompt-or-die/crates/pod-core/src/agent.rs)
- [crates/pod-core/src/tick.rs](/Users/home/Desktop/prompt-or-die/crates/pod-core/src/tick.rs)
- [crates/pod-agents/src/neural_agent.rs](/Users/home/Desktop/prompt-or-die/crates/pod-agents/src/neural_agent.rs)
- [crates/pod-agents/src/llm_agent.rs](/Users/home/Desktop/prompt-or-die/crates/pod-agents/src/llm_agent.rs)
- [crates/pod-agents/src/hybrid_agent.rs](/Users/home/Desktop/prompt-or-die/crates/pod-agents/src/hybrid_agent.rs)

## Phase 10: Neural Runtime Hardening ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Extract the neural feature schema into an explicit versioned contract shared by encoder, ONNX loader, tests, and docs
- [x] Extract the neural action schema into an explicit registry instead of hard-coded positional assumptions
- [x] Add model metadata and compatibility checks so mismatched feature/action layouts fail loudly
- [x] Add deterministic tests for schema mismatch and inference fallback behavior
- [x] Surface neural-runtime compatibility and fallback status through introspection and telemetry

**Next Action**:
- Start the reward/replay contract by moving training outcomes away from caller-local `record_experience()` calls and toward authoritative action-outcome attribution.

**Known Issues**:
- The neural path is functional, but still mostly a thin inference scaffold compared with the LLM and hybrid agents.
- Reward attribution, dataset export discipline, and scenario evaluation still depend on follow-up phases and are not yet first-class runtime contracts.

## Phase 11: Reward, Experience, and Replay Dataset Contract ⏳
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Define authoritative reward/outcome attribution primitives instead of relying on caller-local `record_experience()` usage
- [x] Derive replay training rows from action outcomes, encounter transitions, and telemetry windows with stable semantics
- [x] Added reward-aware dataset export in `apps/pod-headless` via `--dataset-output`, emitting replay-derived training rows enriched with runtime profile metadata and authoritative reward reasons
- [x] Add deterministic tests for reward attribution, sample derivation, and terminal-state handling

**Next Action**:
- Thread reward/evaluation outputs into the broader remote topology and parity harnesses now that admission-aware team/world identity, quest-line state, and evaluation summaries survive beyond the local runner.

**Discovered Follow-up**:
- The long-term proving ground should be headless multi-world team orchestration, not more browser-first scaffolding.
- The first-pass topology contracts for that direction now live in [crates/pod-core/src/contract.rs](/Users/home/Desktop/prompt-or-die/crates/pod-core/src/contract.rs) and are documented in [docs/multi-world-agent-topology.md](/Users/home/Desktop/prompt-or-die/docs/multi-world-agent-topology.md).

## Phase 14: Multi-World Teams and Reality Links ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Added first-pass topology contracts in `pod-core` for `AgentTeamDefinition`, `WorldRealityDefinition`, `CrossWorldLinkDefinition`, and `WorldTournamentDefinition`
- [x] Documented the intended Deadman-style / neural-swarm architecture in `docs/multi-world-agent-topology.md`
- [x] Added `apps/pod-headless`, a deterministic multi-world runner that executes the built-in `deadman-neural-cup` scenario, emits a JSON report, and projects cross-world effects from authoritative reward telemetry
- [x] Added deterministic team admission in `apps/pod-headless`, so dataset rows and standings now carry admitted team identity per world instead of world-level aggregates only
- [x] Added applied target-world state aggregation in `apps/pod-headless`, so cross-world effects are rolled into per-world team/resource/faction/objective state summaries instead of only link-local projections
- [x] Added canonical quest graph definitions plus per-world quest-line state reporting in `apps/pod-headless`, so alternate-reality `ObjectiveStateShift` effects now resolve into explicit quest progression with start/current/completed/pending stages
- [x] Added shared `RemoteTopologyBundle` contracts in `pod-core` for world quest bindings, applied world state, and scenario evaluation, so remote surfaces have one portable topology artifact instead of app-local JSON shapes
- [x] Extended `apps/pod-headless` with `--topology-output`, so the headless runner can emit that portable remote-topology artifact alongside the existing report and dataset outputs
- [x] Consume the shared remote topology artifact in `pod-net` / `pod-stdb`, including `pod-stdb` cache resolution helpers plus `pod-net::client_stdb` snapshot metadata refresh on `RemoteTopologyUpdated`
- [x] Added replay/evaluation coverage for linked-world tournaments and neural swarms across both `apps/pod-headless` and the public `pod-net::SpacetimeDBClient` topology surface
- [x] Added a TOON-document ingest path for `RemoteTopologyBundle` in `pod-stdb`, preserving the source document via `StdbEvent::RemoteTopologyDocumentReceived` and decoding the authority-style `remote_topology_bundle` payload into the existing cache resolution path
- [x] Added `pod-net::SpacetimeDBClient::apply_remote_topology_document(...)`, forwarded remote topology source documents through `ServerMessage::DebugDocument`, and covered the document-fed path with deterministic unit/integration tests
- [x] Generalized the authority-document ingress in `pod-stdb` with `receive_debug_document(...)`, so topology, tool-call, rollup, focused-summary, and versioned telemetry TOON documents now share one dispatch path instead of growing more topology-only hooks
- [x] Added `pod-net::SpacetimeDBClient::apply_debug_document(...)` and moved the remote-topology document coverage onto that generic path, keeping `apply_remote_topology_document(...)` as a thin compatibility alias
- [x] Added a real `remote_topology_document` public row plus `publish_remote_topology_document` reducer in `pod-stdb`, so authority tooling has an actual SpacetimeDB publication surface for `RemoteTopologyBundle`
- [x] Extended `pod-stdb` and `pod-net::SpacetimeDBClient` with row-based `receive_remote_topology_document_row(...)` ingestion, stale-row protection, and subscription query coverage so remote topology can now arrive as an authority-published feed row instead of only as direct document injection
- [x] Added `GeneratedRuntimeEvent` plus `GeneratedRuntimeAdapter` to `pod-stdb::StdbClient`, so generated mode can connect, subscribe, and consume authority-fed `remote_topology_document` rows through `frame_tick()` instead of hard-failing immediately
- [x] Added generated-mode coverage in both `pod-stdb` and `pod-net::SpacetimeDBClient`, proving runtime-fed topology rows update resolved world/evaluation state and forward the source document through the existing debug stream
- [x] Added `pod-net` networking integration coverage for authority-fed topology churn and world switching, proving a newer `remote_topology_document` row rebuilds snapshot metadata while a stale older row cannot roll the active world/evaluation state back
- [x] Added `topology_parity` plus `world_quest_bindings` to the `pod-headless` report, so the headless evaluation surface now verifies that the exported `RemoteTopologyBundle` exactly matches the applied world state and evaluation data it publishes
- [x] Promoted the `pod-headless` topology parity surface into the moat benchmark path: `scripts/run_moat_benchmarks.ts` now runs `pod-headless`, records `headlessTopology`, fails on parity drift, and `scripts/publish_moat_snapshots.ts` now preserves the same headless topology data in committed shard-target snapshots
- [x] Extended authority-fed churn coverage beyond world switching: `pod-stdb` and `pod-net` now both prove a newer `remote_topology_document` row can update quest bindings, applied world state, and evaluation within the same resolved world while a stale older row cannot roll those quest/effect updates back
- [x] Replaced the generated-mode ad hoc test runtimes with `GeneratedRuntimeBridge` plus `GeneratedRuntimeHandle`, so `pod-stdb` and `pod-net` now share one reusable generated callback/event queue instead of duplicating fake runtime implementations in each test module
- [x] Added generated-path same-world quest/effect churn coverage in both `pod-stdb` and `pod-net`, proving newer generated-mode topology rows update quest bindings, applied world state, evaluation, and snapshot metadata while stale older rows are ignored

**Next Action**:
- Wire real generated SpacetimeDB binding callbacks into `GeneratedRuntimeBridge`, then thread that live generated row feed into the parity/evaluation harnesses.
