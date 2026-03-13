import { describe, expect, test } from "bun:test";

import {
  buildHeadlessTopologyMeasurements,
  topologyFeedChecksPassed,
} from "./run_moat_benchmarks";

describe("run moat benchmarks", () => {
  test("summarizes passing headless topology parity checks", () => {
    const measurements = buildHeadlessTopologyMeasurements({
      schema_version: 2,
      scenario: "deadman-neural-cup",
      profile: "ci-smoke",
      teams: [{ team_id: "iron-sigil" }, { team_id: "gloam-mesh" }],
      worlds: [{ world_id: "deadman-prime" }, { world_id: "deadman-shadow" }],
      links: [{ link_id: "prime-to-shadow" }],
      world_quest_bindings: [
        {
          world_id: "deadman-prime",
          quest_graph_ids: ["deadman-prime-season"],
        },
      ],
      applied_world_states: [{ world_id: "deadman-shadow" }],
      evaluation: {
        worlds: [{ world_id: "deadman-prime" }, { world_id: "deadman-shadow" }],
      },
      topology_parity: {
        consistent: true,
        teams_match: true,
        worlds_match: true,
        links_match: true,
        quest_graphs_match: true,
        world_quest_bindings_match: true,
        applied_world_states_match: true,
        evaluation_match: true,
        missing_world_quest_binding_ids: [],
        unexpected_world_quest_binding_ids: [],
        missing_applied_world_ids: [],
        unexpected_applied_world_ids: [],
        missing_evaluation_world_ids: [],
        unexpected_evaluation_world_ids: [],
      },
    });

    expect(measurements.sourceSchemaVersion).toBe(2);
    expect(measurements.teamCount).toBe(2);
    expect(measurements.worldQuestBindingCount).toBe(1);
    expect(measurements.appliedWorldStateCount).toBe(1);
    expect(measurements.evaluationWorldCount).toBe(2);
    expect(measurements.allChecksPassed).toBe(true);
    expect(measurements.checks).toHaveLength(8);
    expect(measurements.checks.every((check) => check.passed)).toBe(true);
  });

  test("flags failed headless topology parity checks", () => {
    const measurements = buildHeadlessTopologyMeasurements({
      schema_version: 2,
      scenario: "deadman-neural-cup",
      profile: "shard-target",
      teams: [{ team_id: "iron-sigil" }],
      worlds: [{ world_id: "deadman-prime" }],
      links: [],
      world_quest_bindings: [],
      applied_world_states: [],
      evaluation: {
        worlds: [],
      },
      topology_parity: {
        consistent: false,
        teams_match: true,
        worlds_match: true,
        links_match: true,
        quest_graphs_match: true,
        world_quest_bindings_match: false,
        applied_world_states_match: false,
        evaluation_match: false,
        missing_world_quest_binding_ids: ["deadman-prime"],
        unexpected_world_quest_binding_ids: [],
        missing_applied_world_ids: ["deadman-prime"],
        unexpected_applied_world_ids: [],
        missing_evaluation_world_ids: ["deadman-prime"],
        unexpected_evaluation_world_ids: [],
      },
    });

    expect(measurements.allChecksPassed).toBe(false);
    expect(
      measurements.checks.filter((check) => !check.passed).map((check) => check.metric),
    ).toEqual([
      "topology_parity.consistent",
      "topology_parity.world_quest_bindings_match",
      "topology_parity.applied_world_states_match",
      "topology_parity.evaluation_match",
    ]);
  });

  test("accepts passing topology feed parity checks", () => {
    expect(
      topologyFeedChecksPassed({
        schema_version: 1,
        scenario_id: "deadman-neural-cup",
        profile_id: "ci-smoke",
        world_count: 1,
        worlds: [],
        checks: [
          {
            metric: "authority_row.deadman-prime.resolved_world_matches",
            passed: true,
            expected: "true",
            observed: "\"deadman-prime\"",
          },
        ],
      }),
    ).toBe(true);
  });

  test("flags failed topology feed parity checks", () => {
    expect(
      topologyFeedChecksPassed({
        schema_version: 1,
        scenario_id: "deadman-neural-cup",
        profile_id: "ci-smoke",
        world_count: 1,
        worlds: [],
        checks: [
          {
            metric: "generated_runtime.deadman-prime.quest_binding_matches",
            passed: false,
            expected: "true",
            observed: "false",
          },
        ],
      }),
    ).toBe(false);
  });
});
