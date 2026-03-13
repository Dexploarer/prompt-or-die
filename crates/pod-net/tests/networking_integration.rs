//! Integration tests for SpacetimeDB network client behavior.
//!
//! These tests exercise the public `pod-net` StDB adapter API end-to-end without
//! requiring a real SpacetimeDB service. The stubbed `pod-stdb` client in this
//! repository is still fully exercised for profile transitions, reducer guards, and
//! frame polling behavior.

#![cfg(feature = "spacetimedb")]

use glam::Vec2;
use pod_core::{
    Action, AppliedWorldStateSummary, ControllerEvaluationSummary, QuestLineStateSummary,
    QuestStageApplicationSummary, RemoteTopologyBundle, TeamDeathMarkSummary, TeamDeltaSummary,
    WorldEvaluationSummary, WorldQuestBinding, WorldRealityDefinition, WorldRealityRole,
    WorldTournamentDefinition,
};
use pod_net::{SpacetimeDBClient, SpacetimeDBClientConfig, StdbClientError};
use pod_stdb::client::StdbConnectionMode;

#[test]
fn integration_connect_stages_default_subscriptions_without_network() {
    let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());

    assert!(!client.is_connected());
    assert!(!client
        .subscribe_as_spectator()
        .expect("staging spectator subscriptions"));
    assert!(!client
        .subscribe_as_editor()
        .expect("staging editor subscriptions"));
    assert!(!client
        .subscribe_as_editor_with_debug_telemetry()
        .expect("staging editor debug telemetry subscriptions"));
    assert!(!client
        .subscribe_for_player(7)
        .expect("staging player subscriptions"));
    assert!(!client
        .subscribe_for_player_with_interest(7, 100.0, 200.0, 50.0)
        .expect("staging interest query subscriptions"));
    assert!(!client
        .subscribe_for_player_with_interest_partitioned(7, 100.0, 200.0, 50.0, 10.0)
        .expect("staging partitioned interest query subscriptions"));
    assert!(!client
        .subscribe_custom(vec!["SELECT * FROM world_state".to_string()])
        .expect("staging custom subscriptions"));
}

#[test]
fn integration_connect_guard_and_rejects_duplicate_connect_attempt() {
    let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());

    client
        .connect()
        .expect("initial connect should move to connecting");
    assert!(!client.is_connected());

    let second_connect = client.connect();
    assert!(matches!(
        second_connect,
        Err(StdbClientError::InvalidState(_))
    ));

    // Connection is asynchronous in this layer, so polling should not panic and should
    // not emit any synthetic server messages in stub mode.
    assert!(client.poll_updates().is_empty());
}

#[test]
fn integration_send_actions_guarded_by_connection_and_spectator_profile() {
    let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());

    client
        .subscribe_as_spectator()
        .expect("staging spectator profile");
    client.queue_action(Action::Move {
        direction: Vec2::new(1.0, 0.0),
    });

    let not_connected = client.send_actions(0);
    assert!(matches!(not_connected, Err(StdbClientError::NotConnected)));
}

#[test]
fn integration_remote_topology_surfaces_linked_world_quest_and_evaluation_state() {
    let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
        db_name: "deadman-shadow".into(),
        connection_mode: StdbConnectionMode::Emulated,
        ..SpacetimeDBClientConfig::default()
    });

    client.connect().expect("connect in emulated mode");
    client
        .subscribe_as_spectator()
        .expect("staging spectator subscriptions");

    let mut shadow =
        WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow-seasonal");
    shadow.role = WorldRealityRole::Shadow;
    shadow.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
    shadow.linked_world_ids = vec!["deadman-prime".into()];

    client
        .apply_remote_topology(RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 42,
            tournament: WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![],
            worlds: vec![shadow],
            links: vec![],
            world_quest_bindings: vec![WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-hunt".into()],
            }],
            quest_graphs: vec![],
            applied_world_states: vec![AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
                team_scores: vec![TeamDeltaSummary {
                    team_id: "iron-sigil".into(),
                    total_delta: 10,
                }],
                death_marks: vec![TeamDeathMarkSummary {
                    team_id: "gloam-mesh".into(),
                    applications: 2,
                    total_duration_ticks: 1200,
                }],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![pod_core::ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    stage_tag: "marked-by-kills".into(),
                    applications: 2,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    display_name: "Deadman Shadow: Mirror Hunt".into(),
                    current_stage_ids: vec!["marked-by-kills".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["rift-collapse".into()],
                    next_stage_ids: vec!["rift-collapse".into()],
                    progress_basis_points: 6666,
                    terminal: false,
                    stage_applications: vec![QuestStageApplicationSummary {
                        stage_id: "marked-by-kills".into(),
                        title: "Marked by Kills".into(),
                        applications: 2,
                    }],
                }],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![ControllerEvaluationSummary {
                    agent_type: "neural_agent".into(),
                    row_count: 3,
                    reward_total: 13.5,
                    average_reward_per_row: 4.5,
                }],
                worlds: vec![WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: WorldRealityRole::Shadow,
                    average_reward_per_row: 4.5,
                    controller_mix: vec![ControllerEvaluationSummary {
                        agent_type: "neural_agent".into(),
                        row_count: 3,
                        reward_total: 13.5,
                        average_reward_per_row: 4.5,
                    }],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 6666,
                    applied_score_delta_total: 10,
                    applied_death_mark_count: 2,
                    applied_death_mark_ticks: 1200,
                    applied_objective_shift_count: 2,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        })
        .expect("topology applies");

    assert_eq!(client.remote_world_id(), Some("deadman-shadow"));
    let applied = client
        .remote_applied_world_state()
        .expect("applied world state should resolve");
    assert_eq!(applied.quest_lines[0].quest_graph_id, "deadman-shadow-hunt");
    assert_eq!(applied.quest_lines[0].stage_applications[0].applications, 2);
    assert_eq!(applied.death_marks[0].total_duration_ticks, 1200);

    let evaluation = client
        .remote_world_evaluation()
        .expect("world evaluation should resolve");
    assert_eq!(evaluation.controller_mix[0].agent_type, "neural_agent");
    assert_eq!(evaluation.controller_mix[0].row_count, 3);
    assert_eq!(evaluation.applied_score_delta_total, 10);
    assert_eq!(evaluation.average_quest_progress_basis_points, 6666);
}

#[test]
fn integration_remote_topology_document_surfaces_debug_and_evaluation_state() {
    let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
        db_name: "deadman-shadow".into(),
        connection_mode: StdbConnectionMode::Emulated,
        ..SpacetimeDBClientConfig::default()
    });

    client.connect().expect("connect in emulated mode");
    client
        .subscribe_as_spectator()
        .expect("staging spectator subscriptions");

    let mut shadow =
        WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow-seasonal");
    shadow.role = WorldRealityRole::Shadow;
    shadow.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];

    let document = RemoteTopologyBundle {
        version: pod_core::RuntimeContractVersion::V1,
        scenario_id: "deadman-neural-cup".into(),
        profile_id: "ci-smoke".into(),
        generated_at_unix_ms: 42,
        tournament: WorldTournamentDefinition::new(
            "deadman-neural-cup",
            "Deadman Neural Cup",
        ),
        teams: vec![],
        worlds: vec![shadow],
        links: vec![],
        world_quest_bindings: vec![WorldQuestBinding {
            world_id: "deadman-shadow".into(),
            quest_graph_ids: vec!["deadman-shadow-hunt".into()],
        }],
        quest_graphs: vec![],
        applied_world_states: vec![],
        evaluation: pod_core::ScenarioEvaluationSummary {
            controller_mix: vec![],
            worlds: vec![WorldEvaluationSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
                average_reward_per_row: 4.5,
                controller_mix: vec![ControllerEvaluationSummary {
                    agent_type: "neural_agent".into(),
                    row_count: 3,
                    reward_total: 13.5,
                    average_reward_per_row: 4.5,
                }],
                quest_line_count: 1,
                progressed_quest_line_count: 1,
                average_quest_progress_basis_points: 6666,
                applied_score_delta_total: 0,
                applied_death_mark_count: 0,
                applied_death_mark_ticks: 0,
                applied_objective_shift_count: 0,
                applied_reputation_delta_total: 0,
                applied_encounter_delta_total: 0,
                applied_resource_delta_total: 0,
            }],
        },
    }
    .to_toon_document();

    client
        .apply_remote_topology_document(document.clone())
        .expect("document applies");

    let messages = client.poll_updates();
    assert!(messages.iter().any(|message| matches!(
        message,
        pod_net::ServerMessage::DebugDocument { document: current }
            if current == &document
    )));
    assert_eq!(client.last_debug_document(), Some(document.as_str()));
    assert_eq!(client.remote_world_id(), Some("deadman-shadow"));
    assert_eq!(
        client
            .remote_world_evaluation()
            .and_then(|world| world.controller_mix.first())
            .map(|controller| controller.agent_type.as_str()),
        Some("neural_agent")
    );
}
