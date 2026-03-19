# Benchmark Suite

This suite exists to keep Prompt or Die honest about the category it wants to
win.

The goal is not generic engine vanity metrics. The goal is to measure whether
POD is becoming the best platform for deterministic, authoritative,
AI-agent-native worlds.

> Audience: contributors and operators running the repo's proof surfaces,
> weekly snapshots, and retained-history workflow.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Reference Bootstrap](./reference-bootstrap.md) ·
> [Platform Stabilization](./platform-stabilization.md) ·
> [Benchmark Snapshot History](./benchmark-snapshots/README.md)

## Metrics that matter

| Metric | Why it matters | Source | Status |
| --- | --- | --- | --- |
| Deterministic replay fidelity | Proves authority and parity are real, not marketing | `pod-core` acceptance harness | Automated |
| Authoritative tick stability | Proves shard simulation stays within budget | `pod-core` acceptance harness | Automated |
| Agent action acceptance/rejection transparency | Proves creators and operators can explain runtime decisions | `pod-core` telemetry | Automated |
| Browser/native parity | Proves web is a first-class runtime, not an afterthought | `pod-render` tests plus `pod-web` checks | Automated |
| Controller parity and evaluation | Proves scripted, LLM, hybrid, and neural agents are measured against the same scenario contract | `pod-agents` controller harness | Automated |
| Multi-world topology parity | Proves the exported remote-topology contract matches quest/effect/evaluation truth | `pod-headless` scenario runner via moat suite | Automated |
| Remote topology feed parity | Proves `pod-net` resolves the same world/quest/effect/evaluation state from both authority rows and generated-mode topology ingress | `pod-net` topology feed benchmark | Automated |
| Creator time-to-first-agent-world | Measures creator adoption friction | Reference bootstrap flow | Scripted |
| Cost per 100/1000 active agents | Measures operational competitiveness | Acceptance scale target plus host-cost normalization | Semi-automated |

## Benchmark requirement tiers

The repo treats moat/report commands in two tiers:

| Tier | Meaning |
| --- | --- |
| Platform requirement gate | A proof surface that directly protects deterministic authority, parity, transport durability, or retained moat evidence. |
| Local tooling / proof surface | A useful benchmark or publication helper that supports analysis, export strategy, or operator ergonomics without being the moat by itself. |

The current classification lives in [Platform Stabilization](./platform-stabilization.md). Use that document when deciding whether a new benchmark belongs in merge-critical workflow or in optional local proof tooling.

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

Controller parity report:

```bash
cd /Users/home/Desktop/prompt-or-die
cargo run -q -p pod-agents --example controller_parity_benchmark -- --fail-on-checks
```

Combined moat benchmark suite:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/run_moat_benchmarks.ts --profile shard-target --monthly-host-cost-usd 300 --output artifacts/moat-benchmarks.json
```

TOON export proof benchmark:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/benchmark_toon_exports.ts --profile extensive --output artifacts/toon-export-benchmark-extensive.json --html-output artifacts/toon-export-benchmark-extensive.html --markdown-output artifacts/toon-export-benchmark-extensive.md --charts-dir artifacts/toon-export-benchmark-extensive-charts --fail-on-checks
```

The extensive run publishes a benchmark bundle:

- `artifacts/toon-export-benchmark-extensive.json`
- `artifacts/toon-export-benchmark-extensive.html`
- `artifacts/toon-export-benchmark-extensive.md`
- `artifacts/toon-export-benchmark-extensive-charts/*.svg`

Historical snapshot comparison:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/compare_moat_snapshots.ts --baseline docs/benchmark-snapshots/2026-W10-shard-target.json --candidate docs/benchmark-snapshots/2026-W11-shard-target.json --output artifacts/benchmark-snapshot-comparison.json --fail-on-regressions
```

Retained snapshot history index/report:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/index_benchmark_snapshots.ts
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

- generic `local-sandbox` gameplay-input smoke on the default auto-selected route plus the explicit worker route
- fixed-time `bootstrap-showcase` screenshot regression capture for the directed first-world intro

The `local-sandbox` smoke now also asserts that `window.podRender.getStats().runtimePerf`
is populated on both shipped play routes, which gives the current Phase 5 baseline for:

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
Frame-stability and asset-load timing stats are still recorded on every route,
but they now live as artifact-grade comparison data rather than hard CI gates,
because the shipped asset set produced environment-sensitive cold-load timings
and coarse frame-step variance.

Outside Playwright smoke, `apps/pod-web/scripts/measure-render-routes.ts`
provides an artifact-grade browser sample of the same main-vs-worker
`local-sandbox` route pair. The default no-param route remains on the shipped
main-thread path, so the sampler uses explicit `renderThread=main` and
`renderThread=worker` URLs to compare the playable route against the worker
experiment directly. It writes
`apps/pod-web/artifacts/render-route-measurements.json` and captures:

- per-route `runtimePerf` and `mainThreadPerf` payloads
- per-route geometry/sprite load timing stats from `window.podRender.getStats()`
- recorded gate booleans for stability and average/slowest geometry/sprite load ceilings
- enforced gate pass/fail results for stable-frame floors, completed-asset-load floors, and worker-route chatter ceilings
- main-vs-worker frame-submission reduction percentage
- stable-frame and slow-frame deltas between the two routes

`scripts/run_moat_benchmarks.ts` now includes that payload as
`browserRouteMeasurements`, so the combined moat artifact records both the
browser parity checks and the live route measurement summary. The same route
sampler also has a failing gate mode now: `bun run measure:render-routes:check`.
Use that for local/CI validation when you want a cheap deterministic browser
gate without running the full smoke suite.

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

### TOON export benchmark

Produced by `scripts/benchmark_toon_exports.ts` and folded into the combined
moat artifact by `scripts/run_moat_benchmarks.ts` as `toonExportBenchmark`.

It records:

- dataset-oriented comparisons for:
  - pretty JSON
  - compact JSON
  - TOON with comma delimiters
  - TOON with tab delimiters
- byte, token, encode-latency, and decode-latency measurements on five shapes:
  - uniform tick/event batches
  - Toonscape-style wide donor event batches
  - semi-uniform operational logs
  - nested world snapshots
  - deep multiverse metadata trees
- strict TOON validation failures for row-width mismatches and truncation
- streaming decode coverage through `decodeStreamSync(...)`
- dataset-specific recommendations that decide where TOON actually wins

This benchmark follows the same useful pattern visible in Toonscape:

- compare TOON against a real compact-JSON baseline
- use a stable uniform nullable row schema for event batches
- keep a Toonscape donor track so the suite captures the 70%+ win shape that
  wide, null-heavy telemetry can achieve
- prove world/event exports separately from deep config-style metadata

The repo now keeps the contract split explicit:

- `pod shell --agent` stays newline-delimited JSON for control-plane requests,
  replies, hooks, and lifecycle events
- `pod export events --format toon` and `pod export world --format toon` are
  the TOON-first paths because those payloads actually beat compact JSON
- `pod export multiverse` remains JSON-first because the deep branch metadata
  tree does not compress cleanly enough to justify TOON as the default

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

Local live SDK parity has now been proven end to end with this sequence:

```bash
cargo run -q -p pod-headless -- --profile ci-smoke --scenario deadman-neural-cup --output /tmp/pod-headless-report.json --dataset-output /tmp/pod-headless-dataset.json --topology-output /tmp/pod-headless-topology.json
spacetime start --listen-addr 127.0.0.1:3100 --in-memory --non-interactive
for world in deadman-prime deadman-shadow sanctuary-echo; do
  spacetime publish "$world" --anonymous --server http://127.0.0.1:3100 --bin-path /Users/home/Desktop/prompt-or-die/.cargo-target/wasm32-unknown-unknown/release/pod_stdb.wasm -y
done
cargo run -q -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input /tmp/pod-headless-topology.json --output /Users/home/Desktop/prompt-or-die/artifacts/topology-feed-live-local.json --generated-sdk-host http://127.0.0.1:3100 --generated-sdk-timeout-ms 5000 --fail-on-checks
```

That local run produced `/Users/home/Desktop/prompt-or-die/artifacts/topology-feed-live-local.json`
with `30/30` checks passing across `deadman-prime`, `deadman-shadow`, and
`sanctuary-echo`. The same live path has now also been exercised on the
`shard-target` profile, producing
`/Users/home/Desktop/prompt-or-die/artifacts/topology-feed-live-shard-local.json`
and the first committed weekly snapshot at
`/Users/home/Desktop/prompt-or-die/docs/benchmark-snapshots/2026-W11-shard-target.json`.
`pod-core` also owns the shared `build_remote_topology_bundle(...)`,
`RemoteTopologyParitySummary`, and parity/binding builder helpers now, so
headless and moat parity checks compare exported topology artifacts through one
engine-level assembly/consistency definition instead of app-local report code.
The same topology-feed report now also checks
`RemoteTopologyBundle.tournament_control_plane` and
`RemoteTopologyBundle.tournament_orchestration` on both the authority-row and
generated-runtime paths, so tournament standings/control-plane and
world-pressure drift are part of the same remote parity artifact as quest
bindings, applied world state, and evaluation.

`scripts/run_moat_benchmarks.ts` now includes that payload as
`topologyFeedMeasurements`, and the same combined moat artifact now also keeps
`headlessTopology.tournamentOrchestration` plus the new tournament-control-plane
and tournament-orchestration parity checks. That means the artifact records
core, transport, browser, headless topology, and remote topology feed parity
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

- `steady-delta` total delta bytes (`1392`) and max delta size (`174`)
- `recovery-success` total recovery snapshot bytes (`234`) and max full-snapshot size (`78`)
- `queue-pressure-timeout` total / peak pending queue depth (`6`) and inbound bytes (`44`)
- aggregate total full-snapshot bytes (`1220`), total recovery bytes (`234`), total delta bytes (`1904`), peak pending queue depth (`6`), and queue-pressure event count (`1`)

Phase 6 transport counters still exist on the direct-connect debug path too.
`shard_transport_summary` documents include:

- full snapshot count, bytes, and max full-snapshot size
- recovery snapshot bytes
- delta message count, bytes, max delta size, and entity churn (`updated` / `destroyed`)
- current and peak pending queue depth plus queue-pressure incident counts

Those counters remain exposed through the browser debug transport summary and
are still exercised by targeted reconnect/recovery regression tests in
`apps/pod-web/src/direct-connect.test.ts` and `crates/pod-net/src/server.rs`.
Historical drift tracking is now live too: the first committed shard-target
weekly snapshot at
`/Users/home/Desktop/prompt-or-die/docs/benchmark-snapshots/2026-W11-shard-target.json`
captures transport, browser-route, headless topology, topology feed, and live
topology feed data together. The workflow gap is closed too:
`scripts/run_shard_target_snapshot.ts` now wraps the full capture, publication,
comparison, and retained-history refresh flow into one reproducible local
command.

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
- Keep the same host-cost assumption across weekly comparisons unless the
  infrastructure profile changes.

## Weekly benchmark routine

Run this every week before updating the competitor matrix:

1. Run `bun ./scripts/run_shard_target_snapshot.ts --label YYYY-Www`.
2. Inspect the retained history report at `docs/benchmark-snapshots/README.md`.
3. Inspect the generated summary artifact at `artifacts/shard-target-snapshot-run.json` when you need the exact command history or chosen baseline.
4. Record any required responses from the retained comparison/history report in `IMPLEMENTATION_PLAN.md`.

`run_shard_target_snapshot.ts` now wraps the previously manual chain:

- shard-target moat capture
- browser render-route capture
- headless topology export
- local SpacetimeDB startup plus module publish
- live generated-SDK topology benchmark
- weekly snapshot publication
- retained history index/report publication

If the browser render-route gate fails again but `apps/pod-web/artifacts/render-route-measurements.json`
is still produced, the wrapper records that status as `artifact_only` and still
publishes the snapshot so drift review is not blocked by browser harness
regressions. On the current shipped asset set, the normal browser route status
is `passed`.

`compare_moat_snapshots.ts` writes a structured JSON report with per-metric
status (`improved`, `regressed`, `changed`, `unchanged`) across transport,
browser-route, headless-topology, topology-feed, and live-topology-feed data.
That now includes tournament/swarm orchestration drift from
`headlessTopology.tournamentOrchestration` and the per-world topology-feed
orchestration parity booleans. The tournament/swarm orchestration metrics are no
longer informational-only: the comparison report now records explicit baseline
envelopes for `phase`, `activeWorldCount`, `contestedWorldCount`,
`activeLinkCount`, `leadingTeamCount`, `atRiskTeamCount`, `pressureWorldCount`,
and `neuralSwarmWorldCount`, so drift in those deterministic shard-target
metrics shows up as a regression instead of a generic changed value. Use
`--fail-on-regressions` when you want the comparison itself to act as a gate.

`run_shard_target_snapshot.ts` now wires that compare step into the one-command
weekly workflow. Pass `--compare-baseline <snapshot>` for an explicit
week-over-week review, or rerun an existing week label and the wrapper will
reuse the current same-label snapshot as a temporary baseline before it
publishes the refreshed artifact. When a later week exists, the wrapper now
auto-selects the latest prior `docs/benchmark-snapshots/YYYY-Www-shard-target.json`
file as the baseline, so week-over-week review no longer depends on always
passing `--compare-baseline` by hand. When comparison runs, the wrapper also
retains the generated report as
`docs/benchmark-snapshots/YYYY-Www-shard-target-comparison.json` and records
that published path in the run summary, so the weekly history now keeps both
the snapshot and the diff artifact. The wrapper now also refreshes
`docs/benchmark-snapshots/index.json` and `docs/benchmark-snapshots/README.md`,
so weekly review has a stable retained history surface instead of raw JSON
inspection.

## Interpretation rules

- A feature that improves visual breadth but hurts tick stability is not a win.
- A feature that adds AI capability but reduces replay fidelity is not a win.
- A feature that improves native only while breaking browser parity is not a win.
- A feature that improves power but increases creator time or operating cost
  without moat gain is not a win.
