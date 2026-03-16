# Reference Bootstrap

This is the official first-world bootstrap for Prompt or Die.

> Audience: anyone trying to get to the first canonical POD world quickly.
>
> Related docs: [Documentation Hub](./README.md) ·
> [Asset Pipeline](./asset-pipeline.md) ·
> [Bootstrap Showcase Research](./bootstrap-showcase-research.md)

It is intentionally opinionated:

- web-first
- one command
- controllable human plus autonomous agents in the same world
- authored bootstrap showcase, not an empty shell

## Why this is the bootstrap

If POD is going to win on creator speed, it needs a canonical answer to:

`How do I get to my first agent world?`

The answer should not be a doc safari. It should be one repeatable command.

## Command

The script defaults to `--hold`, `--host 127.0.0.1`, `--port 4178`, and
`--timeout-ms 60000`.

Hold the dev server open:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/bootstrap_reference_world.ts --hold
```

Measure bootstrap time and exit:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/bootstrap_reference_world.ts --measure
```

Show available options:

```bash
cd /Users/home/Desktop/prompt-or-die
bun ./scripts/bootstrap_reference_world.ts --help
```

## What it boots

The bootstrap runs `bun run dev -- --host ... --port ...` in
`apps/pod-web`, waits for the showcase route to answer successfully, and then
targets:

```text
http://127.0.0.1:4178/?world=bootstrap-showcase&backend=webgl2
```

That route resolves to the local `bootstrap-showcase` preset
(`Resonant Shore`). It is the canonical "first world" because it already gives
creators:

- a controllable human
- autonomous agents
- authored chunked world state
- a camera-directed first impression
- the real browser runtime contract

In `--hold` mode the dev server stays up until Ctrl+C. In `--measure` mode the
script prints JSON with the measured `startupTimeMs`, the resolved `url`, the
working directory, and source-of-truth notes, then exits cleanly.

## Benchmark use

`scripts/run_moat_benchmarks.ts` uses this bootstrap by default for the
creator-time metric by invoking:

```bash
bun ./scripts/bootstrap_reference_world.ts --measure --host 127.0.0.1 --port 4178
```

Override that default with `--creator-command` only if the official starter
flow changes.

## Next evolution

This bootstrap is the current official first-world path. The next version should
expand from "launch the flagship local world" into "create and launch a fresh
reference project" once POD has a stable starter-project scaffold.
