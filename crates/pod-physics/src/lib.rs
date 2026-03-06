//! pod-physics — Rapier2D integration layer for the Prompt or Die game engine
//!
//! This crate provides a 2D physics simulation using Rapier2D with deterministic behavior.
//! It syncs between the hecs ECS world and Rapier's physics pipeline.

#![allow(clippy::field_reassign_with_default)]
#![allow(clippy::for_kv_map)]

use glam::Vec2;
use hecs::World;
use log::{debug, warn};
use rapier2d::dynamics::{IntegrationParameters, IslandManager, RigidBodyHandle, RigidBodySet};
use rapier2d::geometry::{ColliderHandle, ColliderSet, DefaultBroadPhase, NarrowPhase};
use rapier2d::pipeline::PhysicsPipeline;
use rapier2d::prelude::*;
use std::collections::HashMap;

pub use pod_core::component::{BodyType, Collider, ColliderShape, RigidBody, Transform, Velocity};
pub use pod_core::event::{Event, EventBus};
pub use pod_core::id::EntityId;

// ============================================================
// PHYSICS HANDLE COMPONENT
// ============================================================

/// Stores Rapier handles for an entity that has been synced to the physics world.
/// This component is added by sync_to_rapier() and should not be manually created.
#[derive(Debug, Clone, Copy)]
pub struct PhysicsHandle {
    pub body_handle: RigidBodyHandle,
    pub collider_handle: ColliderHandle,
}

// ============================================================
// PHYSICS WORLD
// ============================================================

/// Wraps the entire Rapier2D physics pipeline and manages synchronization
/// with the hecs ECS world.
pub struct PhysicsWorld {
    /// Rapier's rigid body collection
    bodies: RigidBodySet,
    /// Rapier's collider collection
    colliders: ColliderSet,
    /// Physics integration parameters (timestep, gravity, etc.)
    integration_params: IntegrationParameters,
    /// Main physics pipeline
    pipeline: PhysicsPipeline,
    /// Island manager for sleeping/waking bodies
    island_manager: IslandManager,
    /// Broad phase collision detection
    broad_phase: DefaultBroadPhase,
    /// Narrow phase collision detection and contact resolution
    narrow_phase: NarrowPhase,
    /// Impulse-based joint constraints
    impulse_joints: ImpulseJointSet,
    /// Multibody joint constraints
    multibody_joints: MultibodyJointSet,
    /// CCD (continuous collision detection) solver
    ccd_solver: CCDSolver,
    /// Physics query pipeline
    query_pipeline: QueryPipeline,

    /// Gravity vector (default: (0, 0) for top-down, can set to (0, -981) for side-view)
    pub gravity: Vec2,

    /// Fixed timestep for physics (default: 1/60)
    pub timestep: f32,

    /// Mapping from EntityId to Rapier handles (for fast lookup during sync)
    entity_handles: HashMap<EntityId, PhysicsHandle>,
    /// Reverse mapping from RigidBodyHandle to EntityId (for fast lookup in queries)
    body_handle_to_entity: HashMap<RigidBodyHandle, EntityId>,

    /// Track contact pairs from previous frame for trigger exit detection
    previous_contacts: HashMap<(EntityId, EntityId), bool>,
    /// Current frame contacts (updated during step)
    current_contacts: HashMap<(EntityId, EntityId), bool>,
}

impl PhysicsWorld {
    /// Creates a new PhysicsWorld with default settings
    pub fn new() -> Self {
        let mut params = IntegrationParameters::default();
        params.dt = 1.0 / 60.0; // 60 FPS fixed timestep

        Self {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            integration_params: params,
            pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
            gravity: Vec2::ZERO,
            timestep: 1.0 / 60.0,
            entity_handles: HashMap::new(),
            body_handle_to_entity: HashMap::new(),
            previous_contacts: HashMap::new(),
            current_contacts: HashMap::new(),
        }
    }

    /// Creates a new PhysicsWorld with a custom timestep
    pub fn with_timestep(timestep: f32) -> Self {
        let mut world = Self::new();
        world.timestep = timestep;
        world.integration_params.dt = timestep;
        world
    }

    /// Syncs hecs world entities to the Rapier physics world.
    /// Creates new Rapier bodies/colliders for entities that don't have PhysicsHandle yet.
    /// Updates existing bodies' positions/velocities/properties.
    pub fn sync_to_rapier(&mut self, hecs_world: &mut World) -> Result<(), String> {
        // Collect entities with physics components
        let mut to_sync = Vec::new();

        for (entity_id_raw, (transform, rigid_body, collider)) in hecs_world
            .query::<(&Transform, &RigidBody, &Collider)>()
            .iter()
        {
            let entity_id = EntityId(entity_id_raw.id().into());
            to_sync.push((entity_id, *transform, *rigid_body, *collider));
        }

        // Sync each entity
        for (entity_id, transform, rigid_body, collider) in to_sync {
            if let Err(e) = self.sync_entity(hecs_world, entity_id, transform, rigid_body, collider)
            {
                warn!("Failed to sync entity {}: {}", entity_id, e);
            }
        }

        Ok(())
    }

    /// Syncs a single entity to the physics world
    fn sync_entity(
        &mut self,
        hecs_world: &World,
        entity_id: EntityId,
        transform: Transform,
        rigid_body: RigidBody,
        collider: Collider,
    ) -> Result<(), String> {
        // Check if this entity already has physics
        if let Some(handle) = self.entity_handles.get(&entity_id).copied() {
            // Update existing body
            self.update_body(&handle, transform, rigid_body, collider)?;
        } else {
            // Create new body
            self.create_body(hecs_world, entity_id, transform, rigid_body, collider)?;
        }

        Ok(())
    }

    /// Creates a new Rapier body and collider for an entity
    fn create_body(
        &mut self,
        _hecs_world: &World,
        entity_id: EntityId,
        transform: Transform,
        rigid_body: RigidBody,
        collider: Collider,
    ) -> Result<(), String> {
        // Create RigidBody descriptor
        let body_type = match rigid_body.body_type {
            BodyType::Static => RigidBodyType::Fixed,
            BodyType::Dynamic => RigidBodyType::Dynamic,
            BodyType::Kinematic => RigidBodyType::KinematicPositionBased,
        };

        let mut rb = RigidBodyBuilder::new(body_type)
            .translation(vector![transform.position.x, transform.position.y])
            .rotation(transform.rotation)
            .linvel(vector![0.0, 0.0])
            .angvel(0.0)
            .linear_damping(0.0)
            .angular_damping(0.0);

        // Set mass properties for dynamic bodies
        if rigid_body.body_type == BodyType::Dynamic && rigid_body.mass > 0.0 {
            rb = rb.additional_mass(rigid_body.mass);
        }

        let body_handle = self.bodies.insert(rb.build());

        // Create Collider
        let mut col = match collider.shape {
            ColliderShape::Circle { radius } => ColliderBuilder::ball(radius),
            ColliderShape::Box {
                half_width,
                half_height,
            } => ColliderBuilder::cuboid(half_width, half_height),
            ColliderShape::Capsule {
                half_height,
                radius,
            } => ColliderBuilder::capsule_y(half_height, radius),
        }
            .friction(rigid_body.friction)
            .restitution(rigid_body.restitution)
            .sensor(collider.is_trigger)
            .active_events(ActiveEvents::COLLISION_EVENTS);

        // Convert collision groups/masks
        let group = collision_group_from_u32(collider.collision_group);
        let mask = collision_group_from_u32(collider.collision_mask);
        col = col.collision_groups(InteractionGroups::new(group, mask));

        let collider_handle = self.colliders.insert_with_parent(col.build(), body_handle, &mut self.bodies);

        // Store the handle and reverse mapping
        let physics_handle = PhysicsHandle {
            body_handle,
            collider_handle,
        };
        self.entity_handles.insert(entity_id, physics_handle);
        self.body_handle_to_entity.insert(body_handle, entity_id);

        debug!(
            "Created physics body for entity {} (type: {:?})",
            entity_id, rigid_body.body_type
        );

        Ok(())
    }

    /// Updates an existing Rapier body's properties
    fn update_body(
        &mut self,
        handle: &PhysicsHandle,
        transform: Transform,
        rigid_body: RigidBody,
        collider: Collider,
    ) -> Result<(), String> {
        // Update body position and rotation
        if let Some(body) = self.bodies.get_mut(handle.body_handle) {
            // Only update position if it's kinematic or static (dynamic bodies are moved by physics)
            if matches!(body.body_type(), RigidBodyType::Fixed | RigidBodyType::KinematicPositionBased)
            {
                body.set_position(
                    Isometry::new(
                        vector![transform.position.x, transform.position.y],
                        transform.rotation,
                    ),
                    true,
                );
            }
        }

        // Update collider properties
        if let Some(collider_obj) = self.colliders.get_mut(handle.collider_handle) {
            collider_obj.set_friction(rigid_body.friction);
            collider_obj.set_restitution(rigid_body.restitution);
            collider_obj.set_sensor(collider.is_trigger);

            // Update collision groups
            let group = collision_group_from_u32(collider.collision_group);
            let mask = collision_group_from_u32(collider.collision_mask);
            collider_obj.set_collision_groups(InteractionGroups::new(group, mask));
        }

        Ok(())
    }

    /// Steps the physics simulation by one fixed timestep
    pub fn step(&mut self) {
        // Update contact tracking for trigger events
        self.previous_contacts = self.current_contacts.clone();
        self.current_contacts.clear();

        // Build gravity vector
        let gravity_vector = vector![self.gravity.x, self.gravity.y];

        // Step the physics pipeline
        self.pipeline.step(
            &gravity_vector,
            &self.integration_params,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            Some(&mut self.query_pipeline),
            &(),
            &(),
        );

        // Record current contacts from contact pairs
        for contact_pair in self.narrow_phase.contact_pairs() {
            let entity_a = self.get_entity_id_from_collider(contact_pair.collider1);
            let entity_b = self.get_entity_id_from_collider(contact_pair.collider2);

            if let (Some(a), Some(b)) = (entity_a, entity_b) {
                let key = if a.0 < b.0 { (a, b) } else { (b, a) };
                self.current_contacts.insert(key, contact_pair.has_any_active_contact);
            }
        }

        debug!("Physics step completed");
    }

    /// Syncs Rapier body states back to the hecs world
    /// Updates Transform and Velocity components from Rapier bodies
    pub fn sync_from_rapier(&self, hecs_world: &mut World) -> Result<(), String> {
        let mut query = hecs_world.query::<(&mut Transform, &mut Velocity)>();
        for (entity, (transform, velocity)) in &mut query {
            let entity_id = EntityId(entity.id().into());
            let Some(handle) = self.entity_handles.get(&entity_id) else {
                continue;
            };
            let Some(body) = self.bodies.get(handle.body_handle) else {
                continue;
            };

            let position = body.position().translation.vector;
            let rotation = body.position().rotation.angle();
            let linear_vel = body.linvel();
            let angular_vel = body.angvel();

            transform.position = Vec2::new(position.x, position.y);
            transform.rotation = rotation;
            velocity.linear = Vec2::new(linear_vel.x, linear_vel.y);
            velocity.angular = angular_vel;
        }

        Ok(())
    }

    /// Collects physics events and emits them to the EventBus
    pub fn collect_events(&self, event_bus: &mut EventBus) {
        // Process active collisions from contact pairs
        for contact_pair in self.narrow_phase.contact_pairs() {
            let entity_a = self.get_entity_id_from_collider(contact_pair.collider1);
            let entity_b = self.get_entity_id_from_collider(contact_pair.collider2);

            if let (Some(a), Some(b)) = (entity_a, entity_b) {
                if contact_pair.has_any_active_contact {
                    // Approximate contact point/normal from body centers.
                    let body_a = self
                        .colliders
                        .get(contact_pair.collider1)
                        .and_then(|c| c.parent())
                        .and_then(|h| self.bodies.get(h));
                    let body_b = self
                        .colliders
                        .get(contact_pair.collider2)
                        .and_then(|c| c.parent())
                        .and_then(|h| self.bodies.get(h));

                    let (collision_point, collision_normal) = if let (Some(a), Some(b)) = (body_a, body_b) {
                        let a_pos = a.translation();
                        let b_pos = b.translation();
                        let point = Vec2::new((a_pos.x + b_pos.x) * 0.5, (a_pos.y + b_pos.y) * 0.5);
                        let delta = Vec2::new(b_pos.x - a_pos.x, b_pos.y - a_pos.y);
                        let normal = if delta.length_squared() > 1e-6 {
                            delta.normalize()
                        } else {
                            Vec2::Y
                        };
                        (point, normal)
                    } else {
                        (Vec2::ZERO, Vec2::Y)
                    };

                    // Check if either collider is a trigger
                    let is_a_trigger = self.colliders
                        .get(contact_pair.collider1)
                        .map(|c| c.is_sensor())
                        .unwrap_or(false);
                    let is_b_trigger = self.colliders
                        .get(contact_pair.collider2)
                        .map(|c| c.is_sensor())
                        .unwrap_or(false);

                    if is_a_trigger {
                        event_bus.emit(
                            collision_point,
                            Event::TriggerEnter {
                                trigger: a,
                                entity: b,
                            },
                        );
                    } else if is_b_trigger {
                        event_bus.emit(
                            collision_point,
                            Event::TriggerEnter {
                                trigger: b,
                                entity: a,
                            },
                        );
                    } else {
                        event_bus.emit(
                            collision_point,
                            Event::Collision {
                                entity_a: a,
                                entity_b: b,
                                point: collision_point,
                                normal: collision_normal,
                            },
                        );
                    }
                }
            }
        }

        // Process trigger exits
        for ((a, b), _) in &self.previous_contacts {
            let key = if a.0 < b.0 { (*a, *b) } else { (*b, *a) };
            if !self.current_contacts.contains_key(&key) {
                // This contact existed last frame but not this frame
                let is_a_trigger = self.entity_handles
                    .get(a)
                    .and_then(|h| self.colliders.get(h.collider_handle))
                    .map(|c| c.is_sensor())
                    .unwrap_or(false);

                if is_a_trigger {
                    event_bus.emit_global(Event::TriggerExit {
                        trigger: *a,
                        entity: *b,
                    });
                } else {
                    event_bus.emit_global(Event::TriggerExit {
                        trigger: *b,
                        entity: *a,
                    });
                }
            }
        }
    }

    /// Applies a force to a body (accumulated over the frame)
    pub fn apply_force(&mut self, entity_id: EntityId, force: Vec2) -> Result<(), String> {
        if let Some(handle) = self.entity_handles.get(&entity_id) {
            if let Some(body) = self.bodies.get_mut(handle.body_handle) {
                body.add_force(vector![force.x, force.y], true);
                return Ok(());
            }
        }
        Err(format!("Entity {} not found in physics world", entity_id))
    }

    /// Applies an impulse to a body (instantaneous change in velocity)
    pub fn apply_impulse(&mut self, entity_id: EntityId, impulse: Vec2) -> Result<(), String> {
        if let Some(handle) = self.entity_handles.get(&entity_id) {
            if let Some(body) = self.bodies.get_mut(handle.body_handle) {
                body.apply_impulse(vector![impulse.x, impulse.y], true);
                return Ok(());
            }
        }
        Err(format!("Entity {} not found in physics world", entity_id))
    }

    /// Applies an angular impulse (torque) to a body
    pub fn apply_torque_impulse(&mut self, entity_id: EntityId, torque: f32) -> Result<(), String> {
        if let Some(handle) = self.entity_handles.get(&entity_id) {
            if let Some(body) = self.bodies.get_mut(handle.body_handle) {
                body.apply_torque_impulse(torque, true);
                return Ok(());
            }
        }
        Err(format!("Entity {} not found in physics world", entity_id))
    }

    /// Sets the linear velocity of a body
    pub fn set_velocity(&mut self, entity_id: EntityId, velocity: Vec2) -> Result<(), String> {
        if let Some(handle) = self.entity_handles.get(&entity_id) {
            if let Some(body) = self.bodies.get_mut(handle.body_handle) {
                body.set_linvel(vector![velocity.x, velocity.y], true);
                return Ok(());
            }
        }
        Err(format!("Entity {} not found in physics world", entity_id))
    }

    /// Sets the angular velocity of a body
    pub fn set_angular_velocity(&mut self, entity_id: EntityId, angular_velocity: f32) -> Result<(), String> {
        if let Some(handle) = self.entity_handles.get(&entity_id) {
            if let Some(body) = self.bodies.get_mut(handle.body_handle) {
                body.set_angvel(angular_velocity, true);
                return Ok(());
            }
        }
        Err(format!("Entity {} not found in physics world", entity_id))
    }

    /// Gets the current velocity of a body
    pub fn get_velocity(&self, entity_id: EntityId) -> Result<Velocity, String> {
        if let Some(handle) = self.entity_handles.get(&entity_id) {
            if let Some(body) = self.bodies.get(handle.body_handle) {
                let linvel = body.linvel();
                let angvel = body.angvel();
                return Ok(Velocity {
                    linear: Vec2::new(linvel.x, linvel.y),
                    angular: angvel,
                });
            }
        }
        Err(format!("Entity {} not found in physics world", entity_id))
    }

    /// Gets the current position of a body
    pub fn get_position(&self, entity_id: EntityId) -> Result<Vec2, String> {
        if let Some(handle) = self.entity_handles.get(&entity_id) {
            if let Some(body) = self.bodies.get(handle.body_handle) {
                let pos = body.position().translation.vector;
                return Ok(Vec2::new(pos.x, pos.y));
            }
        }
        Err(format!("Entity {} not found in physics world", entity_id))
    }

    /// Removes a body from the physics world
    pub fn remove_body(&mut self, entity_id: EntityId) -> Result<(), String> {
        if let Some(handle) = self.entity_handles.remove(&entity_id) {
            self.body_handle_to_entity.remove(&handle.body_handle);
            let _ = self.bodies.remove(
                handle.body_handle,
                &mut self.island_manager,
                &mut self.colliders,
                &mut self.impulse_joints,
                &mut self.multibody_joints,
                true,
            );
            return Ok(());
        }
        Err(format!("Entity {} not found in physics world", entity_id))
    }

    /// Checks if an entity has a physics body
    pub fn has_body(&self, entity_id: EntityId) -> bool {
        self.entity_handles.contains_key(&entity_id)
    }

    /// Gets the Rapier handle for an entity (internal use)
    pub fn get_handle(&self, entity_id: EntityId) -> Option<PhysicsHandle> {
        self.entity_handles.get(&entity_id).copied()
    }

    /// Helper to get EntityId from a ColliderHandle
    fn get_entity_id_from_collider(&self, collider_handle: ColliderHandle) -> Option<EntityId> {
        if let Some(collider) = self.colliders.get(collider_handle) {
            if let Some(body_handle) = collider.parent() {
                return self.body_handle_to_entity.get(&body_handle).copied();
            }
        }
        None
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// HELPER FUNCTIONS
// ============================================================

/// Converts a u32 collision group bitmask to Rapier's Group type
fn collision_group_from_u32(bits: u32) -> Group {
    Group::from_bits_retain(bits)
}

/// Initializes the physics crate (called on startup)
pub fn init() {
    log::info!("{} initialized", env!("CARGO_PKG_NAME"));
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physics_world_creation() {
        let world = PhysicsWorld::new();
        assert_eq!(world.gravity, Vec2::ZERO);
        assert_eq!(world.timestep, 1.0 / 60.0);
    }

    #[test]
    fn test_physics_world_with_timestep() {
        let world = PhysicsWorld::with_timestep(1.0 / 120.0);
        assert!((world.timestep - 1.0 / 120.0).abs() < 0.0001);
    }

    #[test]
    fn test_entity_id_ordering() {
        let a = EntityId(1);
        let b = EntityId(2);
        let key = if a.0 < b.0 { (a, b) } else { (b, a) };
        assert_eq!(key, (EntityId(1), EntityId(2)));
    }
}
