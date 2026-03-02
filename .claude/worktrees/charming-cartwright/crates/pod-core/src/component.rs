use glam::Vec2;
use serde::{Deserialize, Serialize};
use crate::id::AgentId;

// ============================================================
// SPATIAL COMPONENTS
// ============================================================

/// Position, rotation, scale in 2D world space
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform {
    pub position: Vec2,
    pub rotation: f32, // radians
    pub scale: Vec2,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
        }
    }
}

impl Transform {
    pub fn at(x: f32, y: f32) -> Self {
        Self {
            position: Vec2::new(x, y),
            ..Default::default()
        }
    }
}

/// Linear and angular velocity
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Velocity {
    pub linear: Vec2,
    pub angular: f32,
}

// ============================================================
// PHYSICS COMPONENTS
// ============================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum BodyType {
    /// Doesn't move, infinite mass (walls, ground)
    Static,
    /// Moved by forces (players, projectiles)
    Dynamic,
    /// Moved by code, pushes dynamic bodies (platforms)
    Kinematic,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RigidBody {
    pub body_type: BodyType,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32, // bounciness
    pub gravity_scale: f32,
}

impl Default for RigidBody {
    fn default() -> Self {
        Self {
            body_type: BodyType::Dynamic,
            mass: 1.0,
            friction: 0.3,
            restitution: 0.0,
            gravity_scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ColliderShape {
    Circle { radius: f32 },
    Box { half_width: f32, half_height: f32 },
    Capsule { half_height: f32, radius: f32 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Collider {
    pub shape: ColliderShape,
    pub is_trigger: bool, // triggers events but doesn't block movement
    pub collision_group: u32,
    pub collision_mask: u32,
}

impl Collider {
    pub fn circle(radius: f32) -> Self {
        Self {
            shape: ColliderShape::Circle { radius },
            is_trigger: false,
            collision_group: 0xFFFF_FFFF,
            collision_mask: 0xFFFF_FFFF,
        }
    }

    pub fn rect(width: f32, height: f32) -> Self {
        Self {
            shape: ColliderShape::Box {
                half_width: width / 2.0,
                half_height: height / 2.0,
            },
            is_trigger: false,
            collision_group: 0xFFFF_FFFF,
            collision_mask: 0xFFFF_FFFF,
        }
    }

    pub fn trigger(mut self) -> Self {
        self.is_trigger = true;
        self
    }
}

// ============================================================
// VISUAL COMPONENTS
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sprite {
    pub texture: String,     // asset key
    pub frame: u32,          // for spritesheets
    pub layer: i32,          // draw order
    pub color: [f32; 4],     // RGBA tint
    pub visible: bool,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            texture: String::new(),
            frame: 0,
            layer: 0,
            color: [1.0, 1.0, 1.0, 1.0],
            visible: true,
        }
    }
}

/// Simple colored rectangle for prototyping (no texture needed)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ColorRect {
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
    pub layer: i32,
}

impl ColorRect {
    pub fn new(width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            width,
            height,
            color,
            layer: 0,
        }
    }
}

// ============================================================
// GAMEPLAY COMPONENTS
// ============================================================

/// Marks an entity as controlled by a specific agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentControlled {
    pub agent_id: AgentId,
}

/// Health and damage
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Health {
    pub current: f32,
    pub max: f32,
    pub armor: f32,
    pub invulnerable: bool,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self {
            current: max,
            max,
            armor: 0.0,
            invulnerable: false,
        }
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }

    pub fn damage(&mut self, amount: f32) -> f32 {
        if self.invulnerable {
            return 0.0;
        }
        let effective = (amount - self.armor).max(0.0);
        self.current = (self.current - effective).max(0.0);
        effective
    }

    pub fn heal(&mut self, amount: f32) -> f32 {
        let actual = amount.min(self.max - self.current);
        self.current += actual;
        actual
    }
}

/// A named label for identification and perception
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    pub team: Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Team {
    None,
    Team(u8),
}

impl Team {
    pub fn is_hostile_to(&self, other: &Team) -> bool {
        match (self, other) {
            (Team::None, _) | (_, Team::None) => false,
            (Team::Team(a), Team::Team(b)) => a != b,
        }
    }
}

// ============================================================
// AGENT PERCEPTION
// ============================================================

/// Defines what an agent can perceive about the world
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Perception {
    /// How far the agent can see
    pub vision_range: f32,
    /// Field of view in radians (2π = full circle)
    pub vision_fov: f32,
    /// How far the agent can hear events
    pub hearing_range: f32,
}

impl Default for Perception {
    fn default() -> Self {
        Self {
            vision_range: 300.0,
            vision_fov: std::f32::consts::PI, // 180 degrees
            hearing_range: 500.0,
        }
    }
}

// ============================================================
// ENTITY SCRIPT
// ============================================================

/// Attached Luau script for custom behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub source: String,       // script asset key
    pub enabled: bool,
}

// ============================================================
// MOVEMENT CONSTRAINTS
// ============================================================

/// Movement parameters for agent-controlled entities
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Movement {
    pub max_speed: f32,
    pub acceleration: f32,
    pub deceleration: f32,
    pub turn_rate: f32, // radians per second
}

impl Default for Movement {
    fn default() -> Self {
        Self {
            max_speed: 200.0,
            acceleration: 800.0,
            deceleration: 600.0,
            turn_rate: std::f32::consts::TAU, // full rotation per second
        }
    }
}
