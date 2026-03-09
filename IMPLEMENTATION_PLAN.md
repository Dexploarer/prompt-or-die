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
- [x] 7.15 Add browser render compatibility checks in CI (headless wgpu + feature checks)
- [x] 7.16 Draft public platform docs: architecture, plugin model, agent integration contract

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

### Iteration 35 — Scene/Prefab Provenance Reporting Pass

- [x] Added component provenance layers in `pod-scene` so resolved prefab components can explain which prefab definitions, property overrides, scene-authored components, and entity-reference bindings contributed to final runtime state.
- [x] Extended prefab inheritance resolution with provenance-aware component assembly, preserving full source chains across base/derived prefab merges before overrides are applied.
- [x] Surfaced per-entity component provenance maps in `SceneSpawnResult`, giving editor/debug tooling stable insight into final component origin across prefab inheritance, scene overrides, and entity-reference resolution.
- [x] Added deterministic tests covering inherited prefab provenance, scene-local override provenance precedence, and entity-reference provenance tracking.
- [x] Validated touched crate:
  - `cargo check -p pod-scene`
  - `cargo test -p pod-scene --lib`

### Iteration 36 — Browser Render Compatibility CI Pass

- [x] Added a dedicated GitHub Actions browser-render job that installs `wasm32-unknown-unknown`, runs `pod-render` library tests, and checks `pod-render` against the wasm target.
- [x] Added a headless `wgpu` adapter/device smoke test so CI exercises a real native render backend path without needing a window server.
- [x] Fixed wasm browser compatibility in the render dependency graph by enabling `getrandom`'s JS path for `pod-core` on `wasm32`, unblocking `rand`/`rand_chacha` usage during browser builds.
- [x] Exposed the web render bridge during native test builds and added mixed-mode bridge tests so browser serialization for 2D, 2.5D, and 3D render items is exercised in CI.
- [x] Validated touched targets:
  - `cargo test -p pod-render --lib`
  - `cargo check -p pod-render --target wasm32-unknown-unknown`

### Iteration 37 — Public Platform Docs Draft

- [x] Added a public `README.md` that explains the project, workspace layout, and entry-point documentation for external readers.
- [x] Added `docs/architecture.md` describing the current crate boundaries, data flow, runtime authority model, and subsystem responsibilities.
- [x] Added `docs/plugin-model.md` documenting the extension surfaces that exist today and clearly separating them from the still-unfinished formal plugin lifecycle work.
- [x] Added `docs/agent-integration-contract.md` defining the shared agent pipeline, trait contract, transport invariants, and integration rules for human and AI controllers.
- [x] Validated touched files:
  - `git diff --check`

### Iteration 38 — Godot Scene Importer Foundation

- [x] Extended `pod-assets` with a real Godot `.tscn` import path that recognizes authored scene files, parses a focused subset of 2D node/resource syntax, and converts it into `pod_scene::Scene`.
- [x] Serialized imported Godot scene content into content-addressed scene artifacts so the asset cache now produces a stable authored-scene output instead of metadata alone.
- [x] Mapped common 2D Godot authoring data onto existing native POD bindings (`Transform`, `Sprite`, `ColorRect`) while preserving scene hierarchy through `SceneGraph` parent links.
- [x] Extended `pod-editor` asset browser classification with first-class scene assets so imported Godot/Unity-style scene files no longer fall into `Other`.
- [x] Validated touched crates:
  - `cargo test -p pod-assets`
  - `cargo test -p pod-editor --lib`

### Iteration 39 — Tiled Scene Importer Foundation

- [x] Extended `pod-assets` with a real Tiled JSON (`.tmj`) import path so Tiled-authored 2D worlds now convert into `pod_scene::Scene` instead of being rejected as unsupported.
- [x] Mapped Tiled tile layers and object layers onto existing POD bindings (`Transform`, `Sprite`, `ColorRect`) while preserving imported layer/object hierarchy through `SceneGraph`.
- [x] Reused the existing content-addressed scene artifact output so `.tmj` imports produce serialized scene artifacts beside Godot imports under the same `scene/` cache prefix.
- [x] Added deterministic tests for direct `.tmj` import, serialized artifact output, and editor-side `.tmj` scene asset recognition.
- [x] Validated touched crates:
  - `cargo test -p pod-assets`
  - `cargo test -p pod-editor --lib`

### Iteration 40 — Unity Scene/Prefab Importer Foundation

- [x] Extended `pod-assets` with a real Unity text scene/prefab (`.unity`, `.prefab`) import path that converts a focused authoring subset into `pod_scene::Scene`.
- [x] Mapped Unity `GameObject`, `Transform`/`RectTransform`, and `SpriteRenderer` authoring data onto existing POD bindings (`Transform`, `Transform3D`, `Sprite`) while preserving parent hierarchy through `SceneGraph`.
- [x] Reused the existing content-addressed scene artifact output so Unity-authored scene imports serialize into the same deterministic `scene/` cache path as Godot and Tiled imports.
- [x] Added deterministic tests for direct Unity import, serialized artifact output, and editor-side prefab scene asset recognition.
- [x] Validated touched crates:
  - `cargo test -p pod-assets`
  - `cargo test -p pod-editor --lib`

### Iteration 41 — Unity GUID Asset Resolution

- [x] Extended the Unity importer to scan local `.meta` files and resolve sprite GUID references back to deterministic asset-relative texture paths during scene/prefab import.
- [x] Preserved the previous `unity-guid:*` fallback when the importer cannot find a matching `.meta` asset mapping, so incomplete Unity exports still import instead of failing hard.
- [x] Added deterministic tests covering resolved GUID-to-path imports, serialized prefab artifact output with resolved textures, and fallback behavior when metadata is missing.
- [x] Validated touched crate:
  - `cargo test -p pod-assets`

### Iteration 42 — Three.js WebGPU Browser Bridge

- [x] Extended `pod-render`'s browser bridge with a Three.js/WebGPU-oriented frame payload that batches 3D meshes and billboard sprites by GPU-relevant asset keys for instancing-friendly frontend consumption.
- [x] Preserved the legacy per-item browser payload while adding full 3D transform, billboard, and shadow metadata so JS frontends no longer lose critical world-space information.
- [x] Re-exposed the browser bridge in native test builds and added deterministic tests covering Three.js batch formation, overlay separation, and full 3D payload serialization.
- [x] Validated touched targets:
  - `cargo test -p pod-render --lib`
  - `cargo check -p pod-render --target wasm32-unknown-unknown`

### Iteration 43 — Three.js WebGPU Phase-Aware Material Batching

- [x] Extended `pod-render`'s 3D draw contract to preserve material-surface state (`tint`, `roughness`, `metallic`, `emissive`, `double_sided`) instead of dropping it during render extraction.
- [x] Updated the Three.js/WebGPU browser bridge to classify 3D mesh and sprite batches as opaque or transparent, expose depth-write guidance, and keep incompatible material variants out of the same instanced batch.
- [x] Added deterministic tests covering material metadata preservation, opaque vs transparent batch splitting, and the resulting WebGPU phase hints.
- [x] Validated touched targets:
  - `cargo test -p pod-render --lib`
  - `cargo check -p pod-render --target wasm32-unknown-unknown`

### Iteration 44 — Three.js WebGPU Transparent Sort-Depth Batching

- [x] Updated `pod-render`'s Three.js/WebGPU bridge to avoid cross-depth transparent instancing by splitting transparent mesh and sprite batches on shared world-`z` sort depth.
- [x] Added explicit batch ordering metadata (`sort_depth`, `render_order`) plus precise runtime hints (`sort_metric`, transparent instancing strategy) so Three.js consumers can map batches directly onto `renderOrder` without inferring engine semantics.
- [x] Added deterministic tests covering transparent mesh and sprite depth splitting plus the emitted batch ordering metadata.
- [x] Validated touched targets:
  - `cargo test -p pod-render --lib`
  - `cargo check -p pod-render --target wasm32-unknown-unknown`

### Iteration 45 — Browser Three.js Client Baseline

- [x] Added `apps/pod-web`, a real browser-side Three.js client that consumes `pod-render` frame payloads instead of leaving the WebGPU contract as a Rust-only abstraction.
- [x] Implemented `three/webgpu` initialization with automatic WebGL2 fallback, instanced 3D mesh batching, billboard sprite batching, and a 2D overlay pass for legacy `RenderFrame` content.
- [x] Added deterministic TypeScript tests for camera rig mapping, transparent sprite tint splitting, and batch ordering preservation, plus a runnable demo bridge exposed through `window.podRender`.
- [x] Documented the new browser client in the root README, architecture docs, and app-level README.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run build`

### Iteration 46 — Browser Three.js Quality Scaling and LOD Pass

- [x] Added hardware-aware quality profiles to `apps/pod-web` with adaptive resolution, anisotropy, shadow budgets, environment intensity, and explicit ultra/high/balanced/performance presets.
- [x] Added CPU-side distance/frustum culling plus LOD-aware mesh instancing so the browser client can keep large authored worlds efficient without giving up high-detail near-field visuals.
- [x] Improved the renderer baseline with ACES tone mapping, upgraded lighting/shadow defaults, cached materials, richer atmospheric dressing, correct overlay compositing, and runtime HUD stats.
- [x] Added deterministic TypeScript coverage for LOD split/cull planning and quality profile selection, then verified the live browser client on `http://127.0.0.1:4173/` with WebGPU active.
- [x] Suppressed the repeated upstream `THREE.TSL` inline-`Fn()` warning spam down to a single forwarded warning so the runtime console stays useful.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`

### Iteration 47 — Runtime Kernel MMO Contract Foundation

- [x] Added a first-class `App` / `Plugin` / schedule kernel in `pod-core` with deterministic startup, fixed-tick update, render-prep, broadcast phases, typed resources, and a shared type registry for core components and contracts.
- [x] Added explicit versioned runtime contracts in `pod-core` with `RuntimeContractVersion`, `AgentRuntimeProfile`, `AgentRole`, and `AgentCapabilities` so human clients, local AI, and remote AI can share one auditable parity envelope.
- [x] Added MMO-native POD primitives for the flagship world direction: `CombatLoadout`, `SkillBook`, `Inventory`, `CreatureIdentity`, `CompanionRoster`, and `EncounterState`, designed around RuneScape-style progression/combat with collectible creature companions.
- [x] Extended agent observations and default world spawns so runtime profiles, combat style, skills, inventory, companions, and encounter state are visible through the same observation pipeline used by both humans and AI.
- [x] Added deterministic `pod-core` tests covering schedule ordering, versioned contracts, and MMO-state observation delivery.
- [x] Validated touched crate:
  - `cargo check -p pod-core`
  - `cargo test -p pod-core --lib`

### Iteration 48 — MMO Gameplay Verb and Transport Parity

- [x] Added RuneScape-style MMO gameplay verbs to `pod-core` with strict validation for capture, summon, companion commands, gathering, looting, and auto-retaliate toggling under the shared human/AI action contract.
- [x] Added supporting native runtime primitives `ResourceNode` and `LootContainer`, plus deterministic tick execution for companion capture/summon flows, skill XP gains, loot transfer, and combat cadence driven by `CombatLoadout`.
- [x] Extended the agent parser and network/SpacetimeDB transport layers so the new MMO verbs survive parsing, serialization, authority mirroring, and local simulated reducer paths without being dropped.
- [x] Expanded deterministic coverage in `pod-core` for loadout-driven combat range/damage/cooldowns, capture, summon, companion attack commands, gathering, looting, and auto-retaliate state changes.
- [x] Updated agent-side observation fixtures to stay compatible with the expanded MMO observation schema (`combat_style`, `creature`, and extended `SelfState` fields).
- [x] Validated touched crates:
  - `cargo check -p pod-core -p pod-agents -p pod-net -p pod-stdb`
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-agents --lib`
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-stdb --lib`

### Iteration 49 — Shared Simulation Reconciliation Foundation

- [x] Extended `pod-net` snapshots with deterministic authoritative digests, explicit full-snapshot recovery markers, and replayable predicted action batches so direct-connect clients can detect divergence and rebuild from a known-good baseline.
- [x] Updated the direct-connect wire contract to include controlled-entity assignment on `Welcome` plus per-client acknowledged action ticks on authoritative state updates.
- [x] Split native and web direct-connect clients into authoritative vs predicted snapshot tracking, preserving unacknowledged local action batches and replaying them after authoritative correction instead of patching deltas directly onto predicted state.
- [x] Added deterministic `pod-net` tests for stable snapshot digests, predicted movement replay, and full-snapshot/baseline handling during authoritative update application.
- [x] Validated touched crate:
  - `cargo test -p pod-net --lib`

### Iteration 50 — Pod-Net Build Hygiene and Target Split

- [x] Moved `pod-net`'s native transport dependencies (`tokio`, `quinn`, and Criterion benches) behind non-wasm target gates so the browser client no longer pulls native-only networking code into `wasm32` builds.
- [x] Enabled the specific `web-sys` DOM bindings used by `client_web.rs` (`Window`, `Location`, `Event`, and `console`) so the browser transport compiles under the workspace's wasm target again.
- [x] Fixed the SpacetimeDB snapshot conversion path to populate `EntitySnapshot.movement_speed` from cached authority state and added deterministic coverage for that mapping.
- [x] Moved the workspace's default Cargo target output to `.cargo-target/` and ignored it so normal checks stop inheriting stale tracked artifacts from the repository root `target/` tree.
- [x] Validated touched targets:
  - `cargo check -p pod-net --lib`
  - `cargo check -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`

### Iteration 51 — Root Target De-Tracking Cleanup

- [x] Backed up the unrelated non-`target/` working-tree source edits to `/Users/home/Desktop/prompt-or-die-backups/2026-03-07/unrelated-source-changes.patch` before cleaning the checkout for repo hygiene work.
- [x] Added `/target/` to the root `.gitignore` so the legacy workspace build tree stops appearing in status once it is removed from version control.
- [x] Removed the repository root `target/` tree from Git tracking, eliminating tens of thousands of generated build artifacts from the index while keeping the workspace on `.cargo-target/` for future builds.
- [x] Validated repo hygiene state:
  - `git diff --check`
  - `git ls-files target`
  - `git status --short`

### Iteration 52 — Pod-Net Presentation Interpolation and Catch-Up Pass

- [x] Added reusable `pod-net` snapshot interpolation primitives: bounded authoritative history, render-time drift correction, exact/interpolated/extrapolated sample modes, and local-player presentation overlay.
- [x] Integrated the presentation pipeline into native QUIC, browser WebSocket, and SpacetimeDB clients so each adapter now exposes a smoothed `presentation_snapshot()` and `presentation_tick()` API on top of the existing authoritative/predicted state split.
- [x] Preserved local input responsiveness by overlaying the currently predicted controlled entity onto interpolated authoritative snapshots while keeping remote entities on the smoothed authoritative timeline.
- [x] Added deterministic coverage for interpolation replacement, shared-entity lerping, bounded extrapolation, render-clock catch-up snapping, local-player overlay composition, and SpacetimeDB adapter presentation sampling.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`

### Iteration 53 — Pod-Net Rollback Preview and Catch-Up Diagnostics

- [x] Added reusable rollback/rewind inspection primitives to `pod-net`: authoritative history rewind, `RollbackPreview`, `EntityDrift`, and `CatchUpDiagnostics` built directly from retained snapshot history plus pending predicted input batches.
- [x] Extended the snapshot/render clock layer with history window introspection and delayed-target drift reporting so prediction recovery can be inspected without widening the wire protocol.
- [x] Exposed rollback preview, authoritative rewind, and catch-up diagnostics through native QUIC, browser WebSocket, and SpacetimeDB client adapters.
- [x] Added deterministic coverage for rewind clamping, rollback replay from an arbitrary retained tick, presentation drift diagnostics, native direct-connect diagnostics, and SpacetimeDB rewind accessors.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`

### Iteration 54 — Pod-Net Immediate Resync Recovery

- [x] Added a direct-connect `RequestFullSnapshot` client message so drifted or gapped clients can explicitly request an immediate authoritative full snapshot instead of waiting for the next periodic broadcast.
- [x] Added server-side full-resync response helpers that answer those requests with a full `StateDelta` snapshot including the latest acknowledged action tick for that client.
- [x] Wired native QUIC and browser WebSocket clients to trigger a one-shot recovery request automatically when they detect a snapshot gap or reject an authoritative update due to baseline/digest failure.
- [x] Added deterministic protocol and server coverage for full-snapshot request round-tripping and full-resync message construction.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`

### Iteration 55 — Pod-Net Recovery Retry Telemetry

- [x] Added shared `RecoveryRequestState` to `pod-net` diagnostics so tooling can inspect whether a client is awaiting a full snapshot, how many requests have been issued, and when the next retry becomes eligible.
- [x] Replaced the direct-connect clients' one-bit recovery latch with throttled retry state, allowing native QUIC and browser WebSocket clients to re-request recovery snapshots after a bounded tick interval instead of getting stuck after one failed attempt.
- [x] Threaded recovery telemetry through `CatchUpDiagnostics` for native, web, and SpacetimeDB-facing diagnostics surfaces.
- [x] Added deterministic coverage for retry throttling and recovery telemetry in the snapshot-layer diagnostics tests.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`

### Iteration 56 — Core Agent Telemetry and Trajectory Primitives

- [x] Added `pod-core::telemetry` with authoritative trajectory samples, per-action lifecycle traces, shared tool-call telemetry, tick-scoped agent telemetry frames, and a ring-buffer `TelemetryArchive` resource for tooling/debug overlays.
- [x] Extended `TickResult` so the authoritative observe → decide → validate → execute pipeline emits per-agent telemetry for both human and AI agents, including start/end trajectory samples and submitted/executed/rejected action traces.
- [x] Registered telemetry contracts/resources in `pod-core::App`, automatically persisted the latest tick telemetry into `TelemetryArchive`, and added a versioned `VersionedTickTelemetry` contract primitive alongside existing action/observation wrappers.
- [x] Extended `pod-agents::DecisionEntry` with shared tool-call telemetry primitives so LLM-backed and non-LLM agents can report external side effects through one log schema.
- [x] Added deterministic coverage for trajectory reconstruction, authoritative tick telemetry capture, rejected external-action traces, telemetry archive retention, and decision-log tool-call preservation.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-agents --lib`

### Iteration 57 — One-page App Summary PDF

- [x] Generated a repo-evidence-only one-page summary PDF at `output/pdf/prompt-or-die-summary.pdf`.
- [x] Verified the artifact stays on a single page and remains legible after rendering to PNG for visual QA.

### Iteration 58 — Browser and Editor Telemetry Consumers

- [x] Added `TelemetryConfig` to `pod-core` as the shared retention source for runtime archives, browser debug trails, and editor timelines, and wired `App::new()` to size `TelemetryArchive` from that config.
- [x] Added a typed `pod-web` telemetry contract plus `window.podRender.renderTickTelemetry(...)`, `window.podRender.resetTelemetry()`, and `window.podRender.getTelemetryStats()` for editor/debug clients.
- [x] Added a debug-only browser telemetry HUD with selected-agent cycling, world-space trajectory trails, action and tool-call summaries, and recovery-diagnostic display.
- [x] Added `EditorPanel::Telemetry` and `TelemetryPanelState` to `pod-editor`, reusing the existing selected entity across hierarchy, inspector, viewport, and telemetry views.
- [x] Replaced the old `SpacetimeDashboardState` placeholder counters with authoritative telemetry rollups: latest tick, rejection/error rates, per-agent trajectory summaries, and visible/audible/message counts.
- [x] Applied Three.js toon-material shading to the stylized opaque world-geometry path while keeping transparent/glass surfaces on the standard material path.
- [x] Added deterministic coverage for telemetry config defaults, editor telemetry retention/selection sync, browser telemetry parsing/summary logic, and toon-material selection.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-editor --lib`
  - `bun test`
  - `bun run typecheck`
  - `bun run build`

### Iteration 59 — Authority Telemetry Export and Toon Lookup Correction

- [x] Added opt-in direct-connect telemetry protocol support in `pod-net` with `ClientMessage::SetDebugTelemetry { enabled }` and `ServerMessage::TickTelemetry { frame_json }`.
- [x] Extended the native QUIC and browser WebSocket clients with debug telemetry toggles plus cached access to the most recent authoritative telemetry payload.
- [x] Decoupled debug telemetry delivery from gameplay state-delta emission so debug/editor clients still receive per-tick telemetry on idle world ticks with no replicated state changes.
- [x] Added SpacetimeDB telemetry subscription surfaces, event variants, and adapter bridging so editor/debug consumers can subscribe to `agent_telemetry_tick`, `agent_tool_call_event`, and `agent_tick_rollup`, while `pod-net` emits `TickTelemetry` messages from those rows.
- [x] Added initial SpacetimeDB telemetry table definitions for transient per-agent tick rows, tool/provider event rows, and aggregate rollups.
- [x] Corrected the Three.js toon lookup texture path to keep the gradient map in a non-color data space instead of sRGB color-managed sampling.
- [x] Added deterministic coverage for direct-connect debug telemetry round-tripping, SpacetimeDB telemetry subscription helpers/events, adapter forwarding of telemetry rows, and the toon gradient lookup color-space contract.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo test -p pod-stdb --no-default-features --features client`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`
  - `bun test`
  - `bun run typecheck`

### Iteration 60 — Embedded Tool Contracts, Replay Preservation, and MMO Server Telemetry

- [x] Added first-class embedded tool/runtime contracts to `pod-core`: `ToolDefinition`, `ToolCatalog`, `ToolPolicy`, `ToolBudget`, `ToolInvocationRequest`, and `ToolInvocationResult`, and registered them in the runtime type registry alongside the existing action/observation/telemetry contracts.
- [x] Expanded `ToolCallStatus` with explicit `RateLimited`, `ParseError`, `ApiError`, and `BudgetExceeded` states so agent-side provider telemetry can distinguish transport, provider, parser, and budget failures.
- [x] Added `Agent::drain_tool_calls()` to the shared agent trait and wired authoritative tick execution to record drained tool-call traces into `TickTelemetryFrame`, so the telemetry spine now carries real embedded-agent side effects instead of empty placeholders.
- [x] Instrumented `LlmAgent` and `HybridAgent` provider calls with canonical `llm.complete` tool traces, including success usage units, parse failures, provider failures, and pre-flight budget rejections.
- [x] Fixed `LlmAgent`'s ready-action memory recording path so completed observations are no longer dropped before they can be persisted to conversation memory.
- [x] Extended replay artifacts so `DecisionTrace` preserves `tool_calls`, and added optional embedded `telemetry_windows` support on `ReplayFile` for authoritative telemetry bundling.
- [x] Updated `DecisionLogger::to_replay_file()` so tool-call telemetry survives conversion into replay/debug artifacts.
- [x] Upgraded `apps/pod-server` runtime stats to consume the shared telemetry spine, tracking rejection rate, tool-call error/latency metrics, average agent trajectory distance, tick-budget overruns, and MMO-loop capture/summon/gather/loot counts.
- [x] Added deterministic coverage for tool-contract serialization, agent tool-trace draining into authoritative tick telemetry, provider error-status mapping, LLM/hybrid tool-call instrumentation, replay preservation of tool calls, replay telemetry embedding, and server telemetry rollups.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-agents --lib`
  - `cargo test -p pod-server --bin pod-server`

### Iteration 61 — Replay Training Samples and Encounter-Aware Telemetry

- [x] Extended `AgentTelemetryFrame` with authoritative encounter snapshots so replay/debug artifacts can reason about combat/capture state transitions instead of only pathing and action traces.
- [x] Added replay-side training/export primitives in `pod-core`: `ActionOutcomeSummary`, `EncounterTransition`, and `ReplayTrainingSample`, plus `ReplayFile::training_samples()` for deriving per-agent training rows from embedded telemetry windows.
- [x] Added optional telemetry embedding to `DecisionLogger` via `to_replay_file_with_telemetry(...)` so authoritative tick windows can ride alongside decision traces in replay/debug exports.
- [x] Added a neural-agent helper for extracting agent-specific authoritative training samples from replay artifacts.
- [x] Added deterministic coverage for replay training sample derivation, encounter transition classification, telemetry-aware replay export, neural-agent replay filtering, and the updated editor/server/core telemetry call sites.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-agents --lib`
  - `cargo test -p pod-server --bin pod-server`
  - `cargo test -p pod-editor --lib`

### Iteration 62 — Flagship MMO Acceptance Harness and Parity Validation

- [x] Added a reusable `pod-core::acceptance` module with deterministic flagship MMO scenario configs, shard-scale target metadata, paired human/AI parity reporting, replay export, and authoritative summary metrics.
- [x] Implemented a RuneScape-style acceptance loop covering chat, traversal, combat cadence, capture, summon, companion commands, gather, loot, and embedded tool-call telemetry without leaving the shared observe → decide → validate → execute pipeline.
- [x] Added deterministic parity validation that compares paired human and autonomous agents on observation signatures and decided action schedules while allowing tool-call differences on the AI side.
- [x] Wired `pod-server` validation to consume the acceptance harness and assert that server stats align with authoritative acceptance-summary metrics.
- [x] Added deterministic coverage for shard-target defaults, flagship MMO loop completion, encounter-transition replay exports, and server-stat alignment.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-server --bin pod-server`

### Iteration 63 — Official TOON Agent Prompt/Response Compliance

- [x] Replaced the repo-local pseudo-TOON prompt shape in `pod-agents` with the official TOON format from `https://toonformat.dev/` using the `toon-format` Rust crate.
- [x] Switched `ToonTemplate` to emit spec-compliant TOON documents that round-trip through the official decoder instead of the previous custom block syntax.
- [x] Switched `ToonActionParser` to decode official TOON objects into gameplay actions, preserving the shared structured action payload path used by the JSON parser.
- [x] Updated the LLM-agent integration tests and parser/template coverage to assert actual TOON decoding rather than string-matching the old fake syntax.
- [x] Validated touched targets:
  - `cargo test -p pod-agents --lib`
  - `cargo test -p pod-agents --features agent_sdk_integration_tests`

### Iteration 64 — TOON Replay, Telemetry, World-Building Snapshots, and Ops Summaries

- [x] Added shared TOON document helpers to `pod-core` plus reusable typed export methods for replay, telemetry, tool/runtime contracts, and shard incident summaries.
- [x] Extended `ReplayFile`, `ReplayTrainingSample`, `DecisionTrace`, `TelemetryArchive`, `TickTelemetryFrame`, `AgentTelemetryFrame`, `AgentToolCallTrace`, `ToolInvocationRequest`, `ToolInvocationResult`, and `VersionedTickTelemetry` with TOON export surfaces for debugging and training workflows.
- [x] Extended `pod-agents::DecisionLogger` and `DecisionEntry` with TOON exports, including a dedicated embedded-tool trace export for LLM/runtime side effects.
- [x] Added `ShardIncidentSummary` in `pod-core` and wired `pod-server` stats to emit TOON-ready incident summaries for ops agents with damped severity thresholds.
- [x] Added creator-facing authoring associations in `pod-editor` for models, objects, monsters, and scene entities, with convenience creation APIs and TOON world-snapshot export for world-building agents.
- [x] Switched the editor’s project snapshot action to emit TOON instead of raw JSON so the authoring/debug surface uses the same structured interchange format as agents and ops tooling.
- [x] Added deterministic coverage for TOON document round-tripping across replay/telemetry/contracts, decision-log tool traces, editor world snapshots/creator associations, and server incident summaries.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-agents --lib`
  - `cargo test -p pod-editor --lib`
  - `cargo test -p pod-server --bin pod-server`

### Iteration 65 — TOON Telemetry Transport Parity and Debug Consumer Ingestion

- [x] Added typed TOON decode helpers in `pod-core` so downstream consumers can validate document kinds and extract payloads without reimplementing TOON envelope parsing.
- [x] Switched direct-connect debug telemetry transport in `pod-net` from the legacy JSON wrapper to the versioned TOON telemetry contract while keeping gameplay-state traffic unchanged.
- [x] Updated `pod-net` and `pod-stdb` debug telemetry tests/fixtures to use official TOON documents, and added neutral `last_debug_telemetry_document()` accessors without breaking existing string-based callers.
- [x] Added official TOON parsing to `apps/pod-web` using the TypeScript TOON package, extending the existing browser contract layer to consume TOON tick telemetry, replay files, and shard incident summaries alongside JSON fallback.
- [x] Added browser-side replay and incident summary consumption APIs plus HUD surfacing so the flagship WebGPU client can ingest TOON replay/debug artifacts directly.
- [x] Extended `pod-editor` to import replay and shard incident TOON documents, wiring replay telemetry windows into the existing telemetry panel/dashboard and incident summaries into the operational dashboard state.
- [x] Expanded the editor dashboard with average tool latency, average trajectory distance, MMO-loop action counts, and retained incident summaries so creator tooling stays connected to runtime/ops artifacts.
- [x] Added deterministic coverage for TOON telemetry round-tripping in `pod-net`, SpacetimeDB client/debug events using TOON documents, browser TOON parsing/summaries, and editor replay/incident ingestion.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo test -p pod-stdb --no-default-features --features client`
  - `cargo test -p pod-editor --lib`
  - `bun test`
  - `bun run typecheck`
  - `bun run build`

### Iteration 66
- [x] Added canonical TOON document types in `pod-core` for `agent_tool_call_event` and `agent_tick_rollup`, including rollup derivation helpers so debug consumers share one schema across core, browser, editor, and SpacetimeDB surfaces.
- [x] Extended `pod-net::client_stdb` with a live debug-document queue, retaining TOON telemetry/tool-call/rollup payloads plus injected replay/incident documents for shard debug operations without widening the gameplay message path.
- [x] Extended `apps/pod-web` with live TOON debug-document routing, tool-call/rollup parsing and summaries, and explicit replay/incident streaming hooks for debug overlays.
- [x] Extended `pod-editor` with TOON imports for tool-call events, tick rollups, and generic live debug documents, keeping dashboard/telemetry state aligned with replay and incident ingestion.
- [x] Added deterministic coverage for core TOON telemetry exports, SpacetimeDB debug-document retention, browser live debug routing, and editor live TOON imports.

### Iteration 67
- [x] Extended the SpacetimeDB reducer path to emit authoritative `agent_telemetry_tick` rows as real TOON `versioned_tick_telemetry` documents instead of leaving the debug tables empty.
- [x] Added bounded retention pruning for live telemetry, tool-call, and rollup tables in the SpacetimeDB reducer path so debug subscriptions stay bounded over long shard lifetimes.
- [x] Added authoritative 60-tick `agent_tick_rollup` production in `pod-stdb`, deriving rollups from the retained per-agent telemetry documents already written into the shard tables.
- [x] Added a `ShardOpsDebugStream` in `apps/pod-server` so the in-memory authoritative server loop can emit TOON telemetry, tool-call events, rollups, and incident summaries through one live debug document surface.
- [x] Added deterministic server coverage for the live shard debug stream and validated the SpacetimeDB reducer module on `wasm32`.
- [x] Validated touched targets:
  - `cargo check -p pod-stdb --no-default-features --features module --target wasm32-unknown-unknown`
  - `cargo test -p pod-server --bin pod-server`
  - `cargo test -p pod-net --features spacetimedb --lib`

### Iteration 68
- [x] Added a generic `ServerMessage::DebugDocument` TOON transport in `pod-net` so direct-connect debug consumers can receive authoritative telemetry/tool/rollup documents without inventing another JSON-only side channel.
- [x] Updated native, web, and SpacetimeDB pod-net clients to retain and drain live TOON debug documents while preserving the existing tick-telemetry convenience accessors for tooling that still wants the latest authoritative telemetry frame.
- [x] Extended the direct-connect `GameServer` debug path to retain authoritative telemetry history, emit live tool-call events from embedded agents, and derive 60-tick rollups for debug subscribers over the same live document surface.
- [x] Closed the remaining SpacetimeDB runtime gap by persisting reducer-side embedded tool-call telemetry into `agent_tool_call_event` rows as canonical TOON `agent_tool_call_event` documents.
- [x] Added deterministic coverage for the new protocol message, native/debug client queues, direct-connect server debug broadcasting, and reducer-side tool-call TOON event generation.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo test -p pod-stdb --no-default-features --features client`
  - `cargo check -p pod-stdb --no-default-features --features module --target wasm32-unknown-unknown`

### Iteration 69
- [x] Implemented the missing direct-connect WebSocket server runtime in `pod-net::GameServer`, including browser-safe JSON message decode/encode on top of the existing authoritative world tick loop.
- [x] Reused the same direct-connect session plumbing for QUIC and WebSocket clients so browser fallback clients receive `Welcome`, `StateDelta`, and live TOON `DebugDocument` messages over one authoritative runtime path.
- [x] Wired `apps/pod-server` network mode to expose the WebSocket fallback by default in network runtime, with `POD_ENABLE_WEBSOCKET` and `POD_WEBSOCKET_PORT` overrides plus banner/runtime visibility for browser-first deployment.
- [x] Added deterministic integration coverage for real `ws://` handshake, `Connect -> Welcome`, and debug-telemetry subscription delivery over the WebSocket fallback path.
- [x] Added dedicated `pod-server` runtime coverage for bind parsing and default WebSocket port derivation from the bind address.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`
  - `cargo test -p pod-server --bin pod-server`
  - `git diff --check`

### Iteration 70
- [x] Added typed direct-connect protocol parsing in `apps/pod-web` for authoritative `Welcome`, `StateDelta`, `TickTelemetry`, `DebugDocument`, `Rejected`, and `Pong` websocket payloads emitted by the Rust server.
- [x] Added browser-side authoritative snapshot handling in `apps/pod-web`, including full-snapshot recovery requests, reconnect-token aware websocket reconnects, and opt-in live debug-telemetry subscriptions on the same direct-connect socket.
- [x] Added live world-to-frame adaptation in `apps/pod-web` so authoritative world snapshots and deltas render as stylized WebGPU `ThreeJsWebGpuFrame` content instead of leaving the browser client in demo-only mode.
- [x] Wired the existing browser telemetry overlay to live authoritative websocket debug documents, and surfaced connection/world status directly in the HUD for browser-first shard inspection.
- [x] Added deterministic Bun coverage for browser-side direct-connect config parsing, Rust enum-tagged message parsing, authoritative delta application, browser frame generation, and websocket client message encoding.
- [x] Verified the browser runtime against a real live `pod-server` session over `ws://127.0.0.1:7778`, confirming authoritative rendering and live TOON telemetry ingestion in the browser HUD.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - live browser smoke via Playwright against `cargo run -p pod-server --bin pod-server`
  - `git diff --check`

### Iteration 71
- [x] Extended `apps/pod-web` with browser-side action encoding for the shared `pod-core::Action` contract so human browser input emits the same Rust enum-tagged `ActionBatch` payloads the authoritative server already validates for AI agents.
- [x] Extended the direct-connect browser websocket client with action submission on top of authoritative tick state, keeping reconnect-token recovery and debug-telemetry behavior intact while enabling browser-side human interaction.
- [x] Added a minimal MMO control surface to `pod-web`: keyboard movement, target cycling, combat/interact/gather/loot/capture triggers, companion summon/follow commands, auto-retaliate toggle, and shard chat submission.
- [x] Surfaced target selection and browser action controls directly in the browser HUD so creators can inspect and drive the live authoritative shard without dropping back to a native client or debug console.
- [x] Added deterministic Bun coverage proving the browser websocket client sends a real Rust-shaped `ActionBatch` after `Welcome`, alongside the existing contract tests for direct-connect message parsing and world-frame generation.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 72
- [x] Extended the authoritative direct-connect `GameServer` runtime to broadcast real `EventBatch` gameplay events alongside `StateDelta` and TOON debug documents, instead of leaving browser clients with input but no server-confirmed outcomes.
- [x] Added typed gameplay-event parsing in `apps/pod-web` for websocket `EventBatch` payloads, including compact summaries and involved entity ids for combat, chat, capture, summon, gather, loot, and lifecycle events.
- [x] Wired the browser direct-connect client and HUD to authoritative event feedback so the live shard now surfaces recent MMO outcomes and target-relevant status directly in the client instead of only in telemetry/debug overlays.
- [x] Added deterministic coverage for authoritative event-batch parsing, browser websocket event routing, and direct-connect server event broadcasting.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 73
- [x] Extended the browser direct-connect runtime with authoritative action staging so submitted `ActionBatch` payloads now track pending, acknowledged, and rejected states instead of disappearing into the websocket transport.
- [x] Reclassified post-connect `Rejected` messages as action outcomes for the live shard path, keeping the browser session connected while surfacing the authoritative rejection reason to the player/creator HUD.
- [x] Added a gameplay HUD action-status row in `apps/pod-web` driven by `acknowledged_action_tick` and rejection responses, alongside deterministic websocket coverage for pending -> acknowledged -> rejected transitions.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 74
- [x] Added pure, testable browser target-affordance helpers so the selected target now exposes compact summary and suggested-action hints grounded in live shard state.
- [x] Surfaced selected-target affordances and better local feedback in the gameplay HUD, including immediate client-side guidance for missing targets, empty chat submits, and unavailable direct-connect submission paths.
- [x] Added deterministic Bun coverage for target summary formatting and interaction-hint classification across creature and loot targets.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 75
- [x] Expanded authoritative `pod-net` snapshots with gameplay metadata (`EntityKind`, interaction hints, team/combat/species/resource/encounter fields) so browser and creator tooling can consume shard-side meaning instead of guessing from labels.
- [x] Fixed a deeper snapshot coverage gap by capturing all transformed entities, including non-moving resource nodes, loot containers, and static world props that previously fell out of direct-connect snapshots.
- [x] Upgraded `apps/pod-web` target summaries, affordances, target filtering, and render profile selection to use authoritative metadata first while preserving legacy label fallbacks for older payloads.
- [x] Added deterministic Rust and Bun coverage for static-entity snapshot capture, metadata parsing, and metadata-driven browser affordances/render planning.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cargo fmt --all`
  - `git diff --check`

### Iteration 76
- [x] Added a manifest-driven asset registry in `apps/pod-web` that can resolve semantic creator ids and aliases into real browser assets while keeping procedural fallback geometry/textures as a safe default path.
- [x] Wired `PodThreeWorldRenderer.create(...)` to initialize real runtime loaders (`GLTFLoader`, `MeshoptDecoder`, `KTX2Loader`) after renderer creation, so browser clients can load shipped mesh assets without breaking the existing instanced render contract.
- [x] Added reproducible sample browser assets under `apps/pod-web/public/assets` plus a `bun run sync:assets` workflow that regenerates sample glTFs, semantic asset associations, SVG textures, and bundled Basis transcoders from the local Three.js install.
- [x] Hardened `pod-web` for future worker promotion by removing DOM-only renderer assumptions where possible and allowing the renderer surface to accept `OffscreenCanvas` without a second renderer implementation.
- [x] Added deterministic Bun coverage for manifest parsing, semantic asset lookup, manifest-backed geometry loading, sprite texture fallback, and compressed-texture preference.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun run sync:assets`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 77
- [x] Added parallel asset prewarming to `PodThreeWorldRenderer` so unique mesh and sprite assets resolve concurrently before the instanced sync path instead of serializing one loader await at a time through the render loop.
- [x] Added manifest-registry residency metrics (`resident` / `pending` geometry and sprite assets) and surfaced them in the browser HUD stats row for creator-facing runtime inspection.
- [x] Added deterministic Bun coverage for deduplicated mesh prefetch and residency accounting in the manifest-backed asset registry.
- [x] Updated `apps/pod-web` Vite config to pre-optimize the dynamic Three.js loader paths and assign stable chunk names for the heavy runtime/vendor modules, reducing main-entry bundle pressure and eliminating the dev-session dependency waterfall after first loader use.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - live browser smoke via Playwright against `bun run dev --host 127.0.0.1 --port 4173`
  - `git diff --check`

### Iteration 78
- [x] Added a real `OffscreenCanvas` render-worker runtime in `apps/pod-web`, with a dedicated worker module that owns `PodThreeWorldRenderer` off the main thread while keeping the HUD, input, and authoritative socket flow on the main thread.
- [x] Added a creator-facing render-thread preference (`?renderThread=worker` / `?renderThread=main`) and a safe default of main-thread rendering until broader worker compatibility hardening is complete.
- [x] Added worker-side render-command coalescing plus live worker stats propagation so the HUD can report `main` vs `worker` thread execution without blocking the gameplay/bootstrap path.
- [x] Added deterministic Bun coverage for render-thread preference parsing and worker-capability gating.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - live browser smoke via Playwright against `bun run dev --host 127.0.0.1 --port 4173?renderThread=worker`
  - `git diff --check`

### Iteration 79
- [x] Fixed worker-mode render softness by syncing logical viewport size and device pixel ratio from the main thread into the `OffscreenCanvas` runtime instead of letting the worker fall back to a `1.00x` backing store.
- [x] Added worker-side resize propagation so creator resizing and high-DPI displays keep the off-thread renderer visually aligned with the main-thread path.
- [x] Added deterministic Bun coverage for worker surface-metric measurement.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/assets.test.ts ./src/render-runtime.test.ts ./src/contracts.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - live browser smoke via Playwright against `bun run dev --host 127.0.0.1 --port 4174?renderThread=worker`
  - `git diff --check`

### Iteration 80
- [x] Replaced the minimal local browser sandbox with a richer `Verdant Hollow` authored test biome that includes a safe player spawn, multiple hub NPCs, multiple wild creatures, mining and woodcutting routes, loot caches, and denser landmark scenery for MMO-style browser validation.
- [x] Expanded `apps/pod-web` render-profile mapping so scenery labels such as `glass spire`, `canopy tree`, and `basalt pillar` resolve to shipped manifest-backed assets instead of falling back to generic props.
- [x] Added deterministic Bun coverage for the richer local sandbox layout and the new scenery-to-asset mappings in authoritative frame generation.
- [x] Reduced initial local-shard HUD confusion by refreshing the HUD state immediately after sandbox reset/connect instead of waiting for the next render tick.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/local-world.test.ts ./src/contracts.test.ts ./src/affordances.test.ts ./src/assets.test.ts ./src/render-runtime.test.ts ./src/direct-connect.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - live browser smoke via Playwright against `http://127.0.0.1:4174/` and `http://127.0.0.1:4174/?renderThread=worker`
  - Figma capture of the upgraded world test scene

### Iteration 81
- [x] Extended `apps/pod-web` frame planning with chunk-aware visibility and warmup metadata, including explicit `visibleWorldChunks`, `preloadedWorldChunks`, and prewarm request lists for nearby mesh and sprite assets instead of only prefetching whatever is already visible.
- [x] Updated `PodThreeWorldRenderer` and the browser HUD to surface chunk residency alongside asset residency, so creators can see what the runtime is actively drawing versus warming for nearby traversal.
- [x] Removed the remaining built-in overlay SVG texture path from the runtime texture loader by routing shipped ring overlays through the procedural texture path on all render threads, eliminating the prior `CopyExternalImageToTexture()` warning from POD-owned assets.
- [x] Added deterministic Bun coverage for chunk planning/warmup behavior and the new procedural-overlay routing.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/frame-plan.test.ts ./src/assets.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - live browser smoke via Playwright against `http://127.0.0.1:4174/` and `http://127.0.0.1:4174/?renderThread=worker`
  - `git diff --check`

### Iteration 82
- [x] Added shared presentation primitives in `pod-core` for atmosphere zones, actor presentation, and combat presentation so creators can author biome lighting, silhouette defaults, and combat readability as native engine data instead of ad hoc browser-only overrides.
- [x] Extended authoritative `pod-net` snapshots to carry those presentation primitives, including static atmosphere anchors and non-moving entities, so browser/editor consumers can build rich world views from the same server-owned metadata humans and AI agents share.
- [x] Updated `apps/pod-web` authoritative frame generation and renderer environment application to honor biome atmosphere, actor mesh/material overrides, aura rings, and combat ring styling directly from snapshot metadata.
- [x] Added deterministic coverage for presentation-driven environment selection, actor affordances, updated metadata fixtures, and static-scene affordance typing across the browser tests and `pod-net` snapshot tests.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/frame-plan.test.ts ./src/local-world.test.ts ./src/affordances.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 83
- [x] Added creator-facing world primitives in `pod-core` for `FactionAffiliation`, `QuestAnchor`, `EncounterProfile`, and `SpawnProfile`, and registered them in the core type registry plus fluent world builder so authored MMO content can express faction identity, quest hooks, encounter tables, and biome spawn rules as native engine data.
- [x] Extended authoritative `pod-net` snapshot metadata and hashing to carry those creator-world primitives, including faction context, quest anchors, encounter profile tuning, and spawn profile identity for both moving actors and static landmarks.
- [x] Updated `apps/pod-web` metadata parsing, local test world content, and creator affordance summaries so factions, quest hooks, encounter tables, and spawn profiles are immediately visible and testable in the browser sandbox instead of remaining hidden transport-only data.
- [x] Added deterministic coverage for snapshot capture of the new primitives plus browser-side faction/quest display and local-world authored metadata seeding.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-net --lib`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/local-world.test.ts ./src/affordances.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 84
- [x] Added native creator-region contracts in `pod-core` for `QuestStateGraph`, `FactionReputationTrack`, `RegionEncounterTable`, `WorldChunkDefinition`, and `WorldRegionDefinition`, and registered them in the core type registry so editor/runtime/AI tooling can share explicit quest-graph, reputation, encounter-table, and streamed-region definitions instead of reconstructing them from labels.
- [x] Extended authoritative `pod-net` entity metadata with creator-facing association ids such as `quest_graph_ids`, `faction_track_id`, and `encounter_table_id`, and updated snapshot hashing/tests so browser consumers can reason about authored world relationships from the network layer instead of only the local sandbox.
- [x] Reworked `apps/pod-web` local flagship world into a chunked authored sandbox with live chunk activation/deactivation, persisted per-chunk entity state, progression-gated regional population, and richer debug text for active chunks, current region, quest graphs, faction reputation, and encounter tables.
- [x] Added deterministic coverage for the new core contracts, network metadata associations, chunk streaming transitions, and progression-gated region unlocks in the browser sandbox.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-net --lib`
  - `cd apps/pod-web && bun test ./src/local-world.test.ts ./src/contracts.test.ts ./src/affordances.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 85
- [x] Added editor-native streamed-world graph state in `pod-editor` with seeded `WorldRegionDefinition`, `WorldChunkDefinition`, `QuestStateGraph`, `FactionReputationTrack`, and `RegionEncounterTable` contracts so creators can inspect and export authored world structure directly from the editor instead of only from runtime snapshots.
- [x] Added deterministic editor-side entity-to-world bindings derived from hierarchy and 2D transforms, including chunk key, region id/name, quest graph ids, faction track id, and encounter table id for the selected entity.
- [x] Surfaced the world graph into existing editor tooling by extending the inspector, Spacetime dashboard, and world-building snapshot export with selected-entity binding data and full graph TOON export.
- [x] Added editor helper APIs to sync regions, chunks, quests, factions, and encounter tables programmatically without forcing creators to rebuild graph state by hand.
- [x] Added deterministic coverage for default streamed bindings, movement-driven chunk reassignment, synced graph exports, and TOON world-graph document output.
- [x] Validated touched targets:
  - `cargo test -p pod-editor --lib`
  - `git diff --check`

### Iteration 86
- [x] Added deterministic shard-side streamed population reconciliation in `pod-core`, including active-chunk tracking from authoritative agents, neighbor-chunk activation, inactive streamed-entity eviction, encounter-table-driven spawn filling, and TOON-exportable world/chunk/region population state.
- [x] Extended `pod-net` snapshots and delta application with authoritative `WorldPopulationState`, including digest coverage and direct-connect/client snapshot propagation so browser consumers receive region/chunk pressure, spawn budget, and density metadata from the real shard path.
- [x] Updated `apps/pod-web` and `pod-editor` to consume authoritative population state, surfacing region/chunk population summaries in the browser HUD and adding TOON import plus dashboard rendering for editor-side shard population inspection.
- [x] Replaced the old server default map with a streamed `Verdant Hollow` shard layout in `pod-server`, including authored chunks/regions/encounter tables and deterministic runtime seeding so the flagship MMO world no longer depends on local-only browser sandbox density.
- [x] Added deterministic coverage for streamed population spawning/re-homing in `pod-core`, authoritative population parsing in browser tests, editor TOON population import, and server-map shard seeding.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-editor --lib`
  - `cargo test -p pod-server --bin pod-server`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/local-world.test.ts ./src/affordances.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 87
- [x] Reworked streamed shard population reconciliation into slot-based authoritative runtime state, including remembered live slots, per-slot respawn deadlines, and deterministic refill timing so streamed encounters no longer respawn immediately after despawn.
- [x] Extended `WorldPopulationState` with pending-respawn and next-respawn timing at both chunk and region level, then propagated those new fields through `pod-net` hashing/transport and the browser/editor population contract surfaces.
- [x] Updated `apps/pod-web` HUD summaries and `pod-editor` Spacetime dashboard summaries to show pending respawns and next respawn tick, so creators can tune live density pacing instead of only reading active counts and budget.
- [x] Added deterministic regression coverage for respawn deadline behavior in `pod-core`, updated browser parsing tests for the new population timing fields, and kept the server flagship shard map green under the new authoritative pacing rules.
- [x] Validated touched targets:
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-editor --lib`
  - `cargo test -p pod-server --bin pod-server`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/local-world.test.ts ./src/affordances.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 88
- [x] Added a real browser-side population heatmap in `apps/pod-web`, driven directly from authoritative chunk population state and focused on the controlled entity's current chunk/region instead of relying on static text summaries alone.
- [x] Added deterministic Bun coverage for chunk-key parsing, focused heatmap selection, intensity weighting, and legend formatting so the new browser heatmap stays testable without a DOM renderer.
- [x] Added creator-facing encounter balancing previews in `pod-editor`, derived from authored `RegionEncounterTable` contracts plus authoritative shard population pressure, pending respawns, and effective cap across attached chunks.
- [x] Added editor-side ambient-cap balancing controls and a compact chunk-pressure heat section in the Spacetime dashboard so creators can spot hotspots and tune encounter density directly from live shard data.
- [x] Added deterministic editor regression coverage for encounter balance previews and ambient-cap adjustment/clamping.
- [x] Validated touched targets:
  - `cargo test -p pod-editor --lib`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/local-world.test.ts ./src/affordances.test.ts ./src/population-heatmap.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 89
- [x] Replaced the rigid player-tied debug chase camera in `apps/pod-web` with a browser-side third-person orbit rig: independent yaw/pitch/zoom state, right-drag orbit, wheel zoom, velocity lead, and terrain-aware spring-arm collision so the flagship world reads like a controllable action camera instead of a floating shard overview.
- [x] Smoothed local sandbox locomotion by replacing instant velocity snaps with acceleration/deceleration and turn easing, improving both WASD steering and point-and-click movement feel without changing the action pipeline.
- [x] Kept the grounded terrain/water/daylight path from the previous graphics pass and validated that click-to-move, orbit-relative keyboard movement, and warmed asset residency still behave correctly in the live browser client.
- [x] Added deterministic regression coverage for the new camera pose/collision behavior, updated controls tests for the terrain-aware pick ray, and kept the touched Bun suite green.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/frame-plan.test.ts ./src/controls.test.ts ./src/local-world.test.ts ./src/contracts.test.ts ./src/renderer.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 90
- [x] Strengthened the flagship `pod-web` biome composition with more pronounced ridge/backdrop shaping, improved terrain albedo painting, a dedicated shoreline band, and richer water surfacing so the world reads as a place instead of a debug field.
- [x] Reduced first-view debug weight in the main HUD by tightening the chrome and keeping diagnostics secondary to the playable shard state.
- [x] Fixed the browser timelapse boot state so the world starts in daylight reliably, making the flagship client prove terrain, water, and atmosphere immediately instead of sometimes loading into a dim night frame.
- [x] Re-ran the touched Bun suite, typecheck, build, and live Playwright checks to confirm daylight boot, warm asset residency, and clean browser console behavior apart from the known upstream Three.js TSL warning.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/frame-plan.test.ts ./src/controls.test.ts ./src/local-world.test.ts ./src/contracts.test.ts ./src/renderer.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 91
- [x] Added deterministic ambient chunk dressing for `pod-web`, driven from streamed visible/preloaded chunk keys instead of hand-placed demo clutter, so the flagship shard now fills warm regions with terrain-aware trees, boulders, basalt columns, and spires.
- [x] Reused the existing chunk planner and asset residency pipeline to prewarm ambient chunk meshes before they come on screen, keeping density improvements aligned with the streamed-world runtime instead of bypassing it.
- [x] Surfaced ambient instance counts in the runtime HUD/stats path and validated live in Playwright that the shard now renders with denser chunk presentation on WebGPU while keeping asset residency stable and pending counts at zero after warm-up.
- [x] Added deterministic coverage for ambient planner determinism, lagoon/hub exclusion, and prewarm request generation.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/frame-plan.test.ts ./src/renderer.test.ts ./src/contracts.test.ts ./src/render-runtime.test.ts ./src/controls.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 92
- [x] Added browser-side world-event marker decoration in `pod-web`, so recent authoritative combat, gather, loot, capture, and summon events now render as short-lived world-space markers instead of only appearing in HUD text and event feed logs.
- [x] Kept the marker path inside the shared frame-decoration layer by composing it after interaction markers and before renderer submission, using current snapshot tick age and authoritative event origins/entity positions instead of browser-only heuristics.
- [x] Added deterministic coverage for recent-event filtering, origin/entity fallback positioning, and marker-style decoration without mutating the base Three.js frame contract.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/renderer.test.ts ./src/frame-plan.test.ts ./src/controls.test.ts ./src/render-runtime.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 93
- [x] Differentiated `pod-web` animation-set sampling so critical rings, target markers, destination breadcrumbs, companions, beasts, idle humanoids, and moving humanoids no longer share the same subtle debug wobble profile.
- [x] Added stronger stance/readability cues directly in `sampleAnimatedInstanceTransform(...)`, including critical-ring urgency, companion hover drift, beast crouch/stalk motion, and calmer idle humanoid breathing versus travel gait.
- [x] Added deterministic Bun coverage for ring differentiation and beast-versus-humanoid stance/motion separation so this presentation pass stays locked instead of regressing into one generic animation profile.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/frame-plan.test.ts ./src/contracts.test.ts ./src/renderer.test.ts ./src/controls.test.ts ./src/render-runtime.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 94
- [x] Added a real `pod-web` boot-state model so the browser shard no longer flashes misleading `demo frame` / `offline demo` placeholders while the local sandbox or direct-connect client is still initializing.
- [x] Applied that boot state both in the module bootstrap path and in an inline pre-module HTML script, so first paint matches the actual mode before the renderer and local world finish booting.
- [x] Added deterministic Bun coverage for local-sandbox and direct-connect boot-state selection in `direct-connect.test.ts`.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/direct-connect.test.ts ./src/frame-plan.test.ts ./src/contracts.test.ts ./src/renderer.test.ts ./src/controls.test.ts ./src/render-runtime.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 95
- [x] Fixed `pod-web` terrain anchoring for centered sample meshes by replacing the old `scaleY * 0.5` lift assumption with mesh-specific half-height calibration, so characters, creatures, props, and structures stop rendering partially below the terrain surface.
- [x] Added deterministic contract coverage for the corrected hero and beast world-space height placement in `contracts.test.ts`, grounded against the shared terrain sampler instead of visual guesswork.
- [x] Validated touched targets:
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/direct-connect.test.ts ./src/frame-plan.test.ts ./src/renderer.test.ts ./src/controls.test.ts ./src/render-runtime.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 96
- [x] Added per-client authoritative snapshot interest filtering in `pod-net`, so direct-connect and native clients no longer receive the same full-world delta when they control different shard regions.
- [x] Introduced `SnapshotInterest` plus filtered population derivation in `snapshot.rs`, keeping controlled entities always visible while scoping chunk and region population summaries to the client’s actual interest window.
- [x] Reworked `GameServer` session state to keep `last_sent_snapshot` per client and compute deltas against each client’s last visible world instead of one global shard baseline.
- [x] Updated welcome and full-resync paths to send filtered authoritative snapshots, preserving digest/reconciliation correctness after reconnects and recovery requests.
- [x] Added deterministic coverage for snapshot filtering and per-client server broadcasts, then validated native, wasm, and SpacetimeDB-enabled `pod-net` targets.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`
  - `git diff --check`

### Iteration 97
- [x] Extended `pod-net` server delivery with per-client authoritative event filtering, so `EventBatch` traffic now follows the same shard-interest window as snapshot state instead of broadcasting every event to every client.
- [x] Reused `SnapshotInterest` to scope event visibility by controlled-entity position, radius, and authored chunk membership, keeping direct-connect event delivery aligned with chunk-streamed MMO regions.
- [x] Added deterministic server coverage proving clients in different shard regions receive different authoritative event batches while spectators/unbounded clients retain full event visibility.
- [x] Revalidated native, SpacetimeDB-enabled, and wasm `pod-net` targets after the event-interest change.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`
  - `git diff --check`

### Iteration 98
- [x] Extended `pod-net` direct-connect debug delivery with per-client TOON debug-document filtering, so tick telemetry, tool-call events, and rollups now follow the same shard-interest window as snapshots and gameplay events.
- [x] Preserved full-fidelity behavior for unbounded editor/debug subscribers, while bounded player-linked clients now receive only the telemetry documents relevant to their visible entity set.
- [x] Added deterministic server coverage proving bounded debug clients receive filtered telemetry/tool-call documents and unbounded debug subscribers still receive full tick telemetry.
- [x] Revalidated native, SpacetimeDB-enabled, and wasm `pod-net` targets after the debug-interest change.
- [x] Validated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`
  - `git diff --check`

### Iteration 99
- [x] Added entity-scoped SpacetimeDB debug subscription helpers, so editor/debug consumers can subscribe to raw telemetry, tool-call events, and rollups for a focused agent/entity selection instead of defaulting to full-shard telemetry queries.
- [x] Extended the `pod-net` SpacetimeDB adapter with an entity-scoped editor-debug subscription profile and public API for selected-entity debug telemetry consumption.
- [x] Added deterministic query-generation coverage proving entity-scoped debug subscriptions stay narrow and do not silently widen back to global telemetry tables.
- [x] Validated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `git diff --check`

### Iteration 100
- [x] Hardened the browser gameplay surface so keyboard input explicitly claims the canvas, labels it as an application-grade gameplay target, and clears pressed-key state on blur/visibility loss instead of relying on incidental page focus.
- [x] Exposed `window.podRender.requestGameplayFocus()` and `window.podRender.getGameplayState()` so deterministic browser smoke tests and creator debugging can verify controlled-entity movement without scraping HUD text.
- [x] Added a renderer backend query override (`?backend=webgl2`) plus a Playwright smoke harness that proves both main-thread and worker-thread browser routes accept gameplay focus and movement input under automated test.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/controls.test.ts ./src/render-runtime.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 101
- [x] Added selected-entity raw debug summaries in `pod-editor`, so entity-scoped SpacetimeDB telemetry remains useful even when the editor is intentionally subscribed to tool-call and rollup docs instead of full tick timelines.
- [x] Extended editor snapshot export and telemetry/spacetime panels with selected-entity debug context derived from live tool-call and rollup documents.
- [x] Added `pod-net` helper wiring to mirror editor selection into the active entity-scoped SpacetimeDB debug subscription profile.
- [x] Revalidated touched targets:
  - `cargo test -p pod-editor --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `git diff --check`

### Iteration 102
- [x] Added browser-side retained live-debug state so tool-call events and rollups stay keyed by entity instead of only exposing the latest global debug document.
- [x] Wired the `pod-web` telemetry HUD to the focused entity (`selected target` or controlled entity), so raw selected-entity SpacetimeDB debug streams now surface meaningful focus-aware tool/rollup summaries in runtime.
- [x] Added deterministic `pod-web` coverage for retained per-entity live-debug summaries and replay/incident stream counters.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/live-debug.test.ts ./src/contracts.test.ts ./src/controls.test.ts ./src/render-runtime.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 103
- [x] Reduced the `pod-web` main HUD to a shorter gameplay-facing runtime line and upgraded authoritative event copy so combat/world outcomes read more clearly than raw event kinds.
- [x] Added shared `FocusedEntityDebugSummary` TOON-exportable primitives in `pod-core` and shard-side focused summary/document generation in `pod-server` from retained telemetry archives.
- [x] Added deterministic coverage for compact HUD/event formatting plus focused-entity TOON summary generation.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/hud.test.ts ./src/live-debug.test.ts ./src/contracts.test.ts ./src/controls.test.ts ./src/render-runtime.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-server --bin pod-server`
  - `git diff --check`

### Iteration 104
- [x] Added shared focused-entity debug aggregation in `pod-core`, so both direct-connect runtime code and shard ops tooling derive the same TOON summary from retained telemetry archives.
- [x] Extended `pod-net` with `SetDebugFocus { entity_id }`, per-session focused debug state, and focused summary delivery for debug subscribers without widening gameplay interest windows.
- [x] Wired focused debug summary parsing/retention into `pod-web` and `pod-editor`, so browser and editor consumers can render selected-entity debug state directly from the new document type.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/live-debug.test.ts ./src/direct-connect.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cargo test -p pod-core --lib`
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-editor --lib`
  - `cargo test -p pod-server --bin pod-server`
  - `git diff --check`

### Iteration 105
- [x] Added focused-entity debug summary synthesis on the `pod-net` SpacetimeDB adapter path, so entity-scoped tool-call and rollup streams now emit the same `focused_entity_debug_summary` TOON documents as direct-connect runtime flows.
- [x] Preserved the existing raw TOON debug documents while publishing synthesized focused summaries through the same retained `DebugDocument` surface used by browser and editor consumers.
- [x] Added deterministic `pod-net` coverage proving SpacetimeDB tool-call and rollup events synthesize focused summaries with retained latest-tool metadata and rollup metrics.
- [x] Revalidated touched targets:
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo test -p pod-net --lib`
  - `git diff --check`

### Iteration 106
- [x] Moved focused-entity debug summary synthesis down into `pod-stdb::client`, so raw shard-debug consumers now receive explicit focused-summary events instead of depending on adapter-local document rebuilding.
- [x] Simplified the `pod-net` SpacetimeDB adapter to forward `FocusedEntityDebugSummaryReceived` events through the retained `DebugDocument` surface while preserving raw tool-call and rollup documents.
- [x] Added deterministic `pod-stdb` and `pod-net` coverage proving focused summary documents are emitted with tool-call metadata first and then refreshed with authoritative rollup metrics.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo test -p pod-net --lib`
  - `git diff --check`

### Iteration 107
- [x] Fixed `pod-web` surface control and terrain/water grounding so arrow keys orbit the camera, WASD stays character-relative to the current camera yaw, and click-to-move starts immediately instead of waiting for the next resend loop.
- [x] Unified the local browser test helper with the live gameplay loop by routing `window.advanceTime(...)` through the same camera, movement, and local-sandbox stepping path used by the normal runtime tick.
- [x] Added shared landscape surface sampling and local-world movement collision/water-mode logic so the flagship sandbox now spawns the player on valid ground, blocks movement through solid props, and transitions onto swimmable water instead of walking the lake floor.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/controls.test.ts ./src/contracts.test.ts ./src/local-world.test.ts ./src/frame-plan.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - Playwright local browser proof on `http://127.0.0.1:4178/` for arrow-key orbit plus click-to-swim movement
  - `git diff --check`

### Iteration 108
- [x] Tightened flagship traversal feel by adding stronger movement response profiles, steering-aware slide resolution, and more deterministic collision progress around solid world props instead of sticky stop/start motion.
- [x] Added actual swim presentation feel on top of the existing surface correctness: swimmer animation now glides forward with buoyancy and the camera blends toward a lower, wider swim framing with impact kick preserved for combat-heavy moments.
- [x] Reduced main HUD weight again while keeping stronger action readability through shorter on-screen control copy and upgraded combat/world feedback labels.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/local-world.test.ts ./src/frame-plan.test.ts ./src/hud.test.ts ./src/controls.test.ts ./src/contracts.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - Live Playwright MCP proof on `http://127.0.0.1:4179/` for camera orbit, click/WASD traversal, and swim-state transition
  - `git diff --check`

### Iteration 109
- [x] Removed the debug grid as a default flagship-world surface cue by making it opt-in only (`?grid=1` / `?debugGrid=1`), so the runtime no longer looks like the player is standing under a dev board on first load.
- [x] Added shared real mesh bounds in `apps/pod-web` and reused them for authoritative world-frame placement plus ambient chunk dressing, replacing the drift-prone hard-coded half-height guesses that were causing props and actors to intersect terrain incorrectly.
- [x] Added renderer-side transform smoothing for mesh and sprite batches, so replicated actors and markers no longer snap between updates and the flagship shard reads more smoothly under normal movement/event churn.
- [x] Corrected browser gameplay-state surface reporting to use the shared landscape sampler instead of animation-name inference, keeping swim/ground debug state aligned with actual terrain/water logic.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/mesh-bounds.test.ts ./src/quality.test.ts ./src/contracts.test.ts ./src/renderer.test.ts`
  - `cd apps/pod-web && bun test ./src/controls.test.ts ./src/local-world.test.ts ./src/frame-plan.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 110
- [x] Added live direct-connect shard RTT/jitter sampling in `apps/pod-web`, using the existing ping/pong protocol instead of guessing whether runtime jerkiness is render-side or network-side.
- [x] Surfaced compact network quality information in the browser connection summary without widening the main HUD back into a debug slab.
- [x] Hardened the direct-connect client timer path to use `globalThis` scheduling, keeping runtime logic portable across browser and test environments.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/direct-connect.test.ts ./src/hud.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

### Iteration 111
- [x] Added a pure close-range combat camera-pressure model in `apps/pod-web` so target proximity now tightens framing and camera pressure based on actual engagement distance instead of only generic event shake and low-health state.
- [x] Upgraded actor pulse response in `frame-plan` so combat pulses drive forward lunge/weight for humanoids and heavier forward drive for beasts instead of only vertical bob and uniform scale pop.
- [x] Revalidated both main-thread and worker-route gameplay smoke after the camera/combat change, proving the flagship browser client still accepts movement input on both render paths.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/frame-plan.test.ts ./src/contracts.test.ts ./src/renderer.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 112
- [x] Moved flagship terrain shading into shared `sampleTerrainMaterial(...)` helpers in `apps/pod-web/src/landscape.ts`, so shoreline, rock, highland, and foam tinting no longer drift between baked texture generation and the gameplay world model.
- [x] Added shared `sampleWaterSurfaceStyle(...)` helpers and rewired the renderer to drive lagoon color, emissive response, opacity, scroll offsets, and repeat from the same daylight/time-lapse state used by the rest of the environment.
- [x] Raised landscape surface fidelity by quality preset (`terrainTextureSize`, `waterTextureSize`, `skyTextureSize`) so stronger browser hardware gets sharper baked terrain/water/sky surfaces without changing content contracts.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/quality.test.ts ./src/renderer.test.ts ./src/controls.test.ts ./src/local-world.test.ts ./src/frame-plan.test.ts ./src/contracts.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 113
- [x] Added procedural combat-readability sprite primitives (`combat-banner`, `health-bar`) so selected hostile fights can render world-space focus state without relying on a separate authored HUD asset pack.
- [x] Added `withCombatFocusMarkers(...)` to the shared authoritative browser frame-decoration path and wired it into `main.ts`, so selected attackable targets and the controlled player now get billboarded combat focus/health strips from real snapshot data instead of HUD-only summaries.
- [x] Added lightweight motion treatment for combat banners and health bars in `frame-plan`, keeping the new combat cues readable without introducing another static overlay layer.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/assets.test.ts ./src/contracts.test.ts ./src/frame-plan.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 114
- [x] Added shared shard transport observability contracts in `pod-core` (`ClientTransportSummary`, `ShardTransportSummary`) with first-class TOON export so runtime, editor, and browser debug consumers can inspect per-client traffic, queue depth, and snapshot/event/debug message pressure from one typed document.
- [x] Extended `pod-net` direct-connect server runtime to track inbound/outbound bytes, per-message-class counts, action/full-resync/ping intake, and periodic shard transport summaries, then emit those summaries into the same debug-document stream used by existing telemetry/tool/rollup consumers.
- [x] Extended `pod-web` live-debug parsing/HUD and `pod-editor` dashboard import paths to consume `shard_transport_summary` TOON documents, so debug surfaces can show shard transport pressure without scraping logs or requiring full raw tick telemetry.
- [x] Revalidated touched targets:
  - `cargo test -p pod-core --lib ops::`
  - `cargo test -p pod-net --lib server::`
  - `cargo test -p pod-editor --lib`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/live-debug.test.ts ./src/hud.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `git diff --check`

### Iteration 115
- [x] Added direct-connect heartbeat timeout controls in `pod-net::protocol::ServerConfig` and server-side stale-client pruning, so inactive sessions now disconnect deterministically instead of lingering forever in shard state.
- [x] Extended shard transport summaries with queue-pressure and inactivity metadata (`ticks_since_last_seen`, `queue_pressure`, `queue_pressure_client_count`, `timed_out_clients`, `queue_pressure_events`) and wired those metrics through `pod-core`, `pod-net`, `pod-editor`, and `pod-web`.
- [x] Added queue-pressure detection/logging for saturated pending-action queues in `pod-net`, plus browser/editor-facing compact summaries so degraded shard conditions are visible without scraping raw logs.
- [x] Revalidated touched targets:
  - `cargo test -p pod-core --lib ops::`
  - `cargo test -p pod-net --lib server::`
  - `cargo test -p pod-editor --lib`
  - `cargo test -p pod-server --bin pod-server`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/live-debug.test.ts ./src/hud.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `git diff --check`

### Iteration 116
- [x] Added direct-connect browser heartbeat watchdog controls (`heartbeatTimeoutMs`, `maxPendingActionBatches`) and a real authority-silence timeout path, so the flagship web client now fails fast and reconnects when the shard stops answering pings instead of waiting for the socket to die on its own.
- [x] Added queue-aware browser recovery behavior: when authoritative acknowledgements stall and the pending action backlog saturates, `pod-web` now requests a full snapshot recovery first and escalates to reconnect under stale-authority conditions.
- [x] Extended browser connection state with `clientId` and `heartbeatAgeMs`, then updated local sandbox/default status objects plus direct-connect tests so the stronger connection-health contract stays deterministic.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/direct-connect.test.ts ./src/hud.test.ts ./src/contracts.test.ts ./src/live-debug.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `git diff --check`

**Last updated**: Iteration 116
**Current focus**: Iteration 117 smoother worker-route gameplay input parity proof, tighter close-range hit feedback, and native/web client heartbeat parity beyond the flagship browser runtime
