import { describe, expect, test } from "bun:test";

import {
  buildDefaultSnapshotOutputPath,
  normalizeShardTargetMoatSnapshot,
} from "./publish_moat_snapshots";

describe("publish moat snapshots", () => {
  test("normalizes shard-target transport and browser data into a committed snapshot shape", () => {
    const snapshot = normalizeShardTargetMoatSnapshot(
      {
        profile: "shard-target",
        transportMeasurements: {
          schema_version: 2,
          profile: "shard-target",
          aggregate: {
            all_checks_passed: true,
            scenarios_passed: 2,
            scenario_count: 2,
            published_baseline_profile: "shard-target",
            checks: [
              {
                metric: "published_baseline.aggregate.total_delta_bytes",
                passed: true,
                expected: "1816",
                observed: "1816",
              },
            ],
            total_full_snapshot_bytes: 1187,
            total_recovery_snapshot_bytes: 234,
            total_delta_bytes: 1816,
            total_delta_entities_updated: 8,
            total_delta_entities_destroyed: 8,
            total_queue_pressure_events: 1,
            total_resumed_sessions: 1,
            total_timed_out_clients: 1,
            total_recovery_delivery_failures: 1,
          },
          scenarios: [
            {
              name: "steady-delta",
              description: "delta path",
              all_checks_passed: true,
              summary: {
                latest_tick: 0,
                client_count: 1,
                resumed_sessions: 0,
                recovery_snapshots_sent: 0,
                recovery_delivery_failures: 0,
                total_pending_action_queue_depth: 0,
                peak_pending_action_queue_depth: 0,
                queue_pressure_client_count: 0,
                total_inbound_messages: 0,
                total_outbound_messages: 8,
                total_inbound_bytes: 0,
                total_outbound_bytes: 1304,
                action_batches_received: 0,
                full_snapshots_sent: 0,
                total_full_snapshot_bytes: 0,
                max_full_snapshot_bytes: 0,
                total_recovery_snapshot_bytes: 0,
                full_snapshot_requests: 0,
                state_deltas_sent: 8,
                delta_messages_sent: 8,
                total_delta_bytes: 1304,
                max_delta_bytes: 163,
                total_delta_entities_updated: 8,
                total_delta_entities_destroyed: 8,
                timed_out_clients: 0,
                queue_pressure_events: 0,
              },
              checks: [
                {
                  metric: "published_baseline.steady_delta.total_delta_bytes",
                  passed: true,
                  expected: "1304",
                  observed: "1304",
                },
              ],
            },
          ],
        },
        headlessTopology: {
          sourceSchemaVersion: 2,
          scenario: "deadman-neural-cup",
          profile: "shard-target",
          teamCount: 2,
          worldCount: 3,
          linkCount: 3,
          worldQuestBindingCount: 3,
          appliedWorldStateCount: 2,
          evaluationWorldCount: 3,
          tournamentOrchestration: {
            phase: "Active",
            activeWorldCount: 3,
            contestedWorldCount: 2,
            activeLinkCount: 3,
            leadingTeamCount: 1,
            atRiskTeamCount: 1,
            pressureWorldCount: 2,
            neuralSwarmWorldCount: 1,
          },
          topologyParity: {
            consistent: true,
            teams_match: true,
            worlds_match: true,
            links_match: true,
            quest_graphs_match: true,
            world_quest_bindings_match: true,
            world_admissions_match: true,
            world_control_planes_match: true,
            applied_world_states_match: true,
            evaluation_match: true,
            tournament_control_plane_match: true,
            tournament_orchestration_match: true,
            missing_world_quest_binding_ids: [],
            unexpected_world_quest_binding_ids: [],
            missing_world_admission_ids: [],
            unexpected_world_admission_ids: [],
            missing_world_control_plane_ids: [],
            unexpected_world_control_plane_ids: [],
            missing_applied_world_ids: [],
            unexpected_applied_world_ids: [],
            missing_evaluation_world_ids: [],
            unexpected_evaluation_world_ids: [],
          },
          checks: [
            {
              metric: "topology_parity.consistent",
              passed: true,
              expected: "true",
              observed: "true",
            },
          ],
          allChecksPassed: true,
        },
        topologyFeedMeasurements: {
          schema_version: 1,
          scenario_id: "deadman-neural-cup",
          profile_id: "shard-target",
          world_count: 1,
          worlds: [
            {
              world_id: "deadman-prime",
              authority_row: {
                resolved_world_id: "deadman-prime",
                resolved_world_matches: true,
                quest_binding_matches: true,
                applied_world_state_matches: true,
                evaluation_matches: true,
                world_tournament_orchestration_matches: true,
                tournament_control_plane_matches: true,
                tournament_orchestration_matches: true,
              },
              generated_runtime: {
                resolved_world_id: "deadman-prime",
                resolved_world_matches: true,
                quest_binding_matches: true,
                applied_world_state_matches: true,
                evaluation_matches: true,
                world_tournament_orchestration_matches: true,
                tournament_control_plane_matches: true,
                tournament_orchestration_matches: true,
              },
            },
          ],
          checks: [
            {
              metric: "authority_row.deadman-prime.resolved_world_matches",
              passed: true,
              expected: "true",
              observed: "\"deadman-prime\"",
            },
          ],
        },
        browserRouteMeasurements: {
          schemaVersion: 2,
          routes: [
            {
              label: "worker",
              url: "http://127.0.0.1:4178/?world=local-sandbox&renderThread=worker&backend=webgl2",
              renderThread: "worker",
              requestedRenderThread: "worker",
              renderThreadFallbackReason: null,
              loadsCompleted: 12,
              pendingAssets: 0,
              assetLoadPerf: {
                geometryLoadsCompleted: 8,
                spriteLoadsCompleted: 4,
                averageGeometryLoadMs: 42.345,
                averageSpriteLoadMs: 18.222,
                slowestGeometryLoadMs: 120.987,
                slowestSpriteLoadMs: 44.444,
              },
              mainThreadPerf: {
                warmupMs: 12.345,
                submissionsCompleted: 5,
                averageSubmissionMs: 0.543,
                slowestSubmissionMs: 0.812,
                byKind: {
                  frame: {
                    submissionsCompleted: 5,
                    averageSubmissionMs: 0.543,
                    slowestSubmissionMs: 0.812,
                  },
                  control: {
                    submissionsCompleted: 0,
                    averageSubmissionMs: 0,
                    slowestSubmissionMs: 0,
                  },
                  resize: {
                    submissionsCompleted: 0,
                    averageSubmissionMs: 0,
                    slowestSubmissionMs: 0,
                  },
                },
              },
              runtimePerf: {
                warmupMs: 16.666,
                frameBudgetMs: 16.666,
                framesRendered: 8,
                stableFrames: 6,
                slowFrames: 2,
                stableFramePercent: 75.4321,
                slowestFrameMs: 15.987,
              },
              gates: {
                stableFramePercentFloor: 50,
                stableFramePercentFloorPassed: true,
                completedAssetLoadsFloor: 10,
                completedAssetLoadsFloorPassed: true,
                averageGeometryLoadMsCeiling: 250,
                averageGeometryLoadMsCeilingPassed: true,
                averageSpriteLoadMsCeiling: 500,
                averageSpriteLoadMsCeilingPassed: true,
                slowestGeometryLoadMsCeiling: 2000,
                slowestGeometryLoadMsCeilingPassed: true,
                slowestSpriteLoadMsCeiling: 1000,
                slowestSpriteLoadMsCeilingPassed: true,
                controlSubmissionCeiling: 0,
                controlSubmissionCeilingPassed: true,
                resizeSubmissionCeiling: 0,
                resizeSubmissionCeilingPassed: true,
              },
            },
          ],
          comparison: {
            mainFrameSubmissions: 76,
            workerFrameSubmissions: 5,
            frameSubmissionReductionPercent: 93.421,
            mainStableFramePercent: 98.765,
            workerStableFramePercent: 75.432,
            stableFramePercentDelta: -23.333,
            mainSlowFrames: 1,
            workerSlowFrames: 2,
            slowFrameDelta: 1,
            workerGatesPassed: true,
          },
        },
      },
      "2026-W11",
      null,
      {
        schema_version: 1,
        scenario_id: "deadman-neural-cup",
        profile_id: "shard-target",
        world_count: 1,
        worlds: [
          {
            world_id: "deadman-prime",
            authority_row: {
              resolved_world_id: "deadman-prime",
              resolved_world_matches: true,
              quest_binding_matches: true,
              applied_world_state_matches: true,
              evaluation_matches: true,
              world_tournament_orchestration_matches: true,
              tournament_control_plane_matches: true,
              tournament_orchestration_matches: true,
            },
            generated_runtime: {
              resolved_world_id: "deadman-prime",
              resolved_world_matches: true,
              quest_binding_matches: true,
              applied_world_state_matches: true,
              evaluation_matches: true,
              world_tournament_orchestration_matches: true,
              tournament_control_plane_matches: true,
              tournament_orchestration_matches: true,
            },
          },
        ],
        checks: [
          {
            metric: "generated_runtime.deadman-prime.resolved_world_id",
            passed: true,
            expected: "deadman-prime",
            observed: "deadman-prime",
          },
        ],
      },
    );

    expect(snapshot.schemaVersion).toBe(4);
    expect(snapshot.label).toBe("2026-W11");
    expect(snapshot.transport.aggregate.published_baseline_profile).toBe(
      "shard-target",
    );
    expect(snapshot.transport.scenarios[0]?.summary.total_delta_bytes).toBe(1304);
    expect(snapshot.browserRoutes.routes[0]?.routePath).toBe(
      "/?world=local-sandbox&renderThread=worker&backend=webgl2",
    );
    expect(snapshot.browserRoutes.routes[0]?.assetLoadPerf.averageGeometryLoadMs).toBe(
      42.3,
    );
    expect(snapshot.browserRoutes.routes[0]?.runtimePerf.stableFramePercent).toBe(
      75.4,
    );
    expect(snapshot.browserRoutes.comparison?.frameSubmissionReductionPercent).toBe(
      93.4,
    );
    expect(snapshot.headlessTopology.teamCount).toBe(2);
    expect(snapshot.headlessTopology.topologyParity.consistent).toBe(true);
    expect(snapshot.headlessTopology.tournamentOrchestration.phase).toBe("Active");
    expect(snapshot.topologyFeed.scenarioId).toBe("deadman-neural-cup");
    expect(snapshot.topologyFeed.worlds[0]?.generated_runtime.evaluation_matches).toBe(
      true,
    );
    expect(
      snapshot.topologyFeed.worlds[0]?.generated_runtime.tournament_orchestration_matches,
    ).toBe(true);
    expect(snapshot.liveTopologyFeed?.profileId).toBe("shard-target");
    expect(snapshot.liveTopologyFeed?.checks[0]?.metric).toBe(
      "generated_runtime.deadman-prime.resolved_world_id",
    );
  });

  test("rejects non shard-target moat reports", () => {
    expect(() =>
      normalizeShardTargetMoatSnapshot(
        {
          profile: "ci-smoke",
          transportMeasurements: null,
          headlessTopology: null,
          topologyFeedMeasurements: null,
          browserRouteMeasurements: null,
        },
        "2026-W11",
      ),
    ).toThrow("expected shard-target moat report");
  });

  test("accepts browser routes from a separate artifact when the moat report skips browser execution", () => {
    const snapshot = normalizeShardTargetMoatSnapshot(
      {
        profile: "shard-target",
        transportMeasurements: {
          schema_version: 2,
          profile: "shard-target",
          aggregate: {
            all_checks_passed: true,
            scenarios_passed: 0,
            scenario_count: 0,
            published_baseline_profile: "shard-target",
            checks: [],
            total_full_snapshot_bytes: 1220,
            total_recovery_snapshot_bytes: 234,
            total_delta_bytes: 1904,
            total_delta_entities_updated: 8,
            total_delta_entities_destroyed: 8,
            total_queue_pressure_events: 1,
            total_resumed_sessions: 1,
            total_timed_out_clients: 1,
            total_recovery_delivery_failures: 1,
          },
          scenarios: [],
        },
        headlessTopology: {
          sourceSchemaVersion: 2,
          scenario: "deadman-neural-cup",
          profile: "shard-target",
          teamCount: 2,
          worldCount: 3,
          linkCount: 3,
          worldQuestBindingCount: 3,
          appliedWorldStateCount: 3,
          evaluationWorldCount: 3,
          topologyParity: {
            consistent: true,
            teams_match: true,
            worlds_match: true,
            links_match: true,
            quest_graphs_match: true,
            world_quest_bindings_match: true,
            world_admissions_match: true,
            world_control_planes_match: true,
            applied_world_states_match: true,
            evaluation_match: true,
            tournament_control_plane_match: true,
            tournament_orchestration_match: true,
            missing_world_quest_binding_ids: [],
            unexpected_world_quest_binding_ids: [],
            missing_world_admission_ids: [],
            unexpected_world_admission_ids: [],
            missing_world_control_plane_ids: [],
            unexpected_world_control_plane_ids: [],
            missing_applied_world_ids: [],
            unexpected_applied_world_ids: [],
            missing_evaluation_world_ids: [],
            unexpected_evaluation_world_ids: [],
          },
          checks: [],
          allChecksPassed: true,
        },
        topologyFeedMeasurements: {
          schema_version: 1,
          scenario_id: "deadman-neural-cup",
          profile_id: "shard-target",
          world_count: 3,
          worlds: [],
          checks: [],
        },
        browserRouteMeasurements: null,
      },
      "2026-W11",
      {
        schemaVersion: 2,
        routes: [
          {
            label: "main",
            url: "http://127.0.0.1:4178/?world=local-sandbox&renderThread=main&backend=webgl2",
            renderThread: "main",
            requestedRenderThread: "main",
            renderThreadFallbackReason: null,
            loadsCompleted: 15,
            pendingAssets: 0,
            assetLoadPerf: {
              geometryLoadsCompleted: 10,
              spriteLoadsCompleted: 5,
              averageGeometryLoadMs: 100,
              averageSpriteLoadMs: 150,
              slowestGeometryLoadMs: 200,
              slowestSpriteLoadMs: 250,
            },
            mainThreadPerf: {
              warmupMs: 10,
              submissionsCompleted: 70,
              averageSubmissionMs: 0.5,
              slowestSubmissionMs: 0.9,
              byKind: {
                frame: {
                  submissionsCompleted: 70,
                  averageSubmissionMs: 0.5,
                  slowestSubmissionMs: 0.9,
                },
                control: {
                  submissionsCompleted: 0,
                  averageSubmissionMs: 0,
                  slowestSubmissionMs: 0,
                },
                resize: {
                  submissionsCompleted: 0,
                  averageSubmissionMs: 0,
                  slowestSubmissionMs: 0,
                },
              },
            },
            runtimePerf: {
              warmupMs: 16.6,
              frameBudgetMs: 16.6,
              framesRendered: 8,
              stableFrames: 7,
              slowFrames: 1,
              stableFramePercent: 87.5,
              slowestFrameMs: 17.2,
            },
            gates: {
              stableFramePercentFloor: 90,
              stableFramePercentFloorPassed: false,
              completedAssetLoadsFloor: 10,
              completedAssetLoadsFloorPassed: true,
              averageGeometryLoadMsCeiling: 250,
              averageGeometryLoadMsCeilingPassed: true,
              averageSpriteLoadMsCeiling: 500,
              averageSpriteLoadMsCeilingPassed: true,
              slowestGeometryLoadMsCeiling: 2000,
              slowestGeometryLoadMsCeilingPassed: true,
              slowestSpriteLoadMsCeiling: 1000,
              slowestSpriteLoadMsCeilingPassed: true,
              controlSubmissionCeiling: null,
              controlSubmissionCeilingPassed: null,
              resizeSubmissionCeiling: null,
              resizeSubmissionCeilingPassed: null,
            },
          },
        ],
        comparison: null,
      },
    );

    expect(snapshot.browserRoutes.routes[0]?.label).toBe("main");
    expect(snapshot.browserRoutes.routes[0]?.routePath).toBe(
      "/?world=local-sandbox&renderThread=main&backend=webgl2",
    );
  });

  test("builds the default ISO-week-labeled output path", () => {
    expect(buildDefaultSnapshotOutputPath("2026-W11")).toBe(
      "docs/benchmark-snapshots/2026-W11-shard-target.json",
    );
  });
});
