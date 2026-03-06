# Agent Integration Contract

This document defines how an agent integrates with Prompt or Die. It is the main contract for human control, scripted NPCs, LLM agents, neural policies, and system automation.

## Core invariant

Every agent type goes through the same authoritative pipeline:

```text
Observe -> Decide -> Validate -> Execute -> Broadcast
```

No agent type gets privileged access to world mutation. Agents do not directly change the world. They emit `Action` values, and the runtime decides what is valid.

## Runtime contract

The core trait lives in `pod-core`:

```rust
pub trait Agent: Send {
    fn id(&self) -> AgentId;
    fn agent_type(&self) -> AgentType;
    fn observe(&mut self, observation: Observation);
    fn decide(&mut self) -> Vec<Action>;
    fn constraints(&self) -> &AgentConstraints;
    fn constraints_mut(&mut self) -> &mut AgentConstraints;

    fn on_join(&mut self) {}
    fn on_leave(&mut self) {}
    fn on_death(&mut self) {}
    fn on_respawn(&mut self) {}
    fn on_spawn(&mut self, _entity_id: EntityId) {}
    fn on_damage(&mut self, _amount: f32, _source: Option<AgentId>) {}
    fn on_interact(&mut self, _target: EntityId) {}
    fn introspect(&self) -> AgentIntrospection { /* ... */ }
}
```

The minimum implementation surface is:

- accept an `Observation`
- choose zero or more `Action` values
- expose constraint settings

Everything else is optional lifecycle and debugging support.

## What an agent receives

Each tick, the runtime builds an `Observation` from authoritative world state. That observation may include:

- self state
- visible entities
- heard events or recent world events
- combat and cooldown state
- messages and objectives

The observation is already filtered by perception, range, and other gameplay rules before it reaches the agent.

## What an agent may emit

Agents return `Vec<Action>`. Examples include:

- `Move`
- `Stop`
- `Rotate`
- `LookAt`
- `Attack`
- `AttackTarget`
- `Interact`
- `Speak`
- `Signal`
- `UseAbility`
- `Spawn`
- `Idle`

These are requests, not guarantees. The runtime validates them against:

- action budget
- cooldowns
- reaction gates
- target availability
- range and world-state legality

## Tick semantics

The runtime executes this order every tick:

1. Build observations.
2. Call `observe()` on each connected agent.
3. Call `decide()` and collect actions.
4. Validate actions against `AgentConstraints`.
5. Execute valid actions.
6. Advance movement/controllers and flush events.

This means:

- `decide()` should be side-effect free with respect to world mutation.
- Returning no actions is valid.
- Returning stale actions is tolerated for async agents, but validation still applies.

## Agent categories

### Human agents

Humans integrate through a `HumanAgent` adapter that buffers input and emits standard `Action` values. The runtime treats this exactly like any other agent implementation.

### Scripted agents

Scripted agents are the simplest deterministic integration path. They use observations plus local state to emit actions directly.

### LLM agents

LLM agents should translate observations into prompts and translate model outputs back into `Action` values. They must still obey the same runtime constraints and may return delayed decisions if the model response is asynchronous.

### Neural agents

Neural agents should treat observations as the authoritative sensor input and map them to standard actions or action scores.

### System agents

System agents are allowed as orchestration participants, but they still go through the same validation pipeline if they act through the normal agent path.

## Networking contract

Prompt or Die supports both local and remote agent operation:

- local in-process agents inside the game or server runtime
- direct-connect transport via `pod-net`
- SpacetimeDB-backed remote integration via `pod-stdb`

Regardless of transport, the gameplay contract does not change:

- the agent receives an observation-like payload
- the agent emits standard actions
- the authoritative runtime validates and applies them

## Determinism and safety rules

Agent authors should follow these rules:

- Do not mutate the ECS world directly from agent code.
- Do not assume every emitted action will execute.
- Keep nondeterminism inside the agent implementation, not the world authority path.
- Treat `Observation` as complete for the current tick; do not rely on hidden state.
- Use `introspect()` for debugging instead of side-channel state leaks.

If an agent needs memory, planning, prompt templates, model calls, or policy inference, that belongs inside the agent implementation, not in the world execution layer.

## Minimal example

```rust
use pod_core::{Action, Agent, AgentConstraints, AgentId, AgentType, Observation};

struct SimpleAgent {
    id: AgentId,
    constraints: AgentConstraints,
    last_observation: Option<Observation>,
}

impl Agent for SimpleAgent {
    fn id(&self) -> AgentId { self.id }
    fn agent_type(&self) -> AgentType { AgentType::ScriptedNpc }
    fn observe(&mut self, observation: Observation) {
        self.last_observation = Some(observation);
    }
    fn decide(&mut self) -> Vec<Action> {
        vec![Action::Idle]
    }
    fn constraints(&self) -> &AgentConstraints { &self.constraints }
    fn constraints_mut(&mut self) -> &mut AgentConstraints { &mut self.constraints }
}
```

## Integration checklist

- Implement `Agent`.
- Emit only standard `Action` values.
- Keep world mutation out of agent code.
- Respect constraint-driven execution.
- Add tests for observation handling and chosen actions.
- Use the same path whether the controller is human, scripted, LLM, neural, or remote.
