//! World state serialization for network transmission.
//!
//! Handles capturing world snapshots and computing efficient deltas.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use pod_core::{Transform, Velocity, Health, Label};

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

impl WorldSnapshot {
    /// Capture current world state into a snapshot
    pub fn capture(world: &pod_core::World) -> Self {
        let mut entities = Vec::new();

        for (entity, (transform, velocity)) in world
            .ecs
            .query::<(&Transform, &Velocity)>()
            .iter()
        {
            let id = entity.id();
            let health_opt = world.ecs.get::<&Health>(entity).ok();
            let label_opt = world.ecs.get::<&Label>(entity).ok();

            let health = health_opt.as_ref().map(|h| h.current);
            let max_health = health_opt.as_ref().map(|h| h.max);
            let label = label_opt.map(|l| l.name.clone());

            entities.push(EntitySnapshot {
                id: id as u64,
                position: transform.position,
                velocity: velocity.linear,
                rotation: transform.rotation,
                health,
                max_health,
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
        || old.label != new.label
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
            label: Some("keep".into()),
        });
        base.entities.push(EntitySnapshot {
            id: 11,
            position: Vec2::new(1.0, 1.0),
            velocity: Vec2::new(0.0, 0.0),
            rotation: 0.0,
            health: Some(50.0),
            max_health: Some(50.0),
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
                label: Some("updated".into()),
            }],
            destroyed: vec![11, 99_999],
        };

        let applied = delta.apply_to(&base);
        assert_eq!(applied.entities.len(), 1);
        assert_eq!(applied.entities[0].id, 10);
        assert_eq!(applied.entities.iter().find(|e| e.id == 10).unwrap().label.as_deref(), Some("updated"));
        assert!(applied.entities.iter().all(|entity| entity.id != 99_999));
    }
}
