use crate::action::{Action, AgentAction};
use crate::agent::{Agent, AgentSlot};
use crate::component::*;
use crate::contract::{WorldChunkDefinition, WorldRegionDefinition};
use crate::event::EventBus;
use crate::id::AgentId;
use crate::telemetry::TickTelemetryFrame;
use crate::tick::{execute_tick, TickResult};
use glam::Vec2;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// The complete game world.
///
/// Contains all entities, agents, events, and simulation state.
/// This struct is platform-agnostic — it knows nothing about
/// rendering, input devices, or networking.
pub struct World {
    /// Entity Component System — all game entities and their data
    pub ecs: hecs::World,
    /// Event bus — collects and distributes game events
    pub events: EventBus,
    /// All connected agents (human + AI)
    pub agents: Vec<AgentSlot>,
    /// Current tick number
    pub tick: u64,
    /// Deterministic RNG (seeded, reproducible)
    pub rng: ChaCha8Rng,
    /// Next entity ID counter
    next_entity_id: u64,
    /// Whether the simulation is paused
    pub paused: bool,
    /// Authored streamed-world metadata used by authoritative snapshot and
    /// tooling surfaces.
    pub streaming: WorldStreamingMetadata,
    /// Externally submitted actions (e.g., from network clients).
    external_actions: Vec<AgentAction>,
}

#[derive(Debug, Clone, Default)]
pub struct WorldStreamingMetadata {
    pub chunk_size: f32,
    pub chunks: Vec<WorldChunkDefinition>,
    pub regions: Vec<WorldRegionDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorldChunkMetadata {
    pub chunk_key: String,
    pub region_id: Option<String>,
    pub region_name: Option<String>,
    pub quest_graph_ids: Vec<String>,
    pub faction_track_id: Option<String>,
    pub encounter_table_id: Option<String>,
}

impl WorldStreamingMetadata {
    pub fn new(chunk_size: f32) -> Self {
        Self {
            chunk_size: chunk_size.max(0.001),
            chunks: Vec::new(),
            regions: Vec::new(),
        }
    }

    pub fn resolve_position(&self, position: Vec2) -> ResolvedWorldChunkMetadata {
        let chunk_key = self.chunk_key_for_position(position);
        let chunk = self
            .chunks
            .iter()
            .find(|chunk| chunk.chunk_key == chunk_key);
        let region = chunk
            .and_then(|chunk| {
                self.regions
                    .iter()
                    .find(|region| region.region_id == chunk.region_id)
            })
            .or_else(|| {
                self.regions
                    .iter()
                    .find(|region| region.chunk_keys.iter().any(|value| value == &chunk_key))
            });

        let quest_graph_ids = if let Some(chunk) = chunk {
            if chunk.quest_graph_ids.is_empty() {
                region
                    .map(|region| region.active_quest_graph_ids.clone())
                    .unwrap_or_default()
            } else {
                chunk.quest_graph_ids.clone()
            }
        } else {
            region
                .map(|region| region.active_quest_graph_ids.clone())
                .unwrap_or_default()
        };

        let faction_track_id = chunk
            .and_then(|chunk| chunk.faction_track_ids.first().cloned())
            .or_else(|| {
                region.and_then(|region| {
                    (!region.dominant_faction_track_id.is_empty())
                        .then(|| region.dominant_faction_track_id.clone())
                })
            });

        let encounter_table_id = chunk
            .and_then(|chunk| chunk.encounter_table_ids.first().cloned())
            .or_else(|| region.and_then(|region| region.encounter_table_ids.first().cloned()));

        ResolvedWorldChunkMetadata {
            chunk_key,
            region_id: region.map(|region| region.region_id.clone()),
            region_name: region.map(|region| region.display_name.clone()),
            quest_graph_ids,
            faction_track_id,
            encounter_table_id,
        }
    }

    fn chunk_key_for_position(&self, position: Vec2) -> String {
        let chunk_size = self.chunk_size.max(0.001);
        let chunk_x = (position.x / chunk_size).floor() as i32;
        let chunk_y = (position.y / chunk_size).floor() as i32;
        format!("{chunk_x}:{chunk_y}")
    }
}

impl World {
    /// Create a new empty world with a deterministic seed
    pub fn new(seed: u64) -> Self {
        Self {
            ecs: hecs::World::new(),
            events: EventBus::new(),
            agents: Vec::new(),
            tick: 0,
            rng: ChaCha8Rng::seed_from_u64(seed),
            next_entity_id: 1,
            paused: false,
            streaming: WorldStreamingMetadata::new(8.0),
            external_actions: Vec::new(),
        }
    }

    pub fn set_streaming_metadata(
        &mut self,
        chunk_size: f32,
        chunks: Vec<WorldChunkDefinition>,
        regions: Vec<WorldRegionDefinition>,
    ) {
        self.streaming = WorldStreamingMetadata {
            chunk_size: chunk_size.max(0.001),
            chunks,
            regions,
        };
    }

    pub fn resolve_streaming_metadata(&self, position: Vec2) -> ResolvedWorldChunkMetadata {
        self.streaming.resolve_position(position)
    }

    /// Advance the simulation by one tick
    pub fn step(&mut self) -> TickResult {
        if self.paused {
            return TickResult {
                tick: self.tick,
                events: vec![],
                entity_count: self.ecs.len() as usize,
                actions_processed: 0,
                actions_rejected: 0,
                telemetry: TickTelemetryFrame::empty(self.tick),
            };
        }

        let result = execute_tick(
            &mut self.ecs,
            &mut self.agents,
            &mut self.events,
            self.tick,
            std::mem::take(&mut self.external_actions),
            &mut self.next_entity_id,
        );
        self.tick += 1;
        result
    }

    /// Run N ticks
    pub fn step_n(&mut self, n: u64) -> Vec<TickResult> {
        (0..n).map(|_| self.step()).collect()
    }

    // ========================================
    // AGENT MANAGEMENT
    // ========================================

    /// Add an agent to the world and spawn their controlled entity
    pub fn add_agent(&mut self, agent: Box<dyn Agent>) -> (usize, hecs::Entity) {
        let entity = self.spawn_agent_entity();
        let entity_id = crate::id::EntityId(entity.id() as u64);
        let slot_index = self.agents.len();
        let mut slot = AgentSlot::new(agent);
        slot.entity_id = Some(entity);
        slot.agent.on_join();
        slot.agent.on_spawn(entity_id);
        self.agents.push(slot);
        (slot_index, entity)
    }

    /// Spawn the default entity for an agent
    fn spawn_agent_entity(&mut self) -> hecs::Entity {
        self.ecs.spawn((
            Transform::at(0.0, 0.0),
            Velocity::default(),
            Movement::default(),
            Collider::circle(16.0),
            Health::new(100.0),
            Perception::default(),
            CombatLoadout::default(),
            SkillBook::default(),
            Inventory::default(),
            CompanionRoster::default(),
            ColorRect::new(32.0, 32.0, [0.2, 0.8, 0.3, 1.0]),
            Label {
                name: "Player".into(),
                team: Team::None,
            },
        ))
    }

    /// Remove an agent by index
    pub fn remove_agent(&mut self, index: usize) {
        if index < self.agents.len() {
            let slot = &mut self.agents[index];
            slot.agent.on_leave();
            if let Some(entity) = slot.entity_id {
                let _ = self.ecs.despawn(entity);
            }
            self.agents.remove(index);
        }
    }

    // ========================================
    // ENTITY MANAGEMENT
    // ========================================

    /// Spawn a basic entity at a position
    pub fn spawn_at(&mut self, x: f32, y: f32) -> EntityBuilder<'_> {
        EntityBuilder {
            world: self,
            transform: Transform::at(x, y),
            components: Vec::new(),
        }
    }

    /// Get entity count
    pub fn entity_count(&self) -> usize {
        self.ecs.len() as usize
    }

    /// Get agent count
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Queue an externally sourced action for execution on the next world step.
    ///
    /// This is used by authoritative network/server paths where actions are
    /// submitted outside the local `Agent::decide` loop.
    pub fn submit_external_action(&mut self, agent_id: AgentId, action: Action) {
        self.external_actions.push(AgentAction {
            agent_id,
            tick: self.tick,
            action,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_streaming_metadata_resolves_chunk_and_region_context() {
        let mut world = World::new(9);

        let mut heart_chunk = WorldChunkDefinition::new("0:0", "verdant-heart", "verdant-hollow");
        heart_chunk.quest_graph_ids.push("verdant-intro".into());
        heart_chunk.faction_track_ids.push("verdant-wardens".into());
        heart_chunk
            .encounter_table_ids
            .push("verdant-heart-wildlife".into());

        let mut spire_chunk = WorldChunkDefinition::new("0:1", "spirewatch", "spirewatch");
        spire_chunk
            .encounter_table_ids
            .push("spirewatch-encounters".into());

        let mut heart_region =
            WorldRegionDefinition::new("verdant-heart", "Verdant Heart", "verdant-hollow");
        heart_region.chunk_keys.push("0:0".into());
        heart_region
            .active_quest_graph_ids
            .push("verdant-intro".into());
        heart_region.dominant_faction_track_id = "verdant-wardens".into();
        heart_region
            .encounter_table_ids
            .push("verdant-heart-wildlife".into());

        let mut spire_region = WorldRegionDefinition::new("spirewatch", "Spirewatch", "spirewatch");
        spire_region.chunk_keys.push("0:1".into());
        spire_region
            .active_quest_graph_ids
            .push("spire-attunement".into());
        spire_region.dominant_faction_track_id = "ancient-spirekeepers".into();
        spire_region
            .encounter_table_ids
            .push("spirewatch-encounters".into());

        world.set_streaming_metadata(
            8.0,
            vec![heart_chunk, spire_chunk],
            vec![heart_region, spire_region],
        );

        let heart = world.resolve_streaming_metadata(Vec2::new(2.0, 3.0));
        assert_eq!(heart.chunk_key, "0:0");
        assert_eq!(heart.region_id.as_deref(), Some("verdant-heart"));
        assert_eq!(heart.region_name.as_deref(), Some("Verdant Heart"));
        assert_eq!(heart.quest_graph_ids, vec!["verdant-intro".to_string()]);
        assert_eq!(heart.faction_track_id.as_deref(), Some("verdant-wardens"));
        assert_eq!(
            heart.encounter_table_id.as_deref(),
            Some("verdant-heart-wildlife")
        );

        let spire = world.resolve_streaming_metadata(Vec2::new(1.0, 8.2));
        assert_eq!(spire.chunk_key, "0:1");
        assert_eq!(spire.region_id.as_deref(), Some("spirewatch"));
        assert_eq!(spire.region_name.as_deref(), Some("Spirewatch"));
        assert_eq!(
            spire.encounter_table_id.as_deref(),
            Some("spirewatch-encounters")
        );
        assert_eq!(
            spire.faction_track_id.as_deref(),
            Some("ancient-spirekeepers")
        );
    }
}

// ========================================
// ENTITY BUILDER (fluent API)
// ========================================

/// Builder for spawning entities with a fluent API
pub struct EntityBuilder<'w> {
    world: &'w mut World,
    transform: Transform,
    components: Vec<ComponentToAdd>,
}

enum ComponentToAdd {
    Velocity(Velocity),
    RigidBody(RigidBody),
    Collider(Collider),
    Health(Health),
    Sprite(Sprite),
    ColorRect(ColorRect),
    AtmosphereProfile(AtmosphereProfile),
    AtmosphereVolume(AtmosphereVolume),
    Label(Label),
    Perception(Perception),
    Movement(Movement),
    Script(Script),
    CombatLoadout(CombatLoadout),
    ActorPresentation(ActorPresentation),
    CombatPresentation(CombatPresentation),
    FactionAffiliation(FactionAffiliation),
    QuestAnchor(QuestAnchor),
    EncounterProfile(EncounterProfile),
    SpawnProfile(SpawnProfile),
    SkillBook(SkillBook),
    Inventory(Inventory),
    CreatureIdentity(CreatureIdentity),
    CompanionRoster(CompanionRoster),
    EncounterState(EncounterState),
    ResourceNode(ResourceNode),
    LootContainer(LootContainer),
}

impl<'w> EntityBuilder<'w> {
    pub fn with_velocity(mut self, vx: f32, vy: f32) -> Self {
        self.components.push(ComponentToAdd::Velocity(Velocity {
            linear: Vec2::new(vx, vy),
            angular: 0.0,
        }));
        self
    }

    pub fn with_collider(mut self, collider: Collider) -> Self {
        self.components.push(ComponentToAdd::Collider(collider));
        self
    }

    pub fn with_health(mut self, max: f32) -> Self {
        self.components
            .push(ComponentToAdd::Health(Health::new(max)));
        self
    }

    pub fn with_color(mut self, w: f32, h: f32, color: [f32; 4]) -> Self {
        self.components
            .push(ComponentToAdd::ColorRect(ColorRect::new(w, h, color)));
        self
    }

    pub fn with_label(mut self, name: &str, team: Team) -> Self {
        self.components.push(ComponentToAdd::Label(Label {
            name: name.to_string(),
            team,
        }));
        self
    }

    pub fn with_atmosphere_profile(mut self, atmosphere: AtmosphereProfile) -> Self {
        self.components
            .push(ComponentToAdd::AtmosphereProfile(atmosphere));
        self
    }

    pub fn with_atmosphere_volume(mut self, volume: AtmosphereVolume) -> Self {
        self.components
            .push(ComponentToAdd::AtmosphereVolume(volume));
        self
    }

    pub fn with_movement(mut self, max_speed: f32) -> Self {
        self.components.push(ComponentToAdd::Movement(Movement {
            max_speed,
            ..Default::default()
        }));
        self
    }

    pub fn with_perception(mut self, vision_range: f32) -> Self {
        self.components.push(ComponentToAdd::Perception(Perception {
            vision_range,
            ..Default::default()
        }));
        self
    }

    pub fn with_rigidbody(mut self, body: RigidBody) -> Self {
        self.components.push(ComponentToAdd::RigidBody(body));
        self
    }

    pub fn with_sprite(mut self, sprite: Sprite) -> Self {
        self.components.push(ComponentToAdd::Sprite(sprite));
        self
    }

    pub fn with_script(mut self, script: Script) -> Self {
        self.components.push(ComponentToAdd::Script(script));
        self
    }

    pub fn with_combat_loadout(mut self, loadout: CombatLoadout) -> Self {
        self.components.push(ComponentToAdd::CombatLoadout(loadout));
        self
    }

    pub fn with_actor_presentation(mut self, presentation: ActorPresentation) -> Self {
        self.components
            .push(ComponentToAdd::ActorPresentation(presentation));
        self
    }

    pub fn with_combat_presentation(mut self, presentation: CombatPresentation) -> Self {
        self.components
            .push(ComponentToAdd::CombatPresentation(presentation));
        self
    }

    pub fn with_faction_affiliation(mut self, faction: FactionAffiliation) -> Self {
        self.components
            .push(ComponentToAdd::FactionAffiliation(faction));
        self
    }

    pub fn with_quest_anchor(mut self, quest_anchor: QuestAnchor) -> Self {
        self.components
            .push(ComponentToAdd::QuestAnchor(quest_anchor));
        self
    }

    pub fn with_encounter_profile(mut self, encounter: EncounterProfile) -> Self {
        self.components
            .push(ComponentToAdd::EncounterProfile(encounter));
        self
    }

    pub fn with_spawn_profile(mut self, spawn: SpawnProfile) -> Self {
        self.components.push(ComponentToAdd::SpawnProfile(spawn));
        self
    }

    pub fn with_skill_book(mut self, skill_book: SkillBook) -> Self {
        self.components.push(ComponentToAdd::SkillBook(skill_book));
        self
    }

    pub fn with_inventory(mut self, inventory: Inventory) -> Self {
        self.components.push(ComponentToAdd::Inventory(inventory));
        self
    }

    pub fn with_creature_identity(mut self, creature: CreatureIdentity) -> Self {
        self.components
            .push(ComponentToAdd::CreatureIdentity(creature));
        self
    }

    pub fn with_companion_roster(mut self, roster: CompanionRoster) -> Self {
        self.components
            .push(ComponentToAdd::CompanionRoster(roster));
        self
    }

    pub fn with_encounter_state(mut self, encounter: EncounterState) -> Self {
        self.components
            .push(ComponentToAdd::EncounterState(encounter));
        self
    }

    pub fn with_resource_node(mut self, resource: ResourceNode) -> Self {
        self.components.push(ComponentToAdd::ResourceNode(resource));
        self
    }

    pub fn with_loot_container(mut self, loot: LootContainer) -> Self {
        self.components.push(ComponentToAdd::LootContainer(loot));
        self
    }

    /// Finalize and spawn the entity
    pub fn build(self) -> hecs::Entity {
        let entity = self.world.ecs.spawn((self.transform, Velocity::default()));

        for component in self.components {
            match component {
                ComponentToAdd::Velocity(v) => {
                    self.world.ecs.insert_one(entity, v).unwrap();
                }
                ComponentToAdd::RigidBody(rb) => {
                    self.world.ecs.insert_one(entity, rb).unwrap();
                }
                ComponentToAdd::Collider(c) => {
                    self.world.ecs.insert_one(entity, c).unwrap();
                }
                ComponentToAdd::Health(h) => {
                    self.world.ecs.insert_one(entity, h).unwrap();
                }
                ComponentToAdd::Sprite(s) => {
                    self.world.ecs.insert_one(entity, s).unwrap();
                }
                ComponentToAdd::ColorRect(cr) => {
                    self.world.ecs.insert_one(entity, cr).unwrap();
                }
                ComponentToAdd::AtmosphereProfile(atmosphere) => {
                    self.world.ecs.insert_one(entity, atmosphere).unwrap();
                }
                ComponentToAdd::AtmosphereVolume(volume) => {
                    self.world.ecs.insert_one(entity, volume).unwrap();
                }
                ComponentToAdd::Label(l) => {
                    self.world.ecs.insert_one(entity, l).unwrap();
                }
                ComponentToAdd::Perception(p) => {
                    self.world.ecs.insert_one(entity, p).unwrap();
                }
                ComponentToAdd::Movement(m) => {
                    self.world.ecs.insert_one(entity, m).unwrap();
                }
                ComponentToAdd::Script(s) => {
                    self.world.ecs.insert_one(entity, s).unwrap();
                }
                ComponentToAdd::CombatLoadout(loadout) => {
                    self.world.ecs.insert_one(entity, loadout).unwrap();
                }
                ComponentToAdd::ActorPresentation(presentation) => {
                    self.world.ecs.insert_one(entity, presentation).unwrap();
                }
                ComponentToAdd::CombatPresentation(presentation) => {
                    self.world.ecs.insert_one(entity, presentation).unwrap();
                }
                ComponentToAdd::FactionAffiliation(faction) => {
                    self.world.ecs.insert_one(entity, faction).unwrap();
                }
                ComponentToAdd::QuestAnchor(quest_anchor) => {
                    self.world.ecs.insert_one(entity, quest_anchor).unwrap();
                }
                ComponentToAdd::EncounterProfile(encounter) => {
                    self.world.ecs.insert_one(entity, encounter).unwrap();
                }
                ComponentToAdd::SpawnProfile(spawn) => {
                    self.world.ecs.insert_one(entity, spawn).unwrap();
                }
                ComponentToAdd::SkillBook(skill_book) => {
                    self.world.ecs.insert_one(entity, skill_book).unwrap();
                }
                ComponentToAdd::Inventory(inventory) => {
                    self.world.ecs.insert_one(entity, inventory).unwrap();
                }
                ComponentToAdd::CreatureIdentity(creature) => {
                    self.world.ecs.insert_one(entity, creature).unwrap();
                }
                ComponentToAdd::CompanionRoster(roster) => {
                    self.world.ecs.insert_one(entity, roster).unwrap();
                }
                ComponentToAdd::EncounterState(encounter) => {
                    self.world.ecs.insert_one(entity, encounter).unwrap();
                }
                ComponentToAdd::ResourceNode(resource) => {
                    self.world.ecs.insert_one(entity, resource).unwrap();
                }
                ComponentToAdd::LootContainer(loot) => {
                    self.world.ecs.insert_one(entity, loot).unwrap();
                }
            }
        }

        entity
    }
}
