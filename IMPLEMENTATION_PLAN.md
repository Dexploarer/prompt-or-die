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

### Iteration 117
- [x] Extended shared `pod-net::protocol::ClientConfig` with heartbeat and pending-action limits (`heartbeat_timeout_ms`, `max_pending_actions`) so native and web clients now derive failure/backpressure thresholds from one source of truth instead of browser-only runtime config.
- [x] Added native/web client-side pending-action saturation handling and authoritative heartbeat timeout enforcement in `pod-net`, so both QUIC and WebSocket clients now refuse runaway local input growth and fail fast on silent authority instead of drifting indefinitely.
- [x] Added deterministic native `pod-net` coverage for action saturation and heartbeat-timeout cleanup, and revalidated the touched targets on native, SpacetimeDB-enabled, and wasm builds.
- [x] Revalidated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`
  - `git diff --check`

### Iteration 118
- [x] Added shared ping/pong RTT and jitter tracking in `pod-net` native and web clients, so transport-quality sampling is no longer a `pod-web`-only concern and lower-level runtime consumers can inspect real connection latency on both client implementations.
- [x] Wired RTT/jitter updates directly into `ServerMessage::Pong` handling and added deterministic native coverage for latency/jitter smoothing.
- [x] Revalidated the touched `pod-net` targets on native and wasm builds.
- [x] Revalidated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`
  - `git diff --check`

### Iteration 119
- [x] Added native reconnect/session recovery parity to `pod-net` by teaching the QUIC client to track fatal runtime closure, back off reconnect attempts, and explicitly recover the transport session instead of only failing stale.
- [x] Extended native transport tests with deterministic reconnect-needed, reconnect-backoff, heartbeat-timeout cleanup, and RTT/jitter coverage so the lower-level client path now matches the browser watchdog expectations.
- [x] Revalidated the touched `pod-net` targets on native, SpacetimeDB-enabled, and wasm builds.
- [x] Revalidated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`
  - `git diff --check`

### Iteration 120
- [x] Hardened shard-side transport observability for resumed sessions by extending `ClientTransportSummary` / `ShardTransportSummary` with resume counts and wiring those through `pod-core`, `pod-net`, `pod-editor`, and `pod-web`.
- [x] Updated direct-connect server runtime counters so reconnect-token session resumes increment per-client and shard-wide resume telemetry instead of disappearing into logs.
- [x] Revalidated the touched Rust and browser/editor consumers after the transport-summary schema change.
- [x] Revalidated touched targets:
  - `cargo test -p pod-core --lib ops::`
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-editor --lib`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/live-debug.test.ts ./src/hud.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `git diff --check`

### Iteration 121
- [x] Hardened shard-side recovery observability by extending `ClientTransportSummary` / `ShardTransportSummary` with explicit recovery delivery metrics (`recovery_snapshots_sent`, `recovery_delivery_failures`) instead of relying on generic full-snapshot counters alone.
- [x] Updated `pod-net` direct-connect server recovery paths to increment those metrics when full recovery snapshots are successfully delivered or fail to reach the client, and added deterministic server coverage for both success and failure cases.
- [x] Propagated the new recovery metrics through `pod-core`, `pod-editor`, and `pod-web`, so browser/editor transport summaries now expose real recovery churn alongside resumes, queue pressure, and timeouts.
- [x] Revalidated touched targets:
  - `cargo test -p pod-core --lib ops::`
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo test -p pod-editor --lib`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/live-debug.test.ts ./src/hud.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `git diff --check`

### Iteration 122
- [x] Extended the shared `pod-net` welcome path with `acknowledged_action_tick`, so reconnect/session-resume handshakes can carry forward the server’s last processed action boundary instead of forcing clients to guess.
- [x] Updated native and web `pod-net` clients to preserve and replay only unacknowledged prediction batches on resumed welcomes, keeping replay state alive across reconnect instead of clearing local prediction history on every resumed session.
- [x] Propagated the welcome-contract change through the direct-connect server, SpacetimeDB adapter, and browser direct-connect parser/runtime, then added deterministic native/web parser coverage for replay-aware resumed welcomes.
- [x] Revalidated touched targets:
  - `cargo test -p pod-net --lib`
  - `cargo test -p pod-net --features spacetimedb --lib`
  - `cargo check -p pod-net --target wasm32-unknown-unknown --lib`
  - `cd apps/pod-web && bun test ./src/contracts.test.ts ./src/direct-connect.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `git diff --check`

### Iteration 123
- [x] Added a standing competitive-operating layer for POD with `docs/competitive-matrix.md`, covering Unity, Unreal, Godot, Bevy, Nakama, Inworld, and Convai plus a monthly-delta protocol.
- [x] Converted competitor failures into repo-level merge and roadmap gates in `docs/moat-gates.md`, then wired the moat question into `.github/pull_request_template.md` so every feature must answer whether it strengthens or dilutes the agent-world moat.
- [x] Added a moat benchmark suite: `crates/pod-core/examples/moat_benchmark_suite.rs` emits replay, tick-budget, action-transparency, and normalized cost metrics, while `scripts/run_moat_benchmarks.ts` combines that core report with native/browser parity checks and creator-time tracking.
- [x] Revalidated touched targets:
  - `cargo test -p pod-core --lib acceptance::`
  - `cargo run -p pod-core --example moat_benchmark_suite -- --profile ci-smoke`
  - `bun ./scripts/run_moat_benchmarks.ts --profile ci-smoke --skip-browser --output artifacts/moat-benchmarks-ci.json`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun test`
  - `git diff --check`

### Iteration 124
- [x] Added `scripts/bootstrap_reference_world.ts` as the canonical first-world bootstrap for POD, giving the repo an official one-command path to the flagship local sandbox instead of leaving creator bootstrap implied.
- [x] Added `docs/reference-bootstrap.md` and updated `README.md` / `docs/benchmark-suite.md` so creator-time measurement now points at an official starter flow instead of a manual placeholder.
- [x] Updated `scripts/run_moat_benchmarks.ts` so creator-time defaults to measuring the canonical bootstrap automatically, with manual or alternate scripted overrides still available.
- [x] Revalidated touched targets:
  - `bun ./scripts/bootstrap_reference_world.ts --measure`
  - `bun ./scripts/run_moat_benchmarks.ts --profile ci-smoke --skip-browser --output artifacts/moat-benchmarks-ci.json`
  - `bun ./scripts/run_moat_benchmarks.ts --profile ci-smoke --output artifacts/moat-benchmarks.json`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 125
- [x] Added `docs/bootstrap-showcase-research.md`, a primary-source brief grounded in official three.js docs/examples, Khronos glTF guidance, and camera/NPR research papers instead of subjective visual opinions.
- [x] Mapped those external sources back onto the real `apps/pod-web` files (`assets.ts`, `renderer.ts`, `landscape.ts`, `main.ts`, `sync-assets.mjs`) so the showcase plan is constrained by actual repo capabilities.
- [x] Locked the next bootstrap direction: split a dedicated `bootstrap-showcase` route from the generic systems sandbox, then build around one authored opening vista, a finite asset kit, PMREM-backed lighting, selective toon/outline treatment, and a showcase camera state machine.
- [x] Revalidated touched targets:
  - `git diff --check`

### Iteration 126
- [x] Added a real `bootstrap-showcase` local-world preset in `apps/pod-web`, separate from the generic `verdant-hollow` systems sandbox, with dedicated route parsing via `?world=bootstrap-showcase`.
- [x] Reframed the opening showcase composition around a shoreline vista in `local-world.ts`, updated the canonical bootstrap script/docs to land on that route, and exposed showcase identity through HUD/text-state/runtime inspection surfaces.
- [x] Added a short camera-directed intro blend in `main.ts` that stages the first frame toward the landmark vista until the player takes control, then validated the showcase route through unit tests, full Bun tests, typecheck, build, and browser smoke coverage.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/direct-connect.test.ts ./src/local-world.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 127
- [x] Split browser smoke responsibilities so the generic `local-sandbox` route remains the systems/input validation path while `bootstrap-showcase` gets its own route-specific visual regression coverage.
- [x] Added fixed-time renderer/runtime flags plus paused local-runtime boot support so the showcase intro can be captured deterministically enough for CI screenshot diffing.
- [x] Added a Playwright screenshot baseline for the showcase intro frame, folded it into `bun run test:smoke`, and documented the new browser validation split in `docs/benchmark-suite.md`.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/runtime-flags.test.ts ./src/direct-connect.test.ts ./src/local-world.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bunx playwright test tests/showcase-visual.e2e.ts --update-snapshots`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 128
- [x] Replaced the bootstrap showcase’s generic sandbox prop labels with an authored shoreline kit (`tideglass monolith`, `resonant shrine`, `windward pine`, `shore cairn`, `attunement pylon`) so the first loaded chunk has its own landmark vocabulary instead of reusing `glass spire` / `weathered boulder` naming.
- [x] Added route-specific shoreline render profiles and tideglass atmosphere/region metadata, then refreshed the showcase screenshot baseline so the visual gate now locks the authored material language and `Resonant Strand` world summary instead of leaking `Verdant Hollow` shell copy.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/local-world.test.ts ./src/contracts.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bunx playwright test tests/showcase-visual.e2e.ts --update-snapshots`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 129
- [x] Hardened the browser asset runtime for real creator assets by changing scene extraction from "first mesh only" to merged multi-mesh geometry, with a richest-primitive fallback when merge attributes do not line up.
- [x] Switched the shipped browser mesh pipeline to emit binary `.glb` assets alongside source `.gltf` files, and moved the runtime manifest onto `.glb` so the default path exercises the lower-overhead binary loader instead of the JSON export path.
- [x] Added binary-asset and multi-mesh coverage in `apps/pod-web/src/assets.test.ts`, updated `vite.config.ts` chunking for the new geometry utility dependency, and documented the `.glb` fast path in `apps/pod-web/README.md`.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/assets.test.ts`
  - `cd apps/pod-web && bun run sync:assets`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 130
- [x] Extended `apps/pod-web` asset telemetry from simple residency counts to real load budgets by recording geometry/sprite load counts plus average/slowest durations in `src/assets.ts`, threading those metrics through `src/renderer.ts`, and exposing the averages in the compact HUD runtime line.
- [x] Added deterministic coverage for the new timing aggregates in `apps/pod-web/src/assets.test.ts`, updated `apps/pod-web/src/hud.test.ts`, and documented the new HUD/runtime perf signal in `apps/pod-web/README.md`.
- [x] Turned the `pod-assets` import lane into a real staged-artifact boundary: supported non-scene imports now materialize content-addressed copied artifacts, `.gltf` / `.glb` / `.jpeg` staging preserves the authored source extension, and `crates/pod-assets/examples/stage_import.rs` provides a concrete repo-level CLI entrypoint.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test ./src/assets.test.ts ./src/hud.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun test`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `cargo test -p pod-assets`
  - `cargo check --workspace`
  - `cargo check -p pod-assets --example stage_import`
  - `cargo run -q -p pod-assets --example stage_import -- <temp-source> <temp-output>`
  - `git diff --check`

### Iteration 131
- [x] Extended `pod-assets` source staging to cover SVG sprite sources in addition to the existing mesh/image formats, keeping the current pod-web sample inputs inside one import boundary instead of leaving textures out-of-band.
- [x] Upgraded `crates/pod-assets/examples/stage_import.rs` into a batch-capable, machine-readable CLI with `--json` and `--output-root`, so downstream tooling can stage one asset set in a single process instead of shelling out per file.
- [x] Wired `apps/pod-web/scripts/sync-assets.mjs` into that staged-import boundary: the sample sync now emits generated source assets under `apps/pod-web/artifacts/source-assets`, stages them through `pod-assets`, and writes `apps/pod-web/artifacts/staged-assets/pod-staged-asset-manifest.json` that maps staged source artifacts to shipped runtime paths.
- [x] Updated browser/root docs so the staged-source, staged-import, and runtime-public directories are explicit instead of implied.
- [x] Revalidated touched targets:
  - `cargo test -p pod-assets`
  - `cargo check -p pod-assets --example stage_import`
  - `cargo run -q -p pod-assets --example stage_import -- --json --output-root <temp-output> <temp-source> [<temp-source> ...]`
  - `cd apps/pod-web && bun run sync:assets`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 132
- [x] Added a reusable runtime bundle model in `crates/pod-assets/src/lib.rs` (`RuntimeBundleSpec`, `RuntimeBundleManifest`, and `build_runtime_bundle_manifest`) so staged-to-runtime manifest assembly is library code instead of ad hoc JS object construction.
- [x] Extended the `stage_import` example with `--bundle-spec` and `--base-dir`, allowing one batch staging call to return both staged import records and a resolved runtime bundle manifest.
- [x] Simplified `apps/pod-web/scripts/sync-assets.mjs` so it now writes a bundle spec and delegates staged-manifest assembly to `pod-assets`; the script still generates sample source/runtime assets, but it no longer owns the staged-to-runtime manifest contract.
- [x] Added deterministic Rust coverage for runtime bundle manifest assembly and revalidated the updated bundle path through the pod-web asset sync, build, and browser smoke flow.
- [x] Revalidated touched targets:
  - `cargo test -p pod-assets`
  - `cargo check -p pod-assets --example stage_import`
  - `cd apps/pod-web && bun run sync:assets`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 133
- [x] Added `materialize_runtime_bundle_manifest` to `crates/pod-assets/src/lib.rs` so the runtime-public asset tree can be copied from staged imports using the resolved bundle manifest instead of app-local copy logic.
- [x] Extended `crates/pod-assets/examples/stage_import.rs` with `--materialize-runtime`, allowing one batch staging call to both emit the bundle manifest and materialize the runtime-public asset outputs.
- [x] Updated `apps/pod-web/scripts/sync-assets.mjs` so sample meshes are staged from generated `.glb` source files, human-inspectable `.gltf` files remain as sidecars in `artifacts/source-assets`, and runtime asset writes are delegated back to `pod-assets`.
- [x] Added deterministic Rust coverage for runtime bundle materialization and revalidated the sample source-to-staged-to-runtime path through `sync:assets`, workspace check, browser build, and browser smoke.
- [x] Revalidated touched targets:
  - `cargo test -p pod-assets`
  - `cargo check -p pod-assets --example stage_import`
  - `cd apps/pod-web && bun run sync:assets`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 134
- [x] Extended `crates/pod-assets/src/lib.rs` so `.ktx2` is a first-class staged source format and sprite runtime bundle specs can declare an optional compressed texture sidecar in addition to the primary runtime texture output.
- [x] Hardened the runtime bundle contract with explicit validation for duplicate runtime output paths and non-`ktx2` compressed sprite variants, turning malformed bundle specs into deterministic errors instead of silent drift.
- [x] Added deterministic Rust coverage for staged `ktx2` imports, optional compressed sprite variant records, duplicate-path rejection, and compressed variant materialization into the runtime-public asset tree.
- [x] Stabilized the `pod-web` Playwright smoke harness by waiting for the actual `threejs` frame source and using the explicit gameplay-focus API instead of flaky canvas clicks.
- [x] Revalidated touched targets:
  - `cargo test -p pod-assets runtime_bundle -- --nocapture`
  - `cargo test -p pod-assets ktx2 -- --nocapture`
  - `cargo test -p pod-assets`
  - `cargo check -p pod-assets --example stage_import`
  - `cd apps/pod-web && bun run sync:assets`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 135
- [x] Refactored `apps/pod-web/scripts/sync-assets.mjs` into a callable entrypoint so the script can expose pure helpers without re-running asset generation during tests.
- [x] Added a deterministic projection step that reads `pod-assets` staged bundle manifests and writes any sprite `compressed_variant.runtime_path` into the shipped `pod-asset-manifest.json` as `ktx2Path`, removing the remaining app-local compressed-texture duplication.
- [x] Added Bun coverage for the projection helper and documented the exact malformed-bundle failure modes now enforced across the shared pipeline and `pod-web` sync surface.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test scripts/sync-assets.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run sync:assets`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 136
- [x] Extended `apps/pod-web/scripts/sync-assets.mjs` so the sample lane auto-detects authored `artifacts/source-assets/textures/<asset-id>.ktx2` sidecars and emits them into the shared runtime bundle spec as `compressed_variant` entries.
- [x] Added deterministic Bun coverage proving the bundle spec now expresses optional compressed sprite sidecars with the expected `.ktx2` runtime output path.
- [x] Updated the repo docs and session tracker so Phase 3 closes on an explicit browser contract: shipped `.glb` meshes plus optional precompressed sprite sidecars, with Phase 4 now owning the real compression/LOD rollout.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test scripts/sync-assets.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run sync:assets`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 137
- [x] Extended `crates/pod-assets/src/lib.rs` so runtime bundle specs and manifests can express explicit mesh LOD variants alongside optional compressed sprite sidecars, and materialization now copies those LOD outputs into the runtime-public asset tree.
- [x] Reworked `apps/pod-web/scripts/sync-assets.mjs` to generate real sample mesh LOD variants, project them into the shared bundle spec, emit a shipped manifest with `lods` plus runtime variant metadata, and write a deterministic `pod-runtime-budget-report.json`.
- [x] Updated `apps/pod-web/src/assets.ts` so manifest-backed runtime loading selects explicit LOD or compressed sprite paths deterministically from the new metadata instead of relying on ad hoc path logic.
- [x] Added deterministic Rust and Bun coverage for the new runtime-variant contract, including LOD bundle assembly/materialization, manifest parsing/path selection, and budget-report enforcement.
- [x] Revalidated touched targets:
  - `cargo test -p pod-assets runtime_bundle -- --nocapture`
  - `cd apps/pod-web && bun test scripts/sync-assets.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run sync:assets`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 138
- [x] Extended `apps/pod-web/scripts/sync-assets.mjs` with a real KTX2 fixture generator using Three.js `KTX2Exporter`, so the sample lane now emits valid `.ktx2` ring sidecars from the same authored ring definitions as the SVG source assets.
- [x] Updated the shipped sprite manifest/runtime budget projection so real `.ktx2` fixtures are surfaced as `ktx2Path` plus runtime variant metadata, while `preferredEncoding` follows the budget report instead of blindly preferring the KTX2 container.
- [x] Added deterministic Bun coverage for KTX2 fixture generation and for the budget-driven sprite encoding choice, then regenerated the shipped asset bundle with real `.ktx2` outputs under `artifacts/source-assets/textures` and `public/assets/textures`.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test scripts/sync-assets.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run sync:assets`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 139
- [x] Replaced the transient KTX2-exported ring path in `apps/pod-web/scripts/sync-assets.mjs` with generated PNG sprite sources plus checked-in BasisU-authored `.ktx2` fixtures under `apps/pod-web/fixtures/textures`, so the sample lane no longer depends on runtime exporter output to exercise real supercompressed texture delivery.
- [x] Updated the shipped sprite manifest and runtime budget report so `danger-ring`, `mist-ring`, and `selection-ring` now prefer `ktx2` by budget (`8068→1972`, `8892→1969`, `8211→1821`) instead of only exposing compressed variants structurally.
- [x] Hardened the `pod-web` Playwright smoke harness by advancing local-sandbox movement deterministically through `window.advanceTime(...)` and by switching the showcase visual gate to a clipped paused-canvas snapshot, then refreshed the showcase baseline for the stable capture path.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test scripts/sync-assets.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run sync:assets`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 140
- [x] Extended `crates/pod-assets/src/lib.rs` so the shared runtime bundle contract now carries optional meshopt-compressed mesh variants alongside base and LOD `.glb` outputs, validates them as staged glTF/GLB imports, and materializes them into the runtime-public asset tree.
- [x] Added checked-in `.meshopt.glb` fixtures under `apps/pod-web/fixtures/meshes`, updated `apps/pod-web/scripts/sync-assets.mjs` to stage and project them, and taught the shipped manifest/runtime budget report to expose `meshoptLods`, `runtime.preferredEncoding`, and `runtime.compressedVariants` for meshes.
- [x] Revalidated the shared mesh-compression fast path so shipped sample geometry now prefers meshopt where it wins by budget, including representative reductions like `adventurer-avatar 25612→4796`, `rift-beast 24264→4828`, and `glass-spire 7040→2672`.
- [x] Revalidated touched targets:
  - `cargo test -p pod-assets runtime_bundle -- --nocapture`
  - `cargo test -p pod-assets`
  - `cd apps/pod-web && bun test scripts/sync-assets.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run sync:assets`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 141
- [x] Started Phase 5 by extending `apps/pod-web/src/renderer.ts` with explicit runtime warmup and frame-stability counters, so both main-thread and worker routes now report `runtimePerf.warmupMs`, frame-budget metadata, stable/slow frame counts, stable-frame percentage, and slowest frame time through `window.podRender.getStats()`.
- [x] Surfaced the new counters in the compact runtime HUD via `apps/pod-web/src/hud.ts`, then added deterministic coverage in `apps/pod-web/src/render-runtime.test.ts` and updated HUD expectations in `apps/pod-web/src/hud.test.ts`.
- [x] Hardened `apps/pod-web/tests/worker-input.e2e.ts` so the browser smoke no longer just proves movement input; it now also asserts that both main-thread and worker routes publish non-empty warmup/frame-stability counters after the shipped meshopt + KTX2 manifest has warmed.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test src/render-runtime.test.ts src/hud.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 142
- [x] Extended `apps/pod-web/src/render-runtime.ts` with explicit main-thread submission counters, so both render routes now report `mainThreadPerf.warmupMs`, submission count, average submission time, and slowest submission time in addition to render-thread `runtimePerf`.
- [x] Added explicit requested-vs-actual render-thread metadata plus concrete fallback reasons (`missing-worker-constructor`, `missing-offscreen-canvas`, `missing-canvas-transfer-control`) so worker fallback behavior is inspectable instead of only logged vaguely.
- [x] Surfaced average main-thread submission cost in `apps/pod-web/src/hud.ts`, added deterministic coverage in `apps/pod-web/src/render-runtime.test.ts`, and hardened `apps/pod-web/tests/worker-input.e2e.ts` so smoke now asserts both `runtimePerf` and `mainThreadPerf` consistency on main-thread and worker routes.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test src/render-runtime.test.ts src/hud.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 143
- [x] Reworked the worker render runtime so the main thread only posts the newest pending frame while a worker render is in flight, instead of serializing and `postMessage()`-ing every intermediate frame.
- [x] Added explicit `renderComplete` worker acknowledgements in `apps/pod-web/src/render-worker.ts`, which lets the main thread flush coalesced frame submissions only after the worker finishes the prior render.
- [x] Removed the duplicate post-init `resize` sync on worker routes by carrying the initial surface metrics through worker initialization and suppressing unchanged surface re-posts.
- [x] Added deterministic worker-runtime tests covering both hot-path fixes in `apps/pod-web/src/render-runtime.test.ts`.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test src/render-runtime.test.ts src/hud.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 144
- [x] Extended `mainThreadPerf` with per-kind worker submission buckets (`frame`, `control`, `resize`) so the remaining hot-path cost is attributable instead of blended into one average.
- [x] Recorded those buckets in `apps/pod-web/src/render-runtime.ts` for worker routes, while keeping main-thread routes on the same aggregate contract.
- [x] Tightened `apps/pod-web/tests/worker-input.e2e.ts` so browser smoke now verifies that the per-kind counts reconcile with the aggregate submission count.
- [x] Fixed the paused showcase visual gate by explicitly advancing `window.advanceTime(...)` until the route is ready, which removes the prior flake where the screenshot could capture either a staging frame or a partially booted vista.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test src/render-runtime.test.ts src/hud.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 145
- [x] Batched worker-route control traffic in `apps/pod-web/src/render-runtime.ts` so multiple world-event and telemetry updates collapse into a single `applyControlState` post per microtask instead of multiple standalone control messages.
- [x] Preserved frame ordering by flushing any queued control state immediately before posting the next worker render frame, so telemetry/event state still lands ahead of the next rendered frame.
- [x] Added worker-side support for the combined control message in `apps/pod-web/src/render-worker.ts` and deterministic coverage in `apps/pod-web/src/render-runtime.test.ts`.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test src/render-runtime.test.ts src/hud.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 146
- [x] Turned the worker-route per-kind submission buckets into deterministic browser regression ceilings in `apps/pod-web/tests/worker-input.e2e.ts`.
- [x] Locked the local-sandbox worker smoke route so `mainThreadPerf.byKind.control.submissionsCompleted` and `resize.submissionsCompleted` must remain `0`, while preserving the aggregate/bucket reconciliation assertions already in the suite.
- [x] Revalidated the worker-route ceilings with targeted Playwright sampling plus the full serialized browser smoke suite.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test src/render-runtime.test.ts src/hud.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 147
- [x] Added explicit `runtimePerf` quality gates to `apps/pod-web/tests/worker-input.e2e.ts`, requiring the main-thread local-sandbox route to hold at least `90%` stable frames and the worker route to hold at least `50%` stable frames with more stable than slow frames.
- [x] Kept the new browser gate deterministic by avoiding absolute warmup ceilings, which were too environment-sensitive for the serialized Playwright harness.
- [x] Revalidated the stability gates with targeted worker-input Playwright coverage plus the focused Bun test set.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bunx playwright test tests/worker-input.e2e.ts --config=playwright.config.ts`
  - `cd apps/pod-web && bun test src/render-runtime.test.ts src/hud.test.ts src/assets.test.ts`

### Iteration 148
- [x] Added `apps/pod-web/scripts/measure-render-routes.ts`, a reusable browser sampler that measures the main-thread and worker `local-sandbox` routes, emits `artifacts/render-route-measurements.json`, and records the same runtime/submission gates used by browser smoke.
- [x] Added deterministic Bun coverage in `apps/pod-web/scripts/measure-render-routes.test.ts` for route projection, worker-relief comparison, and report assembly.
- [x] Threaded the new browser report into `scripts/run_moat_benchmarks.ts` so the combined moat artifact now includes `browserRouteMeasurements` alongside the existing browser parity checks.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test scripts/measure-render-routes.test.ts src/render-runtime.test.ts src/hud.test.ts src/assets.test.ts`
  - `cd apps/pod-web && bun run measure:render-routes`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `bun scripts/run_moat_benchmarks.ts --skip-creator`
  - `git diff --check`

### Iteration 149
- [x] Extended the shard transport contract in `crates/pod-core/src/ops.rs` and `crates/pod-net/src/server.rs` with bounded snapshot/delta/recovery/queue metrics, including full-snapshot bytes, recovery snapshot bytes, delta bytes, delta entity churn, peak pending queue depth, and per-client queue-pressure incident counts.
- [x] Threaded those metrics through the browser transport contract in `apps/pod-web/src/contracts.ts`, surfaced them in the debug-side transport summary via `apps/pod-web/src/hud.ts` and `apps/pod-web/src/main.ts`, and kept the gameplay HUD compact by leaving `formatConnectionSummary()` on the shorter route.
- [x] Added deterministic coverage in `crates/pod-core/src/ops.rs`, `crates/pod-net/src/server.rs`, `apps/pod-web/src/contracts.test.ts`, `apps/pod-web/src/live-debug.test.ts`, and `apps/pod-web/src/hud.test.ts`, then revalidated the browser build and smoke path.
- [x] Revalidated touched targets:
  - `cargo test -p pod-core shard_transport_summary_exports_to_toon -- --nocapture`
  - `cargo test -p pod-net transport -- --nocapture`
  - `cargo test -p pod-net test_send_to_client_tracks_delta_bytes_and_entity_churn -- --nocapture`
  - `cargo test -p pod-net test_send_full_snapshot_tracks_recovery_delivery_success -- --nocapture`
  - `cd apps/pod-web && bun test src/contracts.test.ts src/hud.test.ts src/live-debug.test.ts`
  - `cd apps/pod-web && bun run typecheck`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 150
- [x] Added degraded-network regression coverage in `apps/pod-web/src/direct-connect.test.ts`, proving stale-authority backlog saturation forces reconnect instead of requesting local recovery when the heartbeat watchdog already considers the authority stale.
- [x] Added deterministic `pod-net` server tests in `crates/pod-net/src/server.rs` that exercise `ClientMessage::RequestFullSnapshot` through `handle_connections()` and reconnect-token session resume, so recovery and resume paths now prove the new transport counters instead of only exposing them in steady-state summaries.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test src/direct-connect.test.ts`
  - `cargo test -p pod-net handle_connections -- --nocapture`
  - `cargo test -p pod-net transport -- --nocapture`
  - `cargo check --workspace`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run test:smoke`
  - `git diff --check`

### Iteration 151
- [x] Expanded `docs/plugin-model.md` so the current plugin model names the actual contract surfaces integrators should depend on today (`pod-scene`, `pod-assets`, direct-connect transport, and browser debug consumers) instead of only listing broad subsystem categories.
- [x] Clarified `docs/architecture.md` with a current extension seam map, explicitly separating exported crate boundaries from app composition roots like `apps/pod-web/src/main.ts`.
- [x] Recorded the seam-level validation rule directly in the plugin docs so Phase 7 now requires crate-boundary proof before app-level smoke.
- [x] Identified the concrete missing lifecycle hooks still forcing integrators into composition roots: server world/bootstrap, browser runtime bootstrap, editor panel registration, and transport policy injection.
- [x] Defined near-term conventions for imports, runtime registration, and extension testing so extension authors have a stable discipline before a formal plugin SDK exists.
- [x] Revalidated current extension paths against the clarified contract:
  - `cargo test -p pod-scene test_scene_instantiation_tracks_component_provenance_across_prefab_and_scene_layers -- --nocapture`
  - `cargo test -p pod-assets build_runtime_bundle_manifest_maps_staged_imports_to_runtime_paths -- --nocapture`
  - `cargo test -p pod-net handle_connections -- --nocapture`
  - `cd apps/pod-web && bun test src/direct-connect.test.ts src/contracts.test.ts src/hud.test.ts`

### Iteration 152
- [x] Added `apps/pod-web/scripts/verify-generated-assets.ts` plus `bun run verify:assets`, which reruns `sync:assets` and fails if the committed generated source/staged/runtime asset trees drift.
- [x] Extended the shared browser gate surface in `apps/pod-web/src/render-runtime-gates.ts`, `apps/pod-web/tests/worker-input.e2e.ts`, and `apps/pod-web/scripts/measure-render-routes.ts` with minimum completed-asset-load counts plus average/slowest geometry and sprite load ceilings.
- [x] Added `bun run measure:render-routes:check`, so the route sampler now records `artifacts/render-route-measurements.json` and fails when browser frame-quality, worker-chatter, or asset-load thresholds regress.
- [x] Wired both gates into `.github/workflows/ci.yml` and `scripts/run_moat_benchmarks.ts`, and the `pod-web` CI job now uploads `apps/pod-web/artifacts/render-route-measurements.json` plus `apps/pod-web/artifacts/staged-assets/pod-runtime-budget-report.json`.
- [x] Revalidated touched targets:
  - `cd apps/pod-web && bun test scripts/measure-render-routes.test.ts scripts/verify-generated-assets.test.ts`
  - `cd apps/pod-web && bun run verify:assets`
  - `cd apps/pod-web && bun run typecheck`
  - `cd apps/pod-web && bun run build`
  - `cd apps/pod-web && bun run measure:render-routes:check`
  - `cd apps/pod-web && bun run test:smoke`
  - `bun ./scripts/run_moat_benchmarks.ts --profile ci-smoke --skip-creator --output artifacts/moat-benchmarks-ci-local.json`
  - `git diff --check`

### Iteration 153
- [x] Added `crates/pod-net/examples/transport_benchmark_suite.rs`, a deterministic in-process transport benchmark that exercises steady delta delivery, recovery success, recovery failure, reconnect-token resume, and queue-pressure/timeout paths.
- [x] Added structured `TransportBenchmarkReport` surfaces in `crates/pod-net/src/server.rs`, including per-scenario `ShardTransportSummary` payloads plus explicit pass/fail checks, and covered them with deterministic Rust tests.
- [x] Threaded the new report into `scripts/run_moat_benchmarks.ts` as `transportMeasurements`, bumped the combined moat artifact schema to `2`, and used `--fail-on-checks` so the moat runner now fails if the direct-connect transport benchmark regresses.
- [x] Documented the new transport benchmark command and the combined moat artifact shape in `docs/benchmark-suite.md` and `README.md`.
- [x] Revalidated touched targets:
  - `cargo test -p pod-net transport_benchmark_suite -- --nocapture`
  - `cargo check -p pod-net --example transport_benchmark_suite`
  - `cargo run -p pod-net --example transport_benchmark_suite -- --profile ci-smoke --fail-on-checks`
  - `bun ./scripts/run_moat_benchmarks.ts --profile ci-smoke --skip-browser --skip-creator --output artifacts/moat-benchmarks-ci-local.json`
  - `git diff --check`

### Iteration 154
- [x] Added published shard-target transport baselines directly to `crates/pod-net/src/server.rs`, so the deterministic transport benchmark now checks exact byte and queue-depth envelopes instead of only generic scenario invariants.
- [x] Extended the transport report aggregate with baseline metadata/checks, bumped the transport report schema to `2`, and documented the shard-target envelope values in `docs/benchmark-suite.md` and `README.md`.
- [x] Covered the new baseline checks in the deterministic Rust benchmark tests and revalidated the shard-target transport/moat commands.
- [x] Revalidated touched targets:
  - `cargo test -p pod-net transport_benchmark_suite -- --nocapture`
  - `cargo check -p pod-net --example transport_benchmark_suite`
  - `cargo run -p pod-net --example transport_benchmark_suite -- --profile shard-target --fail-on-checks`
  - `bun ./scripts/run_moat_benchmarks.ts --profile shard-target --skip-browser --skip-creator --output artifacts/moat-benchmarks-shard-local.json`
  - `git diff --check`

### Iteration 155
- [x] Pivoted the active roadmap away from browser/asset infrastructure and back onto the agent stack, based on the live `pod-core` and `pod-agents` source of truth rather than the previous Phase 8 benchmark queue.
- [x] Added `docs/agent-runtime-audit.md`, documenting how the shared `Agent` runtime, authoritative tick loop, LLM controller, hybrid controller, neural controller, replay surfaces, and ONNX path actually work today.
- [x] Updated `docs/agent-integration-contract.md` so the public contract matches the real runtime surface (`runtime_profile`, `drain_tool_calls`, telemetry expectations, and the current neural-policy shape).
- [x] Repointed `IMPLEMENTATION_PHASES.md` and `SESSION.md` to the new active track: neural runtime hardening, reward/replay dataset contracts, evaluation harnesses, and remote agent topology on SpacetimeDB.
- [x] Revalidated touched targets:
  - `cargo check -p pod-core -p pod-agents`
  - `git diff --check`

### Iteration 156
- [x] Added explicit neural runtime schema/version metadata in `crates/pod-agents/src/neural_agent.rs`, including `NEURAL_INTERFACE_VERSION`, `NEURAL_FEATURE_COUNT`, `NEURAL_ACTION_COUNT`, `NeuralRuntimeSchema`, `NeuralModelMetadata`, and compatibility validation errors.
- [x] Replaced scattered neural magic numbers with shared schema constants in the encoder and action-selection path, and re-exported the new schema surface from `crates/pod-agents/src/lib.rs`.
- [x] Extended `crates/pod-agents/src/onnx_network.rs` so the ONNX path validates caller-supplied model metadata against the shared neural runtime schema instead of relying only on implicit `32 -> 10` assumptions.
- [x] Added deterministic neural/ONNX tests for schema consistency and metadata mismatch handling, then revalidated the `pod-agents` library test surface.
- [x] Revalidated touched targets:
  - `cargo test -p pod-agents --lib`
  - `cargo check -p pod-core -p pod-agents`
  - `git diff --check`

### Iteration 157
- [x] Replaced the neural action table with an explicit named action schema registry in `crates/pod-agents/src/neural_agent.rs`, so policy outputs are no longer only implied by raw positional constants.
- [x] Extended the `PolicyNetwork` trait with runtime-status reporting and taught `NeuralAgent::introspect()` to surface policy identity, schema version, compatibility mode, fallback state, last chosen action, and experience-buffer depth.
- [x] Extended the ONNX policy implementation with runtime-status tracking for last inference fallback and model identity, while preserving the existing uniform-output safety fallback on inference failure.
- [x] Added deterministic tests for named action-schema contents and neural introspection output, then revalidated the `pod-agents` library test surface.
- [x] Revalidated touched targets:
  - `cargo test -p pod-agents --lib`
  - `cargo check -p pod-core -p pod-agents`
  - `git diff --check`

### Iteration 158
- [x] Added first-pass multi-world runtime contracts to `crates/pod-core/src/contract.rs` for `AgentTeamDefinition`, `WorldRealityDefinition`, `CrossWorldLinkDefinition`, and `WorldTournamentDefinition`, so developer-controlled squads, Deadman-style worlds, and alternate-reality links have native engine vocabulary.
- [x] Exported the new topology surface from `crates/pod-core/src/lib.rs` and covered the TOON/document contract with deterministic unit tests.
- [x] Added `docs/multi-world-agent-topology.md` and updated `docs/architecture.md`, `docs/agent-integration-contract.md`, `IMPLEMENTATION_PHASES.md`, and `SESSION.md` so the repo now explicitly treats headless multi-world team orchestration as a first-class future runtime surface.
- [x] Revalidated touched targets:
  - `cargo test -p pod-core contract -- --nocapture`
  - `cargo check -p pod-core -p pod-agents`
  - `git diff --check`

### Iteration 159
- [x] Added authoritative reward telemetry to `crates/pod-core/src/telemetry.rs` via `AgentRewardSignal`, `RewardSource`, and `RewardReason`, and threaded reward traces into `AgentTelemetryFrame`.
- [x] Added canonical reward attribution in `crates/pod-core/src/tick.rs`, mapping action outcomes plus authoritative world events like damage, kills, capture, gathering, loot, summons, and skill XP into per-agent reward signals.
- [x] Extended `crates/pod-core/src/replay.rs` so replay-derived `ReplayTrainingSample` rows now include `RewardAttributionSummary` with total reward, polarity totals, signal counts, and terminal-state flags.
- [x] Updated the agent/runtime docs and live planning state to reflect that reward attribution is now authoritative and replay-derived training rows carry reward summaries.
- [x] Revalidated touched targets:
  - `cargo test -p pod-core replay_training_samples_capture_path_action_and_encounter_transitions -- --nocapture`
  - `cargo test -p pod-core reward_signal_exports_to_toon_document -- --nocapture`
  - `cargo test -p pod-core combat_events_generate_authoritative_reward_signals -- --nocapture`
  - `cargo check -p pod-core -p pod-agents -p pod-net`
  - `git diff --check`

### Iteration 160
- [x] Added `apps/pod-headless` as a new workspace app and first non-UI entrypoint for deterministic multi-world agent/team evaluation.
- [x] Implemented the built-in `deadman-neural-cup` scenario on top of `AgentTeamDefinition`, `WorldRealityDefinition`, `CrossWorldLinkDefinition`, and `WorldTournamentDefinition`, with deterministic per-world seed derivation and per-world `run_flagship_mmo_acceptance(...)` execution.
- [x] Added authoritative report assembly for world runtime metrics, replay-derived reward totals/reason counts, cross-world trigger matching, projected effect envelopes, and team standings.
- [x] Added deterministic unit coverage for seed derivation, topology wiring, propagation math, trigger tagging, effect projection, and standing accumulation.
- [x] Revalidated touched targets:
  - `cargo test -p pod-headless`
  - `cargo check -p pod-headless`
  - `cargo run -p pod-headless -- --profile ci-smoke --scenario deadman-neural-cup --output /tmp/pod-headless-report.json`
  - `git diff --check`

### Iteration 161
- [x] Extended `apps/pod-headless` with `--dataset-output`, so the new app can emit a reward-aware dataset artifact in addition to the scenario summary report.
- [x] Added replay-derived dataset rows that carry world metadata, runtime profile metadata, `ReplayTrainingSample`, and authoritative reward-reason breakdowns for each agent/tick row.
- [x] Added dataset summary aggregation to the main `pod-headless` report so reward totals and reason counts are visible even when only the scenario report is consumed.
- [x] Added deterministic coverage for reward-reason aggregation and dataset summary totals, then revalidated the live app path with both `--output` and `--dataset-output`.
- [x] Revalidated touched targets:
  - `cargo test -p pod-headless`
  - `cargo check -p pod-headless`
  - `cargo run -p pod-headless -- --profile ci-smoke --scenario deadman-neural-cup --output /tmp/pod-headless-report.json --dataset-output /tmp/pod-headless-dataset.json`
  - `git diff --check`

### Iteration 162
- [x] Added deterministic roster admission in `apps/pod-headless`, binding authoritative runtime agents to admitted teams per world based on `active_team_ids`, `allowed_world_ids`, and `max_agents`.
- [x] Threaded that admission metadata into reward-aware dataset rows so each exported training row now carries `team_id` and `team_slot` in addition to world metadata and runtime profile.
- [x] Reworked team standings to use admission-aware assigned-agent counts, dataset-row counts, and world reward totals, then layered applied cross-world score/death-mark state on top.
- [x] Added `applied_world_states` to the main scenario report, aggregating projected cross-world effects into target-world team/resource/faction/objective state summaries.
- [x] Added deterministic unit coverage for roster admission, applied target-world state aggregation, and the new standings shape.
- [x] Revalidated touched targets:
  - `cargo test -p pod-headless`
  - `cargo run -p pod-headless -- --profile ci-smoke --scenario deadman-neural-cup --output /tmp/pod-headless-report.json --dataset-output /tmp/pod-headless-dataset.json`
  - `git diff --check`

### Iteration 163
- [x] Added canonical `QuestStateGraph` definitions to the `deadman-neural-cup` scenario in `apps/pod-headless`, plus per-world quest bindings so the headless runner has authored quest-line state instead of treating `ObjectiveStateShift` as an anonymous counter.
- [x] Extended `apps/pod-headless` reports with `quest_graphs`, `applied_world_states[].quest_lines`, and `unresolved_objective_state_shifts`, so alternate-reality objective links now resolve into explicit start/current/completed/pending quest progression per world.
- [x] Tightened cross-world application semantics in `apps/pod-headless` so zero-application projections no longer mutate applied team/resource/quest state, and covered the new quest progression path with deterministic unit tests.
- [x] Revalidated touched targets:
  - `cargo test -p pod-headless`
  - `cargo check -p pod-headless`
  - `cargo run -p pod-headless -- --profile ci-smoke --scenario deadman-neural-cup --output /tmp/pod-headless-report.json --dataset-output /tmp/pod-headless-dataset.json`
  - `git diff --check`

### Iteration 164
- [x] Added `evaluation` to the `apps/pod-headless` scenario report, summarizing controller mix across the full run and per world using the same replay-derived dataset rows already exported by `--dataset-output`.
- [x] Added per-world evaluation metrics for quest-line progress and applied cross-world effects, so linked-world runs now expose objective progression, score pressure, death-mark pressure, and world-level controller mix without requiring downstream ad hoc analysis.
- [x] Added deterministic unit coverage for the evaluation aggregator and revalidated the live headless scenario artifact with the widened report schema.
- [x] Revalidated touched targets:
  - `cargo test -p pod-headless`
  - `cargo check -p pod-headless`
  - `cargo run -p pod-headless -- --profile ci-smoke --scenario deadman-neural-cup --output /tmp/pod-headless-report.json --dataset-output /tmp/pod-headless-dataset.json`
  - `git diff --check`

### Iteration 165
- [x] Added shared remote-topology summary contracts to `crates/pod-core/src/contract.rs`, including `WorldQuestBinding`, `AppliedWorldStateSummary`, `ScenarioEvaluationSummary`, and the top-level `RemoteTopologyBundle`.
- [x] Exported that shared topology bundle surface from `crates/pod-core/src/lib.rs` and covered the TOON/document contract with deterministic unit tests.
- [x] Extended `apps/pod-headless` with `--topology-output`, so the headless runner now emits a reusable remote-topology artifact alongside the scenario report and reward-aware dataset export.
- [x] Replaced the app-local quest/evaluation report structs in `apps/pod-headless` with the shared `pod-core` summary contracts and added deterministic unit coverage for the new CLI path plus bundle assembly.
- [x] Revalidated touched targets:
  - `cargo test -p pod-core contract -- --nocapture`
  - `cargo test -p pod-headless`
  - `cargo check -p pod-core -p pod-headless`
  - `git diff --check`

### Iteration 166
- [x] Added shared remote-topology caching and resolution helpers to `crates/pod-stdb/src/client.rs`, so the client cache can resolve active world identity, admitted team keys, world quest bindings, applied world state, and world evaluation summaries from a `RemoteTopologyBundle`.
- [x] Added `StdbEvent::RemoteTopologyUpdated` and used it in `crates/pod-net/src/client_stdb.rs` to rebuild a full snapshot when remote topology changes after welcome/subscription handoff.
- [x] Extended `crates/pod-net/src/snapshot.rs` and `crates/pod-net/src/client_stdb.rs` so entity snapshots now carry remote world/team metadata (`team_key`, `world_id`, `world_role`, `world_active_quest_graph_ids`) instead of leaving those relationships trapped in app-local report JSON.
- [x] Added deterministic unit/integration coverage in `crates/pod-stdb` and `crates/pod-net` for topology resolution, widened `StdbEvent` surface construction, and topology-triggered snapshot refresh behavior.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client`
  - `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
  - `cargo check -p pod-stdb --no-default-features --features client`
  - `cargo check -p pod-net --features spacetimedb`
  - `git diff --check`

### Iteration 167
- [x] Extended `crates/pod-net/src/client_stdb.rs` with public remote-topology accessors (`remote_topology`, `remote_world_id`, `remote_applied_world_state`, `remote_world_evaluation`) so remote consumers can inspect linked-world quest/evaluation state without reaching into the raw `StdbClient`.
- [x] Added multi-world linked-world / neural-swarm coverage in `apps/pod-headless/src/main.rs`, proving `RemoteTopologyBundle` preserves quest-line progress, cross-world application counts, and world-level neural evaluation for shadow-world tournament flows.
- [x] Added remote-client coverage in both `crates/pod-net/src/client_stdb.rs` and `crates/pod-net/tests/networking_integration.rs`, proving the public SpacetimeDB client resolves linked-world quest/evaluation state and still projects the correct team/world quest metadata into snapshots.
- [x] Revalidated touched targets:
  - `cargo test -p pod-headless`
  - `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb --test networking_integration -- --nocapture`
  - `git diff --check`

### Iteration 168
- [x] Added a TOON-document ingest path to `crates/pod-stdb/src/client.rs`, so `RemoteTopologyBundle` can be decoded and applied from an authority-style `remote_topology_bundle` document instead of only as an injected Rust struct.
- [x] Added `StdbEvent::RemoteTopologyDocumentReceived` plus a typed `StdbError::DocumentError`, so remote-topology source documents are preserved for inspection and document decode failures stop collapsing into generic state errors.
- [x] Extended `crates/pod-net/src/client_stdb.rs` with `apply_remote_topology_document(...)`, forwarded the source document through `ServerMessage::DebugDocument`, and added deterministic unit/integration coverage proving the decoded topology still resolves world/evaluation state.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client`
  - `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb --test networking_integration -- --nocapture`
  - `cargo check -p pod-net --features spacetimedb`
  - `git diff --check`

### Iteration 169
- [x] Added a generic `receive_debug_document(...)` ingress to `crates/pod-stdb/src/client.rs`, so `remote_topology_bundle`, `versioned_tick_telemetry`, `agent_tool_call_event`, `agent_tick_rollup`, and `focused_entity_debug_summary` TOON documents now share one authority-document dispatch path.
- [x] Added deterministic `pod-stdb` coverage for the new generic path, including remote-topology decode, tool-call dispatch plus focused-summary synthesis, and rejection of unsupported debug document kinds.
- [x] Extended `crates/pod-net/src/client_stdb.rs` with `apply_debug_document(...)`, moved the remote-topology document unit/integration coverage onto that generic path, and kept `apply_remote_topology_document(...)` as a compatibility alias instead of the primary remote ingress seam.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client`
  - `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb --test networking_integration -- --nocapture`
  - `git diff --check`

### Iteration 170
- [x] Added a public `remote_topology_document` event row in `crates/pod-stdb/src/events.rs` plus a `publish_remote_topology_document` reducer in `crates/pod-stdb/src/reducers.rs`, so authority tooling has a real SpacetimeDB table/reducer surface for `remote_topology_bundle` publication instead of depending on client-local injection.
- [x] Added reducer-side topology publish summarization coverage in `crates/pod-stdb/src/reducers.rs`, validating the canonical TOON document type and extracted publish metadata (`generated_at_unix_ms`, scenario/profile id, world/team counts).
- [x] Extended `crates/pod-stdb/src/client.rs` with row-based `receive_remote_topology_document_row(...)` ingestion, stale-row protection, and updated subscription query sets so spectator/player/editor surfaces all subscribe to `remote_topology_document`.
- [x] Extended `crates/pod-net/src/client_stdb.rs` and `crates/pod-net/tests/networking_integration.rs` to use the row-based remote-topology feed path, proving the public SpacetimeDB client now resolves world/evaluation state from an authority-published row instead of only from direct document injection.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client`
  - `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb --test networking_integration -- --nocapture`
  - `cargo check -p pod-net --features spacetimedb`
  - `cargo check -p pod-stdb --no-default-features --features module --target wasm32-unknown-unknown`
  - `git diff --check`

### Iteration 171
- [x] Added `GeneratedRuntimeEvent` plus `GeneratedRuntimeAdapter` in `crates/pod-stdb/src/client.rs`, giving generated mode a minimal runtime seam for connect/disconnect, subscription application, and authority-fed `remote_topology_document` row delivery.
- [x] Extended `crates/pod-stdb/src/client.rs` so `StdbConnectionMode::Generated` now uses the injected runtime adapter instead of hard-failing immediately, while preserving the explicit error path when no runtime is wired.
- [x] Added deterministic generated-mode coverage in `crates/pod-stdb/src/client.rs`, `crates/pod-stdb/tests/client_integration.rs`, and `crates/pod-net/src/client_stdb.rs`, proving runtime-fed topology rows update resolved world/evaluation state and forward the source document through `ServerMessage::DebugDocument`.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client --lib`
  - `cargo test -p pod-stdb --no-default-features --features client --test client_integration`
  - `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
  - `cargo check -p pod-stdb --no-default-features --features client`
  - `cargo check -p pod-net --features spacetimedb`
  - `git diff --check`

### Iteration 172
- [x] Added `integration_remote_topology_feed_rows_handle_world_switch_and_stale_churn` in `crates/pod-net/tests/networking_integration.rs`, proving the public authority-fed row path can switch the resolved world/quest metadata on a newer `remote_topology_document` and ignore an older stale row without rolling back state.
- [x] Revalidated touched targets:
  - `cargo test -p pod-net --features spacetimedb --test networking_integration -- --nocapture`
  - `git diff --check`

### Iteration 173
- [x] Extended `apps/pod-headless/src/main.rs` so the main report now carries `world_quest_bindings` plus a `topology_parity` summary that verifies the exported `RemoteTopologyBundle` matches the teams/worlds/links/quest graphs, applied world states, and evaluation data already published by the headless scenario runner.
- [x] Added deterministic `pod-headless` coverage proving the topology parity report passes for matching bundles and flags missing evaluation/binding state when the topology artifact drifts.
- [x] Revalidated touched targets:
  - `cargo test -p pod-headless`
  - `cargo check -p pod-headless`
  - `git diff --check`

### Iteration 174
- [x] Extended `scripts/run_moat_benchmarks.ts` so the combined moat artifact now runs `pod-headless`, records `headlessTopology`, and fails if `topology_parity` drifts from the exported `RemoteTopologyBundle`.
- [x] Added deterministic Bun coverage in `scripts/run_moat_benchmarks.test.ts` for passing and failing headless topology parity projection.
- [x] Extended `scripts/publish_moat_snapshots.ts` and `scripts/publish_moat_snapshots.test.ts` so committed shard-target moat snapshots preserve the new headless topology parity summary instead of dropping it.
- [x] Revalidated touched targets:
  - `bun test scripts/run_moat_benchmarks.test.ts scripts/publish_moat_snapshots.test.ts`
  - `bun ./scripts/run_moat_benchmarks.ts --profile ci-smoke --skip-browser --output artifacts/moat-benchmarks-ci-local.json`
  - `git diff --check`

### Iteration 175
- [x] Added `pod-stdb` coverage proving a newer `remote_topology_document` row can update quest bindings, applied world state, and evaluation inside the same resolved world without requiring a world switch.
- [x] Added matching public `pod-net` integration coverage proving the authority-fed row path rebuilds snapshot metadata for same-world quest binding churn and preserves the newer quest/effect state when a stale older row arrives afterward.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client test_receive_remote_topology_document_row_updates_quest_and_effect_state_within_same_world -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb --test networking_integration integration_remote_topology_feed_rows_update_quest_and_effect_state_within_same_world -- --nocapture`
  - `git diff --check`

### Iteration 176
- [x] Replaced the ad hoc generated-runtime test fakes with a reusable `GeneratedRuntimeBridge` plus `GeneratedRuntimeHandle` in `crates/pod-stdb/src/client.rs`, so generated-mode callbacks now flow through the same queue/event drain path in both `pod-stdb` and `pod-net`.
- [x] Ported `pod-stdb` generated integration coverage onto that bridge in `crates/pod-stdb/tests/client_integration.rs`, proving generated-mode topology rows still update resolved state and preserve the expected subscription flow without per-test runtime implementations.
- [x] Added matching generated-path same-world quest/effect churn coverage in `crates/pod-stdb/src/client.rs` and `crates/pod-net/src/client_stdb.rs`, proving newer generated-mode topology rows update quest bindings, applied world state, evaluation, and snapshot metadata while stale older rows are ignored.
- [x] Removed the last leftover `FakeGeneratedRuntime` unit-test helper from `crates/pod-stdb/src/client.rs`, so all in-tree generated-mode tests now exercise the shared bridge/handle path instead of split fake-runtime implementations.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client generated -- --nocapture`
  - `cargo test -p pod-stdb --no-default-features --features client generated_mode_runtime_adapter_processes_topology_rows -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb generated_runtime -- --nocapture`
  - `git diff --check`

**Last updated**: Iteration 176
**Current focus**: Iteration 177 benchmark the authority-row and generated-bridge topology feed paths against exported `RemoteTopologyBundle` artifacts, then move that harness into the moat suite

### Iteration 177
- [x] Added `build_topology_feed_measurements(...)` plus serialized `TopologyFeedMeasurementsReport` contracts in `crates/pod-net/src/client_stdb.rs`, so `pod-net` can replay an exported `RemoteTopologyBundle` through both direct authority-row ingestion and generated-bridge ingestion and check world-by-world quest/applied-state/evaluation parity.
- [x] Added the runnable `cargo run -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input ... --fail-on-checks` surface in `crates/pod-net/examples/topology_feed_benchmark_suite.rs`.
- [x] Added deterministic `pod-net` coverage proving the benchmark report passes on a canonical multi-world topology bundle and that both ingestion paths resolve the same world/quest/effect/evaluation state.
- [x] Revalidated touched targets:
  - `cargo test -p pod-net --features spacetimedb test_build_topology_feed_measurements_matches_authority_and_generated_paths -- --nocapture`
  - `cargo check -p pod-net --features spacetimedb --example topology_feed_benchmark_suite`
  - `cargo run -q -p pod-headless -- --profile ci-smoke --topology-output /tmp/pod-headless-topology.json`
  - `cargo run -q -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input /tmp/pod-headless-topology.json --fail-on-checks`
  - `git diff --check`

**Last updated**: Iteration 177
**Current focus**: Iteration 178 integrate `topology_feed_benchmark_suite` into the combined moat suite, then replace its generated-bridge hook path with real generated SpacetimeDB callback wiring when the binding layer is available

### Iteration 178
- [x] Extended `scripts/run_moat_benchmarks.ts` so the combined moat artifact now runs `topology_feed_benchmark_suite`, emits `topologyFeedMeasurements`, fails on topology feed parity drift, and bumps the combined artifact schema to `4`.
- [x] Added deterministic Bun coverage in `scripts/run_moat_benchmarks.test.ts` for passing and failing `topologyFeedMeasurements` parity checks.
- [x] Extended `scripts/publish_moat_snapshots.ts` and `scripts/publish_moat_snapshots.test.ts` so committed shard-target moat snapshots preserve the remote topology feed benchmark under `topologyFeed` and bump the published snapshot schema to `3`.
- [x] Revalidated touched targets:
  - `bun test scripts/run_moat_benchmarks.test.ts scripts/publish_moat_snapshots.test.ts`
  - `bun ./scripts/run_moat_benchmarks.ts --profile ci-smoke --skip-browser --skip-creator --output artifacts/moat-benchmarks-ci-local.json`
  - `git diff --check`

**Last updated**: Iteration 178
**Current focus**: Iteration 179 replace the remaining ad hoc generated-topology callback wiring with the shared callback bridge and extend the generated path from same-world churn to linked-world quest/effect updates

### Iteration 179
- [x] Added `GeneratedBindingCallbacks`, `GeneratedRemoteTopologyDocumentRow`, `GeneratedRuntimeTrace`, and `build_generated_runtime_callback_bridge(...)` in `crates/pod-stdb/src/client.rs`, so generated-mode benchmarks and tests now drive topology updates through the same typed callback surface a real generated SpacetimeDB binding layer would use.
- [x] Replaced the last ad hoc generated-topology bridge wiring in `crates/pod-stdb/tests/client_integration.rs` and `crates/pod-net/src/client_stdb.rs`, including the moat-facing `build_topology_feed_measurements(...)` path, with the shared callback bridge plus typed row inserts.
- [x] Added generated-path linked-world quest/effect churn coverage in both `crates/pod-stdb/src/client.rs` and `crates/pod-net/src/client_stdb.rs`, proving newer generated-mode topology rows update linked-world quest bindings, applied state, evaluation, and snapshot metadata while stale older rows cannot roll the shadow-world state back.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client generated -- --nocapture`
  - `cargo test -p pod-stdb --no-default-features --features client generated_mode_runtime_adapter_processes_topology_rows -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb generated_runtime -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb test_build_topology_feed_measurements_matches_authority_and_generated_paths -- --nocapture`
  - `cargo check -p pod-stdb --no-default-features --features client`
  - `cargo check -p pod-net --features spacetimedb`
  - `git diff --check`

**Last updated**: Iteration 179
**Current focus**: Iteration 180 move the moat/public generated path onto a command-driven binding runtime, then swap that seam over to actual generated SpacetimeDB callbacks when the binding layer is available

### Iteration 180
- [x] Added `GeneratedBindingCommand`, `GeneratedBindingRuntime`, and `GeneratedBindingEndpoint` in `crates/pod-stdb/src/client.rs`, so generated mode now has a command-driven runtime seam that records outbound connect/subscribe/disconnect requests and accepts inbound callbacks separately instead of auto-acking connect/subscription hooks.
- [x] Moved the public generated-mode integration path in `crates/pod-stdb/tests/client_integration.rs` and the moat/public generated path in `crates/pod-net/src/client_stdb.rs` onto that command-driven runtime, including explicit connect/subscription command assertions before topology-row callbacks are delivered.
- [x] Kept `GeneratedRuntimeBridge` as a lightweight hook seam for focused unit tests, but updated docs/comments to make `GeneratedBindingRuntime` the live-like generated binding path.
- [x] Added `install_generated_binding_runtime(...)` to both `StdbClient` and `pod-net::SpacetimeDBClient`, so generated-mode consumers can install the command-driven runtime without manual `GeneratedBindingRuntime::new()` plus adapter injection boilerplate.
- [x] Revalidated touched targets:
  - `cargo test -p pod-stdb --no-default-features --features client generated_mode_runtime_adapter_processes_topology_rows -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb generated_runtime -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb test_build_topology_feed_measurements_matches_authority_and_generated_paths -- --nocapture`
  - `cargo check -p pod-net --features spacetimedb`
  - `git diff --check`

**Last updated**: Iteration 180
**Current focus**: Iteration 181 install the real generated SpacetimeDB SDK runtime and route live callback delivery through it, then push that live feed into parity/evaluation harnesses
- [x] Installed real generated Rust bindings for `pod-stdb` under `crates/pod-stdb/src/module_bindings` and upgraded the repo toolchain to Rust `1.93.0`, so the generated client path is now buildable in-tree instead of blocked on SDK/toolchain drift.
- [x] Added `GeneratedSdkRuntime` in `crates/pod-stdb/src/client.rs`, so generated mode can now use the actual generated `DbConnection`, typed `remote_topology_document` table callbacks, and real subscription lifecycle instead of only the synthetic command-queue seam.
- [x] Added `install_generated_sdk_runtime(...)` to both `StdbClient` and `pod-net::SpacetimeDBClient`, plus closed-port regression tests proving generated mode now attempts the real SDK-backed connection path and reports connection failures through the public error surface.

**Last updated**: Iteration 222
**Current focus**: Iteration 223 replace static HTTP shard-scope token maps with a shared authz policy source so browser/editor consumers do not depend on hardcoded per-service shard allowlists
- [x] Added `TopologyFeedMeasurementsOptions`, `TopologyFeedGeneratedRuntimeMode`, and `LiveGeneratedSdkTopologyFeedConfig` in `crates/pod-net/src/client_stdb.rs`, so the topology feed benchmark can now choose between the deterministic command-driven generated path and a live SDK-backed generated path.
- [x] Added a live generated SDK publisher path in `crates/pod-net/src/client_stdb.rs`, so `build_topology_feed_measurements_with_options(...)` can connect with `install_generated_sdk_runtime()`, publish `publish_remote_topology_document`, and wait for real `remote_topology_document` callbacks when pointed at a running module.
- [x] Extended `crates/pod-net/examples/topology_feed_benchmark_suite.rs` with `--generated-sdk-host`, `--generated-sdk-auth-token`, and `--generated-sdk-timeout-ms`, plus deterministic tests proving the new example flags parse and closed-port live SDK failures surface cleanly.
- [x] Ran `pod-headless` to export `/tmp/pod-headless-topology.json`, started a local in-memory SpacetimeDB on `127.0.0.1:3100`, published `pod_stdb.wasm` to `deadman-prime`, `deadman-shadow`, and `sanctuary-echo`, and executed `topology_feed_benchmark_suite --generated-sdk-host http://127.0.0.1:3100 --fail-on-checks`.
- [x] Captured the first live generated SDK parity artifact at `artifacts/topology-feed-live-local.json`; all `30/30` checks passed across authority-row and generated-runtime resolution for the three benchmark worlds.
- [x] Refreshed the published shard-target transport baselines in `crates/pod-net/src/server.rs` to the current deterministic benchmark output, with `steady-delta total/max = 1392/174` and aggregate full/recovery/delta totals of `1220/234/1904`.
- [x] Extended `scripts/publish_moat_snapshots.ts` and `scripts/publish_moat_snapshots.test.ts` so shard-target weekly snapshots can merge a moat report with separately generated browser render-route and live topology-feed artifacts.
- [x] Captured the first live shard-target topology artifact at `artifacts/topology-feed-live-shard-local.json` and published the first committed weekly snapshot at `docs/benchmark-snapshots/2026-W11-shard-target.json`.
- [x] Added `scripts/run_shard_target_snapshot.ts` plus `scripts/run_shard_target_snapshot.test.ts`, so the previously manual shard-target topology capture, local SpacetimeDB publish, live generated-SDK benchmark, and weekly snapshot publication flow now runs through one reproducible Bun command.
- [x] Documented the new one-command weekly routine in `README.md` and `docs/benchmark-suite.md`, including the current `artifact_only` behavior when browser render-route gates fail but still write an artifact.
- [x] Added `crates/pod-agents/src/controller_harness.rs` plus `crates/pod-agents/examples/controller_parity_benchmark.rs`, so scripted, LLM, hybrid, and neural agents now run through the same curated evaluation harness with published validity, objective, encounter, latency, tool-call, and parity metrics.
- [x] Added deterministic `pod-agents` coverage for the controller parity harness and documented the standalone benchmark command in `README.md` and `docs/benchmark-suite.md`.
- [x] Added shared remote-agent gameplay contract types in `crates/pod-core/src/contract.rs`, including explicit observation budgets, action budgets, heartbeat limits, and fallback/runtime-status state for SpacetimeDB-backed remote agents.
- [x] Threaded the shared observation/action contract constants through `crates/pod-stdb`, so observation caps and default per-tick action budgets now come from `pod-core` instead of private duplicated literals.
- [x] Extended `crates/pod-stdb/src/client.rs` with cached observation ticks, enabling remote clients to measure stale-authority age from the authoritative observation stream instead of inferring freshness indirectly.
- [x] Extended `crates/pod-net/src/client_stdb.rs` with `connect_remote_agent(...)`, explicit remote-agent contract/status accessors, and client-side rejection of budget-overflow, missing-observation, stale-observation, and heartbeat-timeout action batches before they hit the reducer path.
- [x] Added deterministic remote-agent stale-decision coverage in `crates/pod-net/src/client_stdb.rs` plus the supporting `pod-stdb` cache assertion, and revalidated with targeted `pod-core`, `pod-stdb`, and `pod-net` tests plus `cargo check`.
- [x] Added `scripts/compare_moat_snapshots.ts` plus `scripts/compare_moat_snapshots.test.ts`, so committed shard-target weekly snapshots can now be compared as a structured report instead of manual JSON inspection.
- [x] Documented the comparison command in `README.md` and `docs/benchmark-suite.md`, and moved the roadmap backlog forward so the remaining missed item is the red browser render-route gate.
- [x] Added `WorldAdmissionAssignment`, `WorldAdmissionSummary`, `assign_roster_to_world_teams(...)`, and `build_world_admission_summary(...)` in `crates/pod-core/src/contract.rs`, so admitted team-slot assignment is now a shared topology contract instead of `apps/pod-headless` private logic.
- [x] Extended `RemoteTopologyBundle` and `RemoteTopologyParitySummary` with `world_admissions`, then moved `apps/pod-headless`, `crates/pod-stdb`, and `crates/pod-net` onto that shared admission surface.
- [x] Added deterministic contract/headless/client coverage for the shared admission surface, including `pod-net::SpacetimeDBClient::remote_world_admissions()`.

### Iteration 192
- [x] Added `AgentTypeCountSummary`, `WorldControlAssignmentSummary`, `WorldTeamControlSummary`, `WorldControlPlaneSummary`, and `build_world_control_plane_summary(...)` in `crates/pod-core/src/contract.rs`, so admitted roster/controller composition is now a shared topology contract instead of `apps/pod-headless` private report logic.
- [x] Extended `RemoteTopologyBundle` and `RemoteTopologyParitySummary` with `world_control_planes`, then moved `apps/pod-headless`, `crates/pod-stdb`, and `crates/pod-net` onto that shared control-plane surface.
- [x] Added deterministic contract/headless/client/integration coverage for the shared control-plane surface, including `pod-stdb::StdbClient::resolved_remote_world_control_plane()` and `pod-net::SpacetimeDBClient::remote_world_control_plane()`.
- [x] Validation:
  - `cargo test -p pod-core contract -- --nocapture`
  - `cargo test -p pod-headless -- --nocapture`
  - `cargo test -p pod-stdb --no-default-features --features client -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb --test networking_integration -- --nocapture`
  - `cargo check -p pod-core -p pod-headless`
  - `cargo check -p pod-stdb --no-default-features --features client`
  - `cargo check -p pod-net --features spacetimedb`
  - `git diff --check`

### Iteration 193
- [x] Added `TeamRewardLedgerSummary`, `TournamentTeamStandingSummary`, `TournamentControlPlaneSummary`, and `build_tournament_control_plane_summary(...)` in `crates/pod-core/src/contract.rs`, so tournament standings/control-plane rollups are now shared runtime contracts instead of `apps/pod-headless` private aggregation.
- [x] Moved `apps/pod-headless` onto the shared tournament-control-plane builder, so the main report now emits `tournament_control_plane` from `pod-core` and keeps `standings` as a compatibility copy of the shared summary.
- [x] Added deterministic contract/headless coverage for the shared tournament-control-plane surface and revalidated downstream consumer compilation with `pod-stdb` and `pod-net` checks.
- [x] Validation:
  - `cargo test -p pod-core contract -- --nocapture`
  - `cargo test -p pod-headless -- --nocapture`
  - `cargo check -p pod-core -p pod-headless`
  - `cargo check -p pod-stdb --no-default-features --features client`
  - `cargo check -p pod-net --features spacetimedb`
  - `git diff --check`

### Iteration 194
- [x] Extended `RemoteTopologyBundle` and `RemoteTopologyParitySummary` with the shared `tournament_control_plane` surface, so tournament standings/control-plane state now travels through the same remote topology contract as admissions, world control planes, quest bindings, applied world state, and evaluation.
- [x] Updated `apps/pod-headless`, `crates/pod-stdb`, and `crates/pod-net` to emit, resolve, and expose the shared tournament control plane, including `StdbClient::resolved_remote_tournament_control_plane()` and `pod-net::SpacetimeDBClient::remote_tournament_control_plane()`.
- [x] Extended `crates/pod-net/src/client_stdb.rs` topology-feed parity reporting to check tournament-control-plane parity on both authority-row and generated-runtime paths.
- [x] Validation:
  - `cargo test -p pod-core contract -- --nocapture`
  - `cargo test -p pod-headless topology_parity -- --nocapture`
  - `cargo test -p pod-stdb --no-default-features --features client -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb client_stdb -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb --test networking_integration -- --nocapture`
  - `cargo check -p pod-core -p pod-headless`
  - `cargo check -p pod-stdb --no-default-features --features client`
  - `cargo check -p pod-net --features spacetimedb`
  - `git diff --check`

### Iteration 195
- [x] Added `TournamentOrchestrationPhase`, `WorldTournamentOrchestrationSummary`, `TournamentOrchestrationSummary`, and `build_tournament_orchestration_summary(...)` in `crates/pod-core/src/contract.rs`, so world-by-world tournament pressure and phase/state rollups are now shared runtime contracts instead of future `pod-headless`-local aggregation.
- [x] Extended `RemoteTopologyBundle` and `RemoteTopologyParitySummary` with the shared `tournament_orchestration` surface, so world-pressure drift now travels through the same remote topology contract as admissions, world control planes, tournament control planes, quest bindings, applied world state, and evaluation.
- [x] Updated `apps/pod-headless`, `crates/pod-stdb`, and `crates/pod-net` to emit, resolve, and expose the shared tournament orchestration summary, including `StdbClient::resolved_remote_tournament_orchestration()`, `StdbClient::resolved_remote_world_tournament_orchestration()`, `pod-net::SpacetimeDBClient::remote_tournament_orchestration()`, and `pod-net::SpacetimeDBClient::remote_world_tournament_orchestration()`.
- [x] Extended `crates/pod-net/src/client_stdb.rs` topology-feed parity reporting to check tournament-orchestration parity on both authority-row and generated-runtime paths.
- [x] Validation:
  - `cargo test -p pod-core contract -- --nocapture`
  - `cargo test -p pod-headless topology_parity -- --nocapture`
  - `cargo test -p pod-stdb --no-default-features --features client generated_mode_runtime_adapter_processes_topology_rows -- --nocapture`
  - `cargo test -p pod-stdb --no-default-features --features client test_apply_remote_topology_resolves_world_and_team_metadata -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb generated_runtime -- --nocapture`
  - `cargo test -p pod-net --features spacetimedb test_build_topology_feed_measurements_matches_authority_and_generated_paths -- --nocapture`
  - `cargo check -p pod-core -p pod-headless`
  - `cargo check -p pod-stdb --no-default-features --features client`
  - `cargo check -p pod-net --features spacetimedb`
  - `git diff --check`

### Iteration 196
- [x] Extended `scripts/run_moat_benchmarks.ts` and `scripts/run_moat_benchmarks.test.ts` so the moat artifact now preserves `headlessTopology.tournamentOrchestration` plus the new `tournament_control_plane_match` / `tournament_orchestration_match` parity checks instead of dropping them from the TypeScript layer.
- [x] Extended `scripts/publish_moat_snapshots.ts`, `scripts/publish_moat_snapshots.test.ts`, `scripts/compare_moat_snapshots.ts`, and `scripts/compare_moat_snapshots.test.ts` so committed weekly shard-target snapshots and structured comparisons now preserve and report tournament/swarm orchestration drift, including topology-feed orchestration parity flags on both authority-row and generated-runtime paths.
- [x] Regenerated `artifacts/moat-benchmarks-shard-local.json`, `artifacts/topology-feed-live-shard-local.json`, and `docs/benchmark-snapshots/2026-W11-shard-target.json` through the scripted shard-target flow, so the committed history now actually contains the new orchestration metrics instead of only the code paths to emit them.
- [x] Validation:
  - `bun test scripts/run_moat_benchmarks.test.ts scripts/publish_moat_snapshots.test.ts scripts/compare_moat_snapshots.test.ts`
  - `bun ./scripts/run_moat_benchmarks.ts --profile shard-target --skip-browser --skip-creator --output artifacts/moat-benchmarks-shard-local.json`
  - `bun ./scripts/run_shard_target_snapshot.ts --label 2026-W11 --reuse-browser-routes`
  - `git diff --check`

### Iteration 197
- [x] Extended `scripts/run_moat_benchmarks.ts` and `scripts/publish_moat_snapshots.ts` TypeScript contracts so `headlessTopology.topologyParity` now explicitly carries `world_admissions_match` and `world_control_planes_match` alongside the already shared tournament parity surfaces.
- [x] Extended `scripts/run_moat_benchmarks.test.ts`, `scripts/publish_moat_snapshots.test.ts`, and the regenerated shard-target artifact path so the published moat/snapshot history reflects the widened headless parity check set instead of silently dropping those shared admission/control-plane invariants.
- [x] Turned tournament/swarm orchestration history from informational-only into an explicit regression gate in `scripts/compare_moat_snapshots.ts`, using exact baseline envelopes for `phase`, `activeWorldCount`, `contestedWorldCount`, `activeLinkCount`, `leadingTeamCount`, `atRiskTeamCount`, `pressureWorldCount`, and `neuralSwarmWorldCount`.
- [x] Extended `scripts/compare_moat_snapshots.test.ts` so orchestration drift now fails as a regression instead of being reported as a generic changed metric, and added explicit envelope metadata to the structured comparison output.
- [x] Revalidated by regenerating the shard-target moat/snapshot artifacts and running a self-compare against `docs/benchmark-snapshots/2026-W11-shard-target.json`, confirming zero regressions while the new orchestration envelopes are active.
- [x] Validation:
  - `bun test scripts/run_moat_benchmarks.test.ts scripts/publish_moat_snapshots.test.ts scripts/compare_moat_snapshots.test.ts scripts/run_shard_target_snapshot.test.ts`
  - `bun ./scripts/run_moat_benchmarks.ts --profile shard-target --skip-browser --skip-creator --output artifacts/moat-benchmarks-shard-local.json`
  - `bun ./scripts/run_shard_target_snapshot.ts --label 2026-W11 --reuse-browser-routes`
  - `bun ./scripts/compare_moat_snapshots.ts --baseline docs/benchmark-snapshots/2026-W11-shard-target.json --candidate docs/benchmark-snapshots/2026-W11-shard-target.json --output artifacts/benchmark-snapshot-comparison.json --fail-on-regressions`
  - `git diff --check`

### Iteration 198
- [x] Extended `scripts/run_shard_target_snapshot.ts` and `scripts/run_shard_target_snapshot.test.ts` with `--compare-baseline`, a structured `comparison` summary block, and automatic snapshot-comparison execution after publication, so the one-command shard-target workflow can now fail on orchestration regressions instead of requiring a separate manual compare command.
- [x] Taught the shard-target wrapper to reuse an existing same-label snapshot as a temporary baseline when `--compare-baseline` is omitted, so deterministic reruns of the current weekly snapshot also pass through the same regression envelope gate.
- [x] Revalidated the integrated workflow by running `scripts/run_shard_target_snapshot.ts` with `--compare-baseline docs/benchmark-snapshots/2026-W11-shard-target.json --reuse-browser-routes`, confirming the wrapper now emits `comparison.status = \"passed\"` and records `compare-shard-snapshot` in the command history.
- [x] Validation:
  - `bun test scripts/run_moat_benchmarks.test.ts scripts/publish_moat_snapshots.test.ts scripts/compare_moat_snapshots.test.ts scripts/run_shard_target_snapshot.test.ts`
  - `bun ./scripts/run_shard_target_snapshot.ts --label 2026-W11 --compare-baseline docs/benchmark-snapshots/2026-W11-shard-target.json --reuse-browser-routes`
  - `git diff --check`

### Iteration 199
- [x] Added `findLatestPriorSnapshotFilename(...)` in `scripts/run_shard_target_snapshot.ts` plus deterministic coverage in `scripts/run_shard_target_snapshot.test.ts`, so the wrapper can auto-discover the latest prior weekly shard-target snapshot instead of relying only on a manual `--compare-baseline` path.
- [x] Fixed the baseline-copy bug in `scripts/run_shard_target_snapshot.ts` by snapshotting the selected baseline into a temporary file before publish, so explicit same-label baselines and auto-selected prior-week baselines cannot be overwritten by the candidate snapshot before comparison runs.
- [x] Revalidated the live wrapper without `--compare-baseline` by running `scripts/run_shard_target_snapshot.ts --label 2026-W11 --reuse-browser-routes`, confirming the command history now compares against the temporary copied baseline instead of the post-publish output path.
- [x] Validation:
  - `bun test scripts/run_moat_benchmarks.test.ts scripts/publish_moat_snapshots.test.ts scripts/compare_moat_snapshots.test.ts scripts/run_shard_target_snapshot.test.ts`
  - `bun ./scripts/run_shard_target_snapshot.ts --label 2026-W11 --reuse-browser-routes`
  - `git diff --check`

### Iteration 200
- [x] Added `buildPublishedComparisonOutputPath(...)` in `scripts/run_shard_target_snapshot.ts` plus deterministic coverage in `scripts/run_shard_target_snapshot.test.ts`, so the shard-target wrapper now retains each successful comparison as `docs/benchmark-snapshots/YYYY-Www-shard-target-comparison.json` instead of leaving the diff report only in `artifacts/`.
- [x] Updated `scripts/run_shard_target_snapshot.ts` to copy the generated comparison artifact into the published benchmark-snapshot directory, normalize the retained baseline/candidate paths back to the published snapshot history, and point both `comparison.report` and `paths.comparisonReport` at that retained path, so historical review keeps the same published location the workflow reports back to operators.
- [x] Revalidated the live wrapper by running `scripts/run_shard_target_snapshot.ts --label 2026-W11 --reuse-browser-routes`, confirming the retained comparison artifact is published beside the weekly snapshot and recorded in the run summary.
- [x] Validation:
  - `bun test scripts/run_moat_benchmarks.test.ts scripts/publish_moat_snapshots.test.ts scripts/compare_moat_snapshots.test.ts scripts/run_shard_target_snapshot.test.ts`
  - `bun ./scripts/run_shard_target_snapshot.ts --label 2026-W11 --reuse-browser-routes`
  - `git diff --check`

### Iteration 201
- [x] Added `scripts/index_benchmark_snapshots.ts` plus deterministic coverage in `scripts/index_benchmark_snapshots.test.ts`, so the retained shard-target benchmark history now publishes both a machine-readable index at `docs/benchmark-snapshots/index.json` and a human-readable report at `docs/benchmark-snapshots/README.md`.
- [x] Wired `scripts/run_shard_target_snapshot.ts` to refresh that retained history index/report automatically after snapshot and comparison publication, so the one-command weekly workflow now keeps the published history surface up to date instead of depending on a separate manual rebuild step.
- [x] Revalidated the retained history path by regenerating the current 2026-W11 shard-target run, confirming the committed comparison artifact, history index, and Markdown report all refresh together.
- [x] Validation:
  - `bun test scripts/index_benchmark_snapshots.test.ts scripts/run_moat_benchmarks.test.ts scripts/publish_moat_snapshots.test.ts scripts/compare_moat_snapshots.test.ts scripts/run_shard_target_snapshot.test.ts`
  - `bun ./scripts/index_benchmark_snapshots.ts`
  - `bun ./scripts/run_shard_target_snapshot.ts --label 2026-W11 --reuse-browser-routes`
### Iteration 202
- [x] Switched the retained shard-target benchmark cadence from monthly labels to ISO week labels, republished the committed weekly baseline as `docs/benchmark-snapshots/2026-W11-shard-target.json`, and updated the wrapper/tests/docs so week-over-week review now uses `YYYY-Www` labels consistently.
  - `git diff --check`

### Iteration 203
- [x] Extracted a typed dedicated-server bootstrap seam in `apps/pod-server/src/main.rs` via `WorldBootstrapPlan`, so map loading and initial idle-agent population now run through one explicit authority bootstrap contract instead of inline app-root logic.
- [x] Extracted a typed dedicated-server transport seam in `apps/pod-server/src/main.rs` via `TransportPolicy` plus `ServerConfig::network_server_config()`, so direct-connect snapshot cadence, inactivity timeout, and queue-pressure thresholds are no longer hardcoded at the `GameServer` call site.
- [x] Added deterministic `pod-server` coverage for transport-policy composition and authoritative bootstrap population, and updated `docs/plugin-model.md` plus `docs/architecture.md` so the current lifecycle docs reflect the new app-local seam instead of claiming the values are still fully hardcoded.
- [x] Validation:
  - `cargo test -p pod-server --bin pod-server runtime_tests -- --nocapture`
  - `cargo check -p pod-server`
  - `git diff --check`

### Iteration 204
- [x] Added `apps/pod-server/src/lib.rs`, exporting `ServerConfig`, `TransportPolicy`, `WorldBootstrapPlan`, `parse_bind_target(...)`, and `build_authoritative_world(...)` so dedicated authority composition now lives on a reusable crate surface instead of only inside the binary entry point.
- [x] Simplified `apps/pod-server/src/main.rs` to consume that exported library contract for config parsing, authoritative world creation, and network-config composition, leaving the binary focused on process startup, runtime loop wiring, and shutdown behavior.
- [x] Moved the dedicated-server seam tests onto the `pod-server` library target and updated the lifecycle docs to point at `apps/pod-server/src/lib.rs` as the current dedicated-authority contract while clarifying that the next remaining gap is pushing that host seam down into a shared engine/runtime crate.
- [x] Validation:
  - `cargo test -p pod-server --lib -- --nocapture`
  - `cargo test -p pod-server --bin pod-server`
  - `cargo check -p pod-server`
  - `git diff --check`

### Iteration 205
- [x] Added `crates/pod-net/src/authority.rs`, moving the authority-host lifecycle contract into a shared runtime crate with `AuthorityRuntimeConfig`, `TransportPolicy`, `WorldBootstrapPlan`, `parse_bind_target(...)`, and `build_authoritative_world(...)`.
- [x] Re-exported that authority surface from `crates/pod-net/src/lib.rs`, updated `apps/pod-server/src/main.rs` to consume `pod-net` directly, and reduced `apps/pod-server/src/lib.rs` to a compatibility re-export so the app crate is no longer the source of truth for authority composition.
- [x] Moved the authority seam tests into `pod-net`, updated the architecture/plugin docs to point at `crates/pod-net/src/authority.rs`, and clarified that the remaining gap is a transport-neutral engine/runtime host contract rather than an app-crate export.
- [x] Validation:
  - `cargo test -p pod-net authority -- --nocapture`
  - `cargo test -p pod-server --bin pod-server`
  - `cargo check -p pod-net -p pod-server`
  - `git diff --check`

### Iteration 206
- [x] Added `crates/pod-core/src/authority.rs`, moving the transport-neutral world/bootstrap half of the authority lifecycle contract into the core runtime as `AuthorityWorldConfig`, `WorldBootstrapPlan`, and `build_authoritative_world(...)`.
- [x] Reduced `crates/pod-net/src/authority.rs` to the transport adapter half of the contract, so `AuthorityRuntimeConfig` now composes `pod_core::AuthorityWorldConfig` plus direct-connect transport policy instead of owning world/bootstrap state itself.
- [x] Updated `apps/pod-server` to build worlds from `config.world`, refreshed the docs to point at the split `pod-core` + `pod-net` lifecycle contract, and clarified that the remaining gap is a single neutral host crate or lifecycle API that composes both halves without app-local glue.
- [x] Validation:
  - `cargo test -p pod-core authority -- --nocapture`
  - `cargo test -p pod-net authority -- --nocapture`
  - `cargo test -p pod-server --bin pod-server`
  - `cargo check -p pod-core -p pod-net -p pod-server`
  - `git diff --check`

### Iteration 207
- [x] Narrowed `crates/pod-net/src/authority.rs` to a pure direct-connect transport adapter by renaming the host-facing config to `DirectConnectTransportConfig`, dropping world/runtime-mode ownership, and keeping only bind/websocket/client/policy composition plus `server_config(tick_rate)`.
- [x] Added `crates/pod-host/src/lib.rs` as the neutral authority host lifecycle crate with `AuthorityHostConfig`, `AuthorityTransportMode`, `AuthorityHostRuntime`, and `DirectConnectAuthorityRuntime`, so one public surface now composes `pod-core` world bootstrap with the selected authority transport.
- [x] Updated `apps/pod-server` to consume `pod-host` instead of stitching `pod-core` and `pod-net` together manually, and refreshed the lifecycle docs so the remaining MMO gap is now multi-shard supervision rather than single-host composition.
- [x] Validation:
  - `cargo test -p pod-net authority -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check -p pod-host -p pod-server`
  - `git diff --check`

### Iteration 208
- [x] Extended `crates/pod-host/src/lib.rs` with `AuthorityShardConfig`, `AuthorityShardSummary`, `ShardSupervisorConfig`, `ShardSupervisorSummary`, `PreparedAuthorityShard`, and `PreparedShardSupervisor`, so one crate-level seam can now validate shard topology, summarize planned capacity, and prepare multiple authority hosts at once.
- [x] Added supervisor launch support in `pod-host` through `PreparedShardSupervisor::run_direct_connect_until_failure()`, using a Tokio `LocalSet` so non-`Send` direct-connect shard runtimes can still be launched concurrently from one orchestrating runtime thread.
- [x] Updated the compatibility re-exports and lifecycle docs so the remaining MMO gap is now aggregated shard health/control-plane supervision rather than merely multi-shard configuration.
- [x] Validation:
  - `cargo test -p pod-host -- --nocapture`
  - `cargo check -p pod-host`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 209
- [x] Extended `crates/pod-net/src/server.rs` with shard-aware `GameServer::new_with_shard_id(...)` construction plus optional `ShardTransportSummary` watch publication, so live direct-connect transport health now reports under real shard ids instead of a hardcoded `direct-connect` label.
- [x] Added `AuthorityShardControlPlaneHandle`, `AuthorityShardControlPlaneSummary`, `ShardSupervisorControlPlaneHandle`, and `ShardSupervisorControlPlaneSummary` in `crates/pod-host/src/lib.rs`, wiring `DirectConnectAuthorityRuntime`, `PreparedAuthorityShard`, and `PreparedShardSupervisor` to expose aggregate live transport/control-plane snapshots across the supervised shard set.
- [x] Updated the `pod-server` compatibility exports and lifecycle docs to point at the new shared control-plane surface, narrowing the remaining MMO gap to incident rollups and coordinated shard lifecycle commands instead of raw transport visibility.
- [x] Validation:
  - `cargo test -p pod-net test_transport_summary -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 210
- [x] Extended `crates/pod-net/src/server.rs` with lifecycle command/state plumbing, so direct-connect shards can now enter drain mode or perform an immediate supervised shutdown instead of only running until process death.
- [x] Added host-level lifecycle and incident control-plane types in `crates/pod-host/src/lib.rs`, including derived `ShardIncidentSummary` payloads, shard/supervisor lifecycle phase rollups, and coordinated `request_drain*` / `request_shutdown*` command fan-out across the supervised shard set.
- [x] Updated the `pod-server` compatibility exports and lifecycle docs to point at the new incident-plus-lifecycle surface, narrowing the remaining MMO gap to shared gameplay/tick incident feeds and durable ops publication rather than raw shard command/control mechanics.
- [x] Validation:
  - `cargo test -p pod-net test_lifecycle_control -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 211
- [x] Added `ShardGameplayIncidentTracker` in `crates/pod-core/src/ops.rs`, moving tick-budget, action-rejection, tool-call, trajectory, and flagship MMO action counters out of `apps/pod-server` private stats and into a shared gameplay-incident tracker with deterministic summary coverage.
- [x] Extended `crates/pod-net/src/server.rs` so `GameServer` records gameplay incidents per tick, publishes live `ShardIncidentSummary` watches, and emits shard incident debug documents beside the existing transport documents.
- [x] Updated `crates/pod-host/src/lib.rs` to carry gameplay incident summaries through `AuthorityShardControlPlaneHandle` and supervisor snapshots, then refreshed the lifecycle docs so the remaining MMO gap is durable shard/supervisor ops publication rather than missing gameplay telemetry on the shared control-plane.
- [x] Validation:
  - `cargo test -p pod-core ops -- --nocapture`
  - `cargo test -p pod-net gameplay_incident_summary -- --nocapture`
  - `cargo test -p pod-net test_lifecycle_control -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 212
- [x] Added `LocalAuthorityRuntime`, `AuthorityShardOpsHandle`, and `ShardSupervisorOpsHandle` in `crates/pod-host/src/lib.rs`, so local and direct-connect authority hosts now expose one shared live TOON ops-feed surface instead of leaving local publication trapped in `apps/pod-server`.
- [x] Extended `crates/pod-net/src/server.rs` with host-facing ops-document broadcasts that stay active without debug clients, then wired `DirectConnectAuthorityRuntime` and `PreparedShardSupervisor` onto that shared host surface.
- [x] Simplified `apps/pod-server/src/main.rs` onto `LocalAuthorityRuntime::step(...)`, removed the app-private shard ops stream, refreshed compatibility exports in `apps/pod-server/src/lib.rs`, and updated the lifecycle docs so the remaining MMO gap is retained multi-shard ops aggregation rather than basic live publication.
- [x] Validation:
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-net ops_documents_for_host_subscribers -- --nocapture`
  - `cargo test -p pod-net broadcast_updates -- --nocapture`
  - `cargo test -p pod-net test_lifecycle_control -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 213
- [x] Added `pod_net::OpsDocumentStream` in `crates/pod-net/src/server.rs`, replacing the old raw host-facing ops broadcast with a shared retained ring-buffer plus live subscription surface that `GameServer` can publish into directly.
- [x] Updated `crates/pod-host/src/lib.rs` so `AuthorityShardOpsHandle` and `ShardSupervisorOpsHandle` now expose retained `AuthorityShardOpsSnapshot` / `ShardSupervisorOpsSnapshot` views for both local and direct-connect runtimes, letting late-joining shard/supervisor consumers inspect recent TOON docs without app-local buffering.
- [x] Refreshed the `pod-server` compatibility exports plus the architecture/plugin docs so the new retained history surface is the documented source of truth, narrowing the next MMO gap to durable ops persistence/export beyond the current in-memory host process.
- [x] Validation:
  - `cargo test -p pod-net test_broadcast_updates_emits_ops_documents_for_host_subscribers -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 214
- [x] Extended `pod_net::OpsDocumentStream` in `crates/pod-net/src/server.rs` with optional persistent JSONL archives that flush each TOON document durably and reload the recent tail on startup, so retained ops history can survive authority-host restarts instead of living only in RAM.
- [x] Updated `crates/pod-host/src/lib.rs` with `OpsPersistenceConfig`, `POD_OPS_ARCHIVE_DIR` wiring, and persisted-count/archive-path fields on `AuthorityShardOpsSnapshot` / `ShardSupervisorOpsSnapshot`, so local and direct-connect hosts now share one durable ops-persistence surface.
- [x] Refreshed the `pod-server` compatibility exports plus the architecture/plugin docs so the new archive-backed shard/supervisor ops surface is the documented source of truth, narrowing the next MMO gap to a shared query/relay layer above those per-shard archives.
- [x] Validation:
  - `cargo test -p pod-net ops_document_stream_persistent_archive_reloads_recent_history -- --nocapture`
  - `cargo test -p pod-host local_runtime_persists_and_reloads_ops_history_from_archive -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 215
- [x] Added `OpsDocumentArchiveSnapshot` in `crates/pod-net/src/server.rs`, so persisted ops archives now have one shared typed loader instead of forcing downstream crates to parse the JSONL format themselves.
- [x] Updated `crates/pod-host/src/lib.rs` with `AuthorityShardOpsArchiveHandle`, `ShardSupervisorOpsArchiveHandle`, and matching snapshot/error types, plus `ops_archive_handle()` helpers on shard/supervisor config and live ops handles, so retained archive queries now ride one crate-level authority seam instead of bespoke file access.
- [x] Refreshed the `pod-server` compatibility exports plus the architecture/plugin docs so the new archive-query surface is the documented source of truth, narrowing the next MMO gap to a process-external relay/service boundary above those in-process handles.
- [x] Validation:
  - `cargo test -p pod-host shard_and_supervisor_archive_handles_query_persisted_history -- --nocapture`
  - `cargo test -p pod-host local_runtime_persists_and_reloads_ops_history_from_archive -- --nocapture`
  - `cargo test -p pod-net ops_document_stream_persistent_archive_reloads_recent_history -- --nocapture`
  - `cargo test -p pod-net broadcast_updates -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

**Next focus**: Add a process-external shard/supervisor archive relay or service layer above the new archive handles so browser/editor/ops consumers can inspect retained history without running inside the authority host.

### Iteration 216
- [x] Added a minimal process-external archive query surface in `crates/pod-host/src/lib.rs` through `OpsArchiveServiceConfig`, `OpsArchiveServiceRequest`, `OpsArchiveServiceResponse`, `OpsArchiveServiceClient`, and `ShardSupervisorOpsArchiveService`, so retained shard/supervisor ops history can now be queried over a bounded JSON-over-TCP request/response seam instead of only through in-process handles.
- [x] Extended the existing shard/supervisor archive surfaces with `archive_service(...)` / `service(...)` constructors and refreshed `apps/pod-server/src/lib.rs` re-exports, so app binaries and external tooling can build against one shared authority-host API instead of bespoke bridge code.
- [x] Added deterministic `pod-host` coverage for end-to-end archive-service querying over TCP and refreshed the architecture/plugin docs so the new process-external query service is the documented source of truth.
- [x] Validation:
  - `cargo test -p pod-host supervisor_archive_service_queries_persisted_history_over_tcp -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

**Next focus**: Add an authenticated or streaming consumer-facing relay above the new process-external archive query service so browser/editor/ops clients can follow retained history without embedding raw TCP request/response logic.

### Iteration 217
- [x] Added an authenticated live relay in `crates/pod-host/src/lib.rs` through `OpsRelayConfig`, `OpsRelayRequest`, `OpsRelayEvent`, `OpsRelayClient`, `OpsRelaySubscription`, and `ShardSupervisorOpsRelayService`, so external consumers can subscribe to an initial retained shard/supervisor snapshot and then follow live ops documents over one bounded line-delimited JSON stream.
- [x] Extended the existing live ops surfaces with `ShardSupervisorOpsHandle::relay(...)` and `PreparedShardSupervisor::ops_relay(...)`, and refreshed `apps/pod-server/src/lib.rs` re-exports, so app binaries and external tooling can build against one shared authenticated relay seam instead of hand-rolled live document bridges.
- [x] Added deterministic `pod-host` coverage for auth rejection plus live document streaming over the new relay and refreshed the architecture/plugin docs so the authenticated relay is the documented source of truth.
- [x] Validation:
  - `cargo test -p pod-host supervisor_ops_relay_requires_auth_and_streams_live_documents -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

**Next focus**: Add a browser/editor-friendly HTTP or WebSocket facade above the new authenticated relay so external clients can consume shard ops streams without embedding the raw TCP contract directly.

### Iteration 218
- [x] Added a browser/editor-friendly HTTP facade in `crates/pod-host/src/lib.rs` through `OpsHttpServiceConfig`, `OpsHttpError`, and `ShardSupervisorOpsHttpService`, exposing retained shard/supervisor archive snapshots as bounded JSON `GET` endpoints and live shard ops as authenticated SSE streams.
- [x] Extended the live ops surfaces with `ShardSupervisorOpsHandle::http_service(...)` and `PreparedShardSupervisor::ops_http_service(...)`, and refreshed `apps/pod-server/src/lib.rs` re-exports so app binaries and tooling can build against one shared HTTP/SSE seam instead of raw TCP relay wiring.
- [x] Added deterministic `pod-host` coverage for HTTP auth rejection, archive snapshot JSON, and retained-plus-live SSE streaming, then revalidated the touched Rust surfaces.
- [x] Validation:
  - `cargo test -p pod-host supervisor_ops_http_service_serves_archive_json_and_sse_streams -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 219
- [x] Added monotonic per-shard ops document sequencing in `crates/pod-net/src/server.rs`, plus archive-backed replay loading through `OpsDocumentRecord` and `OpsDocumentArchiveReplaySnapshot`, so retained shard history can now resume from a cursor instead of only replaying the latest retained window wholesale.
- [x] Added shard and supervisor replay cursor/snapshot surfaces in `crates/pod-host/src/lib.rs`, including `AuthorityShardOpsReplayCursor`, `AuthorityShardOpsReplaySnapshot`, `ShardSupervisorOpsReplayCursor`, and `ShardSupervisorOpsReplaySnapshot`, so in-process consumers can resume retained shard history from shared typed APIs instead of rebuilding replay logic around raw archives.
- [x] Extended `ShardSupervisorOpsHttpService` with replay JSON endpoints and cursor-aware SSE startup, refreshed the `apps/pod-server/src/lib.rs` compatibility re-exports, and added deterministic replay/resume coverage proving HTTP and SSE consumers can resume from cursor state without reloading the entire retained stream.
- [x] Validation:
  - `cargo test -p pod-net ops_document_archive_replay_snapshot_loads_records_after_cursor -- --nocapture`
  - `cargo test -p pod-net broadcast_updates -- --nocapture`
  - `cargo test -p pod-host supervisor_ops_http_service_replays_from_cursor_over_http_and_sse -- --nocapture`
  - `cargo test -p pod-host supervisor_ops_relay_requires_auth_and_streams_live_documents -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 220
- [x] Added durable shard and supervisor replay bookmark helpers in `crates/pod-host/src/lib.rs`, including `encode_*_replay_bookmark(...)`, `decode_*_replay_bookmark(...)`, and `OpsReplayBookmarkError`, so reconnecting clients can persist one opaque resume token instead of reconstructing cursor maps manually.
- [x] Extended `AuthorityShardOpsReplaySnapshot` and `ShardSupervisorOpsReplaySnapshot` with `next_bookmark`, taught the HTTP replay routes to accept `bookmark=...` as an alternative to raw cursor parameters, and updated SSE `shard_document` payloads to carry the latest bookmark as live events advance.
- [x] Refreshed the `apps/pod-server/src/lib.rs` compatibility exports and added deterministic `pod-host` coverage for bookmark round-trips plus bookmark-based HTTP/SSE replay resume.
- [x] Validation:
  - `cargo test -p pod-host replay_bookmarks_round_trip_for_shard_and_supervisor_cursors -- --nocapture`
  - `cargo test -p pod-host supervisor_ops_http_service_replays_from_cursor_over_http_and_sse -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 221
- [x] Added supervisor-level shard selection to `crates/pod-host/src/lib.rs` by extending `ShardSupervisorOpsReplayCursor` and `ShardSupervisorOpsReplaySnapshot` with `selected_shard_ids`, preserving that selection through replay cursors, replay snapshots, and durable supervisor bookmark tokens.
- [x] Extended the supervisor HTTP replay and SSE routes with `shards=...`, validated selected shard ids against the live supervisor handle set, filtered retained replay snapshots plus live SSE subscriptions down to the requested shard subset, and kept bookmark-based resume scoped to the same selected shards.
- [x] Added deterministic `pod-host` coverage for bookmark backward compatibility, filtered supervisor replay over HTTP, and filtered supervisor SSE subscription wiring.
- [x] Validation:
  - `cargo test -p pod-host replay_bookmarks_round_trip_for_shard_and_supervisor_cursors -- --nocapture`
  - `cargo test -p pod-host supervisor_stream_subscription_filters_selected_live_handles -- --nocapture`
  - `cargo test -p pod-host supervisor_ops_http_service_filters_selected_shards_and_preserves_bookmarks -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

### Iteration 222
- [x] Added static HTTP shard-scope authorization in `crates/pod-host/src/lib.rs` through `OpsHttpAuthorizedToken`, keeping the existing full-access `auth_token` path while allowing browser/editor consumers to authenticate with shard-scoped bearer tokens.
- [x] Applied that authorization across supervisor replay, supervisor SSE, and shard-specific archive/replay/stream routes by defaulting supervisor replay scope to the token’s allowed shard set, rejecting disallowed shard requests with `403 Forbidden`, and preserving scoped supervisor bookmarks on live SSE updates.
- [x] Refreshed the `apps/pod-server/src/lib.rs` compatibility exports and added deterministic `pod-host` coverage for scoped token defaults plus forbidden shard rejection.
- [x] Validation:
  - `cargo test -p pod-host supervisor_ops_http_service_applies_scoped_tokens_and_rejects_forbidden_shards -- --nocapture`
  - `cargo test -p pod-host supervisor_stream_subscription_applies_scoped_token_defaults -- --nocapture`
  - `cargo test -p pod-host supervisor_ops_http_service_filters_selected_shards_and_preserves_bookmarks -- --nocapture`
  - `cargo test -p pod-host -- --nocapture`
  - `cargo test -p pod-server --bin pod-server -- --nocapture`
  - `cargo check --workspace`
  - `git diff --check`

**Next focus**: Replace static HTTP shard-scope token maps with a shared authz policy source so browser/editor consumers do not depend on hardcoded per-service shard allowlists.

**Audit backlog surfaced during the 2026-03-13 roadmap scrub**:
- [x] Repaired the browser render-route perf gate so `bun run measure:render-routes:check` now passes on the current shipped asset set, and `apps/pod-web/package.json` now runs showcase and worker smoke as isolated Playwright invocations to avoid the dead web-server handoff that previously masked the gate repair.
