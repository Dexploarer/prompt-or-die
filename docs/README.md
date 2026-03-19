# Documentation Hub

This directory mixes runtime contracts, workflow guides, benchmark/operator
docs, research notes, release notes, and generated history. Use this page as
the index instead of jumping into files at random.

## Start here

If you are new to the repo, read these in order:

1. [Root README](../README.md)
2. [CLI Surface](./cli-surface.md)
3. [Reference Bootstrap](./reference-bootstrap.md)
4. [Architecture Overview](./architecture.md)
5. [Benchmark Suite](./benchmark-suite.md)

## Documentation map

### Runtime and architecture

- [CLI Surface](./cli-surface.md)
  The canonical command catalog for developers, users, agents, and
  agent-assisted development workflows. Humans should prefer `pod <area> <alias>`
  and `pod shell` for attached terminal sessions; automation should prefer
  stable IDs plus `--json`, interactive agents should use
  `pod shell --agent` as the newline-delimited JSON machine shell, and
  LLM-facing world data should flow through `pod export ... --format toon`.
- [Architecture Overview](./architecture.md)
  What the platform looks like today: crate boundaries, authority model, and
  extension seams.
- [Plugin Model](./plugin-model.md)
  The current extension contract and which surfaces are safe to build against.
- [Asset Pipeline](./asset-pipeline.md)
  The staged-import, runtime-bundle, and browser asset contract.

### Agents and multi-world runtime

- [Agent Integration Contract](./agent-integration-contract.md)
  The public contract for human, scripted, LLM, hybrid, neural, and remote
  agents.
- [Agent Runtime Audit](./agent-runtime-audit.md)
  The grounded “what the code actually does today” audit for agent runtimes.
- [Multi-World Agent Topology](./multi-world-agent-topology.md)
  Teams, linked worlds, tournaments, and the shared remote-topology artifact.
- [RS-SDK Integration Notes](./rs-sdk-integration-notes.md)
  External proving-ground notes. Reference only, not the active implementation
  path.

### Creator and benchmark workflows

- [Reference Bootstrap](./reference-bootstrap.md)
  The canonical first-world path.
- [Bootstrap Showcase Research](./bootstrap-showcase-research.md)
  Why the flagship browser showcase should look and feel the way it does.
- [Benchmark Suite](./benchmark-suite.md)
  Commands, artifact shapes, and weekly benchmark workflow.
- [Benchmark Snapshot History](./benchmark-snapshots/README.md)
  Generated retained history for weekly shard-target snapshots.

### Product and review framing

- [Competitive Matrix](./competitive-matrix.md)
  The standing comparison against engines, online backend stacks, and AI
  character platforms.
- [Moat Gates](./moat-gates.md)
  Repo-level review gates derived from that competitive framing.

### Historical and generated docs

- [Release notes](./releases/0.1.0-alpha.2.md)
  Historical snapshots of what each prerelease changed.
- [Migration notes](./migrations/2026-03-03-wave1-reconnect-and-stdb-mode.md)
  Breaking-change callouts that affect callers directly.
- [`docs/benchmark-snapshots/*`](./benchmark-snapshots/README.md)
  Generated benchmark history. Do not hand-edit the generated snapshot JSON or
  history index.

## Suggested reading paths

### I want to run the product

1. [Root README](../README.md)
2. [CLI Surface](./cli-surface.md)
3. [Reference Bootstrap](./reference-bootstrap.md)
4. [`apps/pod-web/README.md`](../apps/pod-web/README.md)

### I want to understand the runtime

1. [Architecture Overview](./architecture.md)
2. [Agent Integration Contract](./agent-integration-contract.md)
3. [Agent Runtime Audit](./agent-runtime-audit.md)
4. [Multi-World Agent Topology](./multi-world-agent-topology.md)

### I want to extend the platform

1. [Plugin Model](./plugin-model.md)
2. [Architecture Overview](./architecture.md)
3. [Asset Pipeline](./asset-pipeline.md)

### I want to run proof surfaces or historical comparisons

1. [Benchmark Suite](./benchmark-suite.md)
2. [Benchmark Snapshot History](./benchmark-snapshots/README.md)
3. [Moat Gates](./moat-gates.md)
4. [Competitive Matrix](./competitive-matrix.md)

### I want to automate against the platform

1. [CLI Surface](./cli-surface.md)
2. [Benchmark Suite](./benchmark-suite.md)
3. [Agent Integration Contract](./agent-integration-contract.md)

### I want an attached terminal workflow

1. [`pod shell`](../scripts/pod.ts)
2. [`pod shell --agent`](../scripts/pod.ts)
3. [CLI Surface](./cli-surface.md)

## Documentation rules for contributors

- If a command changes, update the owning deep doc and the relevant entry point
  (`README.md` or this hub) in the same change.
- If a supported top-level CLI surface changes, update `scripts/cli_surface.ts`
  and rerun `bun ./scripts/verify_cli_surface.ts --write` in the same change.
- When documenting POD CLI usage, prefer alias-first examples for humans and
  stable ID plus `--json` examples for automation.
- If a public contract changes, update the contract doc and the grounded audit
  doc together.
- If a document is generated, keep the generator as the source of truth and say
  so explicitly in the file.
- Prefer adding a focused deep doc over burying long workflow details in the
  root README.
