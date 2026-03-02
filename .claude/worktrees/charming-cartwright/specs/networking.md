# Spec: Networking Architecture

## Job to Be Done
Unify networking around SpacetimeDB 2.0 while keeping fallback direct-connect mode. Support massive multiplayer with autonomous agents as first-class networked entities.

## Requirements

### 1. SpacetimeDB Primary Mode
- **Server module** — game tick runs as a SpacetimeDB reducer
- **Client subscriptions** — clients subscribe to SQL queries matching their perception
- **Incremental sync** — SpacetimeDB pushes deltas, clients maintain local cache
- **Event tables** — combat, speech, world events pushed to relevant subscribers
- **Identity** — SpacetimeAuth for player identity; system identity for AI agents

### 2. Direct-Connect Fallback
- Keep existing QUIC (native) / WebSocket (web) for LAN play
- Simpler protocol: full state snapshots + action batches
- No SpacetimeDB dependency for offline/local play

### 3. Agent Networking
- LLM agents can run remotely — connect via SpacetimeDB, subscribe to observations, submit actions
- Neural agents run server-side (low latency required)
- Scripted agents always server-side
- Human players connect as clients

### 4. Scalability
- World partitioning — large worlds split into spatial regions
- Agents only subscribe to entities in perception range
- Interest management via SpacetimeDB SQL query filtering
- Target: 1000+ concurrent agents per world instance

### 5. Lobby & Matchmaking
- Game lobbies stored as SpacetimeDB tables
- Matchmaking reducer — queue players/agents, create game instances
- Spectator mode — subscribe to full world state (read-only)

## Success Criteria
- [ ] SpacetimeDB module runs game tick as reducer
- [ ] Client connects, subscribes, sees real-time updates
- [ ] Remote LLM agent connects and plays via SpacetimeDB
- [ ] Direct-connect mode works for LAN without SpacetimeDB
- [ ] 100+ agents in single world instance at 60 TPS
- [ ] Lobby creation and joining works
- [ ] `cargo test -p pod-net` passes
