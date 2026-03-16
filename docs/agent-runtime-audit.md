# Agent Runtime Audit

This document is the grounded map of how agents currently work in Prompt or Die, based on the live code in `pod-core` and `pod-agents`.

> Audience: contributors verifying what the runtime actually does today rather
> than what the roadmap says it should do.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Agent Integration Contract](./agent-integration-contract.md) ·
> [Benchmark Suite](./benchmark-suite.md)

## Source of truth

- Shared runtime trait: `/Users/home/Desktop/prompt-or-die/crates/pod-core/src/agent.rs`
- Authoritative tick pipeline: `/Users/home/Desktop/prompt-or-die/crates/pod-core/src/tick.rs`
- Advanced agent implementations: `/Users/home/Desktop/prompt-or-die/crates/pod-agents/src/lib.rs`
- Neural policy path: `/Users/home/Desktop/prompt-or-die/crates/pod-agents/src/neural_agent.rs`
- Optional ONNX inference adapter: `/Users/home/Desktop/prompt-or-die/crates/pod-agents/src/onnx_network.rs`
- Async LLM controller: `/Users/home/Desktop/prompt-or-die/crates/pod-agents/src/llm_agent.rs`
- Hybrid strategic/reactive controller: `/Users/home/Desktop/prompt-or-die/crates/pod-agents/src/hybrid_agent.rs`

## Core invariant

Every controller goes through one authoritative pipeline:

```text
Observe -> Decide -> Validate -> Execute -> Broadcast
```

That is not a design aspiration. It is what the runtime already does in `execute_tick()`.

## Authoritative runtime flow

`pod-core::tick::execute_tick()` currently owns the gameplay loop:

1. Build `Observation` values from ECS state and recent events.
2. Deliver each observation to the owning `Agent`.
3. Collect actions from `decide()`.
4. Validate every action against shared constraints.
5. Execute only valid actions.
6. Advance movement/controllers.
7. Flush events and emit tick telemetry.

Important consequence:

- Agents do not mutate world state directly.
- Human, scripted, LLM, hybrid, and neural controllers all enter through the same contract.
- Async or stale decisions are tolerated, but still validated at execution time.

## Agent contract

The live trait in `pod-core` currently requires:

- `id()`
- `agent_type()`
- `runtime_profile()`
- `observe(observation)`
- `decide() -> Vec<Action>`
- `constraints()` / `constraints_mut()`
- optional `drain_tool_calls()`
- optional lifecycle hooks
- optional `introspect()`

`runtime_profile()` matters because parity audits, transport, and replay tooling already distinguish runtime class without changing gameplay semantics.

`drain_tool_calls()` matters because agent side effects are already part of the shared telemetry spine.

## Agent families

## HumanAgent

Defined in `pod-core`. This is a thin adapter that buffers local input and emits standard `Action` values on `decide()`.

Maturity: high

Why:

- Simple contract
- Runs through the same validation path as AI
- Already production-shaped

## ScriptedAgent

Defined in `pod-agents`. This is the deterministic baseline: behavior tree / FSM / utility-style local logic emitting standard actions.

Maturity: high

Why:

- Good fit for deterministic content and fallback behavior
- No async coupling
- Useful evaluation baseline for every other agent class

## LlmAgent

Defined in `pod-agents/src/llm_agent.rs`.

Current shape:

- background provider call via channel
- double-buffered decision queue
- prompt templates and parsers are pluggable
- conversation memory is built in
- token budgeting exists
- stale action replay exists
- decision traces and tool-call traces already integrate with telemetry/replay

Maturity: high

Weak spots:

- still depends on prompt/parser quality for action correctness
- needs broader scenario coverage in the parity harness and shard-target
  benchmark flow, not another local architecture rewrite

## HybridAgent

Defined in `pod-agents/src/hybrid_agent.rs`.

Current shape:

- LLM produces strategic directives on a slower cadence
- behavior tree executes every tick
- blackboard is the integration seam
- triggers can force replanning on health/threat/objective transitions

Maturity: medium-high

Weak spots:

- blackboard contract is still mostly convention, not a versioned schema
- needs scenario evaluation against scripted and pure LLM baselines

## NeuralAgent

Defined in `pod-agents/src/neural_agent.rs`.

Current shape:

- versioned runtime schema for feature/action compatibility
- named action schema registry instead of a purely positional action table
- pluggable `PolicyNetwork`
- pluggable `ActionSelector`
- observation encoder produces a fixed 32-float feature vector
- discrete action space currently contains 10 actions
- experience buffer exists
- replay-side sample extraction exists
- optional ONNX inference exists behind the `onnx` feature
- policy runtime status is inspectable through agent introspection

Maturity: medium-low

What is already real:

- the policy network is actually used during `decide()`
- observations are encoded from authoritative state
- replay artifacts can already be filtered into neural training samples
- reward summaries and terminal flags are now derived from authoritative tick telemetry
- ONNX inference can load models from file or bytes
- the curated controller parity harness already compares scripted, LLM,
  hybrid, and neural controllers on one shared report surface

What is still missing:

- persistent model registry/checkpoint lifecycle beyond per-model metadata validation
- a trainer/export workflow grounded in authoritative replay artifacts and
  replay-derived reward summaries
- broader scenario and remote-runtime evaluation coverage beyond the curated
  controller parity harness

## Neural feature contract today

The current encoder now has an explicit runtime schema contract: interface version `1`, feature count `32`, action count `10`.

The 32 encoded features are still:

- self position, velocity, rotation, health
- visible-entity counts and nearest threat/ally
- top visible entity distances plus salience
- audio/message summaries
- objective progress summaries
- normalized tick and elapsed time

The current discrete action space is exactly 10 entries:

- `Idle`
- `Move` up
- `Move` down
- `Move` left
- `Move` right
- `Stop`
- `Attack`
- `Interact`
- `Drop { slot: 0 }`
- `Rotate { angle: PI / 4 }`

This is now a real versioned interface with metadata validation. It is still
intentionally narrow, and it is not yet the full long-horizon policy surface a
production neural runtime would eventually want.

## ONNX runtime position

The ONNX adapter is useful, but narrow by design:

- expects 32 input features
- expects 10 output logits
- can validate caller-supplied model metadata against the current runtime schema
- falls back to uniform output on inference failure

That makes it a safe inference surface, not a complete neural platform.

## Telemetry and replay position

The runtime is already stronger than the neural stack in one important way:

- per-agent telemetry exists in the authoritative tick loop
- action lifecycle traces already exist
- tool-call traces already exist
- replay exports already derive training-oriented samples

This is the real moat for agent work. The next neural work should build on this spine, not bypass it.

## Current priority assessment

Strongest parts of the agent system:

- authoritative shared agent pipeline
- observation building
- action validation
- telemetry and replay export
- authoritative reward attribution and replay-derived training samples
- curated controller parity reporting across scripted, LLM, hybrid, and neural controllers
- LLM and hybrid controller infrastructure

Weakest parts of the agent system:

- persistent model lifecycle and checkpoint tooling
- trainer/export workflow above authoritative replay data
- scenario breadth for controller evaluation outside the curated parity harness

## Recommended execution order

1. Keep the neural interface stable unless benchmarks force a schema change.
   The versioned feature/action contract already exists and should be treated
   as an explicit compatibility boundary.

2. Build trainer/export tooling on top of the authoritative replay surface.
   Training rows already come from tick/replay truth; the missing piece is the
   workflow that turns them into repeatable model inputs and checkpoints.

3. Expand evaluation coverage.
   The curated controller parity harness is real, but it should grow into
   broader scenario suites and remote-runtime checks before model complexity
   increases.

4. Only then expand model sophistication and action breadth.
   Better architectures are not the first bottleneck. Better training loops
   and stronger comparative evidence are.

5. Keep remote neural/LLM paths on the same observation/action contract.
   Transport and multi-world topology should continue to reuse the existing
   authoritative agent contract instead of inventing a second gameplay model.
