# Infrastructure Implementation Phases

**Project**: Prompt or Die infrastructure hardening
**Primary surfaces**: `apps/pod-web`, `crates/pod-assets`, `crates/pod-render`, `crates/pod-net`, `crates/pod-stdb`, docs/tooling
**Planning basis**: current repo architecture, `IMPLEMENTATION_PLAN.md`, and active browser/runtime work already underway
**Execution model**: small atomic phases, each independently verifiable, each safe to resume after context loss

## Scope assumptions

- The immediate priority is infrastructure, not more showcase fiction or encounter content.
- Runtime asset delivery should converge on precompiled browser-ready formats, not direct heavy authoring-format loading in the browser.
- `apps/pod-web` is the current highest-leverage proving ground because it exercises rendering, asset ingest, transport, and tooling together.
- Native/runtime crates should only be touched when the browser-side proof surface exposes a real platform-level gap.

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
**Status**: Pending
**Estimated**: 2-4 hours
**Files**: CI config/scripts, `docs/benchmark-suite.md`, test harnesses

**Tasks**:
- [ ] Add asset-pipeline validation to standard local/CI command surfaces
- [ ] Add a cheap benchmark or threshold gate for asset load regressions
- [ ] Ensure smoke routes and binary asset sync remain part of routine validation

**Verification Criteria**:
- [ ] CI/local scripts fail on broken asset sync or materially regressed load metrics
- [ ] Benchmark guidance is documented and repeatable

**Exit Criteria**:
- Infrastructure regressions are caught by automation rather than manual spot checks

---

## Execution Rules

- Work one phase at a time, but split again if a phase exceeds 5-8 touched files or 2-4 focused hours.
- Every phase ends with a validation block copied into `SESSION.md`.
- If context runs low, resume from `SESSION.md` without needing the prior chat.
- Do not expand showcase/gameplay content unless it directly unlocks or validates infrastructure.

## Immediate Next Actions

1. Start Phase 7 by turning the current plugin/runtime seams into explicit contracts, beginning with the existing extension points already exercised by `pod-assets`, `pod-editor`, and `pod-web`.
2. Identify which bootstrap and registration paths are still “internal by convention” instead of documented/typed extension hooks.
3. Keep the current direct-connect transport tests as the regression floor while Phase 7 clarifies where integrators are allowed to attach behavior.
