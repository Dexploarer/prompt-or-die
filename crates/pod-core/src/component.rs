use crate::id::{AgentId, EntityId};
use glam::{Quat, Vec2, Vec3};
use serde::{Deserialize, Serialize};

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

/// Position, rotation, and scale in 3D world space
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Transform3D {
    pub position: Vec3,
    /// Unit quaternion (x, y, z, w) used for mesh orientation
    pub rotation: [f32; 4],
    pub scale: Vec3,
}

impl Default for Transform3D {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY.to_array(),
            scale: Vec3::ONE,
        }
    }
}

impl Transform3D {
    pub fn at(x: f32, y: f32, z: f32) -> Self {
        Self {
            position: Vec3::new(x, y, z),
            ..Default::default()
        }
    }

    pub fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation.to_array();
        self
    }

    pub fn with_scale(mut self, scale: Vec3) -> Self {
        self.scale = scale;
        self
    }
}

/// Parent transform linkage for hierarchy-aware transform composition.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Parent3D {
    /// `hecs` entity id for the parent transform.
    /// Generation is intentionally omitted to keep the component serializable;
    /// callers should avoid reusing entity ids in the same render frame.
    pub parent: u64,
}

impl Default for Parent3D {
    fn default() -> Self {
        Self { parent: u64::MAX }
    }
}

/// Orbit-style camera controller for 3D follow-like behavior.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrbitCameraController {
    pub target: [f32; 3],
    pub radius: f32,
    pub min_radius: f32,
    pub max_radius: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub yaw_speed: f32,
    pub pitch_speed: f32,
}

impl Default for OrbitCameraController {
    fn default() -> Self {
        Self {
            target: [0.0, 0.0, 0.0],
            radius: 6.0,
            min_radius: 1.0,
            max_radius: 100.0,
            yaw: 0.0,
            pitch: -0.35,
            yaw_speed: 0.0,
            pitch_speed: 0.0,
        }
    }
}

/// Free-fly camera controller for 3D gameplay camera movement.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FlyCameraController {
    pub yaw: f32,
    pub pitch: f32,
    pub move_speed: f32,
    pub move_input: [f32; 3],
    pub yaw_delta: f32,
    pub pitch_delta: f32,
    pub damping: f32,
}

impl Default for FlyCameraController {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            move_speed: 1.0,
            move_input: [0.0, 0.0, 0.0],
            yaw_delta: 0.0,
            pitch_delta: 0.0,
            damping: 0.9,
        }
    }
}

/// Target-follow camera controller for 3D entities.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FollowCameraController {
    pub target: u64,
    pub offset: [f32; 3],
    pub follow_speed: f32,
}

impl Default for FollowCameraController {
    fn default() -> Self {
        Self {
            target: u64::MAX,
            offset: [0.0, 3.0, -8.0],
            follow_speed: 8.0,
        }
    }
}

/// Mesh asset reference component for 3D renderables
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mesh {
    pub asset_id: String,
    pub visible: bool,
    pub layer: i32,
    pub cast_shadows: bool,
    pub receive_shadows: bool,
}

impl Default for Mesh {
    fn default() -> Self {
        Self {
            asset_id: String::new(),
            visible: true,
            layer: 0,
            cast_shadows: true,
            receive_shadows: true,
        }
    }
}

/// Material description component for 3D renderables
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Material {
    pub asset_id: String,
    pub tint: [f32; 4],
    pub roughness: f32,
    pub metallic: f32,
    pub emissive: [f32; 3],
    pub visible: bool,
    pub double_sided: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            asset_id: String::new(),
            tint: [1.0, 1.0, 1.0, 1.0],
            roughness: 0.5,
            metallic: 0.0,
            emissive: [0.0, 0.0, 0.0],
            visible: true,
            double_sided: false,
        }
    }
}

/// Camera component for 3D rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Camera3D {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y_radians: f32,
    pub aspect_ratio: f32,
    pub near_plane: f32,
    pub far_plane: f32,
    pub is_active: bool,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 1.5, 4.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y_radians: 60.0f32.to_radians(),
            aspect_ratio: 16.0 / 9.0,
            near_plane: 0.1,
            far_plane: 1_000.0,
            is_active: true,
        }
    }
}

impl Camera3D {
    pub fn new(position: Vec3, target: Vec3, aspect_ratio: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::Y,
            fov_y_radians: 60.0f32.to_radians(),
            aspect_ratio,
            near_plane: 0.1,
            far_plane: 1_000.0,
            is_active: true,
        }
    }
}

/// Light source type for 3D scene lighting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LightType {
    Directional {
        direction: Vec3,
    },
    Point {
        range: f32,
        attenuation: f32,
    },
    Spot {
        range: f32,
        inner_cone_angle: f32,
        outer_cone_angle: f32,
    },
}

/// Light component for 3D scene illumination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Light {
    pub kind: LightType,
    pub color: [f32; 3],
    pub intensity: f32,
    pub enabled: bool,
    pub cast_shadows: bool,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            kind: LightType::Directional {
                direction: Vec3::new(0.0, -1.0, 0.0),
            },
            color: [1.0, 1.0, 1.0],
            intensity: 1.0,
            enabled: true,
            cast_shadows: true,
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
    pub texture: String, // asset key
    pub frame: u32,      // for spritesheets
    pub layer: i32,      // draw order
    pub color: [f32; 4], // RGBA tint
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

/// Zone or biome atmosphere that drives sky, fog, and lighting defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtmosphereProfile {
    pub biome_id: String,
    pub sky_color: [f32; 4],
    pub fog_color: [f32; 4],
    pub fog_near: f32,
    pub fog_far: f32,
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
    pub sun_color: [f32; 3],
    pub sun_intensity: f32,
    pub sun_direction: [f32; 3],
    pub fill_color: [f32; 3],
    pub fill_intensity: f32,
    pub fill_direction: [f32; 3],
    pub rim_color: [f32; 3],
    pub rim_intensity: f32,
    pub ground_color: [f32; 4],
    pub starfield_intensity: f32,
}

impl Default for AtmosphereProfile {
    fn default() -> Self {
        Self {
            biome_id: "neutral-shard".to_string(),
            sky_color: [0.04, 0.06, 0.1, 1.0],
            fog_color: [0.035, 0.06, 0.1, 1.0],
            fog_near: 28.0,
            fog_far: 180.0,
            ambient_color: [0.66, 0.82, 1.0],
            ambient_intensity: 1.2,
            sun_color: [1.0, 0.94, 0.82],
            sun_intensity: 2.6,
            sun_direction: [24.0, 42.0, 18.0],
            fill_color: [0.42, 0.74, 1.0],
            fill_intensity: 0.7,
            fill_direction: [-18.0, 14.0, -10.0],
            rim_color: [0.29, 0.76, 1.0],
            rim_intensity: 12.0,
            ground_color: [0.055, 0.09, 0.14, 1.0],
            starfield_intensity: 0.9,
        }
    }
}

/// Radius/priority used to select atmosphere volumes around the player.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AtmosphereVolume {
    pub radius: f32,
    pub priority: u8,
}

impl Default for AtmosphereVolume {
    fn default() -> Self {
        Self {
            radius: 220.0,
            priority: 0,
        }
    }
}

/// Presentation defaults for actor silhouettes, animation sets, and selection affordances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorPresentation {
    pub profile_id: String,
    pub mesh_asset_id: Option<String>,
    pub material_palette_id: String,
    pub animation_set_id: String,
    pub scale_multiplier: f32,
    pub footprint_radius: f32,
    pub selection_ring_scale: f32,
    pub aura_color: [f32; 4],
}

impl Default for ActorPresentation {
    fn default() -> Self {
        Self {
            profile_id: "default-actor".to_string(),
            mesh_asset_id: None,
            material_palette_id: "default".to_string(),
            animation_set_id: "humanoid-explorer".to_string(),
            scale_multiplier: 1.0,
            footprint_radius: 1.0,
            selection_ring_scale: 2.2,
            aura_color: [0.0, 0.0, 0.0, 0.0],
        }
    }
}

/// Reusable combat readability defaults for hit flashes, ring colors, and impact sizing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatPresentation {
    pub profile_id: String,
    pub hit_flash_color: [f32; 4],
    pub critical_ring_color: [f32; 4],
    pub selection_ring_color: [f32; 4],
    pub emissive_boost: [f32; 3],
    pub impact_scale: f32,
}

impl Default for CombatPresentation {
    fn default() -> Self {
        Self {
            profile_id: "default-combat".to_string(),
            hit_flash_color: [0.92, 0.34, 0.30, 0.22],
            critical_ring_color: [0.92, 0.34, 0.30, 0.22],
            selection_ring_color: [0.62, 0.98, 0.84, 0.34],
            emissive_boost: [0.08, 0.06, 0.02],
            impact_scale: 1.0,
        }
    }
}

/// Relationship of an entity to an authored faction or social group.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FactionDisposition {
    Friendly,
    #[default]
    Neutral,
    Hostile,
}

/// Authored faction context used for NPCs, creatures, props, and quest hubs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactionAffiliation {
    pub faction_id: String,
    pub role_id: String,
    pub disposition: FactionDisposition,
    pub influence_radius: f32,
}

impl Default for FactionAffiliation {
    fn default() -> Self {
        Self {
            faction_id: "neutral-world".to_string(),
            role_id: "wanderer".to_string(),
            disposition: FactionDisposition::Neutral,
            influence_radius: 0.0,
        }
    }
}

/// World-authored quest hooks attached to NPCs, props, and landmarks.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuestAnchor {
    pub quest_ids: Vec<String>,
    pub primary_prompt: String,
    pub stage_tags: Vec<String>,
}

/// Encounter-table identity and tuning for authored creatures and regions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterProfile {
    pub table_id: String,
    pub difficulty_tier: u8,
    pub recommended_party_size: u8,
    pub respawn_ticks: u32,
}

impl Default for EncounterProfile {
    fn default() -> Self {
        Self {
            table_id: "ambient-encounter".to_string(),
            difficulty_tier: 1,
            recommended_party_size: 1,
            respawn_ticks: 60 * 45,
        }
    }
}

/// Spawn-table identity for authored fauna, resources, and region population groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpawnProfile {
    pub profile_id: String,
    pub biome_id: String,
    pub spawn_group: String,
    pub respawn_ticks: u32,
    pub leash_radius: f32,
}

impl Default for SpawnProfile {
    fn default() -> Self {
        Self {
            profile_id: "ambient-spawn".to_string(),
            biome_id: "neutral-shard".to_string(),
            spawn_group: "ambient".to_string(),
            respawn_ticks: 60 * 30,
            leash_radius: 18.0,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Team {
    #[default]
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CombatStyle {
    #[default]
    Melee,
    Ranged,
    Magic,
    Summoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatLoadout {
    pub style: CombatStyle,
    pub attack_range: f32,
    pub attack_speed_ticks: u32,
    pub max_hit: f32,
    pub auto_retaliate: bool,
    pub equipped_weapon: Option<String>,
    pub offhand_item: Option<String>,
    pub active_ability_bar: Vec<String>,
}

impl Default for CombatLoadout {
    fn default() -> Self {
        Self {
            style: CombatStyle::Melee,
            attack_range: 80.0,
            attack_speed_ticks: 30,
            max_hit: 10.0,
            auto_retaliate: true,
            equipped_weapon: Some("bronze-sword".to_string()),
            offhand_item: None,
            active_ability_bar: vec!["slash".to_string(), "kick".to_string()],
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillKind {
    #[default]
    Attack,
    Strength,
    Defence,
    Ranged,
    Magic,
    Constitution,
    Mining,
    Woodcutting,
    Fishing,
    Cooking,
    Smithing,
    Crafting,
    Slayer,
    Taming,
    Bonding,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SkillProgress {
    pub kind: SkillKind,
    pub level: u16,
    pub experience: u32,
    pub xp_to_next_level: u32,
}

impl SkillProgress {
    pub fn new(kind: SkillKind, level: u16, experience: u32, xp_to_next_level: u32) -> Self {
        Self {
            kind,
            level,
            experience,
            xp_to_next_level,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillBook {
    pub combat_level: u16,
    pub total_level: u16,
    pub skills: Vec<SkillProgress>,
}

impl Default for SkillBook {
    fn default() -> Self {
        let skills = vec![
            SkillProgress::new(SkillKind::Attack, 1, 0, 83),
            SkillProgress::new(SkillKind::Strength, 1, 0, 83),
            SkillProgress::new(SkillKind::Defence, 1, 0, 83),
            SkillProgress::new(SkillKind::Ranged, 1, 0, 83),
            SkillProgress::new(SkillKind::Magic, 1, 0, 83),
            SkillProgress::new(SkillKind::Constitution, 10, 1_154, 1_358),
            SkillProgress::new(SkillKind::Taming, 1, 0, 83),
            SkillProgress::new(SkillKind::Bonding, 1, 0, 83),
        ];
        Self {
            combat_level: 3,
            total_level: skills.iter().map(|skill| skill.level).sum(),
            skills,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub item_id: String,
    pub display_name: String,
    pub quantity: u32,
    pub stackable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Inventory {
    pub capacity: u8,
    pub carried_weight: f32,
    pub coins: u64,
    pub items: Vec<ItemStack>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self {
            capacity: 28,
            carried_weight: 0.0,
            coins: 0,
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceNode {
    pub skill: SkillKind,
    pub tier: u8,
    pub remaining_uses: u32,
    pub respawn_ticks: u32,
    pub experience: u32,
    pub yield_item: ItemStack,
}

impl Default for ResourceNode {
    fn default() -> Self {
        Self {
            skill: SkillKind::Mining,
            tier: 1,
            remaining_uses: 1,
            respawn_ticks: 300,
            experience: 25,
            yield_item: ItemStack {
                item_id: "copper-ore".to_string(),
                display_name: "Copper Ore".to_string(),
                quantity: 1,
                stackable: true,
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LootContainer {
    pub coins: u64,
    pub items: Vec<ItemStack>,
    pub owner: Option<EntityId>,
    pub claimed: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CreatureTemperament {
    Aggressive,
    Timid,
    #[default]
    Neutral,
    Loyal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreatureIdentity {
    pub species_id: String,
    pub species_name: String,
    pub elemental_affinity: String,
    pub level: u16,
    pub temperament: CreatureTemperament,
    pub capture_difficulty: f32,
    pub is_wild: bool,
}

impl Default for CreatureIdentity {
    fn default() -> Self {
        Self {
            species_id: String::new(),
            species_name: String::new(),
            elemental_affinity: "neutral".to_string(),
            level: 1,
            temperament: CreatureTemperament::Neutral,
            capture_difficulty: 0.5,
            is_wild: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionCreature {
    pub creature: CreatureIdentity,
    pub nickname: Option<String>,
    pub current_health: f32,
    pub max_health: f32,
    pub combat_style: CombatStyle,
    pub mood: f32,
}

impl Default for CompanionCreature {
    fn default() -> Self {
        Self {
            creature: CreatureIdentity::default(),
            nickname: None,
            current_health: 10.0,
            max_health: 10.0,
            combat_style: CombatStyle::Summoning,
            mood: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompanionRoster {
    pub active_slot: Option<u8>,
    pub party_capacity: u8,
    pub creatures: Vec<CompanionCreature>,
}

impl Default for CompanionRoster {
    fn default() -> Self {
        Self {
            active_slot: None,
            party_capacity: 6,
            creatures: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EncounterKind {
    #[default]
    OpenWorld,
    Duel,
    WildCreature,
    Boss,
    Raid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterState {
    pub encounter_id: u64,
    pub kind: EncounterKind,
    pub threat_level: f32,
    pub primary_target: Option<EntityId>,
    pub active_turn_owner: Option<EntityId>,
    pub capture_allowed: bool,
    pub in_combat: bool,
}

impl Default for EncounterState {
    fn default() -> Self {
        Self {
            encounter_id: 0,
            kind: EncounterKind::OpenWorld,
            threat_level: 0.0,
            primary_target: None,
            active_turn_owner: None,
            capture_allowed: false,
            in_combat: false,
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
    pub source: String, // script asset key
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
