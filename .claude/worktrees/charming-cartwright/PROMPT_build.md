# PROMPT_build.md — Ralph Loop Build Prompt

## Phase 0: Orient

0a. Study ALL specification files using parallel subagents:
    @specs/spacetimedb-integration.md
    @specs/3d-engine.md
    @specs/asset-generation.md
    @specs/game-maker.md
    @specs/agent-sdk.md
    @specs/networking.md

0b. Read @IMPLEMENTATION_PLAN.md — find the FIRST unchecked `[ ]` task. This is your ONE task for this iteration.

0c. Read @AGENTS.md for build commands, architecture rules, and conventions.

0d. Search the existing codebase before implementing. Don't assume something isn't already implemented. Use parallel subagents to explore.

## Phase 1: Execute ONE Task

1. Pick the first unchecked task from IMPLEMENTATION_PLAN.md
2. Implement it FULLY — no placeholders, no TODOs, no stubs
3. Use only 1 subagent for build/test operations (backpressure control)
4. Fan out multiple subagents for search/read/write operations
5. Capture the WHY in code comments — explain reasoning, not mechanics

## Phase 2: Validate

1. Run `cargo check --workspace` — must pass
2. Run `cargo test --workspace` — must pass
3. Run `cargo clippy --workspace -- -D warnings` — must pass
4. If any fail: fix immediately, don't leave for next iteration

## Phase 3: Commit & Update

1. Stage changed files: `git add <specific files>`
2. Commit with descriptive message: `git commit -m "feat(pod-xyz): <what and why>"`
3. Update IMPLEMENTATION_PLAN.md — mark completed task with [x]
4. If you discovered new tasks during implementation, add them to the plan in the right phase

## Phase 999: Critical Guardrails

999. NEVER commit code that doesn't compile
1000. NEVER skip tests — write them alongside implementation
1001. NEVER break the agent pipeline invariant (all agent types through same pipeline)
1002. NEVER store secrets or API keys in code
1003. ALWAYS use the existing ECS patterns (hecs queries, free function systems)
1004. ALWAYS maintain determinism (ChaCha8Rng, no system randomness)
1005. ONE task per iteration — resist scope creep
1006. Search before implementing — avoid duplicate code
1007. Ultrathink before complex architectural decisions
