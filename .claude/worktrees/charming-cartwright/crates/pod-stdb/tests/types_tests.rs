//! Tests for shared type definitions.
//!
//! These tests validate the types module which is available on all targets
//! regardless of feature flags. Run with:
//! ```bash
//! cargo test -p pod-stdb --no-default-features --test types_tests
//! ```

use pod_stdb::types::*;

// ============================================================
// AGENT TYPE
// ============================================================

#[test]
fn agent_type_clone_and_eq() {
    let a = AgentType::Human;
    let b = a.clone();
    assert_eq!(a, b);
}

#[test]
fn agent_type_all_variants_distinct() {
    let variants = vec![
        AgentType::Human,
        AgentType::LlmAgent,
        AgentType::NeuralAgent,
        AgentType::ScriptedNpc,
        AgentType::System,
    ];
    // All variants are distinct
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(variants[i], variants[j], "Variants at index {i} and {j} should differ");
        }
    }
}

#[test]
fn agent_type_debug_format() {
    assert_eq!(format!("{:?}", AgentType::Human), "Human");
    assert_eq!(format!("{:?}", AgentType::LlmAgent), "LlmAgent");
    assert_eq!(format!("{:?}", AgentType::NeuralAgent), "NeuralAgent");
    assert_eq!(format!("{:?}", AgentType::ScriptedNpc), "ScriptedNpc");
    assert_eq!(format!("{:?}", AgentType::System), "System");
}

// ============================================================
// BODY KIND
// ============================================================

#[test]
fn body_kind_variants() {
    let variants = vec![BodyKind::Static, BodyKind::Dynamic, BodyKind::Kinematic];
    assert_eq!(variants.len(), 3);
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(variants[i], variants[j]);
        }
    }
}

// ============================================================
// COLLIDER SHAPE KIND
// ============================================================

#[test]
fn collider_shape_kind_variants() {
    let variants = vec![
        ColliderShapeKind::Circle,
        ColliderShapeKind::Box,
        ColliderShapeKind::Capsule,
    ];
    assert_eq!(variants.len(), 3);
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(variants[i], variants[j]);
        }
    }
}

// ============================================================
// ACTION KIND
// ============================================================

#[test]
fn action_kind_all_variants() {
    let variants = vec![
        ActionKind::Move,
        ActionKind::Stop,
        ActionKind::Rotate,
        ActionKind::LookAt,
        ActionKind::Attack,
        ActionKind::AttackTarget,
        ActionKind::UseAbility,
        ActionKind::Interact,
        ActionKind::InteractWith,
        ActionKind::Pickup,
        ActionKind::Drop,
        ActionKind::UseItem,
        ActionKind::Speak,
        ActionKind::Signal,
        ActionKind::Idle,
        ActionKind::Spawn,
    ];
    // 16 action kinds
    assert_eq!(variants.len(), 16);
    // All distinct
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(variants[i], variants[j], "ActionKind at index {i} and {j} should differ");
        }
    }
}

// ============================================================
// ABILITY TARGET KIND
// ============================================================

#[test]
fn ability_target_kind_variants() {
    let variants = vec![
        AbilityTargetKind::Position,
        AbilityTargetKind::Entity,
        AbilityTargetKind::Direction,
        AbilityTargetKind::None,
    ];
    assert_eq!(variants.len(), 4);
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(variants[i], variants[j]);
        }
    }
}

// ============================================================
// SPEAK VOLUME
// ============================================================

#[test]
fn speak_volume_ranges_monotonically_increase() {
    assert!(SpeakVolume::Whisper.range() < SpeakVolume::Normal.range());
    assert!(SpeakVolume::Normal.range() < SpeakVolume::Shout.range());
}

#[test]
fn speak_volume_specific_ranges() {
    assert!((SpeakVolume::Whisper.range() - 50.0).abs() < f32::EPSILON);
    assert!((SpeakVolume::Normal.range() - 200.0).abs() < f32::EPSILON);
    assert!((SpeakVolume::Shout.range() - 500.0).abs() < f32::EPSILON);
}

#[test]
fn speak_volume_clone_eq() {
    let a = SpeakVolume::Shout;
    let b = a.clone();
    assert_eq!(a, b);
}

// ============================================================
// WORLD EVENT KIND
// ============================================================

#[test]
fn world_event_kind_all_variants() {
    let variants = vec![
        WorldEventKind::EntitySpawned,
        WorldEventKind::EntityDespawned,
        WorldEventKind::EntityDied,
        WorldEventKind::InteractionTriggered,
        WorldEventKind::ItemPickedUp,
        WorldEventKind::ItemDropped,
        WorldEventKind::AbilityUsed,
        WorldEventKind::WorldCreated,
        WorldEventKind::WorldReset,
        WorldEventKind::TickAdvanced,
    ];
    // 10 world event kinds
    assert_eq!(variants.len(), 10);
    for i in 0..variants.len() {
        for j in (i + 1)..variants.len() {
            assert_ne!(variants[i], variants[j]);
        }
    }
}

#[test]
fn world_event_kind_debug() {
    assert_eq!(format!("{:?}", WorldEventKind::EntitySpawned), "EntitySpawned");
    assert_eq!(format!("{:?}", WorldEventKind::TickAdvanced), "TickAdvanced");
}
