# Plugin Model

Prompt or Die does not yet ship a formal Bevy-style `App` / `Plugin` lifecycle. That work is still tracked in the Phase 8 and Phase 9 roadmap. This document defines the plugin model that exists today so integrators can extend the platform without inventing incompatible patterns.

## Current status

Today, "plugin" means extending the platform through stable subsystem seams, not through a single runtime registration API.

The practical extension surfaces are:

- New crates inside the workspace
- New components and systems in `pod-core`
- New scene/prefab bindings in `pod-scene`
- New asset import or processing stages in `pod-assets`
- New agent implementations in `pod-agents`
- New scripting functions in `pod-scripting`
- New editor panels or authoring flows in `pod-editor`
- New transport or persistence adapters in `pod-net` / `pod-stdb`

## Current contract map

The table below names the extension seams that are treated as the practical
contract today. If a seam is not in this table, assume it is still internal.

| Seam | Public surface to depend on | What you own | Validation path today |
| --- | --- | --- | --- |
| Authoring to runtime translation | `pod_scene::NativeComponentBinding`, `pod_scene::Prefab`, `pod_scene::PrefabRegistry`, `pod_scene::Scene`, `pod_scene::SceneManager` | New authored component schemas, prefab composition, scene instantiation rules | `cargo test -p pod-scene test_scene_instantiation_tracks_component_provenance_across_prefab_and_scene_layers -- --nocapture` |
| Asset import to shipped runtime bundle | `pod_assets::import_asset`, `pod_assets::build_runtime_bundle_manifest`, `pod_assets::materialize_runtime_bundle_manifest` | New source imports, bundle specs, staged-to-runtime materialization rules | `cargo test -p pod-assets build_runtime_bundle_manifest_maps_staged_imports_to_runtime_paths -- --nocapture` |
| Direct-connect debug transport | `pod_core::ShardTransportSummary`, `pod_net::protocol::{ClientMessage, ServerMessage}` | New typed debug documents, transport counters, recovery/resume behavior | `cargo test -p pod-net handle_connections -- --nocapture` |
| Browser debug/runtime consumer | `apps/pod-web/src/contracts.ts`, `apps/pod-web/src/direct-connect.ts`, `apps/pod-web/src/hud.ts` | Runtime HUD/debug summaries and browser-side degraded-path handling | `cd apps/pod-web && bun test src/direct-connect.test.ts src/contracts.test.ts src/hud.test.ts` |

These are the seams to extend if you need something now. They already have
public types plus deterministic tests, which is the closest thing to a plugin
SDK the repo currently ships.

## Stability tiers

| Tier | Surface | Guidance |
| --- | --- | --- |
| Current platform contract | `Agent` trait, action/observation flow, `pod-scene` bindings, `RenderState`, transport message types | Safe place to integrate against now |
| Draft contract | Public docs in this directory, scene streaming model, prefab provenance and override reporting | Good integration targets, but still moving |
| Internal / not yet stabilized | Formal plugin lifecycle, schedule graph, app startup hooks, versioned extension SDK | Do not build hard dependencies on these yet |

## How to extend the platform today

### 1. Gameplay or simulation feature

Use this path when adding a new game mechanic, system, or authored component:

1. Add components or supporting types in the closest runtime crate.
2. Wire deterministic behavior into the simulation tick or authoritative reducer path.
3. Add scene/prefab bindings if designers need to author the data.
4. Add editor inspection support if it must be editable visually.
5. Add tests at the crate boundary that owns the behavior.

This keeps gameplay state in the world model instead of splitting logic across ad-hoc boot code.

### 2. Rendering extension

Use this path when introducing a new visual representation:

1. Extend world-side render extraction inputs.
2. Extend `pod-render::renderer::DrawType` or the extraction rules where appropriate.
3. Ensure both native and browser surfaces degrade or serialize cleanly.
4. Add tests for the extracted render representation.

The render layer should consume world state, not become a second gameplay state container.

### 3. Authoring extension

Use this path when adding a new designer-facing concept:

1. Define the authored schema in `pod-scene`.
2. Bind it to native component data.
3. Support prefab overrides and provenance when the concept is editable.
4. Expose it in `pod-editor`.

### 4. Agent extension

Use this path when adding a new agent runtime or control surface:

1. Implement the `Agent` trait.
2. Emit standard `Action` values.
3. Respect runtime constraints and observation semantics.
4. Integrate through the same tick path as every other agent type.

This is the most important plugin rule in the repo: no agent type gets special gameplay privileges.

## Bootstrap ownership and non-contract surfaces

The current repo has several important boot modules, but they should still be
treated as composition roots rather than extension APIs.

| Surface | Status | Guidance |
| --- | --- | --- |
| `apps/pod-web/src/main.ts` | Internal composition root | Do not add feature-specific hooks here if the behavior belongs in `pod-scene`, `pod-assets`, `pod-net`, or shared browser contracts. |
| `apps/pod-web/src/runtime-config.ts` and `runtime-flags.ts` | Stable app-local bootstrap inputs | Safe for route/runtime selection and deterministic test toggles, but not a general plugin lifecycle. |
| `crates/pod-core/src/authority.rs` | Current transport-neutral authority world contract | Safe place to compose `AuthorityWorldConfig`, `WorldBootstrapPlan`, and `build_authoritative_world(...)` without depending on transport-layer types. |
| `crates/pod-net/src/authority.rs` | Current direct-connect transport adapter contract | Safe place to compose `DirectConnectTransportConfig`, `TransportPolicy`, `parse_bind_target(...)`, and `server_config(tick_rate)` without re-owning world/bootstrap state. |
| `crates/pod-host/src/lib.rs` | Current neutral authority host lifecycle contract | Safe place to compose `AuthorityHostConfig`, `AuthorityTransportMode`, `AuthorityHostRuntime`, `AuthorityShardConfig`, and `ShardSupervisorConfig` so apps can select single-host or multi-shard authority topologies without stitching `pod-core` and `pod-net` together manually. |
| `apps/pod-server/src/main.rs` | Thin internal entry point | Keep it focused on process startup, shutdown wiring, and calling the exported authority lifecycle surface. |
| Crate `lib.rs` re-exports (`pod-scene`, `pod-assets`, `pod-core`) | Current contract surface | Prefer integrating against these exported types/functions instead of reaching into app boot files. |

This is the near-term rule: extend exported crate boundaries first, and only
touch app bootstrap when you are composing existing subsystems together.

## Missing lifecycle hooks that still block integrators

The current seams are usable, but several hooks are still missing and force
integrators back into app composition roots:

- Multi-shard authority control-plane hook:
  `crates/pod-host/src/lib.rs` now exposes
  `AuthorityShardControlPlaneHandle` and
  `ShardSupervisorControlPlaneHandle`, plus derived shard incident summaries and
  coordinated drain/shutdown commands, so supervised shard sets can snapshot
  live direct-connect transport pressure and issue basic lifecycle control
  without per-shard log scraping. What is still missing is a shared gameplay
  incident feed and durable ops publication surface above the supervised shard
  set.
- Browser mode/bootstrap hook:
  `apps/pod-web/src/main.ts` still owns renderer creation, local-world vs
  direct-connect mode choice, DOM wiring, and telemetry/debug bootstrapping in
  one file. There is no formal registration phase for runtime features before
  or after renderer startup.
- Editor panel registry hook:
  `crates/pod-editor/src/lib.rs` still uses a closed `EditorPanel` enum plus
  hardcoded `render_*panel` dispatch, so new panels require editing the editor
  shell instead of registering themselves.
These are the next seams to formalize if POD wants real plugin/app lifecycle
parity instead of “extend the crate, then patch the app root.”

## Near-term conventions before a formal plugin SDK

Until the lifecycle work lands, use these conventions consistently:

### Imports

- Depend on crate `lib.rs` re-exports when they exist.
- Avoid deep module path coupling from app roots into crate internals unless the
  crate does not export the seam yet.

### Runtime registration

- Put feature-specific setup in the owning crate, exposed as typed constructors,
  config structs, or helper functions.
- Keep app roots responsible only for composing already-exported subsystems
  together.
- If a feature still requires app-root edits, document that as a missing hook
  instead of silently treating the app root as part of the stable API.

### Extension testing

- Prove the seam at the owning crate boundary first.
- Add app-level/browser smoke only when the seam crosses a runtime boundary.
- Keep validation commands next to the roadmap/session updates so future
  integrators can reuse them verbatim.

## Recommended crate pattern

For new platform subsystems, prefer a dedicated crate that depends on the existing runtime boundaries rather than modifying many crates at once.

Typical shape:

```text
crates/pod-your-feature
  src/lib.rs
  src/types.rs
  src/runtime.rs
  src/tests.rs
```

The feature crate should then integrate with:

- `pod-core` for runtime data and system hooks
- `pod-scene` for authored content
- `pod-editor` for tooling
- `pod-net` or `pod-stdb` if the feature crosses the network boundary

## What the formal plugin system will add later

The roadmap items for plugin parity still matter. The future formal model is expected to add:

- Startup and shutdown hooks
- Ordered plugin registration
- Schedule phase hooks
- Resource and system registration
- Versioned extension API boundaries

Those items are already tracked in:

- `8.4 Add plugin/extension ecosystem and versioned SDK API surface`
- `9.1 Implement plugin and app lifecycle system equivalent to Bevy App/Plugin hooks`
- `9.2 Implement full schedule-driven ECS world graph`

## Rules for extension authors

- Treat `pod-core` as the simulation authority.
- Treat `pod-scene` as the authored-content authority.
- Keep new behavior deterministic unless the boundary explicitly allows otherwise.
- Do not create agent-specific gameplay bypasses.
- Prefer additive crate-level integrations over large cross-cutting edits.
- Validate the seam you extend at the owning crate boundary before relying on app-level smoke.

## Current extension testing rule

Until a formal plugin SDK exists, every extension should prove itself in the
crate that owns the seam:

1. Add or update deterministic tests in the boundary crate first.
2. Only use app-level/browser smoke to prove composition, not basic seam correctness.
3. Record the validated seam in `SESSION.md` / `IMPLEMENTATION_PLAN.md` when it becomes part of the expected extension path.

Following those rules now will make the eventual formal plugin API easier to adopt when it lands.
