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

Following those rules now will make the eventual formal plugin API easier to adopt when it lands.
