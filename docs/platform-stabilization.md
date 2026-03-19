# Platform Stabilization

This document defines the current Phase 8 hardening route for Prompt or Die.
It answers four repo-level questions:

1. Which planning document is active right now?
2. Which benchmark/report surfaces are real platform requirements?
3. Which exported contracts should future integrations depend on?
4. Which shipping/authz/SDK boundaries are production commitments versus local scaffolding?

> Audience: contributors deciding whether work is platform hardening, local
> tooling, or roadmap speculation.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Architecture Overview](./architecture.md) ·
> [Plugin Model](./plugin-model.md) ·
> [Benchmark Suite](./benchmark-suite.md) ·
> [Moat Gates](./moat-gates.md)

## Planning route

The repo now has one active execution checklist and one preserved historical log.

| Role | Source of truth | Rule |
| --- | --- | --- |
| Active unchecked execution checklist | `IMPLEMENTATION_PHASES.md` | This is the only planning doc that should carry live unchecked boxes for the current pass. |
| Historical implementation record | `IMPLEMENTATION_PLAN.md` | Keep completion history here; do not reset old checkboxes in place. |
| Current phase and next command | `SESSION.md` | Track the active phase, current focus, and the exact next verification command. |
| Delivery recap | `progress.md` | Append outcome-focused summaries here after implementation lands. |
| Why the split exists | `docs/implementation-reset-audit.md` | Preserve the rationale for the active-checklist vs historical-log split. |

Near-term rule:

- If a task is about "what do we do next," update `IMPLEMENTATION_PHASES.md`.
- If a task is about "what did we already do," update `IMPLEMENTATION_PLAN.md` and `progress.md`.
- If a task changes the active route or next command, update `SESSION.md` in the same change.

## Benchmark requirement tiers

Not every benchmark-shaped command is a moat gate. The repo distinguishes between
platform requirement gates and local tooling/proof surfaces.

### Platform requirement gates

These are the benchmark/report commands that protect the moat directly and can
block Phase 8/9 completion.

| Surface | Primary command | Why it is a requirement |
| --- | --- | --- |
| Core moat benchmark | `cargo run -p pod-core --example moat_benchmark_suite --release -- --profile shard-target --monthly-host-cost-usd 300 --output artifacts/moat-core.json` | Proves deterministic replay, tick stability, transparency, and cost posture. |
| Direct-connect transport benchmark | `cargo run -p pod-net --example transport_benchmark_suite -- --profile shard-target --fail-on-checks --output artifacts/transport-benchmark-shard.json` | Proves recovery, resume, queue pressure, and transport durability instead of hand-waving about scale. |
| Headless multi-world topology parity | `cargo run -p pod-headless -- --profile shard-target --output artifacts/pod-headless-shard.json --topology-output artifacts/pod-headless-topology-shard.json` | Proves the multi-world tournament/evaluation contract without depending on browser code. |
| Remote topology feed parity | `cargo run -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input artifacts/pod-headless-topology-shard.json --fail-on-checks --output artifacts/topology-feed-benchmark-shard.json` | Proves generated/live topology ingestion matches authority truth. |
| Controller parity benchmark | `cargo run -q -p pod-agents --example controller_parity_benchmark -- --fail-on-checks` | Proves scripted, LLM, hybrid, and neural controllers are measured against the same runtime contract. |
| Browser parity and asset gates | `cd apps/pod-web && bun run verify:assets` plus `cd apps/pod-web && bun run measure:render-routes:check` | Protects browser/native parity and the authored-to-runtime asset contract. |
| Combined moat workflow | `bun ./scripts/run_moat_benchmarks.ts --profile shard-target --monthly-host-cost-usd 300 --output artifacts/moat-benchmarks.json` | Rolls the required proof surfaces into one operator-facing artifact. |
| Weekly retained-history workflow | `bun ./scripts/run_shard_target_snapshot.ts --label YYYY-Www --output artifacts/moat-benchmarks-shard-local.json` | Publishes benchmark history and regression comparison as retained evidence, not local lore. |

### Local tooling and proof surfaces

These commands are still useful, but they are not themselves the moat. They
support analysis, integration proof, or operator ergonomics.

| Surface | Primary command | Why it is local/proof tooling |
| --- | --- | --- |
| TOON export proof benchmark | `bun ./scripts/benchmark_toon_exports.ts --profile extensive --output artifacts/toon-export-benchmark-extensive.json --html-output artifacts/toon-export-benchmark-extensive.html --markdown-output artifacts/toon-export-benchmark-extensive.md --charts-dir artifacts/toon-export-benchmark-extensive-charts --fail-on-checks` | Validates the LLM-facing export strategy, but it does not replace authority, parity, or transport proofs. |
| Benchmark history index rebuild | `bun ./scripts/index_benchmark_snapshots.ts` | Maintains published history navigation; it is publication tooling rather than runtime validation. |
| Reference bootstrap measure mode | `bun ./scripts/bootstrap_reference_world.ts --measure` | Useful creator-time evidence, but it is not a merge gate by itself. |
| Snapshot diff inspection | `bun ./scripts/compare_moat_snapshots.ts --baseline ... --candidate ... --output artifacts/benchmark-snapshot-comparison.json --fail-on-regressions` | Important for retained review, but it operates on already-produced benchmark artifacts. |

Merge rule:

- A feature is not "moat complete" unless it strengthens at least one platform
  requirement gate.
- Local tooling can ship when it makes required gates easier to run, inspect,
  or publish without pretending to be the moat itself.

## Public contract surfaces

Future integrations should depend on exported crate seams and contract docs,
not app roots.

| Tier | Surface | Guidance |
| --- | --- | --- |
| Stable now | `pod_core::{App, Plugin, SchedulePhase}`, `Agent` trait, action/observation flow, `pod_core::{RuntimeContractVersion, VersionedObservation, VersionedAgentAction, VersionedTickTelemetry, RustSdkHandoffArtifact, RemoteTopologyBundle}`, `pod_scene::{NativeComponentBinding, Prefab, SceneManager}`, `pod_assets::{import_asset, build_runtime_bundle_manifest, materialize_runtime_bundle_manifest}`, `pod_core::authority::{AuthorityWorldConfig, WorldBootstrapPlan, build_authoritative_world(...)}`, `pod_net::authority::{DirectConnectTransportConfig, TransportPolicy, parse_bind_target(...)}`, `pod_stdb::StdbClient::{install_generated_binding_runtime, install_generated_sdk_runtime}`, `pod_net::SpacetimeDBClient::{install_generated_binding_runtime, install_generated_sdk_runtime}`, `pod_host::{AuthorityHostConfig, OpsPersistenceConfig, AuthorityTransportMode, AuthorityHostRuntime, AuthorityShardConfig, ShardSupervisorConfig}`, `pod shell --agent`, `pod export ... --format toon` | Safe place to build integrations today. |
| Draft but intentional | Browser runtime bootstrap inputs, prefab provenance/override reporting, retained benchmark snapshot workflow, TOON world/event export conventions | Good targets for internal integrations, but expect movement. |
| Internal / not yet stabilized | Repo-wide plugin discovery, browser/editor registration hooks, cross-runtime startup/shutdown ordering, signed or process-external authz policy distribution, versioned external SDK packaging | Do not hard-depend on these yet. |

Integration rule:

- Prefer exported crate `lib.rs` seams and contract docs.
- Treat app entry points (`apps/pod-web/src/main.ts`, `apps/pod-server/src/main.rs`) as composition roots, not reusable APIs.
- Treat [Rust SDK Boundary](./rust-sdk-boundary.md) as the repo-owned contract
  for future Rust SDK hookup work.
- Treat `docs/rs-sdk-integration-notes.md` as reference-only context; the
  current packaged workspace SDK surface now lives in
  `crates/pod-sdk/src/lib.rs`.

## Shipping, authz, and SDK boundaries

### Shipping profiles

| Profile | Current role | Commitment |
| --- | --- | --- |
| `ci-smoke` | Fast local/CI safety check | Required for cheap repeatable guardrails. |
| `shard-target` | Publishable operator and retained-history profile | Required for weekly benchmark history and moat evidence. |
| `extensive` | Heavy proof profile used by the TOON export benchmark | Optional for merge-time work; use when updating LLM-facing export claims. |

### Authorization boundary

Current production expectation:

- `pod-host` may load shard-scoped HTTP authorization through
  `OpsHttpAuthorizationPolicySource`.
- Inline or file-backed JSON policy documents are the current supported policy
  source.

Still missing before this becomes a stronger platform contract:

- signed policy claims
- process-external policy distribution
- a browser/editor path that does not depend on local JSON rollout

### SDK boundary

Current production expectation:

- POD-owned crate exports, contract docs, and CLI surfaces are the integration
  boundary to stabilize first.
- The repo-owned Rust SDK hookup contract lives in
  `docs/rust-sdk-boundary.md`.
- External environment adapters should map onto POD action/observation/runtime
  contracts instead of moving authority outside the platform.

Not yet a production commitment:

- a versioned external SDK package for third-party consumers
- direct dependence on app roots
- treating the external `rs-sdk` notes as the primary implementation route

## Phase 8 completion rule

Phase 8 should be treated as complete only when all of the following are true:

- planning docs still agree on the active checklist vs historical log split
- platform requirement gates are named explicitly instead of implied
- the public contract surface is documented as stable/draft/internal
- shipping profile, authz, and SDK decisions are written down as repo policy

That is the current hardening route. New platform work should extend those
contracts or remove the listed missing boundaries instead of inventing a second
planning system.
