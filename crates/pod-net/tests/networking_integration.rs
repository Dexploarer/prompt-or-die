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
use pod_stdb::client::{CachedEntity, StdbConnectionMode};

fn topology_document_for_single_world(
    world_id: &str,
    display_name: &str,
    role: WorldRealityRole,
    quest_graph_id: &str,
    generated_at_unix_ms: u64,
    average_reward_per_row: f32,
) -> String {
    let mut world = WorldRealityDefinition::new(world_id, display_name, "topology-test");
    world.role = role;
    world.active_team_ids = vec!["iron-sigil".into()];

    RemoteTopologyBundle {
        version: pod_core::RuntimeContractVersion::V1,
        scenario_id: "deadman-neural-cup".into(),
        profile_id: "ci-smoke".into(),
        generated_at_unix_ms: generated_at_unix_ms.into(),
        tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
        teams: vec![],
        worlds: vec![world],
        links: vec![],
        world_quest_bindings: vec![WorldQuestBinding {
            world_id: world_id.into(),
            quest_graph_ids: vec![quest_graph_id.into()],
        }],
        world_admissions: vec![],
        world_control_planes: vec![],
        quest_graphs: vec![],
        applied_world_states: vec![AppliedWorldStateSummary {
            world_id: world_id.into(),
            display_name: display_name.into(),
            role,
            team_scores: vec![TeamDeltaSummary {
                team_id: "iron-sigil".into(),
                total_delta: 3,
            }],
            death_marks: vec![],
            faction_reputation_deltas: vec![],
            encounter_weight_deltas: vec![],
            resource_scarcity_deltas: vec![],
            objective_state_shifts: vec![],
            unresolved_objective_state_shifts: vec![],
            quest_lines: vec![QuestLineStateSummary {
                quest_graph_id: quest_graph_id.into(),
                display_name: format!("{display_name}: Active Quest"),
                current_stage_ids: vec!["active-stage".into()],
                completed_stage_ids: vec!["intro".into()],
                pending_stage_ids: vec!["finale".into()],
                next_stage_ids: vec!["finale".into()],
                progress_basis_points: 5000,
                terminal: false,
                stage_applications: vec![QuestStageApplicationSummary {
                    stage_id: "active-stage".into(),
                    title: "Active Stage".into(),
                    applications: 1,
                }],
            }],
        }],
        evaluation: pod_core::ScenarioEvaluationSummary {
            controller_mix: vec![ControllerEvaluationSummary {
                agent_type: "neural_agent".into(),
                row_count: 2,
                reward_total: average_reward_per_row * 2.0,
                average_reward_per_row,
            }],
            worlds: vec![WorldEvaluationSummary {
                world_id: world_id.into(),
                display_name: display_name.into(),
                role,
                average_reward_per_row,
                controller_mix: vec![ControllerEvaluationSummary {
                    agent_type: "neural_agent".into(),
                    row_count: 2,
                    reward_total: average_reward_per_row * 2.0,
                    average_reward_per_row,
                }],
                quest_line_count: 1,
                progressed_quest_line_count: 1,
                average_quest_progress_basis_points: 5000,
                applied_score_delta_total: 3,
                applied_death_mark_count: 0,
                applied_death_mark_ticks: 0,
                applied_objective_shift_count: 0,
                applied_reputation_delta_total: 0,
                applied_encounter_delta_total: 0,
                applied_resource_delta_total: 0,
            }],
        },
    }
    .to_toon_document()
}

fn topology_document_for_single_world_with_state(
    world_id: &str,
    display_name: &str,
    role: WorldRealityRole,
    quest_graph_id: &str,
    current_stage_id: &str,
    generated_at_unix_ms: u64,
    average_reward_per_row: f32,
    applied_score_delta_total: i32,
    applied_objective_shift_count: usize,
) -> String {
    let mut world = WorldRealityDefinition::new(world_id, display_name, "topology-test");
    world.role = role;
    world.active_team_ids = vec!["iron-sigil".into()];

    RemoteTopologyBundle {
        version: pod_core::RuntimeContractVersion::V1,
        scenario_id: "deadman-neural-cup".into(),
        profile_id: "ci-smoke".into(),
        generated_at_unix_ms: generated_at_unix_ms.into(),
        tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
        teams: vec![],
        worlds: vec![world],
        links: vec![],
        world_quest_bindings: vec![WorldQuestBinding {
            world_id: world_id.into(),
            quest_graph_ids: vec![quest_graph_id.into()],
        }],
        world_admissions: vec![],
        world_control_planes: vec![],
        quest_graphs: vec![],
        applied_world_states: vec![AppliedWorldStateSummary {
            world_id: world_id.into(),
            display_name: display_name.into(),
            role,
            team_scores: vec![TeamDeltaSummary {
                team_id: "iron-sigil".into(),
                total_delta: applied_score_delta_total,
            }],
            death_marks: vec![],
            faction_reputation_deltas: vec![],
            encounter_weight_deltas: vec![],
            resource_scarcity_deltas: vec![],
            objective_state_shifts: vec![pod_core::ObjectiveShiftSummary {
                quest_graph_id: quest_graph_id.into(),
                stage_tag: current_stage_id.into(),
                applications: applied_objective_shift_count,
            }],
            unresolved_objective_state_shifts: vec![],
            quest_lines: vec![QuestLineStateSummary {
                quest_graph_id: quest_graph_id.into(),
                display_name: format!("{display_name}: Active Quest"),
                current_stage_ids: vec![current_stage_id.into()],
                completed_stage_ids: vec!["intro".into()],
                pending_stage_ids: vec!["finale".into()],
                next_stage_ids: vec!["finale".into()],
                progress_basis_points: 7500,
                terminal: false,
                stage_applications: vec![QuestStageApplicationSummary {
                    stage_id: current_stage_id.into(),
                    title: format!("{current_stage_id} title"),
                    applications: applied_objective_shift_count,
                }],
            }],
        }],
        evaluation: pod_core::ScenarioEvaluationSummary {
            controller_mix: vec![ControllerEvaluationSummary {
                agent_type: "neural_agent".into(),
                row_count: 2,
                reward_total: average_reward_per_row * 2.0,
                average_reward_per_row,
            }],
            worlds: vec![WorldEvaluationSummary {
                world_id: world_id.into(),
                display_name: display_name.into(),
                role,
                average_reward_per_row,
                controller_mix: vec![ControllerEvaluationSummary {
                    agent_type: "neural_agent".into(),
                    row_count: 2,
                    reward_total: average_reward_per_row * 2.0,
                    average_reward_per_row,
                }],
                quest_line_count: 1,
                progressed_quest_line_count: 1,
                average_quest_progress_basis_points: 7500,
                applied_score_delta_total,
                applied_death_mark_count: 0,
                applied_death_mark_ticks: 0,
                applied_objective_shift_count,
                applied_reputation_delta_total: 0,
                applied_encounter_delta_total: 0,
                applied_resource_delta_total: 0,
            }],
        },
    }
    .to_toon_document()
}

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
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![],
            worlds: vec![shadow],
            links: vec![],
            world_quest_bindings: vec![WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-hunt".into()],
            }],
            world_admissions: vec![],
            world_control_planes: vec![],
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
fn integration_remote_topology_feed_row_surfaces_debug_and_evaluation_state() {
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
        tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
        teams: vec![],
        worlds: vec![shadow],
        links: vec![],
        world_quest_bindings: vec![WorldQuestBinding {
            world_id: "deadman-shadow".into(),
            quest_graph_ids: vec!["deadman-shadow-hunt".into()],
        }],
        world_admissions: vec![],
        world_control_planes: vec![],
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
        .receive_remote_topology_document_row(
            7,
            42,
            "deadman-neural-cup",
            "ci-smoke",
            document.clone(),
        )
        .expect("document row applies");

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

#[test]
fn integration_remote_topology_feed_rows_handle_world_switch_and_stale_churn() {
    let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
        db_name: "deadman-shadow".into(),
        connection_mode: StdbConnectionMode::Emulated,
        ..SpacetimeDBClientConfig::default()
    });

    client.connect().expect("connect in emulated mode");
    client
        .subscribe_as_spectator()
        .expect("staging spectator subscriptions");

    let _ = client.poll_updates();
    let _ = client.poll_updates();

    let mut entity = CachedEntity::from_entity(17, None, true);
    entity.team_id = Some(1);
    entity.name = Some("Topology Scout".into());
    client.inner_mut().upsert_entity(entity);

    let prime_document = topology_document_for_single_world(
        "deadman-prime",
        "Deadman Prime",
        WorldRealityRole::Tournament,
        "deadman-prime-season",
        100,
        1.5,
    );
    client
        .receive_remote_topology_document_row(
            10,
            100,
            "deadman-neural-cup",
            "ci-smoke",
            prime_document.clone(),
        )
        .expect("prime row applies");

    let prime_messages = client.poll_updates();
    let prime_delta = prime_messages
        .iter()
        .find_map(|message| match message {
            pod_net::ServerMessage::StateDelta { delta, .. } => Some(delta),
            _ => None,
        })
        .expect("prime topology update should rebuild snapshot metadata");
    let prime_entity = prime_delta
        .updated
        .iter()
        .find(|entity| entity.id == 17)
        .expect("tracked entity should be updated for prime topology");
    assert_eq!(client.remote_world_id(), Some("deadman-prime"));
    assert_eq!(
        prime_entity.metadata.world_id.as_deref(),
        Some("deadman-prime")
    );
    assert_eq!(
        prime_entity.metadata.world_active_quest_graph_ids,
        vec!["deadman-prime-season".to_string()]
    );

    let shadow_document = topology_document_for_single_world(
        "deadman-shadow",
        "Deadman Shadow",
        WorldRealityRole::Shadow,
        "deadman-shadow-hunt",
        200,
        4.5,
    );
    client
        .receive_remote_topology_document_row(
            11,
            200,
            "deadman-neural-cup",
            "ci-smoke",
            shadow_document.clone(),
        )
        .expect("shadow row applies");

    let shadow_messages = client.poll_updates();
    let shadow_delta = shadow_messages
        .iter()
        .find_map(|message| match message {
            pod_net::ServerMessage::StateDelta { delta, .. } => Some(delta),
            _ => None,
        })
        .expect("shadow topology update should rebuild snapshot metadata");
    let shadow_entity = shadow_delta
        .updated
        .iter()
        .find(|entity| entity.id == 17)
        .expect("tracked entity should be updated for shadow topology");
    assert_eq!(client.remote_world_id(), Some("deadman-shadow"));
    assert_eq!(
        shadow_entity.metadata.world_id.as_deref(),
        Some("deadman-shadow")
    );
    assert_eq!(
        shadow_entity.metadata.world_role,
        Some(WorldRealityRole::Shadow)
    );
    assert_eq!(
        shadow_entity.metadata.world_active_quest_graph_ids,
        vec!["deadman-shadow-hunt".to_string()]
    );
    assert_eq!(
        client
            .remote_world_evaluation()
            .map(|evaluation| evaluation.average_reward_per_row),
        Some(4.5)
    );

    client
        .receive_remote_topology_document_row(
            9,
            150,
            "deadman-neural-cup",
            "ci-smoke",
            prime_document,
        )
        .expect("stale row should be ignored");
    let stale_messages = client.poll_updates();
    assert!(stale_messages.iter().all(|message| !matches!(
        message,
        pod_net::ServerMessage::DebugDocument { document } if document.contains("deadman-prime")
    )));
    assert_eq!(client.remote_world_id(), Some("deadman-shadow"));
    assert_eq!(
        client
            .remote_world_evaluation()
            .map(|evaluation| evaluation.average_reward_per_row),
        Some(4.5)
    );
}

#[test]
fn integration_remote_topology_feed_rows_update_quest_and_effect_state_within_same_world() {
    let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
        db_name: "deadman-shadow".into(),
        connection_mode: StdbConnectionMode::Emulated,
        ..SpacetimeDBClientConfig::default()
    });

    client.connect().expect("connect in emulated mode");
    client
        .subscribe_as_spectator()
        .expect("staging spectator subscriptions");

    let _ = client.poll_updates();
    let _ = client.poll_updates();

    let mut entity = CachedEntity::from_entity(21, None, true);
    entity.team_id = Some(1);
    entity.name = Some("Quest Relay".into());
    client.inner_mut().upsert_entity(entity);

    let initial_document = topology_document_for_single_world_with_state(
        "deadman-shadow",
        "Deadman Shadow",
        WorldRealityRole::Shadow,
        "deadman-shadow-hunt",
        "marked-by-kills",
        200,
        4.5,
        3,
        1,
    );
    client
        .receive_remote_topology_document_row(
            12,
            200,
            "deadman-neural-cup",
            "ci-smoke",
            initial_document.clone(),
        )
        .expect("initial row applies");
    let _ = client.poll_updates();

    let updated_document = topology_document_for_single_world_with_state(
        "deadman-shadow",
        "Deadman Shadow",
        WorldRealityRole::Shadow,
        "deadman-shadow-collapse",
        "rift-collapse",
        260,
        6.25,
        9,
        4,
    );
    client
        .receive_remote_topology_document_row(
            13,
            260,
            "deadman-neural-cup",
            "ci-smoke",
            updated_document,
        )
        .expect("updated row applies");

    let updated_messages = client.poll_updates();
    let updated_delta = updated_messages
        .iter()
        .find_map(|message| match message {
            pod_net::ServerMessage::StateDelta { delta, .. } => Some(delta),
            _ => None,
        })
        .expect("updated topology row should rebuild snapshot metadata");
    let updated_entity = updated_delta
        .updated
        .iter()
        .find(|entity| entity.id == 21)
        .expect("tracked entity should be updated for same-world quest churn");
    assert_eq!(client.remote_world_id(), Some("deadman-shadow"));
    assert_eq!(
        updated_entity.metadata.world_active_quest_graph_ids,
        vec!["deadman-shadow-collapse".to_string()]
    );

    let applied = client
        .remote_applied_world_state()
        .expect("updated applied state should resolve");
    assert_eq!(applied.team_scores[0].total_delta, 9);
    assert_eq!(
        applied.quest_lines[0].quest_graph_id,
        "deadman-shadow-collapse"
    );
    assert_eq!(
        applied.quest_lines[0].current_stage_ids,
        vec!["rift-collapse".to_string()]
    );
    assert_eq!(applied.quest_lines[0].stage_applications[0].applications, 4);

    let evaluation = client
        .remote_world_evaluation()
        .expect("updated world evaluation should resolve");
    assert_eq!(evaluation.average_reward_per_row, 6.25);
    assert_eq!(evaluation.applied_score_delta_total, 9);
    assert_eq!(evaluation.applied_objective_shift_count, 4);

    client
        .receive_remote_topology_document_row(
            11,
            240,
            "deadman-neural-cup",
            "ci-smoke",
            initial_document,
        )
        .expect("stale row should be ignored");
    let stale_messages = client.poll_updates();
    assert!(stale_messages.iter().all(|message| !matches!(
        message,
        pod_net::ServerMessage::DebugDocument { document }
            if document.contains("deadman-shadow-hunt")
    )));
    assert_eq!(
        client
            .remote_applied_world_state()
            .and_then(|state| state.quest_lines.first())
            .map(|quest| quest.quest_graph_id.as_str()),
        Some("deadman-shadow-collapse")
    );
    assert_eq!(
        client
            .remote_world_evaluation()
            .map(|world| world.average_reward_per_row),
        Some(6.25)
    );
}
