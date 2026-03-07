//! Integration tests for SpacetimeDB network client behavior.
//!
//! These tests exercise the public `pod-net` StDB adapter API end-to-end without
//! requiring a real SpacetimeDB service. The stubbed `pod-stdb` client in this
//! repository is still fully exercised for profile transitions, reducer guards, and
//! frame polling behavior.

#![cfg(feature = "spacetimedb")]

use glam::Vec2;
use pod_core::Action;
use pod_net::{SpacetimeDBClient, SpacetimeDBClientConfig, StdbClientError};

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
