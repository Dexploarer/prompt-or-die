# POD Rust SDK Boundary

This document defines the repo-owned boundary Prompt or Die should stabilize
before shipping a formal external Rust SDK package. It exists so future SDK
work builds on exported crate seams, versioned runtime contracts, and
authoritative replay surfaces instead of app-root glue.

> Audience: contributors preparing the future POD-owned Rust SDK facade or any
> adapter that wants to consume POD from Rust without moving gameplay authority
> out of the platform.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Platform Stabilization](./platform-stabilization.md) ·
> [Agent Integration Contract](./agent-integration-contract.md) ·
> [RS-SDK Integration Notes](./rs-sdk-integration-notes.md)

## Scope

The POD-owned Rust SDK boundary is downstream of authority. It is allowed to:

- receive authoritative observations and topology state
- emit standard POD actions
- consume replay, telemetry, and export artifacts
- attach to generated SpacetimeDB runtime surfaces

It is not allowed to:

- become a second gameplay authority
- depend on app roots as reusable APIs
- introduce rs-sdk-specific verbs into `pod-core`
- redefine shell/control-plane transport away from JSON

## Stable contracts to depend on now

These are the current repo-owned seams a future Rust SDK can build against.

| Layer | Stable surface | Why it matters |
| --- | --- | --- |
| Shared gameplay contract | `pod_core::{Agent, Action, AgentAction, Observation, AgentRuntimeProfile}` | The SDK must speak the same observe/decide/validate/execute contract as every controller already in the runtime. |
| Versioned runtime wire artifacts | `pod_core::{RuntimeContractVersion, VersionedObservation, VersionedAgentAction, VersionedTickTelemetry, RustSdkHandoffArtifact}` | These are the versioned envelopes for external runtime exchange, and `RustSdkHandoffArtifact` is the repo-owned bundle that keeps observation, transport, topology, telemetry, and replay in one SDK-facing package. |
| Multi-world topology contract | `pod_core::RemoteTopologyBundle` | The SDK should ingest the same team/world/link/tournament artifact authority publishes everywhere else. |
| Replay and training artifacts | `pod_core::{ReplayFile, ReplayTrainingSample, RewardAttributionSummary}` | Rollouts and learning/export work must reuse authoritative replay truth rather than invent a second episode format. |
| Generated SpacetimeDB runtime seam | `pod_stdb::StdbClient::{install_generated_binding_runtime, install_generated_sdk_runtime, apply_rust_sdk_handoff_artifact}` | This is the repo-owned bridge between deterministic command-runtime tests, the live generated bindings path, and the canonical observation/topology/telemetry handoff ingest used by a future Rust SDK adapter. |
| Network-facing generated runtime seam | `pod_net::SpacetimeDBClient::{install_generated_binding_runtime, install_generated_sdk_runtime, apply_rust_sdk_handoff_artifact}` | Higher-level Rust SDK consumers should be able to choose the same generated binding or live SDK path through the public client wrapper while forwarding replay/debug documents over the existing public surfaces. |
| Thin adapter host | `pod_net::{RustSdkAdapterHost, RustSdkAdapterRuntimeMode}` | Future rs-sdk integration should enter through this small host surface when it needs runtime-mode selection plus Rust/JSON/TOON handoff decoding without depending on app roots. |
| Repo-owned state/action adapter seam | `pod_net::{RustSdkStateSnapshot, RustSdkActionPlan, build_rust_sdk_action_plan, RustSdkActionExecutorError}` | The SDK now has one repo-owned translation surface for external state snapshots and planner-selected actions plus one host-level execution seam (`bind_state_snapshot_action_entity()` and `execute_action_plan()`) before any live SDK method bindings exist. |
| Thin session facade | `pod_net::{RustSdkAdapterSession, RustSdkAdapterSessionError}` | The future POD-owned rs-sdk facade can now start from one repo-owned session surface that binds snapshot state, submits translated actions, and records replay rows without app-local orchestration glue. |
| Thin POD-owned rs-sdk facade | `pod_net::{RustSdkFacade, RustSdkFacadeConfig, RustSdkFacadeError}` | This is the smallest repo-owned wrapper a future packaged rs-sdk can depend on directly when it wants runtime-mode selection, handoff ingest, action execution, replay finalization, and the live smoke entrypoint without exposing app-local host/session glue. |
| Packaged Rust SDK surface | `pod_sdk::{RustSdkClient, RustSdkClientConfig, RustSdkClientError, RustSdkRuntimeMode, RustSdkActionPlan, RustSdkActionPlanError, build_rust_sdk_action_plan, RustSdkRolloutRecord, RustSdkRolloutRecordError, RustSdkBenchmarkCheck, RustSdkBenchmarkScenarioReport, RustSdkBenchmarkReport, RustSdkBenchmarkRun, RustSdkLiveSmokeConfig, RustSdkLiveSmokeReport, RustSdkLiveSmokeRun, run_rust_sdk_benchmark_suite, run_rust_sdk_live_smoke}` | This is the first packaged workspace crate that wraps the thin facade in package-native config/error, action-plan, rollout-record, and benchmark/live-smoke report types and hosts the canonical smoke/benchmark entrypoints above `pod-net`. |
| Live generated-SDK smoke surface | `pod_net::{RustSdkAdapterLiveSmokeConfig, run_rust_sdk_adapter_live_smoke}` | When a real SpacetimeDB module is running, this proves the repo-owned session facade can spawn, connect, submit, and record over `GeneratedSdk` mode instead of only deterministic emulation. |
| Repo-owned rollout/benchmark seam | `pod_net::{RustSdkRolloutRecorder, RustSdkBenchmarkReport, run_rust_sdk_adapter_benchmark_suite}` | SDK-driven episodes and adapter parity checks can now stay on the same replay/training/report contracts, and the benchmark suite now exercises real queue/send submission instead of only translation. |
| Large agent-facing export surfaces | `pod export world|events|multiverse --format json|toon` | The future SDK can bootstrap context, event batches, and topology proofs from these stable exported datasets instead of scraping app-local state. |

## Adapter lanes

The future POD-owned Rust SDK facade should stabilize four explicit adapter
lanes. These are the seams to harden now.

### `rs_state_adapter`

Responsibility:

- map generated-runtime callbacks, export artifacts, or benchmark fixtures into
  `Observation`, `VersionedObservation`, `RustSdkHandoffArtifact`, and
  `RemoteTopologyBundle`
- centralize that translation through `pod_net::RustSdkStateSnapshot` so SDK
  state, dialog/shop/bank context, and handoff metadata do not become app-local
  glue

Rules:

- preserve `RuntimeContractVersion`
- preserve `AgentRuntimeProfile`
- do not smuggle hidden SDK-local state around authoritative observations
- preserve auxiliary context by turning dialog/shop/bank state into explicit
  observation messages and action hints until the shared observation schema
  grows dedicated fields

### `rs_action_adapter`

Responsibility:

- map SDK-local planner or policy outputs into `Action` / `AgentAction` /
  `VersionedAgentAction`
- expose the reverse lowering through `pod_net::RustSdkActionPlan` plus
  `build_rust_sdk_action_plan()` so future SDK integrations can choose between
  immediate and completion-aware execution without mutating `pod_core::Action`
- execute the lowered plan through `RustSdkAdapterHost::execute_action_plan()`
  after binding the authoritative action entity with
  `RustSdkAdapterHost::bind_state_snapshot_action_entity()`

Rules:

- translate into standard POD actions before validation
- keep SDK-local affordances out of `pod-core`
- let authority reject stale or invalid actions instead of trying to bypass it
- reject world-authority-only actions such as `Spawn` instead of pretending the
  SDK can bypass authority
- keep execution on the existing `queue_action()` / `send_actions()` path
  instead of inventing an rs-sdk-only submission transport

### `rs_rollout_recorder`

Responsibility:

- persist SDK-driven episodes into `ReplayFile`, replay-derived reward rows, and
  agent-facing export artifacts

Rules:

- reuse authoritative telemetry and reward attribution
- keep the replay/training shape shared with local scripted, LLM, hybrid, and
  neural controllers
- prefer `pod export events|world|multiverse` for large context products
- prefer `RustSdkAdapterSession` when the integration wants one repo-owned
  facade that performs snapshot ingest, action execution, and rollout recording
  together
- prefer `pod_net::RustSdkRolloutRecorder` so SDK-side episodes finalize into a
  standard `ReplayFile` plus `ReplayTrainingSample` rows instead of adapter-only
  event logs

### `rs_benchmark_runner`

Responsibility:

- run deterministic scenario evaluation for SDK-backed controllers against the
  same parity and topology proof surfaces POD already trusts

Rules:

- compare against scripted, LLM, hybrid, and neural baselines on the same
  contract
- keep shell/control-plane messages JSON
- use TOON only for large tabular or semi-tabular world data products
- prefer `run_rust_sdk_adapter_benchmark_suite()` and
  `cargo run -p pod-net --features spacetimedb --example rust_sdk_adapter_benchmark_suite -- --fail-on-checks`
  as the deterministic adapter seam smoke/benchmark surface before wiring live
  SDK calls
- prefer `run_rust_sdk_benchmark_suite()` and
  `cargo run -p pod-sdk --example rust_sdk_benchmark_suite -- --fail-on-checks`
  when the goal is to exercise the packaged Rust SDK surface instead of the
  lower-level `pod-net` seam directly
- prefer `run_rust_sdk_adapter_live_smoke()` and
  `cargo run -p pod-net --features spacetimedb --example rust_sdk_adapter_live_smoke -- --host http://127.0.0.1:3100 --db-name deadman-prime --fail-on-checks`
  when a local published module is available and the goal is to prove
  `RustSdkAdapterSession` reaches live generated-SDK `spawn_entity`,
  `connect_agent`, and `submit_action` rows
- prefer `run_rust_sdk_live_smoke()` and
  `cargo run -p pod-sdk --example rust_sdk_live_smoke -- --host http://127.0.0.1:3100 --db-name deadman-prime --fail-on-checks`
  when the packaged Rust SDK surface should own the live-smoke entrypoint
- treat the `pod-net` rust-sdk examples as seam-level compatibility shims
  rather than the canonical package-facing command surface
- keep the benchmark execution-backed: it should submit through the same host
  bind/execute seam over emulated or generated-binding runtime modes rather
  than only checking plan translation

## Generated-runtime handoff

The Rust SDK hookup should not invent a third path between emulated tests and
live generated bindings.

The supported handoff is:

1. Use `install_generated_binding_runtime()` when the adapter host needs to
   inspect outbound commands and inject callbacks deterministically.
2. Use `install_generated_sdk_runtime()` when the adapter should ride the live
   generated SpacetimeDB bindings path.
3. Prefer `RustSdkAdapterHost` when the integration needs one small public
   wrapper that owns runtime-mode selection and Rust/JSON/TOON handoff decode.
4. Prefer `RustSdkStateSnapshot` plus `RustSdkAdapterHost::apply_state_snapshot()`
   when the integration is still translating raw SDK state into repo-owned
   observations and handoff bundles. That host method also hydrates the local
   entity cache so subsequent action execution can use the same snapshot state.
5. Apply the resulting SDK-facing bundle through
   `apply_rust_sdk_handoff_artifact()` so observations, topology, telemetry,
   and replay stay on the same repo-owned client ingress path instead of
   becoming adapter-local glue.
6. Bind the controlled entity with
   `RustSdkAdapterHost::bind_state_snapshot_action_entity()` (or
   `bind_action_entity()`) before calling
   `RustSdkAdapterHost::execute_action_plan()`.
7. Prefer `RustSdkAdapterSession` when the rs-sdk facade wants one repo-owned
   wrapper that composes steps 4-6 with rollout recording.
8. Prefer `RustSdkFacade` when the packaged SDK surface wants one config-bound
   wrapper above that session seam.
9. Prefer `run_rust_sdk_live_smoke()` when a local module is available and the
   goal is to prove that the packaged SDK surface still reaches real
   generated-SDK `spawn_entity`, `connect_agent`, and `submit_action` rows.
10. Keep the `pod-net` rust-sdk examples only as compatibility/debug shims for
    seam-level verification beneath the packaged crate.

This keeps the future SDK aligned with the repo-owned generated runtime seam
instead of re-owning connection or callback semantics in app code.

## Non-goals and forbidden dependencies

Do not build the future Rust SDK boundary on:

- `apps/pod-web/src/main.ts`
- `apps/pod-server/src/main.rs`
- editor panel internals
- local JSON authz rollout details
- TOON shell envelopes
- the external `Dexploarer/rs-sdk` notes as if they were POD authority

Those surfaces may still be useful for reference or composition, but they are
not the repo-owned SDK contract.

## Readiness gates

The SDK boundary should only be treated as ready when these checks stay green:

```bash
cargo test -p pod-core contract -- --nocapture
cargo test -p pod-stdb --no-default-features --features client
cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture
cargo test -p pod-sdk -- --nocapture
cargo check -p pod-net --features spacetimedb --example rust_sdk_adapter_benchmark_suite
cargo run -p pod-net --features spacetimedb --example rust_sdk_adapter_benchmark_suite -- --fail-on-checks
cargo check -p pod-sdk --example rust_sdk_benchmark_suite
cargo run -p pod-sdk --example rust_sdk_benchmark_suite -- --fail-on-checks
cargo run -p pod-core --example rust_sdk_handoff_fixture -- --format toon >/tmp/pod-sdk-handoff.toon
bun ./scripts/pod.ts export world --format json >/tmp/pod-sdk-world.json
bun ./scripts/pod.ts export events --format toon >/tmp/pod-sdk-events.toon
bun ./scripts/pod.ts export multiverse --format json >/tmp/pod-sdk-multiverse.json
bun test scripts/verify_rust_sdk_boundary.test.ts scripts/verify_cli_surface.test.ts
bun ./scripts/verify_rust_sdk_boundary.ts --check
```

Optional live generated-SDK smoke when a local module is available:

```bash
cargo run -p pod-net --features spacetimedb --example rust_sdk_adapter_live_smoke -- --host http://127.0.0.1:3100 --db-name deadman-prime --fail-on-checks
cargo run -p pod-sdk --example rust_sdk_live_smoke -- --host http://127.0.0.1:3100 --db-name deadman-prime --fail-on-checks
```

If any of those surfaces drift, update the owning crate/doc pair together
before claiming the Rust SDK boundary is still stable.

## Current readiness statement

As of the current Phase 8 hardening pass:

- the versioned runtime envelopes exist in `pod-core`
- the generated binding and live SDK runtime install seams exist in `pod-stdb`
  and `pod-net`
- the thin `RustSdkAdapterHost` wrapper exists for runtime selection and
  handoff-document decode above those client seams
- the repo-owned `RustSdkStateSnapshot` and `RustSdkActionPlan` surfaces exist
  in `pod-net`, so future SDK hookups can translate external state/actions
  before any live binding code is written
- the host-level bind/execute seam now exists in `pod-net`, so translated
  action plans already submit through the shared queue/send path instead of a
  benchmark-only translation lane
- the thin `RustSdkFacade` wrapper now exists in `pod-net`, so the future
  packaged rs-sdk can start from one repo-owned config-bound surface instead
  of stitching `RustSdkAdapterHost`, `RustSdkAdapterSession`, and the live
  smoke helper together in app code
- the first packaged workspace SDK surface now exists in `pod-sdk`, so the
  canonical smoke and benchmark commands no longer need to import `pod-net`
  directly just to reach the facade and helper seams
- the packaged crate now owns its benchmark and live-smoke report types, so
  callers no longer need direct `pod-net` benchmark structs just to serialize
  or inspect package-facing SDK results
- the packaged crate now also owns its action-plan and rollout-record write-path
  types, so package consumers can build or inspect SDK-facing action plans and
  replay-bound rollout steps without importing `pod-net` structs directly
- the `pod-net` rust-sdk examples intentionally remain as compatibility shims
  for seam-level debugging, not as the primary documented SDK command surface
- the live generated-SDK smoke harness now exists in `pod-net`, so a running
  module can prove `RustSdkAdapterSession` reaches real `spawn_entity`,
  `connect_agent`, and `submit_action` rows while the facade stays thin
- the thin `RustSdkAdapterSession` facade now exists in `pod-net`, so the
  future POD-owned rs-sdk wrapper can compose snapshot ingest, action
  submission, and replay recording without rebuilding that glue in app code
- the repo-owned `RustSdkRolloutRecorder` and
  `run_rust_sdk_adapter_benchmark_suite()` surfaces exist in `pod-net`, so
  adapter-driven episodes already land on shared replay/training/report
  contracts before live SDK calls are wired, and the benchmark suite now
  covers both emulated and generated-binding submission-backed cases
- large agent-facing export surfaces exist for world, events, and multiverse
- replay/training artifacts already derive from authoritative telemetry

What is still intentionally not promised:

- a versioned third-party Rust SDK release outside this workspace
- app-root lifecycle hooks as reusable SDK APIs
- any SDK-specific action dialect inside `pod-core`

That is the boundary to preserve while the packaged POD Rust SDK surface is
iterated inside the workspace.
