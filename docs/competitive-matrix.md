# Competitive Matrix

Prompt or Die should not benchmark itself against a fake single rival. The real
competitive field is layered:

- Engine/editor platforms: Unity, Unreal, Godot
- Rust-native engine architecture: Bevy
- Authoritative multiplayer backend: Nakama
- AI-character platforms: Inworld, Convai

This file is the standing matrix. Update it monthly. Treat each monthly pass as
product intelligence, not marketing copy.

> Audience: product and strategy contributors reviewing where POD wins or loses
> against the real market.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Moat Gates](./moat-gates.md) ·
> [Benchmark Suite](./benchmark-suite.md)

## Category we are trying to win

Prompt or Die is not trying to be "another general-purpose engine." The target
category is:

`deterministic, authoritative, AI-agent-native online world platform`

If a comparison axis does not matter to that category, it is secondary.

## Scoring rubric

Use `1` to `5` for the matrix below.

- `5`: market-leading for the POD target category
- `4`: strong and strategically relevant
- `3`: competitive but incomplete
- `2`: weak or mostly indirect
- `1`: not meaningful in this category

## Latest baseline

Baseline month: `2026-03`

| Competitor | Editor and authoring | AI-agent-native runtime | Authoritative multiplayer | Browser parity | Replay and observability | Deploy and live ops | Ecosystem | Threat to POD | Structural weakness POD should exploit |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Unity | 5 | 2 | 4 | 3 | 3 | 4 | 5 | High | AI is additive, not a first-class gameplay contract |
| Unreal | 5 | 2 | 4 | 2 | 3 | 4 | 5 | High | Excellent world tools, but not optimized around deterministic agent authority |
| Godot | 4 | 2 | 2 | 4 | 2 | 3 | 4 | Medium | Open and portable, but weaker on persistent authoritative online worlds |
| Bevy | 3 | 2 | 2 | 3 | 3 | 2 | 4 | Medium | Strong Rust engine story, but less opinionated around agents, backend authority, and creator workflow |
| Nakama | 1 | 1 | 5 | 2 | 3 | 5 | 3 | High | Great backend, but not an integrated authored world/runtime/editor stack |
| Inworld | 2 | 3 | 1 | 2 | 2 | 3 | 3 | Medium | Characters are the product; world authority is not |
| Convai | 2 | 3 | 1 | 2 | 2 | 3 | 3 | Medium | Strong AI character UX, weak unified simulation ownership |

## Standing competitor notes

### Unity

- Wins on editor maturity, services breadth, and asset ecosystem.
- Threatens POD when creators optimize for familiarity over architecture.
- POD response: beat Unity on shared human/AI authority, replay truth, and
  integrated world/backend reasoning.

### Unreal

- Wins on high-end world-building, content tooling, animation, and production
  polish.
- Threatens POD when visual ambition becomes the buying decision.
- POD response: do not chase visual parity first; win the persistent
  agent-native world category first.

### Godot

- Wins on open-source accessibility, portability, and approachable exports.
- Threatens POD in indie/open-source creator mindshare.
- POD response: beat Godot on authoritative online worlds, AI runtime
  integration, and debug visibility.

### Bevy

- Wins on Rust-native engine credibility and ecosystem gravity.
- Threatens POD when Rust developers want a clean engine core more than a full
  platform.
- POD response: keep POD opinionated around creator workflow, multiplayer
  authority, and agent tooling instead of collapsing into "just another Rust
  engine."

### Nakama

- Wins on multiplayer backend completeness and live-ops readiness.
- Threatens POD when teams only need a backend layer.
- POD response: make the engine, browser client, editor, persistence, and debug
  path feel like one product instead of five integrations.

### Inworld

- Wins on turnkey AI character deployment and studio-friendly integrations.
- Threatens POD when teams want AI NPCs without changing their engine stack.
- POD response: emphasize full world ownership, deterministic constraints, and
  action-level authority.

### Convai

- Wins on conversational AI character UX and drop-in integrations.
- Threatens POD for short adoption cycles and demos.
- POD response: keep the moat on simulation truth, persistent state, and
  agent-world observability.

## Monthly delta protocol

Every monthly update should add:

1. Competitor release or platform change
2. Whether it changes the matrix score
3. Whether it attacks POD's moat directly or indirectly
4. Required POD response
5. Whether the response belongs in `IMPLEMENTATION_PLAN.md`

Use this template:

```text
Month: YYYY-MM
Competitor:
Change:
Affected matrix columns:
Old score -> New score:
Threat level:
Required POD response:
Repo follow-up:
```

## Questions to answer every month

- Did a competitor close the gap on agent-native runtime behavior?
- Did a competitor make browser or web deployment materially easier?
- Did a competitor add better replay, telemetry, or live-debug tooling?
- Did a competitor make authoritative online worlds easier to build?
- Did a competitor reduce creator setup time or hosting cost enough to pressure
  POD's wedge?

## Sources to recheck on each pass

- Unity official product and manual pages
- Unreal official engine release pages
- Godot docs and export docs
- Bevy official site and release posts
- Heroic Labs Nakama product and docs
- Inworld official runtime/product pages
- Convai official product pages
