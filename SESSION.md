# Session State

**Current Phase**: Phase 8: Lifecycle, SDK, and Shipping Stabilization
**Current Stage**: Rust SDK Boundary Hardening
**Last Checkpoint**: 2026-03-19 (Rust SDK state/action adapter pass)
**Planning Docs**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md), [IMPLEMENTATION_PLAN.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PLAN.md), [progress.md](/Users/home/Desktop/prompt-or-die/progress.md), [docs/platform-stabilization.md](/Users/home/Desktop/prompt-or-die/docs/platform-stabilization.md)
**Planning Note**: `IMPLEMENTATION_PHASES.md` is reset to an active unchecked checklist for this pass. `IMPLEMENTATION_PLAN.md` remains the historical log.

---

## Active Re-verification Track 🔄

- [x] Phase 1: Deterministic Gameplay Kernel (completed in this pass). Commands: `cargo test -p pod-core -- --nocapture`, `cargo test -p pod-physics -- --nocapture`, `cargo test -p pod-spatial -- --nocapture`, `cargo check -p pod-core -p pod-physics -p pod-spatial`.
- [x] Phase 2: Agent Execution Stack (completed in this pass). Commands: `cargo test -p pod-agents -- --nocapture`, `cargo test -p pod-scripting -- --nocapture`, `cargo run -q -p pod-agents --example controller_parity_benchmark -- --fail-on-checks`, `cargo check -p pod-agents -p pod-scripting`.
- [x] Phase 3: Authority Runtime, Networking, and Persistence (completed in this pass). Commands: `cargo test -p pod-host -- --nocapture`, `cargo test -p pod-net broadcast_updates -- --nocapture`, `cargo test -p pod-stdb --no-default-features --features client`, `cargo test -p pod-server --bin pod-server -- --nocapture`, `cargo check -p pod-host -p pod-net -p pod-stdb -p pod-server`.
- [x] Phase 4: Multi-World and Remote Topology (completed in this pass). Commands: `cargo test -p pod-core contract -- --nocapture`, `cargo test -p pod-headless -- --nocapture`, `cargo run -p pod-headless -- --profile ci-smoke --topology-output /tmp/pod-headless-topology.json`, `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`, `cargo run -q -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input /tmp/pod-headless-topology.json --fail-on-checks`.
- [x] Phase 5: Scene, Asset, and Content Pipeline (completed in this pass). Commands: `cargo test -p pod-scene -- --nocapture`, `cargo test -p pod-assets -- --nocapture`, `cargo check -p pod-scene -p pod-assets`, `cargo run -q -p pod-assets --example stage_import -- --json --output-root /tmp/pod-stage-import apps/pod-web/artifacts/source-assets/meshes/adventurer-avatar.glb`.
- [x] Phase 6: Client Runtime Consumers (completed in this pass). Commands: `cargo check -p pod-render -p pod-desktop`, `cd apps/pod-web && bun test`, `cd apps/pod-web && bun run build`, `cd apps/pod-web && bun run verify:assets`, `cd apps/pod-web && bun run measure:render-routes:check`, `cd apps/pod-web && bun run test:smoke`.
- [x] Phase 7: Editor and Authoring Tooling (completed in this pass). Commands: `cargo test -p pod-editor -- --nocapture`, `cargo check -p pod-editor`, `cargo check -p pod-editor -p pod-scene -p pod-assets`.
- [x] Phase 8: Lifecycle, SDK, and Shipping Stabilization (completed in this pass). Commands: `cargo check --workspace`, `cargo test --workspace`, `git diff --check`.

**Next Action**: Build `rs_rollout_recorder` and `rs_benchmark_runner` on top of `RustSdkStateSnapshot`, `RustSdkActionPlan`, and `RustSdkAdapterHost`, starting with deterministic replay capture from applied state snapshots plus action-plan traces.

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

**Next Action**: Historical follow-on resolved in Phase 5.

**Key Files**:
- [crates/pod-assets/src/lib.rs](/Users/home/Desktop/prompt-or-die/crates/pod-assets/src/lib.rs)
- [crates/pod-assets/examples/stage_import.rs](/Users/home/Desktop/prompt-or-die/crates/pod-assets/examples/stage_import.rs)
- [apps/pod-web/scripts/sync-assets.mjs](/Users/home/Desktop/prompt-or-die/apps/pod-web/scripts/sync-assets.mjs)
- [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Known Issues**:
- No open issues in this phase. The worker/main-thread warmup follow-on was completed in Phase 5.
- Sample geometry generation intentionally remains in `apps/pod-web/scripts/sync-assets.mjs`; any future changes here should be driven by measured runtime regressions, not pipeline churn.

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
- The original worker/main-thread measurement gap and the later browser gate regression are both now closed; `bun run measure:render-routes:check` and `bun run test:smoke` are green on the shipped asset set.

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
- The original historical-snapshot gap is closed by `/Users/home/Desktop/prompt-or-die/docs/benchmark-snapshots/2026-W11-shard-target.json`.
- The live shard-target capture/publication path is now wrapped by `bun ./scripts/run_shard_target_snapshot.ts --label YYYY-Www`, and `bun ./scripts/compare_moat_snapshots.ts --baseline ... --candidate ...` now turns published snapshot history into structured drift data.

## Phase 7: Plugin and Runtime Boundary Hardening ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Expanded [docs/plugin-model.md](/Users/home/Desktop/prompt-or-die/docs/plugin-model.md) into an explicit seam map that names the current contract surfaces integrators should depend on now
- [x] Updated [docs/architecture.md](/Users/home/Desktop/prompt-or-die/docs/architecture.md) to distinguish exported crate seams from app composition roots like [`main.ts`](/Users/home/Desktop/prompt-or-die/apps/pod-web/src/main.ts)
- [x] Revalidated documented seams at the crate boundary across `pod-scene`, `pod-assets`, `pod-net`, and `pod-web`
- [x] Identified the remaining missing lifecycle/registration hooks that still force integrators into app bootstrap code

**Next Action**:
- Historical follow-on closed by the first committed shard-target weekly snapshot. Any further work here belongs to snapshot comparison tooling, not snapshot existence.

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
- The committed shard-target snapshot now exists. The remaining missed item is repair work for the currently failing browser render-route perf gate so the recorded artifact and the enforced gate converge again.

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
- Historical follow-on resolved in Phase 11.

**Known Issues**:
- The neural path is functional, but still mostly a thin inference scaffold compared with the LLM and hybrid agents.
- Reward attribution and dataset export are now first-class runtime contracts. The remaining open gap is the Phase 13 remote-agent gameplay contract tracked below.

## Phase 11: Reward, Experience, and Replay Dataset Contract ✅
**Spec**: [IMPLEMENTATION_PHASES.md](/Users/home/Desktop/prompt-or-die/IMPLEMENTATION_PHASES.md)

**Progress**:
- [x] Define authoritative reward/outcome attribution primitives instead of relying on caller-local `record_experience()` usage
- [x] Derive replay training rows from action outcomes, encounter transitions, and telemetry windows with stable semantics
- [x] Added reward-aware dataset export in `apps/pod-headless` via `--dataset-output`, emitting replay-derived training rows enriched with runtime profile metadata and authoritative reward reasons
- [x] Add deterministic tests for reward attribution, sample derivation, and terminal-state handling

**Next Action**:
- Historical follow-on closed in Phases 12-14. Remaining work is the remote-agent gameplay contract plus browser/benchmark follow-ons tracked separately below.

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
- [x] Moved world admission/team-slot assignment into `pod-core` via `assign_roster_to_world_teams(...)`, `build_world_admission_summary(...)`, and `RemoteTopologyBundle.world_admissions`, so `pod-headless`, `pod-stdb`, and `pod-net` now share one admission contract instead of carrying app-local roster logic
- [x] Moved per-world admitted roster/controller composition into `pod-core` via `build_world_control_plane_summary(...)` and `RemoteTopologyBundle.world_control_planes`, so `pod-headless`, `pod-stdb`, and `pod-net` now share one control-plane contract instead of recomputing controller mix per app/runtime surface
- [x] Moved tournament standings/control-plane aggregation into `pod-core` via `build_tournament_control_plane_summary(...)` and `TournamentControlPlaneSummary`, so `pod-headless` no longer owns that summary logic privately
- [x] Carried `TournamentControlPlaneSummary` through `RemoteTopologyBundle`, `RemoteTopologyParitySummary`, `pod-stdb`, `pod-net`, and the topology-feed benchmark path, so tournament standings/control-plane drift now rides the same remote parity surface as admissions, quest bindings, applied world state, and evaluation
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
- [x] Removed the last leftover `FakeGeneratedRuntime` helper from `pod-stdb` unit tests, so the in-tree generated-mode coverage now consistently exercises `GeneratedRuntimeBridge` / `GeneratedRuntimeHandle`
- [x] Added `pod-net` topology feed measurements plus the `topology_feed_benchmark_suite` example, so exported `RemoteTopologyBundle` artifacts can now be replayed through both direct authority-row ingestion and generated-bridge ingestion and checked for per-world quest/effect/evaluation parity outside unit tests
- [x] Integrated `topology_feed_benchmark_suite` into `scripts/run_moat_benchmarks.ts`, so the combined moat artifact now emits `topologyFeedMeasurements`, fails on remote topology feed parity drift, and `scripts/publish_moat_snapshots.ts` now preserves the same topology feed benchmark in committed shard-target snapshots
- [x] Added `GeneratedBindingCallbacks`, `GeneratedRemoteTopologyDocumentRow`, `GeneratedRuntimeTrace`, and `build_generated_runtime_callback_bridge(...)`, so generated-mode topology ingestion now has one typed callback surface instead of bespoke per-test bridge wiring
- [x] Replaced the last ad hoc generated-topology bridge wiring in `pod-stdb` integration coverage and `pod-net` topology-feed measurements with the shared callback bridge plus typed row inserts
- [x] Added generated-path linked-world quest/effect churn coverage in both `pod-stdb` and `pod-net`, proving newer generated-mode topology rows update linked-world quest bindings, applied state, evaluation, and snapshot metadata while stale older rows are ignored
- [x] Added `GeneratedBindingCommand`, `GeneratedBindingRuntime`, and `GeneratedBindingEndpoint`, so generated mode now has a command-driven runtime seam that records outbound connect/subscribe/disconnect requests and accepts inbound callbacks separately instead of auto-acking connect/subscription hooks
- [x] Moved the public generated-mode integration path in `pod-stdb` and the moat/public generated path in `pod-net` onto that command-driven runtime, including explicit connect/subscription command assertions before topology-row callbacks are delivered
- [x] Added `install_generated_binding_runtime(...)` to `StdbClient` and `pod-net::SpacetimeDBClient`, so generated-mode consumers install the command-driven runtime through one public handoff instead of re-creating `GeneratedBindingRuntime::new()` boilerplate at each call site
- [x] Moved `world_quest_bindings` + `topology_parity` ownership out of `pod-headless` private report code and into `pod-core`, which now exports `build_world_quest_bindings(...)`, `RemoteTopologyParitySummary`, and `build_remote_topology_parity_summary(...)` for app, benchmark, and remote runtime reuse
- [x] Moved `build_remote_topology_bundle(...)` out of `pod-headless` and into `pod-core`, so bundle assembly now lives with the shared topology contract instead of the app binary
- [x] Installed real generated Rust bindings for `pod-stdb`, upgraded the repo to Rust `1.93.0`, and added `GeneratedSdkRuntime`, so generated mode can now use the actual generated `DbConnection`, typed topology table callbacks, and real subscription lifecycle instead of only the synthetic command-driven seam
- [x] Added `install_generated_sdk_runtime(...)` to `StdbClient` and `pod-net::SpacetimeDBClient`, plus closed-port regression tests proving the live generated SDK path is reachable and maps failures through the public error surface
- [x] Added `TopologyFeedMeasurementsOptions`, `TopologyFeedGeneratedRuntimeMode`, and `LiveGeneratedSdkTopologyFeedConfig`, so `pod-net` topology parity can now opt into the live generated SDK path without changing the deterministic moat default
- [x] Extended `topology_feed_benchmark_suite` with `--generated-sdk-host`, `--generated-sdk-auth-token`, and `--generated-sdk-timeout-ms`, plus deterministic tests that cover flag parsing and closed-port live SDK failure propagation
- [x] Exported a fresh `pod-headless` topology artifact, started a local in-memory SpacetimeDB on `127.0.0.1:3100`, published `pod_stdb.wasm` to `deadman-prime`, `deadman-shadow`, and `sanctuary-echo`, and ran the live generated SDK topology benchmark successfully
- [x] Wrote the first live parity artifact to `artifacts/topology-feed-live-local.json`; all `30/30` checks passed across the three benchmark worlds
- [x] Refreshed the shard-target transport benchmark baselines in `crates/pod-net/src/server.rs` to the current deterministic byte envelopes, with `steady-delta total/max = 1392/174` and aggregate full/recovery/delta totals of `1220/234/1904`
- [x] Extended `scripts/publish_moat_snapshots.ts` and `scripts/publish_moat_snapshots.test.ts` so shard-target weekly snapshots can merge a moat report with separately generated browser render-route and live topology-feed artifacts
- [x] Captured the first live shard-target topology artifact at `artifacts/topology-feed-live-shard-local.json` and published the first committed weekly shard-target snapshot at `docs/benchmark-snapshots/2026-W11-shard-target.json`
- [x] Extracted `WorldBootstrapPlan`, `TransportPolicy`, and `ServerConfig::network_server_config()` in `apps/pod-server/src/main.rs`, so dedicated-server world/bootstrap and direct-connect transport composition now run through typed app-local contracts instead of inline literals in the binary entry point
- [x] Added deterministic `pod-server` runtime coverage for bootstrap population and transport-policy composition, and updated `docs/plugin-model.md` plus `docs/architecture.md` so the lifecycle docs reflect the new `pod-server` seam accurately
- [x] Added `apps/pod-server/src/lib.rs` and moved the dedicated-server lifecycle seam onto that exported crate surface, so `ServerConfig`, `TransportPolicy`, `WorldBootstrapPlan`, `parse_bind_target(...)`, and `build_authoritative_world(...)` are now reusable outside the binary entry point
- [x] Simplified `apps/pod-server/src/main.rs` to consume the exported library contract and moved the seam tests onto the `pod-server` library target, keeping the binary focused on startup/shutdown plus runtime loop composition
- [x] Added `crates/pod-net/src/authority.rs` and moved the authority-host lifecycle contract into the shared networking/runtime crate, then updated `crates/pod-net/src/lib.rs` to re-export `AuthorityRuntimeConfig`, `TransportPolicy`, `WorldBootstrapPlan`, `parse_bind_target(...)`, and `build_authoritative_world(...)`
- [x] Updated `apps/pod-server/src/main.rs` to consume the shared `pod-net` authority surface directly, reduced `apps/pod-server/src/lib.rs` to a compatibility re-export, and moved the seam tests into `pod-net`
- [x] Added `crates/pod-core/src/authority.rs` and moved the transport-neutral world/bootstrap half of the authority lifecycle contract into the core runtime as `AuthorityWorldConfig`, `WorldBootstrapPlan`, and `build_authoritative_world(...)`
- [x] Reduced `crates/pod-net/src/authority.rs` to the transport adapter half of the contract, updated `apps/pod-server` to build worlds from `config.world`, and refreshed the docs to point at the split `pod-core` + `pod-net` authority lifecycle surface

**Next Action**:
- Add retained supervisor-level ops aggregation above `AuthorityShardOpsHandle` and `ShardSupervisorOpsHandle` so multi-shard browser/editor/ops consumers can attach late and still inspect recent MMO history instead of only the live broadcast.

## Iteration 207 Progress

- [x] Narrowed `crates/pod-net/src/authority.rs` to the direct-connect transport adapter only, renaming the transport config to `DirectConnectTransportConfig` and keeping just bind/websocket/client/policy composition plus `server_config(tick_rate)`.
- [x] Added `crates/pod-host/src/lib.rs` as the neutral authority host lifecycle crate with `AuthorityHostConfig`, `AuthorityTransportMode`, `AuthorityHostRuntime`, and `DirectConnectAuthorityRuntime`, so apps now get one reusable surface that composes `pod-core` world bootstrap with the selected transport.
- [x] Updated `apps/pod-server` to consume `pod-host`, kept the binary focused on process startup plus local-loop stats, and revalidated the host/transport/server seams with deterministic crate-level coverage.

## Iteration 208 Progress

- [x] Added `AuthorityShardConfig`, `AuthorityShardSummary`, `ShardSupervisorConfig`, `ShardSupervisorSummary`, `PreparedAuthorityShard`, and `PreparedShardSupervisor` in `crates/pod-host/src/lib.rs`, so multi-shard authority topology can now be validated, summarized, and prepared from one crate-level seam.
- [x] Added `PreparedShardSupervisor::run_direct_connect_until_failure()` using a Tokio `LocalSet`, which means multiple direct-connect shard runtimes can now be launched concurrently even though the current `GameServer` stack is not `Send`.
- [x] Updated the compatibility re-exports and lifecycle docs to point at the new supervisor surface, moving the next MMO blocker up to shared shard-health/control-plane aggregation instead of basic multi-shard launch configuration.

## Iteration 209 Progress

- [x] Added shard-aware `GameServer::new_with_shard_id(...)` construction plus live `ShardTransportSummary` watch publication in `crates/pod-net/src/server.rs`, so direct-connect authority runtimes now publish transport state under the real shard id instead of a fixed `direct-connect` label.
- [x] Added `AuthorityShardControlPlaneHandle`, `AuthorityShardControlPlaneSummary`, `ShardSupervisorControlPlaneHandle`, and `ShardSupervisorControlPlaneSummary` in `crates/pod-host/src/lib.rs`, so supervised shard sets can snapshot aggregate live transport health without per-shard log scraping.
- [x] Updated the compatibility exports and lifecycle docs to point at the new shared control-plane surface, moving the next MMO blocker up to shared incident rollups and coordinated lifecycle commands.

## Iteration 210 Progress

- [x] Added lifecycle command/state plumbing in `crates/pod-net/src/server.rs`, so direct-connect shard runtimes can now be drained or shut down through a supervised control path instead of only by killing the process.
- [x] Added `AuthorityShardLifecycleState`, derived `ShardIncidentSummary` output, and coordinated `request_drain*` / `request_shutdown*` fan-out in `crates/pod-host/src/lib.rs`, so the shard/supervisor control-plane now exposes both health state and lifecycle control from one surface.
- [x] Updated the compatibility exports and lifecycle docs to point at the new incident-plus-lifecycle surface, moving the next MMO blocker up to shared gameplay/tick incident publication instead of raw shard command/control support.

## Iteration 211 Progress

- [x] Added `ShardGameplayIncidentTracker` in `crates/pod-core/src/ops.rs`, so tick-budget, tool-call, trajectory, and flagship MMO action counters now live on a shared runtime surface instead of `apps/pod-server` private stats.
- [x] Extended `crates/pod-net/src/server.rs` with gameplay incident watch publication and shard incident debug documents, so `GameServer` now emits transport, lifecycle, and gameplay summaries from one authority runtime path.
- [x] Updated `crates/pod-host/src/lib.rs` control-plane snapshots plus the architecture/plugin docs, moving the next MMO blocker up to durable shared ops publication instead of missing gameplay telemetry on the shard surface.

## Iteration 212 Progress

- [x] Added `LocalAuthorityRuntime`, `AuthorityShardOpsHandle`, and `ShardSupervisorOpsHandle` in `crates/pod-host/src/lib.rs`, so both local and direct-connect hosts now publish the same live TOON ops feed from one reusable crate surface.
- [x] Extended `crates/pod-net/src/server.rs` with host-facing ops-document broadcasts that stay active without debug clients, and wired `DirectConnectAuthorityRuntime` onto that path.
- [x] Removed the app-private shard ops stream from `apps/pod-server/src/main.rs`, updated the compatibility exports, and moved the next MMO blocker up to retained multi-shard ops aggregation instead of basic live publication.

## Iteration 213 Progress

- [x] Added `pod_net::OpsDocumentStream` in `crates/pod-net/src/server.rs`, so host-facing TOON ops publication now has one shared retained ring-buffer plus live subscription surface instead of a raw broadcast-only sink.
- [x] Updated `crates/pod-host/src/lib.rs` with retained `AuthorityShardOpsSnapshot` and `ShardSupervisorOpsSnapshot` views over that shared stream, so late-joining shard/supervisor consumers can inspect recent MMO ops history without app-private buffering.
- [x] Updated the compatibility exports and lifecycle docs, moving the next MMO blocker up to durable ops persistence/export beyond the current in-memory retained history surface.

## Iteration 214 Progress

- [x] Extended `pod_net::OpsDocumentStream` with an optional durable JSONL archive that flushes each TOON document and reloads the recent tail on startup, so retained ops history can survive host restarts instead of only the current process lifetime.
- [x] Added `OpsPersistenceConfig` plus `POD_OPS_ARCHIVE_DIR` wiring in `crates/pod-host/src/lib.rs`, and surfaced archive path plus persisted counts through `AuthorityShardOpsSnapshot` / `ShardSupervisorOpsSnapshot` for both local and direct-connect runtimes.
- [x] Updated the compatibility exports and lifecycle docs, moving the next MMO blocker up to a shared archive query/relay surface above the per-shard persisted files.

## Iteration 215 Progress

- [x] Added `pod_net::OpsDocumentArchiveSnapshot`, so persisted shard ops archives now have one shared typed loader instead of forcing downstream crates to parse the JSONL format directly.
- [x] Added `AuthorityShardOpsArchiveHandle` and `ShardSupervisorOpsArchiveHandle` in `crates/pod-host/src/lib.rs`, plus `ops_archive_handle()` helpers on shard/supervisor configs and live ops handles, so retained archive queries now ride one crate-level authority seam.
- [x] Updated the compatibility exports and lifecycle docs, moving the next MMO blocker up to a process-external relay/service surface above the in-process archive handles.

## Iteration 216 Progress

- [x] Added a bounded JSON-over-TCP archive query service in `crates/pod-host/src/lib.rs` through `OpsArchiveServiceConfig`, `OpsArchiveServiceRequest`, `OpsArchiveServiceResponse`, `OpsArchiveServiceClient`, and `ShardSupervisorOpsArchiveService`, so external consumers can query retained shard/supervisor ops history without running inside the authority host.
- [x] Extended the shard/supervisor archive surfaces with `archive_service(...)` / `service(...)` constructors and refreshed the `apps/pod-server/src/lib.rs` compatibility exports, so app binaries and tooling can build against one shared process-external archive-query seam.
- [x] Added deterministic Tokio coverage that persists shard ops history, serves it over the new socket path, and validates the returned supervisor snapshot end to end.

## Iteration 217 Progress

- [x] Added an authenticated live relay in `crates/pod-host/src/lib.rs` through `OpsRelayConfig`, `OpsRelayRequest`, `OpsRelayEvent`, `OpsRelayClient`, `OpsRelaySubscription`, and `ShardSupervisorOpsRelayService`, so external consumers can subscribe to an initial retained snapshot and then follow live shard ops documents over one bounded line-delimited JSON stream.
- [x] Extended the live ops surfaces with `ShardSupervisorOpsHandle::relay(...)` and `PreparedShardSupervisor::ops_relay(...)`, and refreshed the `apps/pod-server/src/lib.rs` compatibility exports, so app binaries and tooling can build against one shared authenticated relay seam.
- [x] Added deterministic Tokio coverage for both auth rejection and live document streaming over the new relay.

## Iteration 218 Progress

- [x] Added `OpsHttpServiceConfig`, `OpsHttpError`, and `ShardSupervisorOpsHttpService` in `crates/pod-host/src/lib.rs`, so shard/supervisor retained ops snapshots can now be queried over bounded HTTP JSON endpoints and followed over authenticated SSE without embedding the raw relay protocol.
- [x] Extended the live ops surfaces with `ShardSupervisorOpsHandle::http_service(...)` and `PreparedShardSupervisor::ops_http_service(...)`, and refreshed the `apps/pod-server/src/lib.rs` compatibility exports, so app binaries and tooling can compose one shared browser/editor-facing HTTP seam.
- [x] Added deterministic Tokio coverage for HTTP auth rejection, archive snapshot JSON, and retained-plus-live SSE delivery over the new facade.

## Iteration 219 Progress

- [x] Added monotonic per-shard ops document sequencing and archive-backed replay loading in `crates/pod-net/src/server.rs`, so retained shard history can now resume from cursor state.
- [x] Added shard/supervisor replay cursor and snapshot surfaces in `crates/pod-host/src/lib.rs`, covering in-process replay, archive-backed replay, and sequence-aware live relay forwarding.
- [x] Extended `ShardSupervisorOpsHttpService` with replay JSON routes and cursor-aware SSE startup, refreshed `apps/pod-server/src/lib.rs` re-exports, and proved the resume path with deterministic Tokio coverage.

## Iteration 220 Progress

- [x] Added durable replay bookmark helpers in `crates/pod-host/src/lib.rs`, so shard and supervisor replay state can now be persisted as opaque resume tokens instead of raw cursor maps.
- [x] Extended replay snapshots with `next_bookmark`, taught the HTTP replay routes to accept `bookmark=...`, and updated HTTP SSE `shard_document` payloads to carry the latest bookmark as live events advance.
- [x] Refreshed `apps/pod-server/src/lib.rs` compatibility exports and proved bookmark round-trips plus bookmark-based replay resume with deterministic Tokio coverage.

## Iteration 221 Progress

- [x] Added supervisor shard-selection replay state in `crates/pod-host/src/lib.rs`, so `ShardSupervisorOpsReplayCursor` and `ShardSupervisorOpsReplaySnapshot` now carry `selected_shard_ids` and preserve that scope through durable bookmark tokens.
- [x] Extended the supervisor HTTP replay and SSE surfaces with `shards=...`, validated requested shard ids against the live supervisor handle set, and filtered both retained replay output and live SSE subscriptions down to the selected shard subset.
- [x] Added deterministic `pod-host` coverage for legacy bookmark decoding, filtered supervisor replay over HTTP, and filtered supervisor SSE subscription wiring, moving the next MMO blocker up to authz-aware replay policy instead of manual shard selection.

## Iteration 222 Progress

- [x] Added static HTTP shard-scope authorization in `crates/pod-host/src/lib.rs` through `OpsHttpAuthorizedToken`, while keeping the existing full-access `auth_token` path intact for admin or trusted consumers.
- [x] Applied that authorization across supervisor replay/SSE and shard-specific archive/replay/stream routes, defaulting supervisor requests to the token’s allowed shard set and rejecting disallowed shard requests with explicit `403 Forbidden` responses.
- [x] Refreshed `apps/pod-server/src/lib.rs` compatibility exports and added deterministic `pod-host` coverage for scoped-token defaults plus forbidden shard rejection, moving the next MMO blocker up to a shared authz policy source instead of static per-service token maps.

## Iteration 223 Progress

- [x] Replaced the static per-service shard token map in `crates/pod-host/src/lib.rs` with `OpsHttpAuthorizationPolicy` plus `OpsHttpAuthorizationPolicySource`, so HTTP shard authorization can now come from a shared inline policy or a file-backed JSON policy document.
- [x] Tightened the authorization fallback so only the existing inline-empty configuration preserves open access; file-backed policies now require a real token match instead of implicitly granting full access when the external policy is empty.
- [x] Refreshed `apps/pod-server/src/lib.rs` compatibility exports and added deterministic `pod-host` coverage proving a live HTTP service reloads updated shard scope from the shared policy file without being rebuilt, moving the next MMO blocker up to signed or process-external authz policy distribution.

## Iteration 225 Progress

- [x] Revalidated `docs/agent-runtime-audit.md` against the current
  `pod-core`/`pod-agents` runtime surfaces, so the public audit now reflects the
  implemented reward-summary path, replay-derived training samples, and curated
  controller parity harness instead of describing those pieces as still missing.
- [x] Updated `docs/agent-integration-contract.md` so the retained
  shard-target/topology history workflow is described as already implemented via
  `run_shard_target_snapshot.ts`, `compare_moat_snapshots.ts`, and
  `docs/benchmark-snapshots/README.md`, not as an unresolved reporting gap.
- [x] Added `docs/agent-runtime-audit.md` and
  `docs/multi-world-agent-topology.md` to the root `README.md` docs index so
  the current public contract/audit set is easier to discover from the repo
  entry point.

## Iteration 226 Progress

- [x] Added `docs/README.md` as the canonical docs hub, grouping the repo docs
  by runtime, workflow, benchmark, product, and historical use case instead of
  leaving discovery to the root `README.md` alone.
- [x] Added `docs/asset-pipeline.md` so staged imports, runtime bundle specs,
  browser asset verification, and runtime selection policy now live in one
  focused deep doc instead of being buried inside the root README.
- [x] Rewrote `README.md` into a shorter task-oriented entry point that points
  readers at the right deep docs for benchmarking, asset workflows, and
  retained benchmark history.
- [x] Added consistent audience/related-docs navigation blocks across the core
  docs set so the major pages now point at one another explicitly instead of
  acting like disconnected standalone notes.

## Iteration 227 Progress

- [x] Revalidated `docs/plugin-model.md` against the current
  `pod-core::{App, Plugin, SchedulePhase}` lifecycle kernel, so the public
  plugin doc now distinguishes the existing in-process app surface from the
  still-missing repo-wide plugin/app lifecycle hooks.
- [x] Revalidated `docs/reference-bootstrap.md` against
  `scripts/bootstrap_reference_world.ts` and `apps/pod-web/src/runtime-config.ts`,
  so the reference bootstrap doc now captures the real defaults, showcase
  route, resolved preset, and measure-mode JSON behavior.
- [x] Corrected the default creator note in
  `scripts/run_moat_benchmarks.ts` so the moat benchmark guidance now matches
  the documented reference bootstrap instead of the older local-sandbox wording.

## Audit Backlog (2026-03-13)

- [x] Browser infra: repaired the render-route perf regression so `bun run measure:render-routes:check` and `bun run test:smoke` now pass again on the shipped asset set.
