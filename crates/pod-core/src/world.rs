use crate::action::{Action, AgentAction};
use crate::agent::{Agent, AgentSlot};
use crate::component::*;
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
    /// Externally submitted actions (e.g., from network clients).
    external_actions: Vec<AgentAction>,
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
            external_actions: Vec::new(),
        }
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
    Label(Label),
    Perception(Perception),
    Movement(Movement),
    Script(Script),
    CombatLoadout(CombatLoadout),
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
