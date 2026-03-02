# IMPLEMENTATION_PLAN.md — Prompt or Die

Priority-sorted task list. One task per iteration. Mark [x] when complete.

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

**Last updated**: Iteration 27
**Current focus**: Phase 9 (Hardening Parity Backlog)
