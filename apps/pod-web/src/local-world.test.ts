import { describe, expect, test } from "bun:test";

import { PodWebLocalWorld, renderGameToText } from "./local-world";

const TICK_MS = 1000 / 60;

function stepTicks(world: PodWebLocalWorld, ticks: number): void {
  world.step(ticks * TICK_MS);
}

function moveToward(world: PodWebLocalWorld, targetId: number, ticks: number): void {
  const snapshot = world.snapshotState();
  const player = snapshot.entities.find((entity) => entity.id === 1);
  const target = snapshot.entities.find((entity) => entity.id === targetId);
  if (!player || !target) {
    throw new Error("Missing player or target");
  }

  const dx = target.position[0] - player.position[0];
  const dy = target.position[1] - player.position[1];
  const length = Math.hypot(dx, dy);
  world.submitActions([{ kind: "move", direction: [dx / length, dy / length] }]);
  stepTicks(world, ticks);
  world.submitActions([{ kind: "stop" }]);
  stepTicks(world, 1);
}

describe("PodWebLocalWorld", () => {
  test("spawns a connected local sandbox with a controlled player and authored entities", () => {
    const world = new PodWebLocalWorld("Scout");
    world.connect();

    const snapshot = world.snapshotState();
    expect(world.currentStatus()).toMatchObject({
      phase: "connected",
      controlledEntity: 1,
      detail: "Local sandbox shard ready"
    });
    expect(snapshot.entities.find((entity) => entity.id === 1)?.label).toBe("Scout");
    expect(snapshot.entities.length).toBeGreaterThan(18);
    expect(snapshot.entities.filter((entity) => entity.metadata.kind === "Npc")).toHaveLength(3);
    expect(snapshot.entities.some((entity) => entity.metadata.kind === "WildCreature")).toBe(true);
    expect(snapshot.entities.some((entity) => entity.metadata.kind === "ResourceNode")).toBe(true);
    expect(snapshot.entities.some((entity) => entity.metadata.kind === "LootContainer")).toBe(true);
    expect(snapshot.entities.some((entity) => entity.label === "glass spire")).toBe(true);
    expect(snapshot.entities.some((entity) => entity.label === "canopy tree")).toBe(true);
    expect(
      snapshot.entities.some(
        (entity) => entity.metadata.faction?.factionId === "verdant-wardens"
      )
    ).toBe(true);
    expect(
      snapshot.entities.some(
        (entity) => (entity.metadata.questAnchor?.questIds.length ?? 0) > 0
      )
    ).toBe(true);
    expect(
      snapshot.entities.some(
        (entity) => entity.metadata.encounterProfile?.tableId === "verdant-lynx-encounters"
      )
    ).toBe(true);
    expect(
      snapshot.entities.some(
        (entity) => entity.metadata.spawnProfile?.biomeId === "verdant-hollow"
      )
    ).toBe(true);
  });

  test("moves the player and exposes high-signal text state", () => {
    const world = new PodWebLocalWorld("Scout");
    world.connect();
    world.submitActions([{ kind: "move", direction: [1, 0] }]);
    stepTicks(world, 60);
    world.submitActions([{ kind: "stop" }]);
    stepTicks(world, 1);

    const snapshot = world.snapshotState();
    const player = snapshot.entities.find((entity) => entity.id === 1);
    expect(player?.position[0]).toBeGreaterThan(4);

    const textState = renderGameToText(
      snapshot,
      world.controlledEntityId(),
      null,
      world.currentActionState(),
      "sandbox ready",
      [],
      world.companionRoster()
    );
    expect(textState).toContain("\"mode\":\"local-sandbox\"");
    expect(textState).toContain("\"world\":\"Verdant Hollow\"");
    expect(textState).toContain("\"coordinateSystem\":\"world x east-west, y north-south\"");
  });

  test("supports gathering and looting loops", () => {
    const world = new PodWebLocalWorld("Scout");
    world.connect();

    moveToward(world, 5, 45);
    world.submitActions([{ kind: "gatherResource", target: 5, skill: "Mining" }]);
    stepTicks(world, 2);

    moveToward(world, 7, 70);
    world.submitActions([{ kind: "loot", target: 7 }]);
    stepTicks(world, 2);

    const batch = world.drainEventBatch();
    expect(batch?.events.some((event) => event.summary.includes("gathered 1 copper-ore"))).toBe(true);
    expect(batch?.events.some((event) => event.summary.includes("looted 48 coins"))).toBe(true);
    expect(world.snapshotState().entities.some((entity) => entity.id === 7)).toBe(false);
  });

  test("supports weakening, capturing, and summoning a companion", () => {
    const world = new PodWebLocalWorld("Scout");
    world.connect();

    moveToward(world, 3, 44);
    world.submitActions([{ kind: "attackTarget", target: 3 }]);
    stepTicks(world, 2);
    world.submitActions([{ kind: "captureCreature", target: 3 }]);
    stepTicks(world, 2);

    expect(world.companionRoster()).toEqual([
      {
        speciesId: "verdant-lynx",
        speciesName: "Verdant Lynx"
      }
    ]);
    expect(world.snapshotState().entities.some((entity) => entity.id === 3)).toBe(false);

    world.submitActions([{ kind: "summonCompanion", slot: 0 }]);
    stepTicks(world, 2);

    const snapshot = world.snapshotState();
    expect(snapshot.entities.some((entity) => entity.metadata.kind === "Companion")).toBe(true);
    const batch = world.drainEventBatch();
    expect(batch?.events.some((event) => event.summary.includes("Captured Verdant Lynx"))).toBe(true);
    expect(batch?.events.some((event) => event.summary.includes("Summoned Verdant Lynx"))).toBe(true);
  });
});
