# Spec: SpacetimeDB 2.0 Integration

## Job to Be Done
Replace the custom server-authoritative networking with SpacetimeDB 2.0 as the unified backend. Game state lives in tables, game logic runs as reducers, clients subscribe to real-time SQL queries.

## Requirements

### 1. SpacetimeDB Module (Server-Side)

#### Tables (Game State)
- `entities` — All game entities with core components (transform, velocity, health)
- `agents` — Agent metadata (type, constraints, owner identity)
- `components_transform` — Position, rotation, scale per entity
- `components_velocity` — Linear/angular velocity per entity
- `components_health` — Current/max HP, alive status
- `components_perception` — Vision range, FOV, hearing range per agent
- `components_visual` — Color, sprite, mesh reference per entity
- `components_collider` — Shape, size, layer per entity
- `components_label` — Display name per entity
- `world_state` — Tick counter, RNG state, world config (singleton)

#### Reducers (Game Logic)
- `create_world(seed, config)` — Initialize world state
- `spawn_entity(agent_type, position, components)` — Create entity
- `submit_actions(agent_id, actions: Vec<Action>)` — Agent submits decisions
- `execute_tick()` — Run one tick: validate actions → execute → physics → events
- `despawn_entity(entity_id)` — Remove entity
- `connect_agent(identity, agent_type)` — Player/agent joins
- `disconnect_agent(identity)` — Player/agent leaves

#### Event Tables (Transient)
- `observation_events` — Per-agent observations (row-level security: agent sees only its own)
- `combat_events` — Damage dealt, kills, deaths
- `speech_events` — Agent speech/chat
- `world_events` — Spawns, despawns, phase changes

### 2. Client SDK Integration

#### Native Client (Rust)
- SpacetimeDB Rust client SDK for type-safe bindings
- Subscribe to relevant tables based on agent perception
- Local cache mirrors server state for low-latency reads
- Submit actions via reducer calls

#### Web Client (WASM)
- SpacetimeDB TypeScript/WASM client
- Same subscription model, JSON serialization
- Local cache for rendering

### 3. Migration Path
- Keep pod-core ECS as the canonical game logic layer
- pod-stdb wraps SpacetimeDB client/module APIs
- Reducers call into pod-core tick logic (compiled to WASM)
- Existing agent trait unchanged — agents still implement observe/decide

### 4. Authentication
- SpacetimeAuth for player identity
- Agent identity tied to SpacetimeDB Identity type
- Row-level security on observation_events (agents only see their own observations)

## Success Criteria
- [ ] SpacetimeDB module compiles and deploys
- [ ] Game entities persist across server restarts
- [ ] Tick pipeline runs as reducer with atomic transactions
- [ ] Clients subscribe and receive real-time updates
- [ ] Observations delivered via event tables with row-level security
- [ ] `cargo test -p pod-stdb` passes all integration tests
- [ ] Performance: 60 TPS with 100+ concurrent agents
