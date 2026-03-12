# Reference Bootstrap

This is the official first-world bootstrap for Prompt or Die.

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

## What it boots

The bootstrap launches the flagship `pod-web` bootstrap showcase on:

```text
http://127.0.0.1:4178/?world=bootstrap-showcase&backend=webgl2
```

That route is the canonical "first world" because it already gives creators:

- a controllable human
- autonomous agents
- authored chunked world state
- a camera-directed first impression
- the real browser runtime contract

## Benchmark use

`scripts/run_moat_benchmarks.ts` uses this bootstrap by default for the
creator-time metric unless another official starter flow is passed with
`--creator-command`.

## Next evolution

This bootstrap is the current official first-world path. The next version should
expand from "launch the flagship local world" into "create and launch a fresh
reference project" once POD has a stable starter-project scaffold.
