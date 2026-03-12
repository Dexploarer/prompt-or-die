import type { DirectConnectActionState } from "./direct-connect";
import type { NetworkGameEvent, NetworkWorldSnapshot, Vec2Tuple } from "./contracts";
import type { CompanionRosterEntry, LocalWorldDebugState } from "./local-world";
import type { LocalWorldPresentation } from "./runtime-config";
import { surfaceModeFromEntity } from "./surface-mode";

const LOCAL_WORLD_CHUNK_SIZE = 8;

export function renderGameToText(
  snapshot: NetworkWorldSnapshot,
  controlledEntity: number | null,
  selectedTargetId: number | null,
  actionState: DirectConnectActionState,
  feedback: string,
  recentEvents: NetworkGameEvent[],
  companionRoster: CompanionRosterEntry[],
  debugState: LocalWorldDebugState,
  presentation: Pick<LocalWorldPresentation, "mode" | "worldName"> | null = null
): string {
  const player = snapshot.entities.find((entity) => entity.id === controlledEntity) ?? null;
  const target = snapshot.entities.find((entity) => entity.id === selectedTargetId) ?? null;

  return JSON.stringify({
    mode: presentation?.mode ?? "local-sandbox",
    world: presentation?.worldName ?? "Verdant Hollow",
    coordinateSystem: "world x east-west, y north-south",
    tick: snapshot.tick,
    player: player
      ? {
          id: player.id,
          label: player.label,
          position: player.position,
          velocity: player.velocity,
          surfaceMode: surfaceModeFromEntity(player),
          animationSetId: player.metadata.actorPresentation?.animationSetId ?? null,
          health: player.health,
          maxHealth: player.maxHealth
        }
      : null,
    target: target
      ? {
          id: target.id,
          label: target.label,
          kind: target.metadata.kind,
          position: target.position,
          health: target.health,
          maxHealth: target.maxHealth
        }
      : null,
    companions: companionRoster,
    streaming: {
      chunkSize: LOCAL_WORLD_CHUNK_SIZE,
      activeChunks: debugState.activeChunkKeys,
      currentRegionId: debugState.currentRegionId,
      currentRegionName: debugState.currentRegionName,
      regionPopulation:
        player?.metadata.regionId == null
          ? null
          : snapshot.population.regions.find(
              (region) => region.regionId === player.metadata.regionId
            ) ?? null
    },
    progression: {
      questGraphs: debugState.questGraphs,
      factionReputation: debugState.factionReputation,
      encounterTables: debugState.encounterTables
    },
    actionState,
    feedback,
    events: recentEvents.slice(-4).map((event) => event.summary),
    nearby: snapshot.entities
      .filter((entity) => entity.id !== controlledEntity && entity.metadata.kind !== "Scenery")
      .sort((left, right) => {
        if (!player) {
          return left.id - right.id;
        }
        return (
          distanceBetween(left.position, player.position) -
          distanceBetween(right.position, player.position)
        );
      })
      .slice(0, 8)
      .map((entity) => ({
        id: entity.id,
        label: entity.label,
        kind: entity.metadata.kind,
        position: entity.position,
        health: entity.health,
        maxHealth: entity.maxHealth
      }))
  });
}

function distanceBetween(a: Vec2Tuple, b: Vec2Tuple): number {
  return Math.hypot(a[0] - b[0], a[1] - b[1]);
}
