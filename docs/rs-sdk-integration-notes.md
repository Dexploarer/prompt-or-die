# RS-SDK Integration Notes

This note records how `Prompt or Die` should treat [`Dexploarer/rs-sdk`](https://github.com/Dexploarer/rs-sdk) if we use it as an external reference or proving ground.

Current direction:

- `rs-sdk` is **not** the active implementation path
- the preferred path is an internal POD headless team/world runner
- this document remains useful only as an external comparison point

> Audience: contributors using `rs-sdk` only as an external comparison or
> proving ground.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Multi-World Agent Topology](./multi-world-agent-topology.md) ·
> [Agent Runtime Audit](./agent-runtime-audit.md)

## Why this matters

We do not need more `pod-web` UI work to move the agent stack forward.

If the goal is to stress:

- agent loops
- observation/action contracts
- runtime control flow
- networking behavior
- autonomous behavior quality

then an SDK-driven external environment is a valid proving ground.

## What rs-sdk actually is

From the upstream README:

- `rs-sdk` is a research-oriented Runescape-style bot starter kit
- it exposes a low-level `BotSDK` plus a higher-level `BotActions` layer
- it can run headless from scripts
- it can optionally open a browser client, but that is not required for automation

Important architecture detail from the upstream README:

- the SDK does **not** talk directly to the game server
- it goes through a gateway
- the gateway forwards messages between SDK instances and a botclient
- the botclient relays game state and executes low-level actions

That means "skip the client UI" is valid, but "talk directly to the actual rs-sdk game server with no relay layer" is not how upstream is currently designed.

## Usable surfaces

Based on the upstream `sdk/README.md` and `sdk/API.md`, the integration surfaces are:

### Low-level surface: `BotSDK`

This is the protocol-level layer.

Useful properties:

- exposes raw state access
- exposes nearby entities and inventory queries
- exposes low-level movement and interaction commands
- resolves actions when the game acknowledges them

Best use in POD:

- networking experiments
- observation translation
- action acknowledgment timing
- replay capture

### High-level surface: `BotActions`

This is the domain-aware effect-completion layer.

Useful properties:

- actions wait for completion, not only acknowledgment
- includes movement, combat, inventory, banking, crafting, dialog, and shop helpers

Best use in POD:

- high-level planner evaluation
- behavior benchmarking
- agent recipe baselines

## Direct implication for Prompt or Die

If we integrate with `rs-sdk`, we should treat it as:

- an external benchmark environment
- an adapter target for the shared POD agent pipeline
- a networking/protocol proving ground

We should **not** treat it as:

- a replacement for POD world authority
- a reason to build more browser UI first
- proof that we need to copy its client architecture

## Recommended integration seam

The cleanest path is:

1. Keep POD agent logic authoritative inside our own runtime and controller stack.
2. Add an adapter that turns external `rs-sdk` state into POD-style `Observation` values.
3. Add an adapter that turns POD-selected actions into `BotSDK` / `BotActions` calls.
4. Record resulting outcomes into POD telemetry/replay surfaces.

That keeps our moat in:

- shared agent contract
- authoritative observation model
- replay/training export
- evaluation harnesses
- controller implementations

instead of moving authority into the external SDK.

## Proposed adapter layers

## `rs_state_adapter`

Responsibility:

- convert `rs-sdk` state snapshots into POD-style observations

Expected outputs:

- self state
- nearby entities
- inventory/equipment summaries
- combat state
- dialog/shop/bank state where relevant

## `rs_action_adapter`

Responsibility:

- map POD action intents onto `BotSDK` / `BotActions`

Important rule:

- keep this as a separate translation layer
- do not contaminate core POD actions with rs-sdk-specific verbs

## `rs_rollout_recorder`

Responsibility:

- persist external-environment episodes into the same replay/training shape POD uses internally

This is the key to making the integration useful for neural and hybrid agent work.

## `rs_benchmark_runner`

Responsibility:

- run deterministic scripted/LLM/hybrid/neural evaluations against rs-sdk tasks

This should eventually compare:

- success rate
- time to completion
- invalid action rate
- recovery behavior
- tool reliance
- reward/outcome quality

## What to avoid

- building more client UI before the agent/runtime contract is stronger
- pushing rs-sdk-specific action/state assumptions down into `pod-core`
- assuming the gateway/botclient relay can be ignored just because the browser view is optional
- treating rs-sdk as primary authority instead of a benchmark/integration target

## Recommended execution order

1. Finish POD reward/outcome attribution in `pod-core`.
2. Finish replay-derived training/export contracts.
3. Add an rs-sdk state/action mapping document or module.
4. Build a small adapter runner outside the main client UI path.
5. Use rs-sdk as an external benchmark surface for agent evaluation.

## Sources

- Upstream repo README: [`Dexploarer/rs-sdk`](https://github.com/Dexploarer/rs-sdk)
- SDK usage and API surface: [`sdk/README.md`](https://github.com/Dexploarer/rs-sdk/tree/main/sdk) and [`sdk/API.md`](https://github.com/Dexploarer/rs-sdk/blob/main/sdk/API.md)
