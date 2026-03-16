# Implementation Reset Audit

This document records the planning reset requested for the repo.

The goal was not to invent a third roadmap. The goal was to:

1. Derive an execution order from the live codebase as if the plan were being written today.
2. Compare that clean plan against the existing planning documents.
3. Choose the route that is most useful for future work.

## Fresh plan derived from the codebase

The from-scratch route that best matches the workspace is:

1. Deterministic gameplay kernel
2. Agent execution stack
3. Authority runtime, networking, and persistence
4. Multi-world and remote topology
5. Scene, asset, and content pipeline
6. Client runtime consumers
7. Editor and authoring tooling
8. Lifecycle, SDK, and shipping stabilization

That ordering comes directly from the workspace boundaries:

- `pod-core`, `pod-physics`, and `pod-spatial` define the bottom of the runtime stack.
- `pod-agents` and `pod-scripting` layer on top of those contracts.
- `pod-host`, `pod-net`, `pod-stdb`, and `pod-server` turn the runtime into an authoritative distributed system.
- `pod-headless` proves the multi-world topology and evaluation model without requiring a client.
- `pod-scene` and `pod-assets` own the authoring-to-runtime path.
- `pod-render`, `pod-web`, and `pod-desktop` are consumers of those contracts.
- `pod-editor` is a tooling consumer with extension gaps, not the foundation of the platform.
- Lifecycle, SDK, authz distribution, and shipping concerns are last-mile stabilization work.

## Comparison with the existing planning docs

| Topic | Existing docs | Fresh-pass conclusion |
| --- | --- | --- |
| `IMPLEMENTATION_PLAN.md` | Valuable historical log with 200+ iterations and completion evidence | Keep as archive; do not reset checkboxes in place |
| `IMPLEMENTATION_PHASES.md` | Recent context-specific roadmap focused on browser asset and agent-runtime follow-through | Replace as the active execution checklist |
| Ordering | Mixed historical accretion and recency-driven priorities | Use dependency order from kernel upward |
| Verification state | Many boxes reflect past completion, not current re-validation | Start the active checklist unchecked and re-prove each layer |
| Planning signal | High provenance, low clarity for “what do we verify next?” | One active unchecked plan plus one archival log is clearer |

## Decision

The optimal route is:

- Preserve `IMPLEMENTATION_PLAN.md` as the historical implementation record.
- Reset `IMPLEMENTATION_PHASES.md` into the new unchecked active execution plan.
- Track current execution in `SESSION.md`.
- Use this audit document to explain why the split exists.

## Why the full in-place reset was rejected

Resetting every checkbox in `IMPLEMENTATION_PLAN.md` would destroy useful information:

- which slices were already implemented
- which validations were already known-good
- how the current architecture was reached
- which later docs depend on that implementation history

That would make the repo harder to reason about, not easier.

The chosen split preserves provenance while still giving you the “start from the beginning and re-check everything” workflow you asked for.
