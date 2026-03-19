# Active Implementation Phases

**Project**: Prompt or Die platform rebuild from first principles
**Purpose**: Active unchecked execution plan derived from the current codebase shape, not from the historical iteration order
**Historical log**: `IMPLEMENTATION_PLAN.md` remains the archival completion record and should not be reset in place
**Planning basis**: workspace crate/app boundaries, `README.md`, `docs/architecture.md`, and `docs/plugin-model.md`
**Execution model**: re-verify from the bottom of the stack upward; only check a box after rerunning the listed validation on the live repo

## Reset rules

- Treat every checkbox below as unchecked until its validation block has been rerun.
- Preserve `IMPLEMENTATION_PLAN.md` as the historical log; use this file as the active execution checklist.
- Prefer dependency order over recency order: kernel before clients, runtime before tooling, tooling before polish.
- If a phase proves too large, split it again rather than carrying a phase that needs broad workspace context to finish.

---

## Phase 1: Deterministic Gameplay Kernel
**Type**: Runtime Foundation
**Status**: Completed (Re-verified 2026-03-18)
**Estimated**: 4-6 focused hours
**Primary surfaces**: `crates/pod-core`, `crates/pod-physics`, `crates/pod-spatial`

**Tasks**:
- [ ] Re-validate the canonical `Observe -> Decide -> Validate -> Execute -> Broadcast` pipeline in `pod-core`
- [ ] Re-validate authoritative world bootstrap, replay, telemetry, and acceptance harness surfaces
- [ ] Re-validate deterministic physics and spatial-query integration against the core world model
- [ ] Re-confirm that world/app/plugin primitives in `pod-core::app` are coherent enough to serve as the eventual lifecycle foundation

**Verification Criteria**:
- [x] `cargo test -p pod-core -- --nocapture`
- [x] `cargo test -p pod-physics -- --nocapture`
- [x] `cargo test -p pod-spatial -- --nocapture`
- [x] `cargo check -p pod-core -p pod-physics -p pod-spatial`

**Exit Criteria**:
- [ ] The deterministic kernel is proven without depending on browser, editor, or network surfaces
- [ ] Replay, telemetry, and acceptance signals are trustworthy enough to validate higher layers
- [ ] Core runtime contracts are stable enough to drive every later phase

---

## Phase 2: Agent Execution Stack
**Type**: Agent Runtime
**Status**: Completed (Re-verified 2026-03-18)
**Estimated**: 4-6 focused hours
**Primary surfaces**: `crates/pod-agents`, `crates/pod-scripting`, `docs/agent-integration-contract.md`, `docs/agent-runtime-audit.md`

**Tasks**:
- [ ] Re-validate the shared `Agent` contract and runtime-profile expectations across scripted, LLM, neural, and hybrid controllers
- [ ] Re-validate neural schema, model metadata, and inference fallback behavior
- [ ] Re-validate tool-call, action-parse, and observation-budget behavior for asynchronous agent controllers
- [ ] Re-validate the scripting sandbox as part of the same gameplay contract, not as a parallel privilege path
- [ ] Re-confirm the local parity/evaluation harnesses that measure controller behavior outside full app boots

**Verification Criteria**:
- [x] `cargo test -p pod-agents -- --nocapture`
- [x] `cargo test -p pod-scripting -- --nocapture`
- [x] `cargo run -q -p pod-agents --example controller_parity_benchmark -- --fail-on-checks`
- [x] `cargo check -p pod-agents -p pod-scripting`

**Exit Criteria**:
- [ ] Every supported controller type still enters the runtime through one shared contract
- [ ] Neural compatibility is explicit and testable instead of inferred from implementation details
- [ ] Agent-quality measurement exists before remote topology and client consumers are revisited

---

## Phase 3: Authority Runtime, Networking, and Persistence
**Type**: Runtime Integration
**Status**: Completed (Re-verified 2026-03-18)
**Estimated**: 5-7 focused hours
**Primary surfaces**: `crates/pod-host`, `crates/pod-net`, `crates/pod-stdb`, `apps/pod-server`

**Tasks**:
- [ ] Re-validate transport-neutral authority host composition for both local and direct-connect execution
- [ ] Re-validate client/server snapshot, recovery, and reconnect behavior in `pod-net`
- [ ] Re-validate SpacetimeDB client/module boundaries and the remote-agent observation/action envelope
- [ ] Re-validate retained ops history, archive-query, relay, and HTTP/SSE ops surfaces in `pod-host`
- [ ] Re-validate the dedicated-server entry point as a thin composition root over exported runtime seams

**Verification Criteria**:
- [x] `cargo test -p pod-host -- --nocapture`
- [x] `cargo test -p pod-net broadcast_updates -- --nocapture`
- [x] `cargo test -p pod-stdb --no-default-features --features client`
- [x] `cargo test -p pod-server --bin pod-server -- --nocapture`
- [x] `cargo check -p pod-host -p pod-net -p pod-stdb -p pod-server`

**Exit Criteria**:
- [ ] Single-host, multi-shard, direct-connect, and SpacetimeDB-backed execution paths are all proven from the same authority contracts
- [ ] External ops/inspection surfaces are real platform seams, not app-local utilities
- [ ] Server startup does not re-own runtime concerns already expressed in the crates

---

## Phase 4: Multi-World and Remote Topology
**Type**: Runtime Integration
**Status**: Completed (Re-verified 2026-03-18)
**Estimated**: 4-6 focused hours
**Primary surfaces**: `crates/pod-core`, `apps/pod-headless`, `crates/pod-net`, `crates/pod-stdb`

**Tasks**:
- [ ] Re-validate teams, worlds, tournaments, and cross-world link contracts in `pod-core`
- [ ] Re-validate the headless scenario runner, dataset export, and topology export as first-class proving grounds
- [ ] Re-validate remote-topology ingestion and parity measurement across both local and generated/live SpacetimeDB paths
- [ ] Re-confirm that remote autonomous agents honor the same gameplay constraints as local controllers

**Verification Criteria**:
- [x] `cargo test -p pod-core contract -- --nocapture`
- [x] `cargo test -p pod-headless -- --nocapture`
- [x] `cargo run -p pod-headless -- --profile ci-smoke --topology-output /tmp/pod-headless-topology.json`
- [x] `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
- [x] `cargo run -q -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input /tmp/pod-headless-topology.json --fail-on-checks`

**Exit Criteria**:
- [ ] Multi-world orchestration is proven as a core runtime capability rather than a headless-only experiment
- [ ] Remote topology artifacts are portable, replayable, and parity-checked
- [ ] The platform’s MMO-specific differentiators are validated before client/tooling polish resumes

---

## Phase 5: Scene, Asset, and Content Pipeline
**Type**: Authoring Foundation
**Status**: Completed (Re-verified 2026-03-18)
**Estimated**: 4-6 focused hours
**Primary surfaces**: `crates/pod-scene`, `crates/pod-assets`, supporting scripts/docs

**Tasks**:
- [ ] Re-validate native scene/prefab bindings, override/provenance tracking, streaming, and save/load seams
- [ ] Re-validate staged import, runtime bundle materialization, and importer boundaries in `pod-assets`
- [ ] Re-validate creator-facing bundle/manifest contracts independently of any single consuming app
- [ ] Re-confirm that asset optimization, bundle validation, and import failure handling are explicit platform behavior

**Verification Criteria**:
- [x] `cargo test -p pod-scene -- --nocapture`
- [x] `cargo test -p pod-assets -- --nocapture`
- [x] `cargo check -p pod-scene -p pod-assets`
- [x] `cargo run -q -p pod-assets --example stage_import -- --json --output-root /tmp/pod-stage-import apps/pod-web/artifacts/source-assets/meshes/adventurer-avatar.glb`

**Exit Criteria**:
- [ ] Authoring data has a stable path into runtime-native contracts
- [ ] Asset staging and runtime materialization are shared platform workflows instead of app-local conventions
- [ ] Scene and asset boundaries are good enough to support both editor and client phases

---

## Phase 6: Client Runtime Consumers
**Type**: Runtime Consumer
**Status**: Completed (Re-verified 2026-03-18)
**Estimated**: 4-6 focused hours
**Primary surfaces**: `crates/pod-render`, `apps/pod-web`, `apps/pod-desktop`

**Tasks**:
- [ ] Re-validate render extraction and representation contracts in `pod-render`
- [ ] Re-validate the browser runtime as a consumer of shared render, transport, and asset contracts
- [ ] Re-validate browser asset verification, smoke, and route-measurement gates
- [ ] Re-validate desktop/native runtime composition as a consumer instead of a special-case path

**Verification Criteria**:
- [x] `cargo check -p pod-render -p pod-desktop`
- [x] `cd apps/pod-web && bun test`
- [x] `cd apps/pod-web && bun run build`
- [x] `cd apps/pod-web && bun run verify:assets`
- [x] `cd apps/pod-web && bun run measure:render-routes:check`
- [x] `cd apps/pod-web && bun run test:smoke`

**Exit Criteria**:
- [ ] Browser and native clients are clearly downstream of shared runtime contracts
- [ ] Performance/asset/debug expectations are enforced by repeatable gates
- [ ] Client surfaces no longer distort the order of the core platform roadmap

---

## Phase 7: Editor and Authoring Tooling
**Type**: Tooling
**Status**: Completed (Re-verified 2026-03-18)
**Estimated**: 3-5 focused hours
**Primary surfaces**: `crates/pod-editor`, `crates/pod-scene`, `crates/pod-assets`

**Tasks**:
- [ ] Re-validate the editor shell, hierarchy, inspector, console, asset browser, and telemetry/dashboard panes against current runtime contracts
- [ ] Re-confirm where the editor still depends on closed enums or hardcoded panel dispatch
- [ ] Re-validate editor-facing scene, asset, and ops inspection workflows as consumers of shared crate seams
- [ ] Isolate the exact extension hooks that must exist before editor pluginization is credible

**Verification Criteria**:
- [x] `cargo test -p pod-editor -- --nocapture`
- [x] `cargo check -p pod-editor`
- [x] `cargo check -p pod-editor -p pod-scene -p pod-assets`

**Exit Criteria**:
- [ ] The editor is grounded on real platform seams instead of private app glue
- [ ] The next editor/plugin refactors are identified as explicit lifecycle gaps
- [ ] Tooling no longer hides architectural debt behind “it works locally”

---

## Phase 8: Lifecycle, SDK, and Shipping Stabilization
**Type**: Platform Hardening
**Status**: Completed (Re-verified 2026-03-18)
**Estimated**: 5-7 focused hours
**Primary surfaces**: `docs/architecture.md`, `docs/plugin-model.md`, CI/scripts, app roots, exported crate APIs

**Tasks**:
- [x] Reconcile the active crate seams with a formal plugin/app lifecycle plan
- [x] Re-validate benchmark/report gates and decide which ones are platform requirements versus local tooling
- [x] Re-validate the public contract/export surface that future integrations should depend on
- [x] Decide what shipping profiles, authz policy sources, and SDK boundaries are truly production requirements versus local/dev scaffolding

**Verification Criteria**:
- [x] `cargo check --workspace`
- [x] `cargo test --workspace`
- [x] `git diff --check`
- [x] Planning docs agree on the same active route and the same archival/history split

**Exit Criteria**:
- [x] POD has one active execution checklist and one preserved historical log
- [x] Missing plugin/app lifecycle hooks are prioritized explicitly instead of being buried inside iteration history
- [x] The repo can move from re-verification into focused new platform work without roadmap ambiguity

---

## Execution Rules

- Work from Phase 1 upward unless a blocking defect forces a surgical detour.
- Do not mark a box complete because the code “looks done”; rerun the validation first.
- Keep `SESSION.md` pointed at the currently active phase and the exact next verification command.
- If a verification surface is missing or too expensive, write that down explicitly before changing scope.
