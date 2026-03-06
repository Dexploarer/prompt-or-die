# IMPLEMENTATION_PLAN.md — Prompt or Die

Priority-sorted task list. One task per iteration. Mark [x] when complete.

---

## Iteration 28: Deterministic Core + Network + Persistence Completion
- [x] Core action semantics implemented in `pod-core` for `Attack`, `AttackTarget`, `Interact`, `InteractWith` with cooldown gating, range checks, and combat/interaction events.
- [x] Observation fields populated in `pod-core` (`cooldowns`, `messages`, `objectives`) from authoritative runtime state/events.
- [x] SpacetimeDB reducer action coverage expanded (`LookAt`, `Attack`, `AttackTarget`, `Interact`, `InteractWith`, `Pickup`, `Drop`, `UseItem`, `UseAbility`, `Signal`, `Spawn`) with deterministic ordering and cooldown handling.
- [x] Authoritative `pod-net` server path wired for real QUIC ingress, action queue limits, per-tick bounded action intake, and delta/full snapshot broadcasting.
- [x] `pod-stdb` client placeholders replaced with concrete typed behavior for connection lifecycle, subscriptions, reducer wrappers, and local cache mutation.
- [x] Scripting mutating APIs (`world.spawn`, `world.destroy`, `world.find_nearest`, `events.emit`) implemented with validation, bounds checks, and per-tick event rate limiting.
- [x] Native and web transport behavior hardened (secure-by-default TLS for native, dev-only insecure escape hatch, web reconnect + out-of-order delta reconciliation).
- [x] `apps/pod-server` runtime mode wiring updated to support either local loop mode or `pod-net` network mode with explicit configuration.

---

## Phase 1: SpacetimeDB Foundation
- [x] 1.1 Add spacetimedb dependency to workspace Cargo.toml
- [x] 1.2 Create pod-stdb crate with module scaffold (tables, reducers, events, types)
- [x] 1.3 Define SpacetimeDB tables mirroring ECS components (entity, transform, velocity, health, perception, visual, collider, label, movement, agent_constraints, script)
- [x] 1.4 Define world_state singleton table (tick counter, RNG seed, config)
- [x] 1.5 Implement `create_world` reducer (+ `set_paused`)
- [x] 1.6 Implement `spawn_entity` reducer (basic + full variants)
- [x] 1.7 Implement `connect_agent` / `disconnect_agent` reducers (+ client lifecycle)
- [x] 1.8 Implement `submit_actions` reducer (receive agent decisions)
- [x] 1.9 Implement `execute_tick` reducer (Move, Stop, Rotate, Speak, Idle + velocity integration)
- [x] 1.10 Define event tables (observation_events, combat_events, speech_events, world_events)
- [x] 1.11 Implement observation building in SpacetimeDB (per-agent perception queries)
- [x] 1.12 Add row-level security to observation_events
- [x] 1.13 Create pod-stdb client wrapper (Rust native client SDK integration)
- [x] 1.14 Write integration tests for pod-stdb module
- [x] 1.15 Update pod-net to support SpacetimeDB connection mode alongside direct-connect

## Phase 2: Enhanced Agent SDK
- [x] 2.1 Add lifecycle hooks to Agent trait (on_spawn, on_damage, on_death, on_interact)
- [x] 2.2 Add introspect() to Agent trait for debugging
- [x] 2.3 Create LLM provider abstraction (OpenAI-compatible trait)
- [x] 2.4 Implement prompt template system for observation formatting
- [x] 2.5 Add token budget management to LlmAgent
- [x] 2.6 Implement structured output parsing (JSON → Action mapping)
- [x] 2.7 Add conversation memory (sliding window)
- [x] 2.8 Create behavior tree node library (patrol, chase, flee, guard, wander)
- [x] 2.9 Create FSM templates (idle↔alert↔combat↔dead)
- [x] 2.10 Implement Utility AI (score-based action selection)
- [x] 2.11 Create hybrid agent framework (LLM strategy + BT execution)
- [x] 2.12 Implement decision logging and replay
- [x] 2.13 Add ONNX runtime integration for neural agents
- [x] 2.14 Write comprehensive agent SDK tests

## Phase 3: 3D Engine Foundation
- [x] 3.1 Add Transform3D, Mesh, Material, Camera3D, Light components
- [x] 3.2 Create forward rendering pipeline with depth buffer in wgpu
- [x] 3.3 Implement WGSL shader system (vertex/fragment/uniform binding)
- [x] 3.4 Add glTF 2.0 import (gltf crate)
- [x] 3.5 Implement frustum culling
- [x] 3.6 Add batched draw calls (group by material)
- [x] 3.7 Create camera controller components (orbit, fly, follow)
- [x] 3.8 Implement parent-child transform hierarchy
- [x] 3.9 Support 2D + 3D mixed rendering in same frame
- [x] 3.10 Write render pipeline tests and benchmarks

## Phase 4: Asset Pipeline
- [x] 4.1 Create pod-assets crate scaffold
- [x] 4.2 Implement content-addressed asset cache
- [x] 4.3 Add asset import pipeline (glTF, OBJ, PNG, JPEG)
- [x] 4.4 Implement mesh processing (LOD generation)
- [x] 4.5 Add texture processing (compression, atlas packing)
- [x] 4.6 Implement hot-reload (file watcher + reprocess)
- [x] 4.7 Add procedural terrain generation (noise-based heightmaps)
- [x] 4.8 Add procedural dungeon generation (BSP tree)
- [x] 4.9 Add procedural texture generation (noise, gradients)
- [x] 4.10 Create AI asset generation integration points (text-to-mesh, text-to-texture)
- [x] 4.11 Write asset pipeline tests

## Phase 5: Game Maker / Editor
- [x] 5.1 Create pod-editor crate with egui scaffold
- [x] 5.2 Implement dockable panel system (viewport, hierarchy, inspector, console)
- [x] 5.3 Build entity hierarchy panel (tree view)
- [x] 5.4 Build component inspector (property editors)
- [x] 5.5 Implement 2D viewport with entity placement gizmos
- [x] 5.6 Add 3D viewport rendering
- [x] 5.7 Build asset browser panel
- [x] 5.8 Implement play/stop/pause mode
- [x] 5.9 Build visual behavior tree editor
- [x] 5.10 Build FSM editor
- [x] 5.11 Add LLM agent configuration panel
- [x] 5.12 Build SpacetimeDB dashboard panel
- [x] 5.13 Implement undo/redo system
- [x] 5.14 Add project save/load
- [x] 5.15 Write editor tests

## Phase 6: Networking & Multiplayer
- [x] 6.1 Implement SpacetimeDB subscription manager in pod-net
- [x] 6.2 Create interest management (spatial SQL query filtering)
- [x] 6.3 Implement lobby system (SpacetimeDB tables + reducers)
- [x] 6.4 Add matchmaking reducer
- [x] 6.5 Remote LLM agent connection via SpacetimeDB
- [x] 6.6 Spectator mode (full world subscription, read-only)
- [x] 6.7 World partitioning for large worlds
- [x] 6.8 Performance benchmarks (target: 1000 agents at 60 TPS)
- [x] 6.9 Write networking integration tests

## Phase 7: Full Engine Parity Platform (2D/2.5D/3D + AI Tooling)
- [ ] 7.1 Define and implement browser-first runtime contract for native and web renderer backends
  - Stable frame loop, fixed timestep simulation, input abstraction (keyboard/mouse/touch/gamepad), deterministic world stepping
  - Determinism tests across desktop + browser CI
- [ ] 7.2 Build first-class 2D framework beyond sprite/text primitives
  - Tilemap + Tiled JSON importer
  - Camera systems (follow, lerp, dead-zone, parallax)
  - Animation playback for sprites (single frame + atlased + timelines)
  - Font/text rendering and canvas-independent UI primitives
- [ ] 7.3 Complete 2.5D projection system
  - Depth-layer policy for mixed 2D/3D sprites
  - Occlusion and ordering rules across 2D, 3D, and 2.5D
  - Billboard/oriented sprites in full transform graph
- [ ] 7.4 Complete 3D runtime baseline
  - Material/pbr basics (PBR texture channels + uniform blocks)
- [ ] 7.5 Build platform-grade scene system
  - Scene graph + prefab + inheritance
  - Addressable entity references, prefab overrides, prefab diff/merge
  - Scene streaming for large worlds
- [ ] 7.6 Add game-dev-grade audio and FX stack
  - SFX/ambient/voice channels, mixer bus routing, positional audio, cue-based playback
  - Post-processing presets for camera and color correction
- [ ] 7.7 Add complete animation + state machinery
  - Animator state graph, timelines, events, blend trees
  - Root-motion support and state transitions with blend duration
- [ ] 7.8 Add physics + navigation parity
  - 2D and 3D colliders, dynamic/static rigid bodies, raycasts
  - Simple navmesh generation + obstacle updates + movement agent integration
- [ ] 7.9 Deliver browser-first editor and debug tooling
  - World scene graph inspector, component editor, transform gizmos, profiler overlay
  - Replay/recording and AI observation tape player
- [ ] 7.10 AI-native game-dev toolchain
  - Behavior authoring templates for agents (combat, exploration, utility use)
  - Observation schema/versioned prompts, deterministic tool calling, action schema validation
  - Guardrails and sandboxed tool policies
- [ ] 7.11 Build deployment and distribution layer
  - One-command browser build (WASM + assets), PWA/offline cache manifests
  - Multiplayer host profile presets (single-player, small coop, scale host)

### Phase 7 Milestone A (Immediate next iteration)
- [x] 7.12 Resolve 2.5D edge-case matrix (cycle-safe parenting, mixed-layer ordering, parented projection fallback)
- [x] 7.13 Finalize mixed-mode API contract (`RenderItem`, `DrawType`, depth key rules) and add docs
- [x] 7.14 Add scene-level transform provenance metadata for editor debugging
- [ ] 7.15 Add browser render compatibility checks in CI (headless wgpu + feature checks)
- [ ] 7.16 Draft public platform docs: architecture, plugin model, agent integration contract

## Phase 8: Shipping Parity
- [ ] 8.1 Create importers and authoring workflows for major Unity/Godot style assets
- [ ] 8.2 Add gameplay sample suite (2D, 2.5D, 3D, isometric, FPS, top-down, RPG)
- [ ] 8.3 Add multiplayer reliability toolkit (interpolation, rewind, rollback, catch-up recovery)
- [ ] 8.4 Add plugin/extension ecosystem and versioned SDK API surface
- [ ] 8.5 Publish benchmark suite and baseline targets (latency, frame-time, memory)

## Phase 9: Bevy / Unity-Class Parity Hardening
- [ ] 9.1 Implement plugin and app lifecycle system equivalent to Bevy `App`/`Plugin` hooks
  - Startup/first-frame ordering, plugin registration order, schedule phase hooks
- [ ] 9.2 Implement full schedule-driven ECS world graph
  - Deterministic system stages, resource staging, explicit change ticks
- [ ] 9.3 Add complete input stack
  - Keyboard/mouse/gamepad/touch abstraction, rebinding, deadzone/curve support, action maps
- [ ] 9.4 Add first-class UI runtime
  - 2D UI layout primitives, text rendering pipeline, focus/input routing, screen-space anchoring
- [ ] 9.5 Complete audio and spatial sound system
  - Bus hierarchy, effects stack, occlusion, streaming and voice channeling
- [ ] 9.6 Complete asset and import parity
  - Texture atlas/array support, compression profiles, import validation, dependency graph
- [ ] 9.7 Add reflection/introspection tooling
  - Serializable type registry, debug scene inspector, script/editor-safe field metadata
- [ ] 9.8 Add deterministic runtime safety
  - Rollback determinism tests, replay validation, seed audit logs, anti-rollback drift checks
- [ ] 9.9 Build world tooling parity
  - Scene streaming, prefab diff/merge, prefab inheritance, editor timeline + prefab overrides
- [ ] 9.10 Add AI coauthoring pipeline
  - Prompt-safe scene generation, behavior authoring lint rules, tool calling contracts, policy sandbox
- [ ] 9.11 Add platform shipping parity
  - Hot reload in editor, CI smoke for web/native, release profile reproducibility, migration docs

### Iteration 29 — Deterministic Core + Runtime Integration Follow-through

- [x] Fixed `pod-scripting` VM/sandbox compilation against current `mlua` API (`Table::get`, `Function::call`, chunk environment wiring).
- [x] Added deterministic `pod-core` tick tests for:
  - `AttackTarget` damage + cooldown,
  - self-target rejection,
  - invulnerability no-op behavior,
  - observation population of `cooldowns`, `messages`, and `objectives`.
- [x] Fixed stale `AgentId` test construction in `pod-core/src/action.rs` (UUID-backed ids).
- [x] Updated legacy `pod-core` test expectations in `constraint` and `orchestrator` to align with implemented budget/reaction/priority formulas.
- [x] Hardened `apps/pod-server` network-mode error conversion to `Send + Sync` compatible errors.
- [x] Validated touched crates:
  - `cargo check -p pod-core -p pod-stdb -p pod-net -p pod-scripting --offline`
  - `cargo test -p pod-core --offline`
  - `cargo test -p pod-scripting --offline`
  - `cargo test -p pod-net --offline`

### Iteration 30 — Native Scene/Prefab Binding Pass

- [x] Added `pod-scene` native component bindings for existing `pod-core` gameplay/render types spanning 2D (`Transform`, `Sprite`, `ColorRect`), 2.5D (`Transform3D` + `Sprite`), and 3D (`Mesh`, `Material`, `Camera3D`, `Light`) while preserving JSON fallback for editor-only component data.
- [x] Converted scene entity component payloads into typed component maps and implemented direct scene-to-`pod_core::World` instantiation, prefab-backed entity resolution, stable scene-entity spawn mapping, and 3D parent graph linkage via `Parent3D`.
- [x] Hardened prefab property override application to support both object paths and vector-style array axes (`x`, `y`, `z`, `w` or numeric indices), so authored overrides work against `glam`-serialized transform fields.
- [x] Added deterministic `pod-scene` tests covering typed prefab round-trips, prefab spawning with native components, override application, ignored editor-only scene metadata, and mixed 2D/2.5D/3D scene instantiation.
- [x] Validated touched crate:
  - `cargo check -p pod-scene`
  - `cargo test -p pod-scene --lib`

### Iteration 31 — Prefab Inheritance + Diff/Merge Pass

- [x] Added prefab inheritance to `pod-scene` via optional `base_prefab` references with cycle-safe recursive resolution in `PrefabRegistry`.
- [x] Updated prefab spawning and scene prefab resolution to use fully resolved prefab state, so inherited native components participate in world instantiation for 2D, 2.5D, and 3D entities.
- [x] Added `PrefabDiff` and `PrefabMetadataDiff` to support authoring-time prefab patch generation and replay (`diff_against` / `apply_diff`) for component add/change/remove and nested-prefab merge behavior.
- [x] Added deterministic tests covering inheritance resolution precedence, inheritance-cycle rejection, resolved spawning, diff/apply round-trips, and scene instantiation through inherited prefabs.
- [x] Validated touched crate:
  - `cargo check -p pod-scene`
  - `cargo test -p pod-scene --lib`

### Iteration 32 — Scene Entity Reference Binding Pass

- [x] Added authored entity reference bindings to `pod-scene` scene entities with stable selectors by scene entity UUID or unique scene entity name.
- [x] Resolved entity references during scene instantiation against the pre-spawned scene entity map, so native components can bind deterministic runtime entity ids for fields like `FollowCameraController.target` and `Parent3D.parent`.
- [x] Reused the prefab JSON path assignment logic as shared component-path mutation infrastructure for both prefab property overrides and scene entity reference application.
- [x] Added deterministic tests covering direct-id entity references, prefab-backed named references, missing-target rejection, and ambiguous-name rejection.
- [x] Validated touched crate:
  - `cargo check -p pod-scene`
  - `cargo test -p pod-scene --lib`

### Iteration 33 — Scene Streaming Foundation Pass

- [x] Added opt-in scene streaming regions with authored 3D bounds, always-loaded support, and deterministic focus-based activation planning for large-world scene partitioning.
- [x] Implemented stream-plan dependency expansion so active entities automatically retain required parent graph ancestors and authored entity-reference targets across region boundaries.
- [x] Added partial scene instantiation for streaming windows, allowing `pod-scene` to spawn only the active subset of a scene while preserving unassigned always-on entities and stable runtime spawn mapping.
- [x] Added deterministic tests covering region selection, dependency closure, partial streamed instantiation, and invalid region membership rejection.
- [x] Validated touched crate:
  - `cargo check -p pod-scene`
  - `cargo test -p pod-scene --lib`

### Iteration 34 — Prefab Override Tracking Pass

- [x] Added structured prefab override reporting in `pod-scene` so prefab resolution now returns applied and ignored override records with previous values for editor/debug inspection.
- [x] Added authored `prefab_overrides` on scene entities, applied during prefab-backed scene instantiation before full local component replacement semantics.
- [x] Surfaced per-entity prefab override reports in `SceneSpawnResult`, and rejected invalid authoring cases such as prefab overrides on entities with no prefab source.
- [x] Hardened override path mutation to reject incompatible JSON shape rewrites instead of silently corrupting typed component payloads.
- [x] Added deterministic tests covering prefab-level override reports, scene-level prefab override application/reporting, local component precedence, and invalid prefab-override usage.
- [x] Validated touched crate:
  - `cargo check -p pod-scene`
  - `cargo test -p pod-scene --lib`

**Last updated**: Iteration 34
**Current focus**: Phase 7.5 scene-system parity and Phase 9 hardening backlog
