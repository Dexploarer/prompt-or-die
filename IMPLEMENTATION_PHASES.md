# Agent and Runtime Implementation Phases

**Project**: Prompt or Die runtime hardening and agent intelligence
**Primary surfaces**: `crates/pod-core`, `crates/pod-agents`, `crates/pod-net`, `crates/pod-stdb`, docs/tooling, with `apps/pod-web` only as a proving ground when agent work needs it
**Planning basis**: current repo architecture, `IMPLEMENTATION_PLAN.md`, `docs/agent-integration-contract.md`, and `docs/agent-runtime-audit.md`
**Execution model**: small atomic phases, each independently verifiable, each safe to resume after context loss

## Scope assumptions

- Foundation infrastructure phases 1-8 are complete enough to stop being the active track.
- The immediate priority is the agent stack: shared runtime contracts, neural policy interfaces, replay-derived training data, evaluation, and remote execution topology.
- `apps/pod-web` remains a consumer and inspection surface, not the driver of the roadmap.
- Visual/content work is out of scope unless it directly unlocks or validates agent behavior.

---

## Phase 1: Browser Asset Fast Path
**Type**: Infrastructure
**Status**: Complete
**Estimated**: 2-3 hours
**Files**: `apps/pod-web/src/assets.ts`, `apps/pod-web/src/assets.test.ts`, `apps/pod-web/scripts/sync-assets.mjs`, `apps/pod-web/public/assets/pod-asset-manifest.json`, `apps/pod-web/vite.config.ts`, `apps/pod-web/README.md`

**Tasks**:
- [x] Promote binary `.glb` to the default shipped runtime path
- [x] Keep `.gltf` sidecars for inspection/debugging
- [x] Fix multi-mesh scene extraction so real creator assets do not silently drop geometry
- [x] Add unit coverage for `.glb` manifest paths and merged scene extraction
- [x] Rebuild shipped sample assets and validate the browser runtime against them

**Verification Criteria**:
- [x] `bun test ./src/assets.test.ts`
- [x] `bun run sync:assets`
- [x] `bun run typecheck`
- [x] `bun test`
- [x] `bun run build`
- [x] `bun run test:smoke`

**Exit Criteria**:
- Runtime manifest points at `.glb`
- Loader handles multi-mesh glTF/GLB scenes correctly
- Browser build and smoke tests pass on the shipped asset bundle

---

## Phase 2: Asset Load Telemetry and Budgets
**Type**: Infrastructure
**Status**: Complete
**Estimated**: 2-4 hours
**Files**: `apps/pod-web/src/assets.ts`, `apps/pod-web/src/assets.test.ts`, `apps/pod-web/src/renderer.ts`, `apps/pod-web/src/hud.ts`, `apps/pod-web/src/hud.test.ts`, `apps/pod-web/README.md`

**Tasks**:
- [x] Extend asset residency stats with timing counters for geometry and sprite loads
- [x] Track totals, resident/pending counts, slowest load, and average load duration without per-frame allocations
- [x] Thread those metrics into renderer stats and compact HUD formatting
- [x] Add deterministic tests for timing aggregation and HUD rendering
- [x] Document how to read the new perf counters during local profiling

**Verification Criteria**:
- [x] `bun test ./src/assets.test.ts ./src/hud.test.ts`
- [x] `bun run typecheck`
- [x] `bun test`
- [x] `bun run build`
- [x] `bun run test:smoke`

**Exit Criteria**:
- Asset loader reports more than simple counts
- Renderer/HUD expose enough information to confirm whether the fast path is actually fast
- No regression to smoke/build/test surface

---

## Phase 3: Creator Import and Precompile Lane
**Type**: Infrastructure
**Status**: Completed
**Estimated**: 3-5 hours
**Files**: `crates/pod-assets/src/lib.rs`, `crates/pod-assets/examples/stage_import.rs`, `apps/pod-web/scripts/sync-assets.mjs` or replacement CLI, `README.md`, docs/workflow notes

**Tasks**:
- [x] Define which source formats are accepted at authoring/import time inside `pod-assets`
- [x] Materialize content-addressed staged artifacts for non-scene imports instead of only indexing metadata
- [x] Preserve authored `.gltf` / `.glb` / `.jpeg` extensions in the staged-import lane so the repo stops mislabeling source artifacts as precompiled runtime outputs
- [x] Add a repo-level staged-import CLI entrypoint instead of requiring direct library wiring
- [x] Separate source assets from processed runtime assets in docs and directory conventions
- [x] Bridge staged imports into the browser/runtime manifest generation path
- [x] Move staged-to-runtime manifest assembly into `pod-assets` instead of leaving that contract inside `apps/pod-web/scripts/sync-assets.mjs`
- [x] Keep sample geometry generation app-local, but move runtime-public asset materialization into `pod-assets` so the canonical runtime write step is shared
- [x] Add explicit runtime bundle validation plus optional staged `.ktx2` sidecar support for compressed sprite variants
- [x] Add final docs/wiring so app-level manifests can project staged compressed variants into runtime loader metadata without hand-maintained duplication
- [x] Define the full canonical runtime target contract: `.glb` meshes + compressed textures

**Verification Criteria**:
- [x] `cargo test -p pod-assets`
- [x] `cargo check --workspace`
- [x] `cargo check -p pod-assets --example stage_import`
- [x] `cargo run -q -p pod-assets --example stage_import -- --json --output-root <temp-output> <temp-source> [<temp-source> ...]`
- [x] `cd apps/pod-web && bun run sync:assets`
- [x] `cd apps/pod-web && bun run build`
- [x] `cd apps/pod-web && bun run test:smoke`
- [x] Processed/runtime asset location is documented and reproducible
- [x] Failure modes are explicit for unsupported or malformed source assets and malformed runtime bundle specs

**Exit Criteria**:
- “Load random source files directly in browser” is no longer the implied workflow
- The platform has a documented import boundary and a documented runtime asset boundary

---

## Phase 4: Compression, LOD, and Shipping Discipline
**Type**: Infrastructure
**Status**: Completed
**Estimated**: 3-5 hours
**Files**: `crates/pod-assets/*`, `apps/pod-web/src/assets.ts`, asset-manifest generation, benchmark docs

**Tasks**:
- [x] Add explicit mesh LOD outputs to the shared staged-to-runtime bundle contract and sample pipeline
- [x] Add at least one real `.ktx2` authored fixture to the sample lane and thread it through the shared bundle contract
- [x] Add precompile hooks for mesh compression beyond optional authored texture sidecars
- [x] Teach the manifest to declare richer per-LOD/runtime metadata
- [x] Ensure the browser runtime chooses compressed/LOD variants deterministically
- [x] Add size and load-time comparison fixtures to prevent regressions

**Verification Criteria**:
- [x] Test fixture assets show measurable size reduction and deterministic selection
- [x] Real `.ktx2` sample fixtures are staged, materialized, and surfaced in the shipped manifest/runtime budget report
- [x] Runtime still falls back safely when optional optimized variants are absent

**Exit Criteria**:
- Asset optimization is part of the standard shipping path, not a TODO

---

## Phase 5: Render Worker and Main-Thread Relief
**Type**: Infrastructure
**Status**: Complete
**Estimated**: 3-4 hours
**Files**: `apps/pod-web/src/render-runtime.ts`, `apps/pod-web/src/renderer.ts`, `apps/pod-web/tests/*.e2e.ts`, docs

**Tasks**:
- [x] Add the first explicit worker vs main-thread warmup and frame-stability counters to the runtime stats surface
- [x] Add main-thread submission counters plus explicit worker fallback reasons so worker-mode comparisons capture actual main-thread relief
- [x] Move any avoidable main-thread post-load work off the hot path
- [x] Add explicit worker/main-thread perf counters and smoke assertions
- [x] Document known worker constraints and fallback behavior
- [x] Attribute remaining worker submission cost by command class
- [x] Trim or batch remaining non-render control traffic
- [x] Turn the measured worker-route buckets into explicit regression ceilings
- [x] Add explicit warmup/stability comparison ceilings or gates
- [x] Emit reusable browser render-route measurements through the benchmark surface

**Verification Criteria**:
- [x] Smoke tests pass on main and worker routes
- [x] Perf counters show stable worker-route frame quality and bounded submission chatter on the local-sandbox route
- [x] Browser benchmark artifacts capture main-vs-worker route measurements outside the smoke suite

**Exit Criteria**:
- Worker mode is a measured infrastructure path, not just an optional toggle

---

## Phase 6: Transport and Snapshot Performance Surface
**Type**: Infrastructure
**Status**: Complete
**Estimated**: 3-5 hours
**Files**: `crates/pod-net/*`, `crates/pod-stdb/*`, `apps/pod-web/src/direct-connect*.ts`, HUD/debug docs

**Tasks**:
- [x] Add bounded transport metrics that expose snapshot size, delta churn, recovery cost, and queue pressure
- [x] Thread those metrics into browser/editor debug surfaces without bloating the gameplay HUD
- [x] Add targeted tests for reconnect, recovery, and degraded-network behavior

**Verification Criteria**:
- [x] Transport summaries include delivery pressure, recovery cost, and queue saturation data
- [x] Browser/editor consumers can display those metrics deterministically
- [x] Reconnect, full-recovery, and backlog-saturation paths move the transport counters through deterministic tests

**Exit Criteria**:
- Multiplayer/runtime performance issues are diagnosable from first-party telemetry instead of guesswork

---

## Phase 7: Plugin and Runtime Boundary Hardening
**Type**: Infrastructure
**Status**: Complete
**Estimated**: 3-4 hours
**Files**: `docs/plugin-model.md`, `docs/architecture.md`, relevant crate scaffolds/tests

**Tasks**:
- [x] Turn current architectural seams into explicit extension contracts
- [x] Identify missing plugin/app lifecycle hooks that block integrators today
- [x] Define near-term crate-level conventions for imports, runtime registration, and extension testing

**Verification Criteria**:
- [x] Docs clearly distinguish stable seams vs draft seams vs internal seams
- [x] At least one current extension path is validated against the clarified contract

**Exit Criteria**:
- Integrators can tell where to plug in without reverse-engineering boot code

---

## Phase 8: CI and Regression Gates
**Type**: Infrastructure
**Status**: Complete
**Estimated**: 2-4 hours
**Files**: CI config/scripts, `docs/benchmark-suite.md`, test harnesses

**Tasks**:
- [x] Add asset-pipeline validation to standard local/CI command surfaces
- [x] Add a cheap benchmark or threshold gate for asset load regressions
- [x] Ensure smoke routes and binary asset sync remain part of routine validation

**Verification Criteria**:
- [x] CI/local scripts fail on broken asset sync or materially regressed load metrics
- [x] Benchmark guidance is documented and repeatable

**Exit Criteria**:
- [x] Infrastructure regressions are caught by automation rather than manual spot checks

---

## Phase 9: Agent Runtime Audit and Contract Alignment
**Type**: Agent Runtime
**Status**: Complete
**Estimated**: 1-2 hours
**Files**: `docs/agent-integration-contract.md`, `docs/agent-runtime-audit.md`, `SESSION.md`, `IMPLEMENTATION_PLAN.md`

**Tasks**:
- [x] Audit the live `Agent` trait, authoritative tick loop, and exported `pod-agents` families instead of planning against stale assumptions
- [x] Align the public integration contract with the actual runtime surface (`runtime_profile`, `drain_tool_calls`, telemetry/replay expectations)
- [x] Publish a grounded maturity assessment for human, scripted, LLM, hybrid, and neural controllers
- [x] Identify the actual weak point in the current stack: neural schema, reward, evaluation, and model lifecycle discipline

**Verification Criteria**:
- [x] `cargo check -p pod-core -p pod-agents`
- [x] `git diff --check`

**Exit Criteria**:
- Agent planning is grounded in the live code, not the old browser-infrastructure queue
- Public docs no longer describe an outdated `Agent` contract

---

## Phase 10: Neural Runtime Hardening
**Type**: Agent Runtime
**Status**: Complete
**Estimated**: 3-5 hours
**Files**: `crates/pod-agents/src/neural_agent.rs`, `crates/pod-agents/src/onnx_network.rs`, `crates/pod-agents/src/lib.rs`, `crates/pod-core/src/contract.rs`, docs/tests`

**Tasks**:
- [x] Extract the neural feature schema into an explicit versioned contract shared by encoder, ONNX loader, tests, and docs
- [x] Extract the neural action schema into an explicit registry instead of hard-coded positional assumptions buried in one file
- [x] Add model metadata and compatibility checks so mismatched feature/action layouts fail loudly
- [x] Add deterministic tests for schema count/version mismatches and inference fallback behavior
- [x] Surface neural-runtime compatibility and fallback status through introspection/telemetry

**Verification Criteria**:
- [ ] `cargo test -p pod-agents neural -- --nocapture`
- [ ] `cargo test -p pod-agents onnx -- --nocapture`
- [ ] `cargo check -p pod-core -p pod-agents`
- [ ] `git diff --check`

**Exit Criteria**:
- Neural models have a stable runtime contract instead of implicit array-shape coupling
- Schema drift is caught by tests before a model is loaded into gameplay

---

## Phase 11: Reward, Experience, and Replay Dataset Contract
**Type**: Agent Runtime
**Status**: In Progress
**Estimated**: 3-5 hours
**Files**: `crates/pod-core/src/telemetry.rs`, `crates/pod-core/src/replay.rs`, `crates/pod-core/src/tick.rs`, `crates/pod-agents/src/neural_agent.rs`, docs/tooling`

**Tasks**:
- [x] Define authoritative reward/outcome attribution primitives instead of relying on caller-local `record_experience()` usage
- [x] Derive replay training rows from action outcomes, encounter transitions, and telemetry windows with stable semantics
- [x] Add export tooling for neural datasets grounded in authoritative replay files
- [x] Add deterministic tests for reward attribution, sample derivation, and terminal-state handling

**Verification Criteria**:
- [x] `cargo test -p pod-headless`
- [x] `cargo run -p pod-headless -- --profile ci-smoke --scenario deadman-neural-cup --output /tmp/pod-headless-report.json --dataset-output /tmp/pod-headless-dataset.json`
- [x] `cargo check -p pod-headless`

**Exit Criteria**:
- Neural training data is derived from authoritative runtime truth
- Replay export is a real dataset surface, not just a debug artifact

---

## Phase 12: Agent Evaluation and Parity Harness
**Type**: Agent Runtime
**Status**: In Progress
**Estimated**: 3-6 hours
**Files**: `crates/pod-core/*`, `crates/pod-agents/*`, benchmark scripts/docs`

**Tasks**:
- [x] Add a first cheap local evaluation/report surface on top of the headless scenario runner
- [ ] Add scenario/replay-based evaluation harnesses for scripted, LLM, hybrid, and neural agents
- [ ] Publish common metrics: action validity, objective progress, encounter outcomes, latency, and tool-call reliance
- [ ] Add parity checks that compare neural behavior against deterministic baselines on curated scenarios
- [ ] Thread the results into a cheap local benchmark/report surface that can run outside direct app invocation

**Verification Criteria**:
- [ ] `cargo test -p pod-agents --lib`
- [ ] `cargo test -p pod-core parity -- --nocapture`
- [ ] Benchmark/report command documented and reproducible

**Exit Criteria**:
- Agent quality is measurable across controller types
- Neural iteration has a non-anecdotal benchmark target

---

## Phase 13: Remote Agent Topology on SpacetimeDB
**Type**: Agent Runtime
**Status**: Planned
**Estimated**: 4-6 hours
**Files**: `crates/pod-stdb/*`, `crates/pod-net/*`, `docs/agent-integration-contract.md`, remote agent tooling/tests`

**Tasks**:
- [ ] Define the observation/action envelope and budget contract for remote agents over SpacetimeDB
- [ ] Clarify admission, heartbeat, timeout, and fallback rules for remote neural/LLM agents
- [ ] Ensure transport preserves the same gameplay contract as local in-process agents
- [ ] Add degraded-network and stale-decision tests for remote autonomous agents

**Verification Criteria**:
- [ ] `cargo test -p pod-stdb -- --nocapture`
- [ ] `cargo test -p pod-net transport -- --nocapture`
- [ ] `cargo check --workspace`

**Exit Criteria**:
- Remote agent execution is an explicit supported topology, not an implied extension of direct-connect tooling

---

## Phase 14: Multi-World Teams and Reality Links
**Type**: Agent Runtime
**Status**: Complete
**Estimated**: 4-6 hours
**Files**: `crates/pod-core/src/contract.rs`, `crates/pod-core/*`, `crates/pod-net/*`, `crates/pod-stdb/*`, docs/tooling`

**Tasks**:
- [x] Define native contract types for teams, worlds, tournaments, and cross-world links
- [x] Add a headless team/world runner that can operate without the browser client
- [x] Define deterministic cross-world effect application rules at authority boundaries
- [x] Add a shared remote-topology export contract for world quest bindings, applied world state, and scenario evaluation
- [x] Consume the shared remote-topology artifact through remote execution topology
- [x] Add replay/evaluation coverage for linked-world tournaments and neural swarms

**Verification Criteria**:
- [x] `cargo test -p pod-core contract -- --nocapture`
- [x] `cargo check -p pod-core -p pod-agents`
- [x] `cargo test -p pod-headless`
- [x] `cargo run -p pod-headless -- --profile ci-smoke --scenario deadman-neural-cup --output /tmp/pod-headless-report.json`
- [x] `cargo test -p pod-stdb --no-default-features --features client`
- [x] `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
- [x] `cargo check -p pod-stdb --no-default-features --features client`
- [x] `cargo check -p pod-net --features spacetimedb`
- [x] `git diff --check`

**Exit Criteria**:
- POD supports developer-controlled teams and neural swarms as first-class runtime topology
- Linked-world tournaments are modeled as explicit engine contracts instead of app-local glue

---

## Execution Rules

- Work one phase at a time, but split again if a phase exceeds 5-8 touched files or 2-4 focused hours.
- Every phase ends with a validation block copied into `SESSION.md`.
- If context runs low, resume from `SESSION.md` without needing the prior chat.
- Do not expand showcase/gameplay content unless it directly unlocks or validates agent-runtime work.

## Immediate Next Actions

1. Publish and review shard-target `topologyFeedMeasurements` snapshots alongside the existing headless/browser/transport moat history now that the first live SDK parity artifact exists.
2. Remove any remaining helper-only generated topology wiring once the live generated runtime is exercised by the parity/evaluation path.
3. Promote the live generated SDK benchmark invocation into a reproducible local script/workflow instead of an ad hoc manual sequence.
