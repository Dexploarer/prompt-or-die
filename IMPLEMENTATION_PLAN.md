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
- [ ] 2.14 Write comprehensive agent SDK tests

## Phase 3: 3D Engine Foundation
- [ ] 3.1 Add Transform3D, Mesh, Material, Camera3D, Light components
- [ ] 3.2 Create forward rendering pipeline with depth buffer in wgpu
- [ ] 3.3 Implement WGSL shader system (vertex/fragment/uniform binding)
- [ ] 3.4 Add glTF 2.0 import (gltf crate)
- [ ] 3.5 Implement frustum culling
- [ ] 3.6 Add batched draw calls (group by material)
- [ ] 3.7 Create camera controller components (orbit, fly, follow)
- [ ] 3.8 Implement parent-child transform hierarchy
- [ ] 3.9 Support 2D + 3D mixed rendering in same frame
- [ ] 3.10 Write render pipeline tests and benchmarks

## Phase 4: Asset Pipeline
- [ ] 4.1 Create pod-assets crate scaffold
- [ ] 4.2 Implement content-addressed asset cache
- [ ] 4.3 Add asset import pipeline (glTF, OBJ, PNG, JPEG)
- [ ] 4.4 Implement mesh processing (LOD generation)
- [ ] 4.5 Add texture processing (compression, atlas packing)
- [ ] 4.6 Implement hot-reload (file watcher + reprocess)
- [ ] 4.7 Add procedural terrain generation (noise-based heightmaps)
- [ ] 4.8 Add procedural dungeon generation (BSP tree)
- [ ] 4.9 Add procedural texture generation (noise, gradients)
- [ ] 4.10 Create AI asset generation integration points (text-to-mesh, text-to-texture)
- [ ] 4.11 Write asset pipeline tests

## Phase 5: Game Maker / Editor
- [ ] 5.1 Create pod-editor crate with egui scaffold
- [ ] 5.2 Implement dockable panel system (viewport, hierarchy, inspector, console)
- [ ] 5.3 Build entity hierarchy panel (tree view)
- [ ] 5.4 Build component inspector (property editors)
- [ ] 5.5 Implement 2D viewport with entity placement gizmos
- [ ] 5.6 Add 3D viewport rendering
- [ ] 5.7 Build asset browser panel
- [ ] 5.8 Implement play/stop/pause mode
- [ ] 5.9 Build visual behavior tree editor
- [ ] 5.10 Build FSM editor
- [ ] 5.11 Add LLM agent configuration panel
- [ ] 5.12 Build SpacetimeDB dashboard panel
- [ ] 5.13 Implement undo/redo system
- [ ] 5.14 Add project save/load
- [ ] 5.15 Write editor tests

## Phase 6: Networking & Multiplayer
- [ ] 6.1 Implement SpacetimeDB subscription manager in pod-net
- [ ] 6.2 Create interest management (spatial SQL query filtering)
- [ ] 6.3 Implement lobby system (SpacetimeDB tables + reducers)
- [ ] 6.4 Add matchmaking reducer
- [ ] 6.5 Remote LLM agent connection via SpacetimeDB
- [ ] 6.6 Spectator mode (full world subscription, read-only)
- [ ] 6.7 World partitioning for large worlds
- [ ] 6.8 Performance benchmarks (target: 1000 agents at 60 TPS)
- [ ] 6.9 Write networking integration tests

---

**Last updated**: Iteration 8 (Phase 2.8–2.10, 2.12–2.13 complete — BT node library, FSM templates, Utility AI, decision logging/replay, ONNX integration; 2.11 hybrid agent in progress)
**Current focus**: Phase 2 — Enhanced Agent SDK (tasks 2.11, 2.14 remaining)
