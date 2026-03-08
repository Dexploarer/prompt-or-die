import { describe, expect, test } from "bun:test";
import { encode } from "@toon-format/toon";

import {
  applyNetworkStateDelta,
  buildAuthoritativeWorldFrame,
  encodeDirectConnectActionBatch,
  encodeDirectConnectConnectMessage,
  encodeDirectConnectDebugTelemetryMessage,
  encodeDirectConnectFullSnapshotRequest,
  type NetworkEntityMetadataSnapshot,
  type NetworkWorldSnapshot,
  parseAgentTickRollup,
  parseAgentToolCallEvent,
  parseDirectConnectServerMessage,
  parseLiveDebugDocument,
  parseReplayFile,
  parseShardIncidentSummary,
  parseTickTelemetryEnvelope,
  summarizeAgentTickRollup,
  summarizeAgentToolCallEvent,
  summarizeReplayFile,
  withInteractionMarkers
} from "./contracts";

function entityMetadata(kind: string, overrides: Record<string, unknown> = {}) {
  return {
    kind,
    chunk_key: null,
    region_id: null,
    region_name: null,
    team_id: null,
    quest_graph_ids: [],
    faction_track_id: null,
    encounter_table_id: null,
    combat_style: null,
    species_id: null,
    species_name: null,
    resource_skill: null,
    resource_tier: null,
    encounter_kind: null,
    faction: null,
    quest_anchor: null,
    encounter_profile: null,
    spawn_profile: null,
    atmosphere: null,
    atmosphere_volume: null,
    actor_presentation: null,
    combat_presentation: null,
    interaction: {
      can_inspect: true,
      can_interact: false,
      can_attack: false,
      can_gather: false,
      can_loot: false,
      can_capture: false,
      can_command_companion: false,
      can_chat: false
    },
    ...overrides
  };
}

function typedEntityMetadata(
  kind: NetworkEntityMetadataSnapshot["kind"],
  overrides: Partial<NetworkEntityMetadataSnapshot> = {}
): NetworkEntityMetadataSnapshot {
  return {
    kind,
    chunkKey: null,
    regionId: null,
    regionName: null,
    teamId: null,
    questGraphIds: [],
    factionTrackId: null,
    encounterTableId: null,
    combatStyle: null,
    speciesId: null,
    speciesName: null,
    resourceSkill: null,
    resourceTier: null,
    encounterKind: null,
    faction: null,
    questAnchor: null,
    encounterProfile: null,
    spawnProfile: null,
    atmosphere: null,
    atmosphereVolume: null,
    actorPresentation: null,
    combatPresentation: null,
    interaction: {
      canInspect: true,
      canInteract: false,
      canAttack: false,
      canGather: false,
      canLoot: false,
      canCapture: false,
      canCommandCompanion: false,
      canChat: false
    },
    ...overrides
  };
}

function populationState(tick: number) {
  return {
    tick,
    chunks: [],
    regions: []
  };
}

describe("TOON contract parsing", () => {
  test("accepts versioned tick telemetry TOON documents", () => {
    const document = encode({
      document_type: "versioned_tick_telemetry",
      payload: {
        version: "V1",
        payload: {
          tick: 9,
          agents: []
        }
      }
    });

    const envelope = parseTickTelemetryEnvelope(document);
    expect(envelope.tickTelemetry.tick).toBe(9);
    expect(envelope.tickTelemetry.agents).toEqual([]);
  });

  test("accepts replay TOON documents and builds summaries", () => {
    const document = encode({
      document_type: "replay_file",
      payload: {
        header: {
          name: "flagship-mmo-loop",
          timestamp: 1_741_315_200,
          world_seed: 42,
          tick_count: 120,
          agent_count: 2,
          notes: "acceptance"
        },
        traces: [
          [
            {
              tick: 12,
              agent_id: "agent-a",
              observation_hash: 44,
              prompt_sent: "observe",
              raw_response: "idle",
              actions_taken: [{ Idle: null }],
              tool_calls: [
                {
                  tick: 12,
                  tool_name: "llm.complete",
                  provider: "qwen",
                  status: "Succeeded",
                  latency_ms: 48,
                  request_units: 120,
                  response_units: 40,
                  error_message: null
                }
              ],
              latency_ms: 48
            }
          ]
        ],
        telemetry_windows: [
          {
            tick: 12,
            agents: []
          }
        ],
        training_samples: [
          {
            tick: 12,
            agent_id: "agent-a",
            path_distance: 16.75,
            action_outcomes: {
              submitted: 1,
              executed: 1,
              rejected: 0,
              queued: 0
            },
            encounter_transition: null,
            tool_call_latency_ms: 48,
            tool_call_error_count: 0
          }
        ]
      }
    });

    const replay = parseReplayFile(document);
    const summary = summarizeReplayFile(replay);

    expect(replay.header.name).toBe("flagship-mmo-loop");
    expect(summary.traceCount).toBe(1);
    expect(summary.trainingSampleCount).toBe(1);
    expect(summary.totalPathDistance).toBe(16.75);
    expect(summary.latestTelemetryTick).toBe(12);
  });

  test("accepts shard incident summary TOON documents", () => {
    const document = encode({
      document_type: "shard_incident_summary",
      payload: {
        shard_id: "alpha-1",
        latest_tick: 360,
        severity: "Warning",
        summary: "Shard alpha-1 requires attention",
        tick_budget_overrun_rate: 0.08,
        action_rejection_rate: 0.02,
        tool_call_error_rate: 0.11,
        average_tool_latency_ms: 820,
        average_trajectory_distance: 3.2,
        peak_entity_count: 512,
        peak_agent_count: 128,
        capture_actions: 4,
        summon_actions: 2,
        gather_actions: 7,
        loot_actions: 9,
        notes: ["tool-call error rate exceeds 10%"]
      }
    });

    const summary = parseShardIncidentSummary(document);
    expect(summary.shard_id).toBe("alpha-1");
    expect(summary.severity).toBe("Warning");
    expect(summary.notes).toHaveLength(1);
  });

  test("accepts tool-call and rollup TOON documents", () => {
    const toolDocument = encode({
      document_type: "agent_tool_call_event",
      payload: {
        tick: 12,
        agent_entity_id: 44,
        trace: {
          tick: 12,
          tool_name: "llm.complete",
          provider: "qwen",
          status: "TimedOut",
          latency_ms: 48,
          request_units: 120,
          response_units: 40,
          error_message: "timeout"
        }
      }
    });
    const rollupDocument = encode({
      document_type: "agent_tick_rollup",
      payload: {
        tick_start: 1,
        tick_end: 60,
        agent_entity_id: 44,
        total_distance: 18.5,
        submitted_action_count: 4,
        executed_action_count: 3,
        rejected_action_count: 1,
        tool_call_count: 1,
        tool_error_count: 1,
        visible_entity_count: 12,
        audible_event_count: 3,
        message_count: 2,
        average_tool_latency_ms: 48
      }
    });

    const toolEvent = parseAgentToolCallEvent(toolDocument);
    const toolSummary = summarizeAgentToolCallEvent(toolEvent);
    expect(toolEvent.agent_entity_id).toBe(44);
    expect(toolSummary.status).toBe("TimedOut");
    expect(toolSummary.errorMessage).toBe("timeout");

    const rollup = parseAgentTickRollup(rollupDocument);
    const rollupSummary = summarizeAgentTickRollup(rollup);
    expect(rollup.agent_entity_id).toBe(44);
    expect(rollupSummary.totalDistance).toBe(18.5);
    expect(rollupSummary.toolErrorCount).toBe(1);
  });

  test("routes live debug documents by TOON document type", () => {
    const replayDocument = encode({
      document_type: "replay_file",
      payload: {
        header: {
          name: "flagship-mmo-loop",
          timestamp: 1_741_315_200,
          world_seed: 42,
          tick_count: 120,
          agent_count: 2,
          notes: "acceptance"
        },
        traces: [],
        telemetry_windows: [],
        training_samples: []
      }
    });

    const replay = parseLiveDebugDocument(replayDocument);
    expect(replay.kind).toBe("replay");

    const telemetry = parseLiveDebugDocument(
      encode({
        document_type: "versioned_tick_telemetry",
        payload: {
          version: "V1",
          payload: {
            tick: 9,
            agents: []
          }
        }
      })
    );
    expect(telemetry.kind).toBe("tickTelemetry");
    if (telemetry.kind !== "tickTelemetry") {
      throw new Error("expected tick telemetry document");
    }
    expect(telemetry.payload.tickTelemetry.tick).toBe(9);
  });

  test("parses direct-connect welcome and delta messages", () => {
    const welcome = parseDirectConnectServerMessage(
      JSON.stringify({
        Welcome: {
          client_id: "client-1",
          reconnect_token: "reconnect-1",
          tick: 12,
          controlled_entity: 44,
          authoritative_digest: 991,
          snapshot: {
            tick: 12,
            entities: [
              {
                id: 44,
                position: [12, 18],
                velocity: [1, 0],
                rotation: 0.4,
                label: "Hero",
                metadata: entityMetadata("Player", {
                  chunk_key: "0:0",
                  region_id: "verdant-heart",
                  region_name: "Verdant Heart",
                  faction: {
                    faction_id: "verdant-wardens",
                    role_id: "initiate",
                    disposition: "Friendly",
                    influence_radius: 18
                  },
                  interaction: {
                    can_inspect: true,
                    can_interact: true,
                    can_attack: true,
                    can_gather: false,
                    can_loot: false,
                    can_capture: false,
                    can_command_companion: false,
                    can_chat: true
                  }
                })
              }
            ]
          }
        }
      })
    );

    expect(welcome.kind).toBe("welcome");
    if (welcome.kind !== "welcome") {
      throw new Error("expected welcome");
    }
    expect(welcome.snapshot.entities[0]?.position).toEqual([12, 18]);
    expect(welcome.snapshot.entities[0]?.metadata.kind).toBe("Player");
    expect(welcome.snapshot.entities[0]?.metadata.chunkKey).toBe("0:0");
    expect(welcome.snapshot.entities[0]?.metadata.regionId).toBe("verdant-heart");
    expect(welcome.snapshot.entities[0]?.metadata.faction?.factionId).toBe("verdant-wardens");

    const delta = parseDirectConnectServerMessage(
      JSON.stringify({
        StateDelta: {
          tick: 13,
          acknowledged_action_tick: 12,
          authoritative_digest: 992,
          is_full_snapshot: false,
          delta: {
            tick: 13,
            updated: [
              {
                id: 44,
                position: { x: 14, y: 19 },
                velocity: [2, 0],
                rotation: 0.55,
                label: "Hero",
                metadata: entityMetadata("Player", {
                  chunk_key: "0:1",
                  region_id: "spirewatch",
                  region_name: "Spirewatch Rise",
                  quest_graph_ids: ["verdant-intro", "spire-attunement"],
                  faction_track_id: "verdant-wardens",
                  encounter_table_id: "spirewatch-encounters",
                  team_id: 2,
                  quest_anchor: {
                    quest_ids: ["verdant-intro"],
                    primary_prompt: "Speak with the wardens",
                    stage_tags: ["intro"]
                  },
                  interaction: {
                    can_inspect: true,
                    can_interact: true,
                    can_attack: true,
                    can_gather: false,
                    can_loot: false,
                    can_capture: false,
                    can_command_companion: false,
                    can_chat: true
                  }
                })
              }
            ],
            destroyed: [99]
          }
        }
      })
    );

    expect(delta.kind).toBe("stateDelta");
    if (delta.kind !== "stateDelta") {
      throw new Error("expected delta");
    }
    expect(delta.delta.updated[0]?.position).toEqual([14, 19]);
    expect(delta.acknowledgedActionTick).toBe(12);
    expect(delta.delta.updated[0]?.metadata.teamId).toBe(2);
    expect(delta.delta.updated[0]?.metadata.chunkKey).toBe("0:1");
    expect(delta.delta.updated[0]?.metadata.regionName).toBe("Spirewatch Rise");
    expect(delta.delta.updated[0]?.metadata.questGraphIds).toEqual([
      "verdant-intro",
      "spire-attunement"
    ]);
    expect(delta.delta.updated[0]?.metadata.factionTrackId).toBe("verdant-wardens");
    expect(delta.delta.updated[0]?.metadata.questAnchor?.questIds).toEqual([
      "verdant-intro"
    ]);
  });

  test("parses authoritative event batches into gameplay summaries", () => {
    const eventBatch = parseDirectConnectServerMessage(
      JSON.stringify({
        EventBatch: {
          tick: 27,
          events: [
            {
              tick: 27,
              origin: [18, 22],
              event: {
                Damage: {
                  source: 44,
                  target: 12,
                  amount: 7.5
                }
              }
            },
            {
              tick: 27,
              origin: [18, 22],
              event: {
                AgentSpoke: {
                  agent_id: "agent-12345678",
                  message: "ready",
                  volume: 200
                }
              }
            }
          ]
        }
      })
    );

    expect(eventBatch.kind).toBe("eventBatch");
    if (eventBatch.kind !== "eventBatch") {
      throw new Error("expected event batch");
    }
    expect(eventBatch.events).toHaveLength(2);
    expect(eventBatch.events[0]?.summary).toBe("E(44) hit E(12) for 7.5");
    expect(eventBatch.events[0]?.entityIds).toEqual([12, 44]);
    expect(eventBatch.events[1]?.summary).toBe("agent-12: ready");
  });

  test("applies authoritative state deltas and builds a live world frame", () => {
    const baseline: NetworkWorldSnapshot = {
      tick: 10,
      population: populationState(10),
      entities: [
        {
          id: 1,
          position: [10, 10],
          velocity: [0, 0],
          rotation: 0,
          label: "Hero",
          metadata: typedEntityMetadata("Player", {
            teamId: 1,
            combatStyle: "Melee",
            interaction: {
              canInspect: true,
              canInteract: true,
              canAttack: true,
              canGather: false,
              canLoot: false,
              canCapture: false,
              canCommandCompanion: false,
              canChat: true
            }
          })
        },
        {
          id: 2,
          position: [20, 12],
          velocity: [0, 0],
          rotation: 0,
          label: "Wall-East",
          metadata: typedEntityMetadata("Scenery", {
            interaction: {
              canInspect: true,
              canInteract: false,
              canAttack: false,
              canGather: false,
              canLoot: false,
              canCapture: false,
              canCommandCompanion: false,
              canChat: false
            }
          })
        }
      ]
    };

    const next = applyNetworkStateDelta(
      baseline,
      {
        tick: 11,
        population: {
          tick: 11,
          chunks: [
            {
              chunkKey: "0:0",
              regionId: "verdant-heart",
              regionName: "Verdant Heart",
              biomeId: "verdant-hollow",
              questGraphIds: ["verdant-intro"],
              factionTrackId: "verdant-wardens",
              encounterTableIds: ["verdant-heart-wildlife"],
              counts: {
                players: 1,
                npcs: 0,
                wildCreatures: 1,
                companions: 0,
                resourceNodes: 0,
                lootContainers: 0,
                scenery: 2
              },
              activeEntityCount: 4,
              ambientPopulationCap: 6,
              spawnBudgetRemaining: 4,
              pendingRespawns: 1,
              nextRespawnTick: 36,
              populationPressure: 0.33
            }
          ],
          regions: [
            {
              regionId: "verdant-heart",
              regionName: "Verdant Heart",
              primaryBiomeId: "verdant-hollow",
              chunkKeys: ["0:0"],
              activeQuestGraphIds: ["verdant-intro"],
              dominantFactionTrackId: "verdant-wardens",
              encounterTableIds: ["verdant-heart-wildlife"],
              activeChunkCount: 1,
              counts: {
                players: 1,
                npcs: 0,
                wildCreatures: 1,
                companions: 0,
                resourceNodes: 0,
                lootContainers: 0,
                scenery: 2
              },
              activeEntityCount: 4,
              ambientPopulationCap: 6,
              spawnBudgetRemaining: 4,
              pendingRespawns: 1,
              nextRespawnTick: 36,
              populationPressure: 0.33
            }
          ]
        },
        updated: [
          {
            id: 1,
            position: [14, 12],
            velocity: [4, 0],
            rotation: 0.3,
            label: "Hero",
            metadata: typedEntityMetadata("Player", {
              teamId: 1,
              combatStyle: "Melee",
              interaction: {
                canInspect: true,
                canInteract: true,
                canAttack: true,
                canGather: false,
                canLoot: false,
                canCapture: false,
                canCommandCompanion: false,
                canChat: true
              }
            })
          },
        {
          id: 3,
          position: [18, 20],
          velocity: [0, 0],
            rotation: 0,
            label: "Monster-Wolf",
            metadata: typedEntityMetadata("WildCreature", {
              combatStyle: "Melee",
              speciesId: "embercub",
              speciesName: "Wild Embercub",
              encounterKind: "WildCreature",
              interaction: {
                canInspect: true,
                canInteract: false,
                canAttack: true,
                canGather: false,
                canLoot: false,
                canCapture: true,
                canCommandCompanion: false,
                canChat: false
            }
          })
        },
        {
          id: 4,
          position: [8, 26],
          velocity: [0, 0],
          rotation: 0,
          label: "Glass Spire",
          metadata: typedEntityMetadata("Scenery", {
            interaction: {
              canInspect: true,
              canInteract: false,
              canAttack: false,
              canGather: false,
              canLoot: false,
              canCapture: false,
              canCommandCompanion: false,
              canChat: false
            }
          })
        },
        {
          id: 5,
          position: [6, 4],
          velocity: [0, 0],
          rotation: 0,
          label: "Canopy Tree",
          metadata: typedEntityMetadata("Scenery", {
            interaction: {
              canInspect: true,
              canInteract: false,
              canAttack: false,
              canGather: false,
              canLoot: false,
              canCapture: false,
              canCommandCompanion: false,
              canChat: false
            }
          })
        }
      ],
      destroyed: [2]
      },
      false
    );

    expect(next.tick).toBe(11);
    expect(next.entities.map((entity) => entity.id)).toEqual([1, 3, 4, 5]);
    expect(next.population.regions[0]?.regionId).toBe("verdant-heart");
    expect(next.population.chunks[0]?.ambientPopulationCap).toBe(6);

    const frame = buildAuthoritativeWorldFrame(next, {
      controlledEntity: 1,
      viewportWidth: 1440,
      viewportHeight: 900
    });

    expect(frame.camera.viewportWidth).toBe(1440);
    expect(frame.meshBatches.some((batch) => batch.mesh.includes("adventurer"))).toBe(true);
    expect(frame.meshBatches.some((batch) => batch.mesh.includes("rift-beast"))).toBe(true);
    expect(frame.meshBatches.some((batch) => batch.mesh === "glass-spire")).toBe(true);
    expect(frame.meshBatches.some((batch) => batch.mesh === "canopy-tree")).toBe(true);
    expect(frame.spriteBatches.some((batch) => batch.texture === "selection-ring")).toBe(true);
  });

  test("builds presentation-driven environments and actor affordances", () => {
    const snapshot: NetworkWorldSnapshot = {
      tick: 18,
      population: populationState(18),
      entities: [
        {
          id: 1,
          position: [8, 10],
          velocity: [2.2, 0.4],
          rotation: 0.25,
          label: "Hero",
          metadata: typedEntityMetadata("Player", {
            actorPresentation: {
              profileId: "heroic-adventurer",
              meshAssetId: "adventurer-hero",
              materialPaletteId: "verdant",
              animationSetId: "hero-runescape",
              scaleMultiplier: 1.15,
              footprintRadius: 1.1,
              selectionRingScale: 3.2,
              auraColor: [0.32, 0.86, 0.74, 0.28]
            },
            combatPresentation: {
              profileId: "heroic-combat",
              hitFlashColor: [1, 0.72, 0.34, 0.38],
              criticalRingColor: [1, 0.28, 0.2, 0.42],
              selectionRingColor: [0.62, 0.98, 0.84, 0.55],
              emissiveBoost: [0.05, 0.04, 0.02],
              impactScale: 1.45
            }
          })
        },
        {
          id: 7,
          position: [9, 9],
          velocity: [0, 0],
          rotation: 0,
          label: "Glass Spire",
          metadata: typedEntityMetadata("Scenery", {
            atmosphere: {
              biomeId: "verdant-hollow",
              skyColor: [0.12, 0.16, 0.22, 1],
              fogColor: [0.18, 0.25, 0.29, 1],
              fogNear: 18,
              fogFar: 130,
              ambientColor: [0.58, 0.82, 0.94],
              ambientIntensity: 1.35,
              sunColor: [0.96, 0.88, 0.74],
              sunIntensity: 2.9,
              sunDirection: [18, 34, 12],
              fillColor: [0.34, 0.8, 0.92],
              fillIntensity: 0.92,
              fillDirection: [-14, 16, -6],
              rimColor: [0.42, 0.96, 1],
              rimIntensity: 13,
              groundColor: [0.1, 0.14, 0.18, 1],
              starfieldIntensity: 0.42
            },
            atmosphereVolume: {
              radius: 8,
              priority: 3
            }
          })
        },
        {
          id: 9,
          position: [12, 10],
          velocity: [0, 0],
          rotation: 0.4,
          health: 10,
          maxHealth: 40,
          label: "Rift Beast",
          metadata: typedEntityMetadata("WildCreature", {
            speciesId: "rift-beast",
            speciesName: "Rift Beast",
            combatPresentation: {
              profileId: "beast-combat",
              hitFlashColor: [1, 0.64, 0.44, 0.34],
              criticalRingColor: [1, 0.18, 0.16, 0.5],
              selectionRingColor: [0.82, 0.34, 0.3, 0.28],
              emissiveBoost: [0.08, 0.02, 0.01],
              impactScale: 1.75
            }
          })
        }
      ]
    };

    const frame = buildAuthoritativeWorldFrame(snapshot, {
      controlledEntity: 1,
      viewportWidth: 1600,
      viewportHeight: 900
    });

    expect(frame.environment.biomeId).toBe("verdant-hollow");
    expect(frame.environment.fogFar).toBe(130);
    expect(frame.meshBatches.some((batch) => batch.material.includes(":verdant"))).toBe(true);
    const heroInstance = frame.meshBatches
      .flatMap((batch) => batch.instances)
      .find((instance) => instance.sourceEntity === 1);
    expect(heroInstance?.animationSetId).toBe("hero-runescape");
    expect(heroInstance?.motionSpeed).toBeGreaterThan(0.3);
    expect(heroInstance?.controlled).toBe(true);
    const selectionRing = frame.spriteBatches.find(
      (batch) => batch.texture === "selection-ring"
    );
    expect(selectionRing?.instances[0]?.scale).toEqual([3.2, 3.2, 1]);
    expect(selectionRing?.instances[0]?.color).toEqual([0.62, 0.98, 0.84, 0.55]);
    expect(selectionRing?.instances[0]?.animationSetId).toBe("selection-ring");

    const auraRing = frame.spriteBatches.find((batch) => batch.texture === "mist-ring");
    expect(auraRing?.instances[0]?.color).toEqual([0.32, 0.86, 0.74, 0.28]);

    const criticalRing = frame.spriteBatches.find(
      (batch) =>
        batch.texture === "danger-ring" &&
        batch.instances.some((instance) => instance.sourceEntity === 9)
    );
    expect(criticalRing?.instances[0]?.scale).toEqual([4.2, 4.2, 1]);
    expect(criticalRing?.instances[0]?.color).toEqual([1, 0.18, 0.16, 0.5]);
  });

  test("adds point-click and target selection markers without mutating the base frame", () => {
    const target = {
      id: 12,
      position: [14, -6],
      velocity: [0, 0],
      rotation: 0,
      health: 18,
      maxHealth: 24,
      movementSpeed: 4,
      label: "Verdant Lynx",
      metadata: typedEntityMetadata("WildCreature", {
        actorPresentation: {
          profileId: "lynx",
          meshAssetId: "rift-beast",
          materialPaletteId: "default",
          animationSetId: "beast-stalker",
          scaleMultiplier: 1,
          footprintRadius: 1.2,
          selectionRingScale: 2.8,
          auraColor: [0, 0, 0, 0]
        }
      })
    } satisfies NetworkWorldSnapshot["entities"][number];
    const baseFrame = {
      camera: {
        x: 0,
        y: 0,
        zoom: 1,
        rotation: 0.4,
        viewportWidth: 1280,
        viewportHeight: 720
      },
      backgroundColor: [0, 0, 0, 1],
      environment: {
        biomeId: "verdant-hollow",
        skyColor: [0.64, 0.8, 0.98, 1],
        fogColor: [0.72, 0.84, 0.78, 1],
        fogNear: 30,
        fogFar: 196,
        ambientColor: [0.82, 0.92, 0.88],
        ambientIntensity: 1.4,
        sunColor: [1, 0.96, 0.84],
        sunIntensity: 2.95,
        sunDirection: [30, 48, 18],
        fillColor: [0.48, 0.76, 0.94],
        fillIntensity: 0.88,
        fillDirection: [-18, 14, -10],
        rimColor: [0.4, 0.88, 0.78],
        rimIntensity: 8.5,
        groundColor: [0.19, 0.33, 0.21, 1],
        starfieldIntensity: 0.08
      },
      overlayCommands: [],
      meshBatches: [],
      spriteBatches: [],
      hints: {
        renderer: "three/webgpu",
        preferredBackend: "webgpu",
        fallbackBackend: "webgl2",
        useInstancing: true,
        sortMetric: "world-z",
        sortOpaqueFrontToBack: true,
        preserveInstanceOrder: true,
        sortTransparentBackToFront: true,
        transparentInstancingStrategy: "shared-sort-depth",
        opaqueDepthWrite: true,
        transparentDepthWrite: false,
        maxPixelRatio: 2
      }
    } satisfies Parameters<typeof withInteractionMarkers>[0];

    const decorated = withInteractionMarkers(baseFrame, {
      moveTarget: [6, -4],
      selectedTarget: target,
      controlledEntity: 1,
      controlledSnapshot: {
        id: 1,
        position: [0, 0],
        velocity: [0, 0],
        rotation: 0,
        health: 24,
        maxHealth: 24,
        movementSpeed: 4,
        label: "WebPlayer",
        metadata: typedEntityMetadata("Player", {})
      }
    });

    expect(baseFrame.spriteBatches).toHaveLength(0);
    expect(decorated.spriteBatches).toHaveLength(3);
    expect(
      decorated.spriteBatches.some((batch) =>
        batch.instances.some((instance) => instance.animationSetId === "destination-ring")
      )
    ).toBe(true);
    const pathBatch = decorated.spriteBatches.find((batch) =>
      batch.instances.some((instance) => instance.animationSetId === "path-node")
    );
    expect(pathBatch?.instances.length).toBeGreaterThanOrEqual(2);
    expect(
      decorated.spriteBatches.some((batch) =>
        batch.instances.some(
          (instance) =>
            instance.animationSetId === "target-ring" && instance.sourceEntity === 12
        )
      )
    ).toBe(true);
  });

  test("encodes browser direct-connect client messages with Rust enum tags", () => {
    expect(JSON.parse(encodeDirectConnectConnectMessage("WebPlayer", "resume-1"))).toEqual({
      Connect: {
        player_name: "WebPlayer",
        reconnect_token: "resume-1"
      }
    });

    expect(JSON.parse(encodeDirectConnectDebugTelemetryMessage(true))).toEqual({
      SetDebugTelemetry: {
        enabled: true
      }
    });

    expect(JSON.parse(encodeDirectConnectFullSnapshotRequest(42, 99))).toEqual({
      RequestFullSnapshot: {
        last_known_tick: 42,
        last_known_digest: 99
      }
    });

    expect(
      JSON.parse(
        encodeDirectConnectActionBatch(77, [
          { kind: "move", direction: [1, 0] },
          { kind: "attackTarget", target: 9 },
          { kind: "speak", message: "hello", volume: "Normal" }
        ])
      )
    ).toEqual({
      ActionBatch: {
        tick: 77,
        actions: [
          { Move: { direction: [1, 0] } },
          { AttackTarget: { target: 9 } },
          { Speak: { message: "hello", volume: "Normal" } }
        ]
      }
    });
  });

  test("parses authoritative world population summaries from snapshots", () => {
    const message = parseDirectConnectServerMessage(
      JSON.stringify({
        Welcome: {
          client_id: "client-1",
          reconnect_token: "resume-1",
          tick: 24,
          controlled_entity: 1,
          authoritative_digest: 77,
          snapshot: {
            tick: 24,
            entities: [],
            population: {
              tick: 24,
              chunks: [
                {
                  chunk_key: "0:0",
                  region_id: "verdant-heart",
                  region_name: "Verdant Heart",
                  biome_id: "verdant-hollow",
                  quest_graph_ids: ["verdant-intro"],
                  faction_track_id: "verdant-wardens",
                  encounter_table_ids: ["verdant-heart-wildlife"],
                  counts: {
                    players: 1,
                    npcs: 2,
                    wild_creatures: 3,
                    companions: 1,
                    resource_nodes: 2,
                    loot_containers: 1,
                    scenery: 5
                  },
                  active_entity_count: 15,
                  ambient_population_cap: 8,
                  spawn_budget_remaining: 1,
                  pending_respawns: 2,
                  next_respawn_tick: 42,
                  population_pressure: 0.875
                }
              ],
              regions: [
                {
                  region_id: "verdant-heart",
                  region_name: "Verdant Heart",
                  primary_biome_id: "verdant-hollow",
                  chunk_keys: ["0:0", "0:1"],
                  active_quest_graph_ids: ["verdant-intro"],
                  dominant_faction_track_id: "verdant-wardens",
                  encounter_table_ids: ["verdant-heart-wildlife"],
                  active_chunk_count: 1,
                  counts: {
                    players: 1,
                    npcs: 2,
                    wild_creatures: 3,
                    companions: 1,
                    resource_nodes: 2,
                    loot_containers: 1,
                    scenery: 5
                  },
                  active_entity_count: 15,
                  ambient_population_cap: 8,
                  spawn_budget_remaining: 1,
                  pending_respawns: 2,
                  next_respawn_tick: 42,
                  population_pressure: 0.875
                }
              ]
            }
          }
        }
      })
    );

    expect(message.kind).toBe("welcome");
    if (message.kind !== "welcome") {
      throw new Error("expected welcome");
    }

    expect(message.snapshot.population.tick).toBe(24);
    expect(message.snapshot.population.chunks[0]?.chunkKey).toBe("0:0");
    expect(message.snapshot.population.chunks[0]?.counts.wildCreatures).toBe(3);
    expect(message.snapshot.population.chunks[0]?.nextRespawnTick).toBe(42);
    expect(message.snapshot.population.regions[0]?.populationPressure).toBe(0.875);
  });
});
