# Benchmark Suite

This suite exists to keep Prompt or Die honest about the category it wants to
win.

The goal is not generic engine vanity metrics. The goal is to measure whether
POD is becoming the best platform for deterministic, authoritative,
AI-agent-native worlds.

## Metrics that matter

| Metric | Why it matters | Source | Status |
| --- | --- | --- | --- |
| Deterministic replay fidelity | Proves authority and parity are real, not marketing | `pod-core` acceptance harness | Automated |
| Authoritative tick stability | Proves shard simulation stays within budget | `pod-core` acceptance harness | Automated |
| Agent action acceptance/rejection transparency | Proves creators and operators can explain runtime decisions | `pod-core` telemetry | Automated |
| Browser/native parity | Proves web is a first-class runtime, not an afterthought | `pod-render` tests plus `pod-web` checks | Automated |
| Creator time-to-first-agent-world | Measures creator adoption friction | Reference bootstrap flow | Scripted |
| Cost per 100/1000 active agents | Measures operational competitiveness | Acceptance scale target plus host-cost normalization | Semi-automated |

## Commands

Core moat benchmark report:

```bash
cd /Users/home/Desktop/prompt-or-die
cargo run -p pod-core --example moat_benchmark_suite --release -- --profile shard-target --monthly-host-cost-usd 300 --output artifacts/moat-core.json
```

Combined moat benchmark suite:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/run_moat_benchmarks.ts --profile shard-target --monthly-host-cost-usd 300 --output artifacts/moat-benchmarks.json
```

Fast CI-oriented smoke profile:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/run_moat_benchmarks.ts --profile ci-smoke --skip-browser --output artifacts/moat-benchmarks-ci.json
```

Canonical first-world bootstrap:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/bootstrap_reference_world.ts --measure
```

## What the combined suite records

### Core report

Produced by `crates/pod-core/examples/moat_benchmark_suite.rs`.

It emits JSON for:

- replay fidelity score
- parity pass/fail
- tick budget compliance
- action acceptance/rejection rates
- rejection-reason coverage
- tool-call error rate
- normalized host cost per 100 and 1000 agents when cost input is supplied

### Browser/native parity report

Produced by `scripts/run_moat_benchmarks.ts`.

It records pass/fail and duration for:

- `cargo test -p pod-render --lib`
- `cd apps/pod-web && bun run typecheck`
- `cd apps/pod-web && bun test`
- `cd apps/pod-web && bun run test:smoke`

`bun run test:smoke` now covers two separate browser responsibilities:

- generic `local-sandbox` gameplay-input smoke on both main-thread and worker routes
- fixed-time `bootstrap-showcase` screenshot regression capture for the directed first-world intro

The `local-sandbox` smoke now also asserts that `window.podRender.getStats().runtimePerf`
is populated on both render routes, which gives the current Phase 5 baseline for:

- time-to-first-rendered-frame (`warmupMs`)
- stable-vs-slow frame counts against the shared frame budget
- stable-frame percentage
- slowest observed frame on the route

It also asserts `window.podRender.getStats().mainThreadPerf`, which is the
current main-thread-relief baseline for:

- time-to-first submitted frame on the main thread
- average main-thread submission cost per frame
- slowest observed main-thread submission
- requested-vs-actual render-thread mode plus explicit worker fallback reason

The current worker route also coalesces pending frame submissions until the
worker sends a `renderComplete` acknowledgement and skips the duplicate
post-init `resize` sync, so these counters now reflect the reduced hot path
rather than the older "post every frame plus an extra unchanged resize" path.
`mainThreadPerf.byKind` now also separates `frame`, `control`, and `resize`
traffic so the remaining worker-route submission cost can be attributed before
the next optimization pass.
Same-turn telemetry and world-event updates now batch into one combined
control message before the next frame, which removes the older pattern of
multiple worker control posts for a single local-authoritative update cycle.
The local-sandbox worker smoke route now enforces the first explicit chatter
ceilings on that data surface: `control` and `resize` submissions must both
remain at `0` for the current route profile.
It now also enforces frame-quality gates on `runtimePerf`: the main-thread
route must hold at least `90%` stable frames, while the worker route must hold
at least `50%` stable frames and more stable frames than slow ones.

Outside Playwright smoke, `apps/pod-web/scripts/measure-render-routes.ts`
provides an artifact-grade browser sample of the same main-vs-worker
`local-sandbox` route pair. It writes
`apps/pod-web/artifacts/render-route-measurements.json` and captures:

- per-route `runtimePerf` and `mainThreadPerf` payloads
- gate pass/fail results for stability and worker-route chatter
- main-vs-worker frame-submission reduction percentage
- stable-frame and slow-frame deltas between the two routes

`scripts/run_moat_benchmarks.ts` now includes that payload as
`browserRouteMeasurements`, so the combined moat artifact records both the
browser parity checks and the live route measurement summary.

Phase 6 transport counters now exist on the direct-connect debug path as well.
`shard_transport_summary` documents include:

- full snapshot count, bytes, and max full-snapshot size
- recovery snapshot bytes
- delta message count, bytes, max delta size, and entity churn (`updated` / `destroyed`)
- current and peak pending queue depth plus queue-pressure incident counts

Those counters are currently exposed through the browser debug transport summary
and are now exercised by targeted reconnect/recovery regression tests in
`apps/pod-web/src/direct-connect.test.ts` and `crates/pod-net/src/server.rs`.
They are still not part of a dedicated moat benchmark lane, so the next
follow-on slice should turn those same degraded-path assertions into a cheap
repeatable benchmark or threshold report.

### Creator time-to-first-agent-world

By default, `scripts/run_moat_benchmarks.ts` measures creator bootstrap time
with the canonical first-world command in
`scripts/bootstrap_reference_world.ts`.

You can still override it:

- pass `--creator-seconds` to record a manual measurement
- pass `--creator-command` if the official starter flow changes

## How to use the cost benchmark

`--monthly-host-cost-usd` is the monthly infrastructure cost for the shard shape
being benchmarked. The core report normalizes that cost against the measured
active-agent count in the selected acceptance profile.

Guidance:

- Use `ci-smoke` for correctness smoke only.
- Use `shard-target` for comparable cost baselines.
- Keep the same host-cost assumption across monthly comparisons unless the
  infrastructure profile changes.

## Monthly benchmark routine

Run this every month before updating the competitor matrix:

1. Run the shard-target core report.
2. Run the combined benchmark suite.
3. Record creator time from the reference bootstrap.
4. Compare deltas against the previous month.
5. Add any regressions or required responses to `IMPLEMENTATION_PLAN.md`.

## Interpretation rules

- A feature that improves visual breadth but hurts tick stability is not a win.
- A feature that adds AI capability but reduces replay fidelity is not a win.
- A feature that improves native only while breaking browser parity is not a win.
- A feature that improves power but increases creator time or operating cost
  without moat gain is not a win.
