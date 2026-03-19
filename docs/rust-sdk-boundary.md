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
| Large agent-facing export surfaces | `pod export world|events|multiverse --format json|toon` | The future SDK can bootstrap context, event batches, and topology proofs from these stable exported datasets instead of scraping app-local state. |

## Adapter lanes

The future POD-owned Rust SDK facade should stabilize four explicit adapter
lanes. These are the seams to harden now.

### `rs_state_adapter`

Responsibility:

- map generated-runtime callbacks, export artifacts, or benchmark fixtures into
  `Observation`, `VersionedObservation`, `RustSdkHandoffArtifact`, and
  `RemoteTopologyBundle`

Rules:

- preserve `RuntimeContractVersion`
- preserve `AgentRuntimeProfile`
- do not smuggle hidden SDK-local state around authoritative observations

### `rs_action_adapter`

Responsibility:

- map SDK-local planner or policy outputs into `Action` / `AgentAction` /
  `VersionedAgentAction`

Rules:

- translate into standard POD actions before validation
- keep SDK-local affordances out of `pod-core`
- let authority reject stale or invalid actions instead of trying to bypass it

### `rs_rollout_recorder`

Responsibility:

- persist SDK-driven episodes into `ReplayFile`, replay-derived reward rows, and
  agent-facing export artifacts

Rules:

- reuse authoritative telemetry and reward attribution
- keep the replay/training shape shared with local scripted, LLM, hybrid, and
  neural controllers
- prefer `pod export events|world|multiverse` for large context products

### `rs_benchmark_runner`

Responsibility:

- run deterministic scenario evaluation for SDK-backed controllers against the
  same parity and topology proof surfaces POD already trusts

Rules:

- compare against scripted, LLM, hybrid, and neural baselines on the same
  contract
- keep shell/control-plane messages JSON
- use TOON only for large tabular or semi-tabular world data products

## Generated-runtime handoff

The Rust SDK hookup should not invent a third path between emulated tests and
live generated bindings.

The supported handoff is:

1. Use `install_generated_binding_runtime()` when the adapter host needs to
   inspect outbound commands and inject callbacks deterministically.
2. Use `install_generated_sdk_runtime()` when the adapter should ride the live
   generated SpacetimeDB bindings path.
3. Apply the resulting SDK-facing bundle through
   `apply_rust_sdk_handoff_artifact()` so observations, topology, telemetry,
   and replay stay on the same repo-owned client ingress path instead of
   becoming adapter-local glue.

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
cargo run -p pod-core --example rust_sdk_handoff_fixture -- --format toon >/tmp/pod-sdk-handoff.toon
bun ./scripts/pod.ts export world --format json >/tmp/pod-sdk-world.json
bun ./scripts/pod.ts export events --format toon >/tmp/pod-sdk-events.toon
bun ./scripts/pod.ts export multiverse --format json >/tmp/pod-sdk-multiverse.json
bun test scripts/verify_rust_sdk_boundary.test.ts scripts/verify_cli_surface.test.ts
bun ./scripts/verify_rust_sdk_boundary.ts --check
```

If any of those surfaces drift, update the owning crate/doc pair together
before claiming the Rust SDK boundary is still stable.

## Current readiness statement

As of the current Phase 8 hardening pass:

- the versioned runtime envelopes exist in `pod-core`
- the generated binding and live SDK runtime install seams exist in `pod-stdb`
  and `pod-net`
- large agent-facing export surfaces exist for world, events, and multiverse
- replay/training artifacts already derive from authoritative telemetry

What is still intentionally not promised:

- a packaged third-party Rust SDK crate
- app-root lifecycle hooks as reusable SDK APIs
- any SDK-specific action dialect inside `pod-core`

That is the boundary to preserve while the POD-owned Rust SDK facade is wired
up.
