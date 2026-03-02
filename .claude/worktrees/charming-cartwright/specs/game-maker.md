# Spec: Game Maker / Visual Editor

## Job to Be Done
Provide a visual game editor that enables developers to build agent-centric games without writing every line of code. Scene editing, agent behavior authoring, playtesting, and publishing — all in one tool.

## Requirements

### 1. Editor Shell
- **egui-based** native editor (cross-platform, Rust-native)
- Dockable panels: viewport, hierarchy, inspector, console, asset browser
- Undo/redo stack for all operations
- Project management (create, open, save, export)

### 2. Scene Editor
- Visual placement of entities in 2D/3D viewport
- Gizmos for translate, rotate, scale
- Grid snapping, alignment tools
- Multi-select, group, hierarchy manipulation
- Prefab instantiation and editing

### 3. Agent Behavior Authoring
- **Visual behavior tree editor** — drag-and-drop nodes, wire connections
- **FSM editor** — state/transition graph with visual preview
- **LLM agent config** — model selection, system prompt, observation format, constraints
- **Neural agent config** — policy network architecture, training parameters
- Live preview: run agent in sandbox, see decisions in real-time

### 4. Inspector
- Component editor — edit any component on selected entity
- Add/remove components via dropdown
- Constraint editor — actions_per_tick, cooldowns, permissions
- Custom property editors for complex types

### 5. Play Mode
- In-editor play/stop/pause
- Switch between editor and play camera
- Live entity inspection during play
- Console output from agents (LLM reasoning, BT state)

### 6. Asset Browser
- Browse project assets (meshes, textures, prefabs, scripts, sounds)
- Drag-and-drop into scene
- Preview thumbnails
- Search and filter

### 7. SpacetimeDB Dashboard
- View live table state
- Monitor reducer performance
- Connected clients list
- Event stream viewer

## Success Criteria
- [ ] Editor launches with dockable panels
- [ ] Place entities in 2D viewport with gizmos
- [ ] Edit components in inspector
- [ ] Visual behavior tree creates functional ScriptedAgent
- [ ] Play mode runs game in-editor
- [ ] Asset browser shows project files
- [ ] `cargo test -p pod-editor` passes
