//! World state serialization for network transmission.
//!
//! Handles capturing world snapshots, computing efficient deltas, and
//! replaying a deterministic subset of client-side prediction.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use pod_core::{Action, Health, Label, Movement, Transform, Velocity};

const FIXED_TICK_DURATION_SECS: f32 = 1.0 / 60.0;
const DEFAULT_PREDICTION_MOVE_SPEED: f32 = 200.0;
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// A serializable snapshot of world state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldSnapshot {
    /// Tick number at time of capture
    pub tick: u64,
    /// All entity states
    pub entities: Vec<EntitySnapshot>,
}

/// A serialized entity state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySnapshot {
    /// Unique entity ID
    pub id: u64,
    /// Entity position
    pub position: Vec2,
    /// Entity velocity
    pub velocity: Vec2,
    /// Entity rotation (radians)
    pub rotation: f32,
    /// Health if applicable
    pub health: Option<f32>,
    /// Max health if applicable
    pub max_health: Option<f32>,
    /// Movement speed if applicable.
    pub movement_speed: Option<f32>,
    /// Entity label/name
    pub label: Option<String>,
}

/// Efficient delta — only changed entities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateDelta {
    /// Tick number of this delta
    pub tick: u64,
    /// Updated entities
    pub updated: Vec<EntitySnapshot>,
    /// Destroyed entity IDs
    pub destroyed: Vec<u64>,
}

/// Tick-aligned locally predicted actions that have been sent to the server but
/// not yet acknowledged by the authoritative simulation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PredictedActionBatch {
    pub tick: u64,
    pub actions: Vec<Action>,
}

/// Result of reconciling local prediction against an authoritative update.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ReconciliationReport {
    pub authoritative_tick: u64,
    pub authoritative_digest: u64,
    pub acknowledged_action_tick: Option<u64>,
    pub pending_action_batches: usize,
    pub replayed_action_count: usize,
    pub predicted_digest: Option<u64>,
    pub used_hard_resync: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotUpdateError {
    MissingBaseline {
        tick: u64,
    },
    DigestMismatch {
        tick: u64,
        expected: u64,
        actual: u64,
    },
}

impl WorldSnapshot {
    /// Capture current world state into a snapshot
    pub fn capture(world: &pod_core::World) -> Self {
        let mut entities = Vec::new();

        for (entity, (transform, velocity)) in world.ecs.query::<(&Transform, &Velocity)>().iter() {
            let id = entity.id();
            let health_opt = world.ecs.get::<&Health>(entity).ok();
            let label_opt = world.ecs.get::<&Label>(entity).ok();

            let health = health_opt.as_ref().map(|h| h.current);
            let max_health = health_opt.as_ref().map(|h| h.max);
            let movement_speed = world.ecs.get::<&Movement>(entity).ok().map(|m| m.max_speed);
            let label = label_opt.map(|l| l.name.clone());

            entities.push(EntitySnapshot {
                id: id as u64,
                position: transform.position,
                velocity: velocity.linear,
                rotation: transform.rotation,
                health,
                max_health,
                movement_speed,
                label,
            });
        }

        Self {
            tick: world.tick,
            entities,
        }
    }

    /// Apply a snapshot to a world (for client-side reconciliation)
    pub fn apply(&self, _world: &mut pod_core::World) {
        // For now, we'll just log this — full reconciliation
        // requires entity mapping and component management
        log::debug!(
            "Applying snapshot at tick {}: {} entities",
            self.tick,
            self.entities.len()
        );
    }

    /// Get entity count in snapshot
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Stable digest for client/server divergence detection.
    pub fn digest(&self) -> u64 {
        let mut hash = FNV_OFFSET_BASIS;
        hash_u64(&mut hash, self.tick);

        let mut entities = self.entities.iter().collect::<Vec<_>>();
        entities.sort_by_key(|entity| entity.id);

        for entity in entities {
            hash_u64(&mut hash, entity.id);
            hash_f32(&mut hash, entity.position.x);
            hash_f32(&mut hash, entity.position.y);
            hash_f32(&mut hash, entity.velocity.x);
            hash_f32(&mut hash, entity.velocity.y);
            hash_f32(&mut hash, entity.rotation);
            hash_option_f32(&mut hash, entity.health);
            hash_option_f32(&mut hash, entity.max_health);
            hash_option_f32(&mut hash, entity.movement_speed);
            hash_option_str(&mut hash, entity.label.as_deref());
        }

        hash
    }

    /// Replays locally predicted action batches over an authoritative snapshot.
    ///
    /// This intentionally models a narrow shared-sim subset: kinematic movement,
    /// stop, and facing updates for the locally controlled entity while advancing
    /// all entities by their current velocities between ticks.
    pub fn replay_predicted_actions(
        &self,
        controlled_entity: Option<u64>,
        batches: &[PredictedActionBatch],
    ) -> Self {
        let mut predicted = self.clone();

        for batch in batches {
            if let Some(controlled_entity) = controlled_entity {
                if let Some(entity) = predicted
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == controlled_entity)
                {
                    for action in &batch.actions {
                        apply_predicted_action(entity, action);
                    }
                }
            }

            for entity in &mut predicted.entities {
                entity.position += entity.velocity * FIXED_TICK_DURATION_SECS;
            }

            predicted.tick = batch.tick;
        }

        predicted
    }
}

impl StateDelta {
    /// Create a delta by comparing two snapshots
    pub fn diff(old: &WorldSnapshot, new: &WorldSnapshot) -> Self {
        let old_map: HashMap<u64, &EntitySnapshot> =
            old.entities.iter().map(|e| (e.id, e)).collect();

        let new_map: HashMap<u64, &EntitySnapshot> =
            new.entities.iter().map(|e| (e.id, e)).collect();

        let mut updated = Vec::new();
        let mut destroyed = Vec::new();

        // Find new and updated entities
        for (id, new_entity) in &new_map {
            if let Some(old_entity) = old_map.get(id) {
                // Check if anything changed
                if entity_changed(old_entity, new_entity) {
                    updated.push((*new_entity).clone());
                }
            } else {
                // New entity
                updated.push((*new_entity).clone());
            }
        }

        // Find destroyed entities
        for id in old_map.keys() {
            if !new_map.contains_key(id) {
                destroyed.push(*id);
            }
        }

        Self {
            tick: new.tick,
            updated,
            destroyed,
        }
    }

    /// Apply delta to a snapshot to create new snapshot
    pub fn apply_to(&self, snapshot: &WorldSnapshot) -> WorldSnapshot {
        let mut entities = snapshot.entities.clone();

        // Remove destroyed entities
        entities.retain(|e| !self.destroyed.contains(&e.id));

        // Update or add new entities
        for updated in &self.updated {
            if let Some(pos) = entities.iter_mut().find(|e| e.id == updated.id) {
                *pos = updated.clone();
            } else {
                entities.push(updated.clone());
            }
        }

        WorldSnapshot {
            tick: self.tick,
            entities,
        }
    }

    /// Get change count (for bandwidth estimation)
    pub fn change_count(&self) -> usize {
        self.updated.len() + self.destroyed.len()
    }
}

/// Check if an entity changed between snapshots
fn entity_changed(old: &EntitySnapshot, new: &EntitySnapshot) -> bool {
    // Allow small position/velocity deltas (floating point noise)
    const EPSILON: f32 = 0.01;

    old.position.distance(new.position) > EPSILON
        || old.velocity.distance(new.velocity) > EPSILON
        || (old.rotation - new.rotation).abs() > EPSILON
        || old.health != new.health
        || old.max_health != new.max_health
        || old.movement_speed != new.movement_speed
        || old.label != new.label
}

/// Applies an authoritative state update and validates its digest.
pub fn apply_authoritative_update(
    previous: Option<&WorldSnapshot>,
    tick: u64,
    is_full_snapshot: bool,
    delta: &StateDelta,
    authoritative_digest: u64,
) -> Result<WorldSnapshot, SnapshotUpdateError> {
    let snapshot = if is_full_snapshot {
        WorldSnapshot {
            tick,
            entities: delta.updated.clone(),
        }
    } else {
        let previous = previous.ok_or(SnapshotUpdateError::MissingBaseline { tick })?;
        delta.apply_to(previous)
    };

    let actual_digest = snapshot.digest();
    if actual_digest != authoritative_digest {
        return Err(SnapshotUpdateError::DigestMismatch {
            tick,
            expected: authoritative_digest,
            actual: actual_digest,
        });
    }

    Ok(snapshot)
}

fn apply_predicted_action(entity: &mut EntitySnapshot, action: &Action) {
    match action {
        Action::Move { direction } => {
            let speed = entity
                .movement_speed
                .unwrap_or(DEFAULT_PREDICTION_MOVE_SPEED);
            entity.velocity = direction.normalize_or_zero() * speed;
        }
        Action::Stop => {
            entity.velocity = Vec2::ZERO;
        }
        Action::Rotate { angle } => {
            entity.rotation = *angle;
        }
        Action::LookAt { target } => {
            let delta = *target - entity.position;
            if delta.length_squared() > f32::EPSILON {
                entity.rotation = delta.y.atan2(delta.x);
            }
        }
        _ => {}
    }
}

fn hash_u64(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn hash_f32(hash: &mut u64, value: f32) {
    hash_u64(hash, value.to_bits() as u64);
}

fn hash_option_f32(hash: &mut u64, value: Option<f32>) {
    match value {
        Some(value) => {
            hash_u64(hash, 1);
            hash_f32(hash, value);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_str(hash: &mut u64, value: Option<&str>) {
    match value {
        Some(value) => {
            hash_u64(hash, 1);
            for byte in value.as_bytes() {
                hash_u64(hash, *byte as u64);
            }
        }
        None => hash_u64(hash, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_default() {
        let snap = WorldSnapshot::default();
        assert_eq!(snap.tick, 0);
        assert_eq!(snap.entity_count(), 0);
    }

    #[test]
    fn test_delta_diff() {
        let mut snap1 = WorldSnapshot::default();
        snap1.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::new(1.0, 0.0),
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: Some(200.0),
            label: Some("Player".into()),
        });

        let mut snap2 = WorldSnapshot::default();
        snap2.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(1.0, 0.0), // Changed
            velocity: Vec2::new(1.0, 0.0),
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: Some(200.0),
            label: Some("Player".into()),
        });

        let delta = StateDelta::diff(&snap1, &snap2);
        assert_eq!(delta.updated.len(), 1);
        assert_eq!(delta.destroyed.len(), 0);
    }

    #[test]
    fn test_delta_apply() {
        let mut snap1 = WorldSnapshot::default();
        snap1.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            health: None,
            max_health: None,
            movement_speed: None,
            label: None,
        });

        let delta = StateDelta {
            tick: 1,
            updated: vec![EntitySnapshot {
                id: 2,
                position: Vec2::new(5.0, 5.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: None,
                label: None,
            }],
            destroyed: vec![],
        };

        let snap2 = delta.apply_to(&snap1);
        assert_eq!(snap2.entities.len(), 2);
    }

    #[test]
    fn test_delta_ignores_small_jitter_as_unchanged() {
        let mut snap1 = WorldSnapshot::default();
        snap1.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::new(0.0, 0.0),
            rotation: 0.0,
            health: None,
            max_health: None,
            movement_speed: Some(200.0),
            label: Some("sprite2d".into()),
        });

        let mut snap2 = WorldSnapshot::default();
        snap2.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(0.005, -0.005),
            velocity: Vec2::new(0.002, 0.003),
            rotation: 0.003,
            health: None,
            max_health: None,
            movement_speed: Some(200.0),
            label: Some("sprite2d".into()),
        });

        let delta = StateDelta::diff(&snap1, &snap2);
        assert_eq!(delta.updated.len(), 0);
        assert_eq!(delta.destroyed.len(), 0);
    }

    #[test]
    fn test_delta_prefers_updated_mode_like_label_change_over_destroy() {
        let mut base = WorldSnapshot::default();
        base.entities.push(EntitySnapshot {
            id: 12,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: Some(200.0),
            label: Some("sprite2d".into()),
        });

        let mut next = WorldSnapshot::default();
        next.entities.push(EntitySnapshot {
            id: 12,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: Some(200.0),
            label: Some("sprite3d".into()),
        });

        let delta = StateDelta::diff(&base, &next);
        assert_eq!(delta.updated.len(), 1);
        assert_eq!(delta.updated[0].label.as_deref(), Some("sprite3d"));

        let delta_with_destroy = StateDelta {
            tick: 2,
            updated: delta.updated.clone(),
            destroyed: vec![12],
        };

        let applied = delta_with_destroy.apply_to(&base);
        assert_eq!(applied.entities.len(), 1);
        let updated = &applied.entities[0];
        assert_eq!(updated.id, 12);
        assert_eq!(updated.label.as_deref(), Some("sprite3d"));
    }

    #[test]
    fn test_delta_apply_keeps_existing_updates_when_destroy_contains_extra_ids() {
        let mut base = WorldSnapshot::default();
        base.entities.push(EntitySnapshot {
            id: 10,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::new(0.0, 0.0),
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: None,
            label: Some("keep".into()),
        });
        base.entities.push(EntitySnapshot {
            id: 11,
            position: Vec2::new(1.0, 1.0),
            velocity: Vec2::new(0.0, 0.0),
            rotation: 0.0,
            health: Some(50.0),
            max_health: Some(50.0),
            movement_speed: None,
            label: Some("to_remove".into()),
        });

        let delta = StateDelta {
            tick: 10,
            updated: vec![EntitySnapshot {
                id: 10,
                position: Vec2::new(5.0, 5.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: Some(100.0),
                max_health: Some(100.0),
                movement_speed: None,
                label: Some("updated".into()),
            }],
            destroyed: vec![11, 99_999],
        };

        let applied = delta.apply_to(&base);
        assert_eq!(applied.entities.len(), 1);
        assert_eq!(applied.entities[0].id, 10);
        assert_eq!(
            applied
                .entities
                .iter()
                .find(|e| e.id == 10)
                .unwrap()
                .label
                .as_deref(),
            Some("updated")
        );
        assert!(applied.entities.iter().all(|entity| entity.id != 99_999));
    }

    #[test]
    fn test_snapshot_digest_is_stable_for_entity_order() {
        let entity_a = EntitySnapshot {
            id: 1,
            position: Vec2::new(1.0, 2.0),
            velocity: Vec2::new(3.0, 4.0),
            rotation: 0.5,
            health: Some(10.0),
            max_health: Some(20.0),
            movement_speed: Some(200.0),
            label: Some("a".into()),
        };
        let entity_b = EntitySnapshot {
            id: 2,
            position: Vec2::new(5.0, 6.0),
            velocity: Vec2::new(7.0, 8.0),
            rotation: 1.5,
            health: None,
            max_health: None,
            movement_speed: None,
            label: Some("b".into()),
        };

        let snapshot_a = WorldSnapshot {
            tick: 4,
            entities: vec![entity_a.clone(), entity_b.clone()],
        };
        let snapshot_b = WorldSnapshot {
            tick: 4,
            entities: vec![entity_b, entity_a],
        };

        assert_eq!(snapshot_a.digest(), snapshot_b.digest());
    }

    #[test]
    fn test_replay_predicted_actions_replays_move_and_stop() {
        let snapshot = WorldSnapshot {
            tick: 10,
            entities: vec![EntitySnapshot {
                id: 7,
                position: Vec2::ZERO,
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: Some(120.0),
                label: Some("player".into()),
            }],
        };

        let predicted = snapshot.replay_predicted_actions(
            Some(7),
            &[
                PredictedActionBatch {
                    tick: 11,
                    actions: vec![Action::Move { direction: Vec2::X }],
                },
                PredictedActionBatch {
                    tick: 12,
                    actions: vec![Action::Stop],
                },
            ],
        );

        let entity = &predicted.entities[0];
        assert_eq!(predicted.tick, 12);
        assert!((entity.position.x - 2.0).abs() < 0.001);
        assert_eq!(entity.velocity, Vec2::ZERO);
    }

    #[test]
    fn test_apply_authoritative_update_requires_baseline_for_delta() {
        let delta = StateDelta {
            tick: 5,
            updated: vec![],
            destroyed: vec![],
        };

        let error = apply_authoritative_update(None, 5, false, &delta, 0).unwrap_err();
        assert_eq!(error, SnapshotUpdateError::MissingBaseline { tick: 5 });
    }

    #[test]
    fn test_apply_authoritative_update_accepts_full_snapshot() {
        let full = StateDelta {
            tick: 5,
            updated: vec![EntitySnapshot {
                id: 1,
                position: Vec2::new(2.0, 3.0),
                velocity: Vec2::X,
                rotation: 0.25,
                health: Some(10.0),
                max_health: Some(10.0),
                movement_speed: Some(200.0),
                label: Some("player".into()),
            }],
            destroyed: vec![],
        };

        let expected_snapshot = WorldSnapshot {
            tick: 5,
            entities: full.updated.clone(),
        };

        let applied =
            apply_authoritative_update(None, 5, true, &full, expected_snapshot.digest()).unwrap();

        assert_eq!(applied.digest(), expected_snapshot.digest());
    }
}
