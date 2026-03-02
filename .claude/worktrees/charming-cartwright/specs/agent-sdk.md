# Spec: Enhanced Agent SDK

## Job to Be Done
Provide a comprehensive SDK so game developers can create, configure, and deploy autonomous AI agents with minimal boilerplate. Support LLMs, neural networks, scripted behaviors, and hybrid agents.

## Requirements

### 1. Agent Trait Evolution
- Current trait: `observe()` + `decide()` — keep as core
- Add lifecycle hooks: `on_spawn`, `on_damage`, `on_death`, `on_interact`, `on_message`
- Add `configure()` for runtime parameter tuning
- Add `introspect()` — expose internal state for debugging/editor

### 2. LLM Agent Improvements
- **Provider abstraction** — OpenAI, Anthropic, local (Ollama), custom endpoints
- **Prompt templates** — configurable observation→prompt formatting
- **Token budget management** — auto-truncate observations to fit context
- **Response parsing** — structured output (JSON mode) → Action mapping
- **Conversation memory** — sliding window, summary, or RAG-based
- **Cost tracking** — tokens used, estimated cost per agent per game-hour
- **Streaming decisions** — start acting on partial LLM response

### 3. Neural Agent Improvements
- **Policy network interface** — generic trait for any NN framework
- **ONNX runtime integration** — load pre-trained models
- **Observation encoding** — configurable feature extraction from Observation
- **Training harness** — record trajectories, compute rewards, export for training
- **Self-play** — pit agents against themselves for improvement

### 4. Scripted Agent Improvements
- **Behavior tree library** — pre-built nodes: patrol, chase, flee, guard, wander
- **FSM templates** — common patterns: idle↔alert↔combat↔dead
- **Utility AI** — score-based action selection as alternative to BT/FSM
- **Lua scripting** — expose agent API to Lua scripts
- **Visual authoring** — behavior trees/FSMs editable in game maker

### 5. Hybrid Agents
- **LLM + BT** — LLM for high-level strategy, BT for low-level execution
- **Neural + Scripted** — neural for combat, scripted for dialogue
- **Hierarchical** — commander agent delegates to subordinate agents
- **Swarm** — group behavior with shared blackboard

### 6. Agent Debugging
- **Decision log** — record every observation→decision with reasoning
- **Replay** — step through agent decisions frame by frame
- **Visualization** — perception cones, pathfinding, target selection
- **Benchmarking** — win rate, survival time, resource efficiency metrics

### 7. SpacetimeDB Integration
- Agent state persisted in SpacetimeDB tables
- Agent decisions submitted as reducer calls
- Remote LLM agents connect via SpacetimeDB subscriptions
- Agent marketplace — share/download agent configurations

## Success Criteria
- [ ] LLM agent plays game via OpenAI-compatible API
- [ ] Neural agent loads ONNX model and makes decisions
- [ ] Behavior tree agent with pre-built patrol/chase behaviors
- [ ] Hybrid agent combining LLM strategy + BT execution
- [ ] Decision log records and replays agent reasoning
- [ ] Agent config serializes to/from file
- [ ] `cargo test -p pod-agents` passes
