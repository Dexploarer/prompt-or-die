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
| Multi-world topology parity | Proves the exported remote-topology contract matches quest/effect/evaluation truth | `pod-headless` scenario runner via moat suite | Automated |
| Remote topology feed parity | Proves `pod-net` resolves the same world/quest/effect/evaluation state from both authority rows and generated-mode topology ingress | `pod-net` topology feed benchmark | Automated |
| Creator time-to-first-agent-world | Measures creator adoption friction | Reference bootstrap flow | Scripted |
| Cost per 100/1000 active agents | Measures operational competitiveness | Acceptance scale target plus host-cost normalization | Semi-automated |

## Commands

Core moat benchmark report:

```bash
cd /Users/home/Desktop/prompt-or-die
cargo run -p pod-core --example moat_benchmark_suite --release -- --profile shard-target --monthly-host-cost-usd 300 --output artifacts/moat-core.json
```

Direct-connect transport benchmark report:

```bash
cd /Users/home/Desktop/prompt-or-die
cargo run -p pod-net --example transport_benchmark_suite -- --profile shard-target --fail-on-checks --output artifacts/transport-benchmark-shard.json
```

Headless multi-world topology parity report:

```bash
cd /Users/home/Desktop/prompt-or-die
cargo run -p pod-headless -- --profile shard-target --output artifacts/pod-headless-shard.json --topology-output artifacts/pod-headless-topology-shard.json
```

Remote topology feed parity report:

```bash
cd /Users/home/Desktop/prompt-or-die
cargo run -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input artifacts/pod-headless-topology-shard.json --fail-on-checks --output artifacts/topology-feed-benchmark-shard.json
```

Combined moat benchmark suite:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/run_moat_benchmarks.ts --profile shard-target --monthly-host-cost-usd 300 --output artifacts/moat-benchmarks.json
```

Browser asset and render-route regression gates:

```bash
cd /Users/home/Desktop/prompt-or-die/apps/pod-web
bun run verify:assets
bun run measure:render-routes:check
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
- `cd apps/pod-web && bun run verify:assets`
- `cd apps/pod-web && bun run typecheck`
- `cd apps/pod-web && bun test`
- `cd apps/pod-web && bun run test:smoke`
- `cd apps/pod-web && bun run measure:render-routes:check`

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
- per-route geometry/sprite load timing stats from `window.podRender.getStats()`
- gate pass/fail results for stability and worker-route chatter
- gate pass/fail results for completed-asset-load floors plus average/slowest geometry and sprite load ceilings
- main-vs-worker frame-submission reduction percentage
- stable-frame and slow-frame deltas between the two routes

`scripts/run_moat_benchmarks.ts` now includes that payload as
`browserRouteMeasurements`, so the combined moat artifact records both the
browser parity checks and the live route measurement summary. The same route
sampler also has a failing gate mode now: `bun run measure:render-routes:check`.
Use that for local/CI validation when you want a cheap browser perf threshold
without running the full smoke suite.

The generated asset lane is also a first-class benchmark gate now.
`bun run verify:assets` reruns `sync:assets` and then fails if any committed
generated source, staged, or runtime asset outputs drift. That keeps the
binary asset fast path and runtime budget report in routine validation instead
of depending on manual “did you remember to resync?” discipline.

### Headless topology parity report

Produced by `apps/pod-headless` and folded into the combined moat artifact by
`scripts/run_moat_benchmarks.ts` as `headlessTopology`.

It records:

- scenario/profile identity for the exported topology report
- admitted team/world/link counts
- world quest binding count
- applied world state count
- evaluation world count
- the full `topology_parity` payload from `pod-headless`
- explicit pass/fail checks for:
  - overall parity consistency
  - teams/worlds/links/quest-graph parity
  - world-quest-binding parity
  - applied-world-state parity
  - evaluation parity

The moat runner now fails immediately if any of those parity checks regress,
which means multi-world quest/effect progress is benchmarked through the same
artifact path as core, transport, and browser surfaces instead of being
inspectable only through raw headless report JSON.

### Remote topology feed parity report

Produced by `crates/pod-net/examples/topology_feed_benchmark_suite.rs`.

It records:

- scenario/profile identity for the input topology bundle
- one report per world for both authority-row and generated-runtime ingestion
- explicit pass/fail checks for resolved world id, quest binding parity,
  applied-world-state parity, and evaluation parity on both ingestion paths

This keeps the remote ingestion layer honest independently of the headless
scenario report: if `pod-net` stops resolving world/quest/effect state the same
way from authority rows and generated-mode ingress, the benchmark fails even if
`pod-headless` still exports a valid topology bundle.

The deterministic generated side still uses a command-driven binding seam that
is much closer to a real generated transport: `GeneratedBindingRuntime` emits
outbound connect/subscribe commands, `GeneratedBindingEndpoint` exposes those
commands to the simulated binding layer, and the same typed callback-facing
surface (`GeneratedBindingCallbacks` plus `GeneratedRemoteTopologyDocumentRow`)
feeds events back into `frame_tick()`. `StdbClient` and
`pod-net::SpacetimeDBClient` install that seam through
`install_generated_binding_runtime(...)`, so the benchmark and integration
paths no longer hand-wire the adapter. The repo now also ships installed
generated Rust bindings plus `GeneratedSdkRuntime`, which can be installed
through `install_generated_sdk_runtime(...)` to drive generated mode through
the real generated `DbConnection` and typed topology table callbacks. The moat
benchmark stays on the deterministic command-driven path until a live
SpacetimeDB module-backed feed is available in CI. The standalone example now
accepts `--generated-sdk-host`, `--generated-sdk-auth-token`, and
`--generated-sdk-timeout-ms` so the same benchmark can be pointed at a real
module without changing the report format. The in-tree coverage now includes
both same-world and linked-world quest/effect churn on the generated paths, plus
deterministic closed-port coverage for the live SDK option.
`pod-core` also owns the shared `build_remote_topology_bundle(...)`,
`RemoteTopologyParitySummary`, and parity/binding builder helpers now, so
headless and moat parity checks compare exported topology artifacts through one
engine-level assembly/consistency definition instead of app-local report code.

`scripts/run_moat_benchmarks.ts` now includes that payload as
`topologyFeedMeasurements`, so the combined moat artifact records core,
transport, browser, headless topology, and remote topology feed parity
together. `scripts/publish_moat_snapshots.ts` now also preserves the same
report under `topologyFeed` in committed shard-target snapshots, which means
remote topology feed drift can be reviewed historically instead of only through
pass/fail output.

### Transport benchmark report

Produced by `crates/pod-net/examples/transport_benchmark_suite.rs`.

It runs deterministic in-process direct-connect scenarios for:

- steady-state delta delivery
- explicit full-snapshot recovery success
- explicit recovery-delivery failure
- reconnect-token session resume
- queue pressure plus inactivity timeout pruning

Each scenario records the full `ShardTransportSummary` payload plus structured
pass/fail checks. The current checks cover:

- full snapshot count, bytes, and max full-snapshot size
- recovery snapshot bytes plus recovery-delivery failure accounting
- delta message count, bytes, max delta size, and entity churn (`updated` / `destroyed`)
- current and peak pending queue depth plus queue-pressure incidents
- session-resume counter preservation across reconnect
- timeout-pruning counters on inactive clients

`scripts/run_moat_benchmarks.ts` now includes that payload as
`transportMeasurements`, so the combined moat artifact records core,
transport, browser, and creator bootstrap surfaces together. The transport
example also supports `--fail-on-checks`, and the combined moat runner uses
that mode so CI fails if the direct-connect transport invariants regress.
On the `shard-target` profile, the benchmark now also enforces published
deterministic baselines for:

- `steady-delta` total delta bytes (`1304`) and max delta size (`163`)
- `recovery-success` total recovery snapshot bytes (`234`) and max full-snapshot size (`78`)
- `queue-pressure-timeout` total / peak pending queue depth (`6`) and inbound bytes (`44`)
- aggregate total full-snapshot bytes (`1187`), total recovery bytes (`234`), total delta bytes (`1816`), peak pending queue depth (`6`), and queue-pressure event count (`1`)

Phase 6 transport counters still exist on the direct-connect debug path too.
`shard_transport_summary` documents include:

- full snapshot count, bytes, and max full-snapshot size
- recovery snapshot bytes
- delta message count, bytes, max delta size, and entity churn (`updated` / `destroyed`)
- current and peak pending queue depth plus queue-pressure incident counts

Those counters remain exposed through the browser debug transport summary and
are still exercised by targeted reconnect/recovery regression tests in
`apps/pod-web/src/direct-connect.test.ts` and `crates/pod-net/src/server.rs`.
The next follow-on gap is historical drift tracking: the benchmark now has
published shard-target baselines, but it still needs a routine snapshot
comparison story across monthly moat reports instead of only pass/fail gates.
That monthly path now preserves `headlessTopology` too, so historical shard
snapshots can compare multi-world parity alongside transport and browser data.

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
