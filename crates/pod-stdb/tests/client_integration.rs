//! Integration tests for the pod-stdb client wrapper.
//!
//! These tests exercise the StdbClient PUBLIC API only. Cache mutation methods
//! (upsert_entity, update_world_state, etc.) are `pub` and tested by
//! the 9 unit tests inside `client.rs`. Integration tests here validate:
//!
//! - Configuration and construction
//! - Connection lifecycle (connect / disconnect)
//! - Reducer calls fail when disconnected
//! - CachedEntity / CachedWorldState struct construction and helpers
//! - SubmittedAction builders
//! - Subscription queries
//! - StdbError Display and std::error::Error impl
//! - StdbEvent variant constructibility
//! - StdbClient Debug impl
//! - Frame tick metrics
//!
//! Run with:
//! ```bash
//! cargo test -p pod-stdb --no-default-features --features client --test client_integration
//! ```

#![cfg(feature = "client")]

use pod_core::{AgentToolCallTrace, TickTelemetryFrame, VersionedTickTelemetry};
use pod_stdb::client::*;
use pod_stdb::types::*;

// ============================================================
// CONFIGURATION
// ============================================================

#[test]
fn default_config_has_expected_values() {
    let config = StdbClientConfig::default();
    assert_eq!(config.host, "http://localhost:3000");
    assert_eq!(config.db_name, "prompt-or-die");
    assert!(config.auth_token.is_none());
    assert_eq!(config.player_name, "Player");
    #[cfg(debug_assertions)]
    assert!(matches!(
        config.connection_mode,
        StdbConnectionMode::Emulated
    ));
    #[cfg(not(debug_assertions))]
    assert!(matches!(
        config.connection_mode,
        StdbConnectionMode::Generated
    ));
}

#[test]
fn custom_config_fields() {
    let config = StdbClientConfig {
        host: "http://my-server:5000".into(),
        db_name: "my-game".into(),
        auth_token: Some("secret-token-123".into()),
        player_name: "Hero".into(),
        connection_mode: StdbConnectionMode::Emulated,
    };
    assert_eq!(config.host, "http://my-server:5000");
    assert_eq!(config.db_name, "my-game");
    assert_eq!(config.auth_token.as_deref(), Some("secret-token-123"));
    assert_eq!(config.player_name, "Hero");
}

#[test]
fn config_clone() {
    let config = StdbClientConfig::default();
    let cloned = config.clone();
    assert_eq!(config.host, cloned.host);
    assert_eq!(config.db_name, cloned.db_name);
}

#[test]
fn config_debug_format() {
    let config = StdbClientConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("StdbClientConfig"));
    assert!(debug.contains("localhost"));
}

// ============================================================
// CLIENT CONSTRUCTION
// ============================================================

#[test]
fn new_client_starts_disconnected() {
    let client = StdbClient::new(StdbClientConfig::default());
    assert!(!client.is_connected());
    assert!(matches!(
        client.connection_state(),
        ConnectionState::Disconnected
    ));
}

#[test]
fn new_client_has_empty_caches() {
    let client = StdbClient::new(StdbClientConfig::default());
    assert!(client.world_state().is_none());
    assert_eq!(client.entity_count(), 0);
    assert!(client.entities().is_empty());
    assert!(client.current_tick().is_none());
    assert!(client.is_paused().is_none());
    assert!(client.controlled_entity().is_none());
    assert!(client.latest_observation(1).is_none());
}

#[test]
fn new_client_has_zero_metrics() {
    let client = StdbClient::new(StdbClientConfig::default());
    assert_eq!(client.frames_processed(), 0);
    assert_eq!(client.reducers_called(), 0);
    assert_eq!(client.events_received(), 0);
}

#[test]
fn new_client_has_no_events() {
    let client = StdbClient::new(StdbClientConfig::default());
    assert!(!client.has_events());
    assert!(client.peek_events().is_empty());
}

#[test]
fn telemetry_subscription_helpers_include_debug_queries() {
    let queries = Subscriptions::editor_with_debug_telemetry();
    assert!(queries
        .iter()
        .any(|query| query.contains("agent_telemetry_tick")));
    assert!(queries
        .iter()
        .any(|query| query.contains("agent_tool_call_event")));
    assert!(queries
        .iter()
        .any(|query| query.contains("agent_tick_rollup")));
}

#[test]
fn telemetry_events_are_constructible() {
    let tick = StdbEvent::AgentTelemetryTickReceived {
        tick: 12,
        agent_entity_id: 7,
        frame_json: VersionedTickTelemetry::new(TickTelemetryFrame::empty(12)).to_toon_document(),
    };
    let tool = StdbEvent::AgentToolCallEventReceived {
        tick: 12,
        agent_entity_id: 7,
        tool_name: "llm.complete".into(),
        provider: "qwen".into(),
        status: "Succeeded".into(),
        document: AgentToolCallTrace::success(12, "llm.complete", "qwen", 18, 128, 64)
            .to_toon_document(),
    };
    let rollup = StdbEvent::AgentTickRollupReceived {
        tick_start: 1,
        tick_end: 60,
        agent_entity_id: 7,
        document: "{\"distance\":84.0}".into(),
    };

    assert!(matches!(tick, StdbEvent::AgentTelemetryTickReceived { .. }));
    assert!(matches!(tool, StdbEvent::AgentToolCallEventReceived { .. }));
    assert!(matches!(rollup, StdbEvent::AgentTickRollupReceived { .. }));
}

#[test]
fn config_accessor_returns_original_config() {
    let config = StdbClientConfig {
        host: "http://test:1234".into(),
        db_name: "test-db".into(),
        auth_token: Some("tok".into()),
        player_name: "Tester".into(),
        connection_mode: StdbConnectionMode::Emulated,
    };
    let client = StdbClient::new(config);
    assert_eq!(client.config().host, "http://test:1234");
    assert_eq!(client.config().db_name, "test-db");
    assert_eq!(client.config().auth_token.as_deref(), Some("tok"));
    assert_eq!(client.config().player_name, "Tester");
}

// ============================================================
// CONNECTION LIFECYCLE
// ============================================================

#[test]
fn connect_transitions_to_connecting() {
    let mut client = StdbClient::new(StdbClientConfig {
        connection_mode: StdbConnectionMode::Emulated,
        ..StdbClientConfig::default()
    });
    assert!(client.connect().is_ok());
    assert!(matches!(
        client.connection_state(),
        ConnectionState::Connecting
    ));
    assert!(!client.is_connected());
}

#[test]
fn connect_while_connecting_returns_error() {
    let mut client = StdbClient::new(StdbClientConfig {
        connection_mode: StdbConnectionMode::Emulated,
        ..StdbClientConfig::default()
    });
    client.connect().unwrap();
    let result = client.connect();
    assert!(result.is_err());
}

#[test]
fn connect_generated_mode_requires_runtime_bindings() {
    let mut client = StdbClient::new(StdbClientConfig {
        connection_mode: StdbConnectionMode::Generated,
        ..StdbClientConfig::default()
    });
    let err = client
        .connect()
        .expect_err("generated mode should require runtime");
    assert!(matches!(err, StdbError::ConnectionFailed(_)));
}

#[test]
fn generated_mode_runtime_adapter_processes_topology_rows() {
    let mut client = StdbClient::new(StdbClientConfig {
        db_name: "deadman-shadow".into(),
        connection_mode: StdbConnectionMode::Generated,
        ..StdbClientConfig::default()
    });
    let endpoint = client.install_generated_binding_runtime();
    let callbacks = endpoint.callbacks();

    client.connect().expect("generated runtime should connect");
    let commands = endpoint.drain_commands();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        GeneratedBindingCommand::Connect { config } => {
            assert_eq!(config.db_name, "deadman-shadow");
            assert!(matches!(
                config.connection_mode,
                StdbConnectionMode::Generated
            ));
        }
        other => panic!("expected connect command, got {other:?}"),
    }
    callbacks.connected(vec![1, 2, 3, 4], "tok-generated");
    client.frame_tick();
    assert!(client.is_connected());

    client
        .subscribe(vec!["SELECT * FROM remote_topology_document".into()])
        .expect("subscription should be routed to runtime");
    let commands = endpoint.drain_commands();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        GeneratedBindingCommand::Subscribe { queries } => {
            assert_eq!(
                queries,
                &vec!["SELECT * FROM remote_topology_document".to_string()]
            );
        }
        other => panic!("expected subscribe command, got {other:?}"),
    }
    callbacks.subscription_applied();

    let topology = pod_core::RemoteTopologyBundle {
        version: pod_core::RuntimeContractVersion::V1,
        scenario_id: "deadman-neural-cup".into(),
        profile_id: "generated-test".into(),
        generated_at_unix_ms: 99,
        tournament: pod_core::WorldTournamentDefinition::new(
            "deadman-neural-cup",
            "Deadman Neural Cup",
        ),
        teams: vec![pod_core::AgentTeamDefinition::new(
            "gloam-mesh",
            "Gloam Mesh",
            "deadman-shadow",
        )],
        worlds: vec![{
            let mut world =
                pod_core::WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow");
            world.role = pod_core::WorldRealityRole::Shadow;
            world.active_team_ids = vec!["gloam-mesh".into()];
            world
        }],
        links: vec![],
        world_quest_bindings: vec![pod_core::WorldQuestBinding {
            world_id: "deadman-shadow".into(),
            quest_graph_ids: vec!["deadman-shadow-hunt".into()],
        }],
        world_admissions: vec![],
        world_control_planes: vec![],
        tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
        tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
        quest_graphs: vec![],
        applied_world_states: vec![],
        evaluation: pod_core::ScenarioEvaluationSummary {
            controller_mix: vec![],
            worlds: vec![],
        },
    };
    let document = topology.to_toon_document();

    callbacks.remote_topology_document_insert(
        GeneratedRemoteTopologyDocumentRow::from_topology_bundle(11, &topology)
            .expect("callback row should build"),
    );
    client.frame_tick();
    assert_eq!(client.resolved_remote_world_id(), Some("deadman-shadow"));

    let events = client.drain_events().collect::<Vec<_>>();
    assert!(events
        .iter()
        .any(|event| matches!(event, StdbEvent::SubscriptionApplied)));
    assert!(events.iter().any(|event| matches!(
        event,
        StdbEvent::RemoteTopologyDocumentReceived { document: current }
            if current == &document
    )));
}

#[test]
fn disconnect_from_disconnected_is_noop() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    // Should not panic
    client.disconnect();
    assert!(matches!(
        client.connection_state(),
        ConnectionState::Disconnected
    ));
    // No events emitted since we weren't connected
    assert!(!client.has_events());
}

// ============================================================
// REDUCER CALLS REQUIRE CONNECTION
// ============================================================

#[test]
fn create_world_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    let result = client.call_create_world(42, 1000.0, 1000.0, 60);
    assert!(result.is_err());
    match result.unwrap_err() {
        StdbError::NotConnected => {} // expected
        other => panic!("Expected NotConnected, got: {other}"),
    }
}

#[test]
fn set_paused_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_set_paused(true).is_err());
}

#[test]
fn spawn_entity_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_spawn_entity(0.0, 0.0, None).is_err());
    assert!(client
        .call_spawn_entity(100.0, 200.0, Some(AgentType::Human))
        .is_err());
}

#[test]
fn connect_agent_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client
        .call_connect_agent(1, AgentType::Human, "Player1".into())
        .is_err());
}

#[test]
fn connect_llm_agent_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_connect_llm_agent(1, "Bot".into()).is_err());
}

#[test]
fn submit_action_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    let action = SubmittedAction::idle(1);
    assert!(client.call_submit_action(&action).is_err());
}

#[test]
fn execute_tick_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_execute_tick().is_err());
}

#[test]
fn destroy_entity_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_destroy_entity(1).is_err());
}

#[test]
fn create_lobby_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client
        .call_create_lobby("arena".into(), 1, 4, false)
        .is_err());
}

#[test]
fn join_lobby_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_join_lobby(1, 1).is_err());
}

#[test]
fn leave_lobby_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_leave_lobby().is_err());
}

#[test]
fn set_lobby_ready_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_set_lobby_ready(1, true).is_err());
}

#[test]
fn start_lobby_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_start_lobby(1).is_err());
}

#[test]
fn join_match_queue_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_join_match_queue(1, 2).is_err());
}

#[test]
fn leave_match_queue_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_leave_match_queue().is_err());
}

#[test]
fn create_match_from_queue_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert!(client.call_create_match_from_queue(2).is_err());
}

#[test]
fn subscribe_fails_when_disconnected() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    let queries: Vec<String> = Subscriptions::all_tables()
        .into_iter()
        .map(|s| s.to_string())
        .collect();
    assert!(client.subscribe(queries).is_err());
}

#[test]
fn all_reducers_fail_when_only_connecting() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    client.connect().unwrap(); // now Connecting, not Connected

    assert!(client.call_create_world(42, 1000.0, 1000.0, 60).is_err());
    assert!(client.call_set_paused(true).is_err());
    assert!(client.call_spawn_entity(0.0, 0.0, None).is_err());
    assert!(client
        .call_connect_agent(1, AgentType::LlmAgent, "Bot".into())
        .is_err());
    assert!(client
        .call_submit_action(&SubmittedAction::idle(1))
        .is_err());
    assert!(client
        .call_create_lobby("arena".into(), 1, 4, false)
        .is_err());
    assert!(client.call_join_lobby(1, 1).is_err());
    assert!(client.call_leave_lobby().is_err());
    assert!(client.call_set_lobby_ready(1, true).is_err());
    assert!(client.call_start_lobby(1).is_err());
    assert!(client.call_join_match_queue(1, 2).is_err());
    assert!(client.call_leave_match_queue().is_err());
    assert!(client.call_create_match_from_queue(2).is_err());
    assert!(client.call_execute_tick().is_err());
    assert!(client.call_destroy_entity(1).is_err());
}

// ============================================================
// CACHED ENTITY — CONSTRUCTION & HELPERS
// ============================================================

#[test]
fn cached_entity_from_entity_minimal() {
    let entity = CachedEntity::from_entity(42, Some(AgentType::Human), true);
    assert_eq!(entity.entity_id, 42);
    assert_eq!(entity.agent_type, Some(AgentType::Human));
    assert!(entity.alive);
    // All optional fields are None
    assert!(entity.pos_x.is_none());
    assert!(entity.pos_y.is_none());
    assert!(entity.rotation.is_none());
    assert!(entity.vel_x.is_none());
    assert!(entity.vel_y.is_none());
    assert!(entity.health.is_none());
    assert!(entity.max_health.is_none());
    assert!(entity.armor.is_none());
    assert!(entity.invulnerable.is_none());
    assert!(entity.name.is_none());
    assert!(entity.team_id.is_none());
    assert!(entity.vision_range.is_none());
    assert!(entity.vision_fov.is_none());
    assert!(entity.hearing_range.is_none());
    assert!(entity.max_speed.is_none());
}

#[test]
fn cached_entity_from_entity_no_agent_type() {
    let entity = CachedEntity::from_entity(1, None, false);
    assert_eq!(entity.entity_id, 1);
    assert!(entity.agent_type.is_none());
    assert!(!entity.alive);
}

#[test]
fn cached_entity_position_both_set() {
    let mut entity = CachedEntity::from_entity(1, None, true);
    entity.pos_x = Some(100.0);
    entity.pos_y = Some(200.0);
    assert_eq!(entity.position(), Some((100.0, 200.0)));
}

#[test]
fn cached_entity_position_partial_returns_none() {
    let mut entity = CachedEntity::from_entity(1, None, true);

    // Only x set
    entity.pos_x = Some(100.0);
    assert_eq!(entity.position(), None);

    // Only y set
    entity.pos_x = None;
    entity.pos_y = Some(200.0);
    assert_eq!(entity.position(), None);

    // Neither set
    entity.pos_y = None;
    assert_eq!(entity.position(), None);
}

#[test]
fn cached_entity_health_fraction_normal() {
    let mut entity = CachedEntity::from_entity(1, None, true);
    entity.health = Some(75.0);
    entity.max_health = Some(100.0);
    let frac = entity.health_fraction().unwrap();
    assert!((frac - 0.75).abs() < 0.001);
}

#[test]
fn cached_entity_health_fraction_full() {
    let mut entity = CachedEntity::from_entity(1, None, true);
    entity.health = Some(100.0);
    entity.max_health = Some(100.0);
    let frac = entity.health_fraction().unwrap();
    assert!((frac - 1.0).abs() < 0.001);
}

#[test]
fn cached_entity_health_fraction_dead() {
    let mut entity = CachedEntity::from_entity(1, None, true);
    entity.health = Some(0.0);
    entity.max_health = Some(100.0);
    let frac = entity.health_fraction().unwrap();
    assert!((frac - 0.0).abs() < 0.001);
}

#[test]
fn cached_entity_health_fraction_missing_health() {
    let mut entity = CachedEntity::from_entity(1, None, true);
    entity.max_health = Some(100.0);
    assert_eq!(entity.health_fraction(), None);
}

#[test]
fn cached_entity_health_fraction_missing_max() {
    let mut entity = CachedEntity::from_entity(1, None, true);
    entity.health = Some(50.0);
    assert_eq!(entity.health_fraction(), None);
}

#[test]
fn cached_entity_health_fraction_zero_max_returns_none() {
    let mut entity = CachedEntity::from_entity(1, None, true);
    entity.health = Some(50.0);
    entity.max_health = Some(0.0);
    // m > 0.0 guard prevents division by zero → returns None
    assert_eq!(entity.health_fraction(), None);
}

#[test]
fn cached_entity_health_fraction_both_none() {
    let entity = CachedEntity::from_entity(1, None, true);
    assert_eq!(entity.health_fraction(), None);
}

#[test]
fn cached_entity_clone() {
    let mut entity = CachedEntity::from_entity(1, Some(AgentType::LlmAgent), true);
    entity.pos_x = Some(10.0);
    entity.pos_y = Some(20.0);
    entity.name = Some("TestBot".into());

    let cloned = entity.clone();
    assert_eq!(cloned.entity_id, 1);
    assert_eq!(cloned.agent_type, Some(AgentType::LlmAgent));
    assert_eq!(cloned.position(), Some((10.0, 20.0)));
    assert_eq!(cloned.name.as_deref(), Some("TestBot"));
}

#[test]
fn cached_entity_debug_format() {
    let entity = CachedEntity::from_entity(7, Some(AgentType::NeuralAgent), true);
    let debug = format!("{:?}", entity);
    assert!(debug.contains("CachedEntity"));
    assert!(debug.contains("NeuralAgent"));
}

// ============================================================
// CACHED WORLD STATE — CONSTRUCTION
// ============================================================

#[test]
fn cached_world_state_construction() {
    let ws = CachedWorldState {
        tick: 100,
        rng_seed: 42,
        ticks_per_second: 60,
        world_width: 2000.0,
        world_height: 1500.0,
        max_entities: 10000,
        paused: false,
    };
    assert_eq!(ws.tick, 100);
    assert_eq!(ws.rng_seed, 42);
    assert_eq!(ws.ticks_per_second, 60);
    assert!((ws.world_width - 2000.0).abs() < f32::EPSILON);
    assert!((ws.world_height - 1500.0).abs() < f32::EPSILON);
    assert_eq!(ws.max_entities, 10000);
    assert!(!ws.paused);
}

#[test]
fn cached_world_state_clone() {
    let ws = CachedWorldState {
        tick: 0,
        rng_seed: 1,
        ticks_per_second: 30,
        world_width: 500.0,
        world_height: 500.0,
        max_entities: 100,
        paused: true,
    };
    let cloned = ws.clone();
    assert_eq!(cloned.tick, 0);
    assert_eq!(cloned.rng_seed, 1);
    assert!(cloned.paused);
}

#[test]
fn cached_world_state_debug_format() {
    let ws = CachedWorldState {
        tick: 42,
        rng_seed: 7,
        ticks_per_second: 60,
        world_width: 1000.0,
        world_height: 1000.0,
        max_entities: 500,
        paused: false,
    };
    let debug = format!("{:?}", ws);
    assert!(debug.contains("CachedWorldState"));
    assert!(debug.contains("42"));
}

// ============================================================
// SUBMITTED ACTION BUILDERS
// ============================================================

#[test]
fn action_builder_move_dir() {
    let action = SubmittedAction::move_dir(1, 0.707, 0.707);
    assert_eq!(action.entity_id, 1);
    assert_eq!(action.action_kind, ActionKind::Move);
    assert_eq!(action.direction_x, Some(0.707));
    assert_eq!(action.direction_y, Some(0.707));
    // Other fields should be None
    assert!(action.angle.is_none());
    assert!(action.target_x.is_none());
    assert!(action.target_y.is_none());
    assert!(action.target_entity_id.is_none());
    assert!(action.message.is_none());
    assert!(action.volume.is_none());
    assert!(action.prefab.is_none());
}

#[test]
fn action_builder_stop() {
    let action = SubmittedAction::stop(5);
    assert_eq!(action.entity_id, 5);
    assert_eq!(action.action_kind, ActionKind::Stop);
    assert!(action.direction_x.is_none());
    assert!(action.direction_y.is_none());
}

#[test]
fn action_builder_rotate() {
    let action = SubmittedAction::rotate(2, 1.5708);
    assert_eq!(action.entity_id, 2);
    assert_eq!(action.action_kind, ActionKind::Rotate);
    assert!((action.angle.unwrap() - 1.5708).abs() < 0.001);
    assert!(action.direction_x.is_none());
    assert!(action.direction_y.is_none());
}

#[test]
fn action_builder_speak() {
    let action = SubmittedAction::speak(3, "Hello, world!".into(), SpeakVolume::Shout);
    assert_eq!(action.entity_id, 3);
    assert_eq!(action.action_kind, ActionKind::Speak);
    assert_eq!(action.message.as_deref(), Some("Hello, world!"));
    assert_eq!(action.volume, Some(SpeakVolume::Shout));
    assert!(action.direction_x.is_none());
    assert!(action.target_entity_id.is_none());
}

#[test]
fn action_builder_attack() {
    let action = SubmittedAction::attack(1, 1.0, 0.0);
    assert_eq!(action.entity_id, 1);
    assert_eq!(action.action_kind, ActionKind::Attack);
    assert_eq!(action.direction_x, Some(1.0));
    assert_eq!(action.direction_y, Some(0.0));
    assert!(action.target_entity_id.is_none());
}

#[test]
fn action_builder_attack_target() {
    let action = SubmittedAction::attack_target(1, 2);
    assert_eq!(action.entity_id, 1);
    assert_eq!(action.action_kind, ActionKind::AttackTarget);
    assert_eq!(action.target_entity_id, Some(2));
    assert!(action.direction_x.is_none());
}

#[test]
fn action_builder_idle() {
    let action = SubmittedAction::idle(7);
    assert_eq!(action.entity_id, 7);
    assert_eq!(action.action_kind, ActionKind::Idle);
    assert!(action.direction_x.is_none());
    assert!(action.message.is_none());
}

#[test]
fn action_clone_and_debug() {
    let action = SubmittedAction::speak(1, "test".into(), SpeakVolume::Normal);
    let cloned = action.clone();
    assert_eq!(cloned.entity_id, 1);
    assert_eq!(cloned.message.as_deref(), Some("test"));

    let debug = format!("{:?}", action);
    assert!(debug.contains("SubmittedAction"));
    assert!(debug.contains("Speak"));
}

// ============================================================
// SUBSCRIPTION QUERIES
// ============================================================

#[test]
fn subscription_all_tables_has_29_queries() {
    let queries = Subscriptions::all_tables();
    assert_eq!(queries.len(), 29);
}

#[test]
fn subscription_lobby_system_has_lobby_tables() {
    let queries = Subscriptions::lobby_system();
    assert_eq!(queries.len(), 2);
    let all_text = queries.join(" ");
    assert!(all_text.contains("lobby"));
    assert!(all_text.contains("lobby_member"));
}

#[test]
fn subscription_all_tables_are_select_statements() {
    let queries = Subscriptions::all_tables();
    for query in &queries {
        assert!(
            query.starts_with("SELECT * FROM "),
            "Query should start with 'SELECT * FROM ': {query}"
        );
    }
}

#[test]
fn subscription_all_tables_covers_key_tables() {
    let queries = Subscriptions::all_tables();
    let all_text = queries.join(" ");

    let expected_tables = [
        "entity",
        "transform",
        "velocity",
        "health",
        "label",
        "perception",
        "movement",
        "agent_constraints",
        "color_rect",
        "sprite",
        "collider",
        "rigid_body",
        "script",
        "world_state",
        "connected_agent",
        "action_submission",
        "observation_event",
        "combat_event",
        "speech_event",
        "world_event",
        "remote_topology_document",
        "match_queue",
        "game_match",
        "match_participant",
        "lobby",
        "lobby_member",
    ];

    for table in &expected_tables {
        assert!(
            all_text.contains(table),
            "Missing table in all_tables(): {table}"
        );
    }
}

#[test]
fn subscription_player_agent_includes_observation_filter() {
    let queries = Subscriptions::player_agent(42);
    let all_text = queries.join(" ");
    assert!(all_text.contains("observation_event"));
    assert!(all_text.contains("42"));
}

#[test]
fn subscription_player_agent_includes_core_tables() {
    let queries = Subscriptions::player_agent(1);
    let all_text = queries.join(" ");
    assert!(all_text.contains("world_state"));
    assert!(all_text.contains("entity"));
    assert!(all_text.contains("transform"));
    assert!(all_text.contains("health"));
    assert!(all_text.contains("remote_topology_document"));
}

#[test]
fn subscription_spectator_includes_events() {
    let queries = Subscriptions::spectator();
    let all_text = queries.join(" ");
    assert!(all_text.contains("combat_event"));
    assert!(all_text.contains("speech_event"));
    assert!(all_text.contains("world_event"));
    assert!(all_text.contains("world_state"));
    assert!(all_text.contains("entity"));
    assert!(all_text.contains("remote_topology_document"));
}

#[test]
fn subscription_editor_includes_config_tables() {
    let queries = Subscriptions::editor();
    let all_text = queries.join(" ");
    assert!(all_text.contains("agent_constraints"));
    assert!(all_text.contains("perception"));
    assert!(all_text.contains("movement"));
    assert!(all_text.contains("script"));
    assert!(all_text.contains("collider"));
    assert!(all_text.contains("remote_topology_document"));
}

#[test]
fn subscription_editor_has_no_events() {
    let queries = Subscriptions::editor();
    let all_text = queries.join(" ");
    // Editor subscribes to state tables, not event tables
    assert!(!all_text.contains("combat_event"));
    assert!(!all_text.contains("speech_event"));
    assert!(!all_text.contains("world_event"));
    assert!(!all_text.contains("observation_event"));
}

// ============================================================
// STDB ERROR
// ============================================================

#[test]
fn stdb_error_display_not_connected() {
    let err = StdbError::NotConnected;
    let msg = format!("{err}");
    assert!(msg.contains("Not connected"));
}

#[test]
fn stdb_error_display_connection_failed() {
    let err = StdbError::ConnectionFailed("timeout".into());
    let msg = format!("{err}");
    assert!(msg.contains("Connection failed"));
    assert!(msg.contains("timeout"));
}

#[test]
fn stdb_error_display_reducer_error() {
    let err = StdbError::ReducerError("invalid args".into());
    let msg = format!("{err}");
    assert!(msg.contains("Reducer error"));
    assert!(msg.contains("invalid args"));
}

#[test]
fn stdb_error_display_subscription_error() {
    let err = StdbError::SubscriptionError("bad SQL".into());
    let msg = format!("{err}");
    assert!(msg.contains("Subscription error"));
    assert!(msg.contains("bad SQL"));
}

#[test]
fn stdb_error_display_document_error() {
    let err = StdbError::DocumentError("bad toon".into());
    let msg = format!("{err}");
    assert!(msg.contains("Document error"));
    assert!(msg.contains("bad toon"));
}

#[test]
fn stdb_error_display_invalid_state() {
    let err = StdbError::InvalidState("already connected".into());
    let msg = format!("{err}");
    assert!(msg.contains("Invalid state"));
    assert!(msg.contains("already connected"));
}

#[test]
fn stdb_error_is_std_error() {
    let err: Box<dyn std::error::Error> = Box::new(StdbError::NotConnected);
    // Should compile — StdbError implements std::error::Error
    let _ = format!("{err}");
}

#[test]
fn stdb_error_clone_and_debug() {
    let err = StdbError::ReducerError("test".into());
    let cloned = err.clone();
    let debug = format!("{:?}", cloned);
    assert!(debug.contains("ReducerError"));
    assert!(debug.contains("test"));
}

// ============================================================
// STDB EVENT — ALL VARIANTS CONSTRUCTIBLE
// ============================================================

#[test]
fn stdb_event_all_21_variants_constructible() {
    // Verify all StdbEvent variants can be created without panicking.
    // This ensures the public API surface matches our expectations.
    let events: Vec<StdbEvent> = vec![
        // Connection lifecycle
        StdbEvent::Connected {
            identity: vec![1, 2, 3],
            token: "test-token".into(),
        },
        StdbEvent::Disconnected {
            reason: "test disconnect".into(),
        },
        StdbEvent::ConnectError {
            message: "connection refused".into(),
        },
        StdbEvent::SubscriptionApplied,
        // World state
        StdbEvent::WorldStateUpdated {
            tick: 1,
            paused: false,
            world_width: 1000.0,
            world_height: 1000.0,
        },
        StdbEvent::TickAdvanced {
            old_tick: 0,
            new_tick: 1,
        },
        // Entity lifecycle
        StdbEvent::EntityInserted { entity_id: 1 },
        StdbEvent::EntityUpdated { entity_id: 1 },
        StdbEvent::EntityDeleted { entity_id: 1 },
        // Agent-specific
        StdbEvent::ObservationReceived {
            tick: 1,
            observer_entity_id: 42,
            observation_json: "{\"entities\":[]}".into(),
        },
        StdbEvent::CombatEventReceived {
            tick: 1,
            attacker_id: 1,
            defender_id: 2,
            damage: 25.0,
            killed: false,
        },
        StdbEvent::SpeechEventReceived {
            tick: 1,
            speaker_id: 3,
            message: "Hello!".into(),
            volume: SpeakVolume::Normal,
        },
        StdbEvent::WorldEventReceived {
            tick: 1,
            event_kind: WorldEventKind::EntitySpawned,
            entity_id: 5,
            secondary_entity_id: None,
            data_json: "{}".into(),
        },
        StdbEvent::AgentTelemetryTickReceived {
            tick: 1,
            agent_entity_id: 5,
            frame_json: VersionedTickTelemetry::new(TickTelemetryFrame::empty(1))
                .to_toon_document(),
        },
        StdbEvent::AgentToolCallEventReceived {
            tick: 1,
            agent_entity_id: 5,
            tool_name: "llm.complete".into(),
            provider: "qwen".into(),
            status: "Succeeded".into(),
            document: AgentToolCallTrace::success(1, "llm.complete", "qwen", 12, 42, 10)
                .to_toon_document(),
        },
        StdbEvent::AgentTickRollupReceived {
            tick_start: 1,
            tick_end: 60,
            agent_entity_id: 5,
            document: "{\"distance\":128.0}".into(),
        },
        StdbEvent::FocusedEntityDebugSummaryReceived {
            agent_entity_id: 5,
            document: "{\"focus\":\"hero\"}".into(),
        },
        StdbEvent::RemoteTopologyDocumentReceived {
            document: "{\"document_type\":\"remote_topology_bundle\"}".into(),
        },
        StdbEvent::RemoteTopologyUpdated {
            scenario_id: "deadman-neural-cup".into(),
            resolved_world_id: Some("deadman-prime".into()),
            world_count: 2,
            team_count: 4,
        },
        // Reducer acknowledgments
        StdbEvent::ReducerCallSuccess {
            reducer_name: "create_world".into(),
        },
        StdbEvent::ReducerCallError {
            reducer_name: "spawn_entity".into(),
            error: "world not created".into(),
        },
    ];

    assert_eq!(events.len(), 21);
}

#[test]
fn stdb_event_clone_and_debug() {
    let event = StdbEvent::TickAdvanced {
        old_tick: 0,
        new_tick: 1,
    };
    let cloned = event.clone();
    let debug = format!("{:?}", cloned);
    assert!(debug.contains("TickAdvanced"));
}

// ============================================================
// CONNECTION STATE ENUM
// ============================================================

#[test]
fn connection_state_variants() {
    let disconnected = ConnectionState::Disconnected;
    assert!(matches!(disconnected, ConnectionState::Disconnected));

    let connecting = ConnectionState::Connecting;
    assert!(matches!(connecting, ConnectionState::Connecting));

    let connected = ConnectionState::Connected {
        identity: vec![0xAB, 0xCD],
        token: "my-token".into(),
    };
    assert!(matches!(connected, ConnectionState::Connected { .. }));

    let error = ConnectionState::Error("timeout".into());
    assert!(matches!(error, ConnectionState::Error(_)));
}

#[test]
fn connection_state_debug_format() {
    let state = ConnectionState::Connected {
        identity: vec![1, 2],
        token: "tok".into(),
    };
    let debug = format!("{:?}", state);
    assert!(debug.contains("Connected"));
}

// ============================================================
// FRAME TICK METRICS
// ============================================================

#[test]
fn frame_tick_increments_counter() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    assert_eq!(client.frames_processed(), 0);

    client.frame_tick();
    assert_eq!(client.frames_processed(), 1);

    client.frame_tick();
    client.frame_tick();
    assert_eq!(client.frames_processed(), 3);
}

#[test]
fn drain_events_returns_empty_initially() {
    let mut client = StdbClient::new(StdbClientConfig::default());
    let events: Vec<StdbEvent> = client.drain_events().collect();
    assert!(events.is_empty());
}

// ============================================================
// STDB CLIENT DEBUG IMPL
// ============================================================

#[test]
fn client_debug_shows_key_info() {
    let client = StdbClient::new(StdbClientConfig::default());
    let debug = format!("{:?}", client);
    assert!(debug.contains("StdbClient"));
    assert!(debug.contains("localhost"));
    assert!(debug.contains("prompt-or-die"));
    assert!(debug.contains("Disconnected"));
    assert!(debug.contains("entities_cached"));
    assert!(debug.contains("frames_processed"));
}

// ============================================================
// CONNECT_AGENT SETS CONTROLLED ENTITY
// ============================================================

#[test]
fn controlled_entity_none_initially() {
    let client = StdbClient::new(StdbClientConfig::default());
    assert!(client.controlled_entity().is_none());
}

// Note: We can't test call_connect_agent setting controlled_entity
// without a connected client (which requires live SpacetimeDB).
// The unit tests in client.rs cover this via pub(crate) access.

// ============================================================
// COMBINED: ENTITY LOOKUP ON EMPTY CACHE
// ============================================================

#[test]
fn entity_lookup_on_empty_cache() {
    let client = StdbClient::new(StdbClientConfig::default());
    assert!(client.entity(0).is_none());
    assert!(client.entity(1).is_none());
    assert!(client.entity(u64::MAX).is_none());
    assert_eq!(client.entity_count(), 0);
}

#[test]
fn latest_observation_on_empty_cache() {
    let client = StdbClient::new(StdbClientConfig::default());
    assert!(client.latest_observation(1).is_none());
    assert!(client.latest_observation(0).is_none());
}
