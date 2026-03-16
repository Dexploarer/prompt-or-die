# Moat Gates

Every feature proposal, implementation, and merge should answer one question
before it ships:

`Does this strengthen the agent-world moat or dilute it?`

If the answer is "dilute," the default decision is no unless the work is a
prerequisite for a stronger moat later.

> Audience: reviewers deciding whether roadmap and PR work strengthens the
> platform moat.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Competitive Matrix](./competitive-matrix.md) ·
> [Benchmark Suite](./benchmark-suite.md)

## The moat we are protecting

Prompt or Die wins if it becomes the easiest way to build and operate
deterministic, authoritative worlds where humans and autonomous agents share the
same rules.

## Competitor failures to convert into repo gates

| Competitor failure | POD gate |
| --- | --- |
| AI added as an afterthought | No privileged AI paths |
| Runtime truth split across editor, client, backend, and services | Authoritative world ownership |
| Opaque online debugging | Action and telemetry transparency |
| Web treated as second-class | Browser/native parity |
| Plugin ecosystems opened too early | Capability-bounded extensibility |
| Shipping and operations added late | Deployment and cost gate |
| Authoring data drifts from runtime behavior | Authoring-to-runtime provenance |
| Scale claims without replay proof | Deterministic replay proof |

## Merge gates

### 1. No privileged AI paths

Required for any AI or control-surface change.

- Human, scripted, LLM, neural, and system agents must still emit standard
  `Action` values.
- No direct world mutation from agent code.
- Validation rules must stay shared.
- New agent behavior must land in the shared runtime contract, not a special
  side channel.

### 2. Deterministic replay proof

Required for any gameplay, simulation, or network-authority change.

- Add or extend deterministic tests.
- Preserve replay/export compatibility where relevant.
- New authoritative behavior must be reconstructible from retained artifacts.
- Seed and contract drift must be visible in tests or debug output.

### 3. Action and telemetry transparency

Required for any validation, tool-calling, or authority behavior change.

- Rejected actions must retain visible reasons.
- Tool-call failures must be inspectable.
- Debug consumers must not need log scraping to explain behavior.
- If a feature increases opacity, it fails this gate.

### 4. Browser/native parity

Required for any render, input, or transport surface change.

- Native and browser behavior must remain aligned or degrade explicitly.
- Browser contract changes need browser tests.
- Native runtime changes must not silently strand the browser path.
- A feature that only works on one surface without a documented plan is not
  platform work; it is debt.

### 5. Authoring-to-runtime provenance

Required for any scene, prefab, asset, or editor-facing change.

- Authored data must instantiate through the nearest platform boundary instead
  of ad-hoc boot code.
- Editable concepts need provenance, diff, or override visibility where
  applicable.
- Editor convenience must not create a second gameplay truth.

### 6. Deployment and cost gate

Required for any feature that adds runtime complexity or new infrastructure.

- The operational impact must be explainable in shard cost or deployment steps.
- New dependencies need an opinionated setup path.
- If a feature adds operational burden without moat gain, it should not merge.

### 7. Capability-bounded extensibility

Required for any new extension or plugin-facing surface.

- New extension points must state which parts are stable now, draft, or
  internal.
- Plugins must not bypass authority, determinism, or validation.
- Broad generic hooks without lifecycle discipline are prohibited.

## Roadmap gates

### Before Phase 8 work is considered complete

These must be green:

- No privileged AI paths
- Deterministic replay proof
- Action and telemetry transparency
- Browser/native parity
- Authoring-to-runtime provenance

### Before Phase 9 ecosystem hardening is considered complete

These must be green:

- All Phase 8 gates
- Deployment and cost gate
- Capability-bounded extensibility

## Feature intake checklist

Use this checklist in every PR, design note, or implementation discussion:

1. Which competitor pressure does this answer?
2. Which moat gate does it strengthen?
3. Does it reduce creator time, operator cost, or debugging opacity?
4. Does it improve agent-world truth, or is it just generic engine surface area?
5. If it dilutes focus, why is it worth doing now?

## Default prioritization rule

Prefer work that improves at least two of these at once:

- deterministic authority
- agent parity
- browser/native parity
- creator speed
- operator visibility
- hosting cost efficiency
