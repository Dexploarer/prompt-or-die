import { describe, expect, test } from "bun:test";

import { describeTargetAffordances, formatTargetSummary } from "./affordances";

describe("target affordances", () => {
  test("formats target summaries with health and distance", () => {
    expect(
      formatTargetSummary(
        {
          id: 9,
          position: [30, 40],
          velocity: [0, 0],
          rotation: 0,
          health: 28,
          maxHealth: 40,
          label: "Monster-Wolf"
        },
        {
          id: 12,
          position: [10, 10],
          velocity: [0, 0],
          rotation: 0,
          label: "Player"
        }
      )
    ).toBe("Monster-Wolf · E(9) · 28/40 hp · 36u away");
  });

  test("describes capture and combat affordances for wild creatures", () => {
    expect(
      describeTargetAffordances({
        id: 9,
        position: [0, 0],
        velocity: [0, 0],
        rotation: 0,
        label: "Wild Creature"
      })
    ).toBe("Space attack · C capture · E inspect");
  });

  test("describes loot affordances for containers", () => {
    expect(
      describeTargetAffordances({
        id: 14,
        position: [0, 0],
        velocity: [0, 0],
        rotation: 0,
        label: "Loot Cache"
      })
    ).toBe("R loot · E inspect");
  });
});
