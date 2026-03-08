use crate::action::{Action, AgentAction};
use crate::agent::{Agent, AgentSlot};
use crate::component::*;
use crate::contract::{
    EncounterSpawnEntry, RegionEncounterTable, WorldChunkDefinition, WorldRegionDefinition,
};
use crate::event::EventBus;
use crate::id::AgentId;
use crate::telemetry::TickTelemetryFrame;
use crate::tick::{execute_tick, TickResult};
use crate::toon::encode_toon_document;
use glam::Vec2;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    /// Previously live authored streaming slots, tracked so despawns can be
    /// converted into deterministic respawn deadlines instead of immediate
    /// refills.
    streaming_live_slots: HashSet<String>,
    /// Per-slot respawn gates for streamed authored populations.
    streaming_respawn_deadlines: HashMap<String, u64>,
    /// Externally submitted actions (e.g., from network clients).
    external_actions: Vec<AgentAction>,
}

#[derive(Debug, Clone, Default)]
pub struct WorldStreamingMetadata {
    pub chunk_size: f32,
    pub chunks: Vec<WorldChunkDefinition>,
    pub regions: Vec<WorldRegionDefinition>,
    pub encounter_tables: Vec<RegionEncounterTable>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorldChunkMetadata {
    pub chunk_key: String,
    pub region_id: Option<String>,
    pub region_name: Option<String>,
    pub biome_id: Option<String>,
    pub quest_graph_ids: Vec<String>,
    pub faction_track_id: Option<String>,
    pub encounter_table_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PopulationBreakdown {
    pub players: u32,
    pub npcs: u32,
    pub wild_creatures: u32,
    pub companions: u32,
    pub resource_nodes: u32,
    pub loot_containers: u32,
    pub scenery: u32,
}

impl PopulationBreakdown {
    fn increment(&mut self, kind: PopulationKind) {
        match kind {
            PopulationKind::Player => self.players += 1,
            PopulationKind::Npc => self.npcs += 1,
            PopulationKind::WildCreature => self.wild_creatures += 1,
            PopulationKind::Companion => self.companions += 1,
            PopulationKind::ResourceNode => self.resource_nodes += 1,
            PopulationKind::LootContainer => self.loot_containers += 1,
            PopulationKind::Scenery => self.scenery += 1,
        }
    }

    fn total(self) -> u32 {
        self.players
            + self.npcs
            + self.wild_creatures
            + self.companions
            + self.resource_nodes
            + self.loot_containers
            + self.scenery
    }

    fn active_spawned_actors(self) -> u32 {
        self.players + self.npcs + self.wild_creatures + self.companions
    }

    fn merge(&mut self, other: Self) {
        self.players += other.players;
        self.npcs += other.npcs;
        self.wild_creatures += other.wild_creatures;
        self.companions += other.companions;
        self.resource_nodes += other.resource_nodes;
        self.loot_containers += other.loot_containers;
        self.scenery += other.scenery;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChunkPopulationState {
    pub chunk_key: String,
    pub region_id: Option<String>,
    pub region_name: Option<String>,
    pub biome_id: Option<String>,
    pub quest_graph_ids: Vec<String>,
    pub faction_track_id: Option<String>,
    pub encounter_table_ids: Vec<String>,
    pub counts: PopulationBreakdown,
    pub active_entity_count: u32,
    pub ambient_population_cap: u32,
    pub spawn_budget_remaining: u32,
    pub pending_respawns: u32,
    pub next_respawn_tick: Option<u64>,
    pub population_pressure: f32,
}

impl ChunkPopulationState {
    fn refresh_metrics(&mut self) {
        self.active_entity_count = self.counts.total();
        let active_spawned = self.counts.active_spawned_actors();
        self.spawn_budget_remaining = self.ambient_population_cap.saturating_sub(active_spawned);
        self.population_pressure = if self.ambient_population_cap == 0 {
            0.0
        } else {
            active_spawned as f32 / self.ambient_population_cap as f32
        };
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RegionPopulationState {
    pub region_id: String,
    pub region_name: String,
    pub primary_biome_id: String,
    pub chunk_keys: Vec<String>,
    pub active_quest_graph_ids: Vec<String>,
    pub dominant_faction_track_id: Option<String>,
    pub encounter_table_ids: Vec<String>,
    pub active_chunk_count: u32,
    pub counts: PopulationBreakdown,
    pub active_entity_count: u32,
    pub ambient_population_cap: u32,
    pub spawn_budget_remaining: u32,
    pub pending_respawns: u32,
    pub next_respawn_tick: Option<u64>,
    pub population_pressure: f32,
}

impl RegionPopulationState {
    fn refresh_metrics(&mut self) {
        self.active_entity_count = self.counts.total();
        let active_spawned = self.counts.active_spawned_actors();
        self.spawn_budget_remaining = self.ambient_population_cap.saturating_sub(active_spawned);
        self.population_pressure = if self.ambient_population_cap == 0 {
            0.0
        } else {
            active_spawned as f32 / self.ambient_population_cap as f32
        };
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct WorldPopulationState {
    pub tick: u64,
    pub chunks: Vec<ChunkPopulationState>,
    pub regions: Vec<RegionPopulationState>,
}

impl WorldPopulationState {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("world_population_state", self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopulationKind {
    Player,
    Npc,
    WildCreature,
    Companion,
    ResourceNode,
    LootContainer,
    Scenery,
}

#[derive(Debug, Clone)]
struct StreamingSpawnRequest {
    slot_key: String,
    chunk_key: String,
    biome_id: String,
    faction_track_id: Option<String>,
    encounter_table_id: String,
    spawn_group: String,
    archetype_id: String,
    slot_index: u32,
    respawn_ticks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamingArchetypeKind {
    WildCreature,
    ResourceNode,
    LootContainer,
    Npc,
}

impl WorldStreamingMetadata {
    pub fn new(chunk_size: f32) -> Self {
        Self {
            chunk_size: chunk_size.max(0.001),
            chunks: Vec::new(),
            regions: Vec::new(),
            encounter_tables: Vec::new(),
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
            biome_id: chunk
                .map(|chunk| chunk.biome_id.clone())
                .or_else(|| region.map(|region| region.primary_biome_id.clone())),
            quest_graph_ids,
            faction_track_id,
            encounter_table_id,
        }
    }

    fn chunk_definition(&self, chunk_key: &str) -> Option<&WorldChunkDefinition> {
        self.chunks.iter().find(|chunk| chunk.chunk_key == chunk_key)
    }

    fn region_definition(&self, region_id: &str) -> Option<&WorldRegionDefinition> {
        self.regions.iter().find(|region| region.region_id == region_id)
    }

    fn encounter_table(&self, table_id: &str) -> Option<&RegionEncounterTable> {
        self.encounter_tables
            .iter()
            .find(|table| table.table_id == table_id)
    }

    fn encounter_table_cap(&self, table_id: &str) -> u32 {
        self.encounter_tables
            .iter()
            .find(|table| table.table_id == table_id)
            .map(|table| table.ambient_cap as u32)
            .unwrap_or_default()
    }

    fn ambient_cap_for_tables(&self, table_ids: &[String]) -> u32 {
        table_ids
            .iter()
            .map(|table_id| self.encounter_table_cap(table_id))
            .sum()
    }

    fn chunk_population_seed(&self, chunk_key: &str) -> ChunkPopulationState {
        let chunk = self.chunk_definition(chunk_key);
        let region = chunk
            .and_then(|chunk| self.region_definition(&chunk.region_id))
            .or_else(|| {
                self.regions
                    .iter()
                    .find(|region| region.chunk_keys.iter().any(|value| value == chunk_key))
            });

        let encounter_table_ids = chunk
            .map(|chunk| chunk.encounter_table_ids.clone())
            .filter(|table_ids| !table_ids.is_empty())
            .or_else(|| region.map(|region| region.encounter_table_ids.clone()))
            .unwrap_or_default();
        let quest_graph_ids = chunk
            .map(|chunk| chunk.quest_graph_ids.clone())
            .filter(|quest_ids| !quest_ids.is_empty())
            .or_else(|| region.map(|region| region.active_quest_graph_ids.clone()))
            .unwrap_or_default();
        let faction_track_id = chunk
            .and_then(|chunk| chunk.faction_track_ids.first().cloned())
            .or_else(|| {
                region.and_then(|region| {
                    (!region.dominant_faction_track_id.is_empty())
                        .then(|| region.dominant_faction_track_id.clone())
                })
            });
        let mut state = ChunkPopulationState {
            chunk_key: chunk_key.to_string(),
            region_id: region.map(|region| region.region_id.clone()),
            region_name: region.map(|region| region.display_name.clone()),
            biome_id: chunk
                .map(|chunk| chunk.biome_id.clone())
                .or_else(|| region.map(|region| region.primary_biome_id.clone())),
            quest_graph_ids,
            faction_track_id,
            ambient_population_cap: self.ambient_cap_for_tables(&encounter_table_ids),
            encounter_table_ids,
            ..Default::default()
        };
        state.refresh_metrics();
        state
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
            streaming_live_slots: HashSet::new(),
            streaming_respawn_deadlines: HashMap::new(),
            external_actions: Vec::new(),
        }
    }

    pub fn set_streaming_metadata(
        &mut self,
        chunk_size: f32,
        chunks: Vec<WorldChunkDefinition>,
        regions: Vec<WorldRegionDefinition>,
        encounter_tables: Vec<RegionEncounterTable>,
    ) {
        self.streaming = WorldStreamingMetadata {
            chunk_size: chunk_size.max(0.001),
            chunks,
            regions,
            encounter_tables,
        };
    }

    pub fn resolve_streaming_metadata(&self, position: Vec2) -> ResolvedWorldChunkMetadata {
        self.streaming.resolve_position(position)
    }

    pub fn population_state(&self) -> WorldPopulationState {
        let controlled_entities = self
            .agents
            .iter()
            .filter_map(|slot| slot.entity_id.map(|entity| entity.id() as u64))
            .collect::<HashSet<_>>();
        let mut chunk_states = HashMap::<String, ChunkPopulationState>::new();

        for chunk in &self.streaming.chunks {
            chunk_states.insert(
                chunk.chunk_key.clone(),
                self.streaming.chunk_population_seed(&chunk.chunk_key),
            );
        }
        for region in &self.streaming.regions {
            for chunk_key in &region.chunk_keys {
                chunk_states
                    .entry(chunk_key.clone())
                    .or_insert_with(|| self.streaming.chunk_population_seed(chunk_key));
            }
        }

        for (entity, (transform,)) in self.ecs.query::<(&Transform,)>().iter() {
            let streaming = self.resolve_streaming_metadata(transform.position);
            let kind = classify_population_kind(self, entity, &controlled_entities);
            let chunk_state = chunk_states
                .entry(streaming.chunk_key.clone())
                .or_insert_with(|| self.streaming.chunk_population_seed(&streaming.chunk_key));

            chunk_state.region_id = streaming.region_id.clone();
            chunk_state.region_name = streaming.region_name.clone();
            if chunk_state.biome_id.is_none() {
                chunk_state.biome_id = streaming.biome_id.clone();
            }
            if chunk_state.quest_graph_ids.is_empty() {
                chunk_state.quest_graph_ids = streaming.quest_graph_ids.clone();
            }
            if chunk_state.faction_track_id.is_none() {
                chunk_state.faction_track_id = streaming.faction_track_id.clone();
            }
            if chunk_state.encounter_table_ids.is_empty() {
                if let Some(encounter_table_id) = streaming.encounter_table_id.clone() {
                    chunk_state.encounter_table_ids.push(encounter_table_id);
                    chunk_state.ambient_population_cap =
                        self.streaming.ambient_cap_for_tables(&chunk_state.encounter_table_ids);
                }
            }
            chunk_state.counts.increment(kind);
            chunk_state.refresh_metrics();
        }

        for (slot_key, deadline) in &self.streaming_respawn_deadlines {
            if *deadline <= self.tick {
                continue;
            }
            let Some((chunk_key, _, _)) = parse_streaming_slot_key(slot_key) else {
                continue;
            };
            let chunk_state = chunk_states
                .entry(chunk_key.to_string())
                .or_insert_with(|| self.streaming.chunk_population_seed(chunk_key));
            chunk_state.pending_respawns += 1;
            chunk_state.next_respawn_tick = Some(
                chunk_state
                    .next_respawn_tick
                    .map_or(*deadline, |current| current.min(*deadline)),
            );
        }

        let mut chunks = chunk_states.into_values().collect::<Vec<_>>();
        chunks.sort_by(|left, right| left.chunk_key.cmp(&right.chunk_key));

        let mut regions = self
            .streaming
            .regions
            .iter()
            .map(|region| {
                let mut state = RegionPopulationState {
                    region_id: region.region_id.clone(),
                    region_name: region.display_name.clone(),
                    primary_biome_id: region.primary_biome_id.clone(),
                    chunk_keys: region.chunk_keys.clone(),
                    active_quest_graph_ids: region.active_quest_graph_ids.clone(),
                    dominant_faction_track_id: (!region.dominant_faction_track_id.is_empty())
                        .then(|| region.dominant_faction_track_id.clone()),
                    encounter_table_ids: region.encounter_table_ids.clone(),
                    ambient_population_cap: self
                        .streaming
                        .ambient_cap_for_tables(&region.encounter_table_ids),
                    ..Default::default()
                };
                for chunk in chunks
                    .iter()
                    .filter(|chunk| chunk.region_id.as_deref() == Some(region.region_id.as_str()))
                {
                    if chunk.active_entity_count > 0 {
                        state.active_chunk_count += 1;
                    }
                    state.counts.merge(chunk.counts);
                    state.pending_respawns += chunk.pending_respawns;
                    state.next_respawn_tick = match (state.next_respawn_tick, chunk.next_respawn_tick)
                    {
                        (Some(current), Some(next)) => Some(current.min(next)),
                        (None, Some(next)) => Some(next),
                        (current, None) => current,
                    };
                }
                state.refresh_metrics();
                state
            })
            .collect::<Vec<_>>();
        regions.sort_by(|left, right| left.region_id.cmp(&right.region_id));

        WorldPopulationState {
            tick: self.tick,
            chunks,
            regions,
        }
    }

    pub fn reconcile_streaming_population(&mut self) {
        if self.streaming.chunks.is_empty() && self.streaming.regions.is_empty() {
            return;
        }

        let active_chunks = self.active_streaming_chunk_keys();
        if active_chunks.is_empty() {
            return;
        }

        let slot_specs = self.streaming_slot_specs(&active_chunks);
        let active_slot_keys = slot_specs
            .iter()
            .map(|spec| spec.slot_key.clone())
            .collect::<HashSet<_>>();

        self.streaming_live_slots
            .retain(|slot_key| active_slot_keys.contains(slot_key));
        self.streaming_respawn_deadlines
            .retain(|slot_key, _| active_slot_keys.contains(slot_key));

        let mut live_slot_keys = HashSet::new();
        let mut stale_entities = Vec::new();
        for (entity, (transform, spawn_profile)) in
            self.ecs.query::<(&Transform, &SpawnProfile)>().iter()
        {
            let chunk_key = self.streaming.chunk_key_for_position(transform.position);
            if !active_chunks.contains(&chunk_key) {
                stale_entities.push(entity);
                continue;
            }
            live_slot_keys.insert(spawn_profile.profile_id.clone());
        }

        for entity in stale_entities {
            let _ = self.ecs.despawn(entity);
        }

        let previous_live_slots = self.streaming_live_slots.clone();
        for slot_key in previous_live_slots.difference(&live_slot_keys) {
            if self.streaming_respawn_deadlines.contains_key(slot_key) {
                continue;
            }
            let Some(spec) = slot_specs.iter().find(|spec| spec.slot_key == *slot_key) else {
                continue;
            };
            self.streaming_respawn_deadlines
                .insert(slot_key.clone(), self.tick + spec.respawn_ticks as u64);
        }

        let mut requests = Vec::new();
        for spec in slot_specs {
            if live_slot_keys.contains(&spec.slot_key) {
                self.streaming_respawn_deadlines.remove(&spec.slot_key);
                continue;
            }

            if self
                .streaming_respawn_deadlines
                .get(&spec.slot_key)
                .is_some_and(|deadline| *deadline > self.tick)
            {
                continue;
            }
            self.streaming_respawn_deadlines.remove(&spec.slot_key);
            requests.push(spec);
        }

        let mut next_live_slots = live_slot_keys;
        for request in requests {
            next_live_slots.insert(request.slot_key.clone());
            self.spawn_streaming_request(request);
        }
        self.streaming_live_slots = next_live_slots;
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

        self.reconcile_streaming_population();

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
        self.reconcile_streaming_population();
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

    fn active_streaming_chunk_keys(&self) -> HashSet<String> {
        let mut active = HashSet::new();

        for entity in self.agents.iter().filter_map(|slot| slot.entity_id) {
            let Ok(transform) = self.ecs.get::<&Transform>(entity) else {
                continue;
            };
            let chunk_key = self.streaming.chunk_key_for_position(transform.position);
            active.insert(chunk_key.clone());
            if let Some(chunk) = self.streaming.chunk_definition(&chunk_key) {
                for neighbor in &chunk.neighbor_chunk_keys {
                    active.insert(neighbor.clone());
                }
            }
        }

        if active.is_empty() {
            for chunk in &self.streaming.chunks {
                active.insert(chunk.chunk_key.clone());
            }
        }

        active
    }

    fn streaming_slot_specs(&self, active_chunks: &HashSet<String>) -> Vec<StreamingSpawnRequest> {
        let mut specs = Vec::new();

        for chunk_key in active_chunks {
            let seed = self.streaming.chunk_population_seed(chunk_key);
            for table_id in &seed.encounter_table_ids {
                let Some(table) = self.streaming.encounter_table(table_id) else {
                    continue;
                };
                if table.entries.is_empty() {
                    continue;
                }

                let biome_id = seed
                    .biome_id
                    .clone()
                    .unwrap_or_else(|| table.biome_id.clone());
                for slot_index in 0..table.ambient_cap as u32 {
                    let Some(entry) = choose_streaming_entry(table, chunk_key, slot_index) else {
                        continue;
                    };
                    let respawn_ticks = streaming_respawn_ticks(
                        &entry.archetype_id,
                        &table.spawn_group,
                        entry.max_count,
                    );
                    specs.push(StreamingSpawnRequest {
                        slot_key: encode_streaming_slot_key(chunk_key, &table.table_id, slot_index),
                        chunk_key: chunk_key.clone(),
                        biome_id: biome_id.clone(),
                        faction_track_id: seed.faction_track_id.clone(),
                        encounter_table_id: table.table_id.clone(),
                        spawn_group: table.spawn_group.clone(),
                        archetype_id: entry.archetype_id.clone(),
                        slot_index,
                        respawn_ticks,
                    });
                }
            }
        }

        specs
    }

    fn spawn_streaming_request(&mut self, request: StreamingSpawnRequest) {
        let position = streaming_spawn_position(
            &request.chunk_key,
            self.streaming.chunk_size,
            request.slot_index,
            &request.encounter_table_id,
        );
        let display_name = archetype_display_name(&request.archetype_id);
        let spawn_profile = SpawnProfile {
            profile_id: request.slot_key.clone(),
            biome_id: request.biome_id.clone(),
            spawn_group: request.spawn_group.clone(),
            respawn_ticks: request.respawn_ticks,
            leash_radius: self.streaming.chunk_size * 0.45,
        };
        let faction = request
            .faction_track_id
            .as_ref()
            .map(|faction_id| FactionAffiliation {
                faction_id: faction_id.clone(),
                ..FactionAffiliation::default()
            });

        match classify_streaming_archetype(&request.archetype_id, &request.spawn_group) {
            StreamingArchetypeKind::WildCreature => {
                let level = streaming_creature_level(&request.archetype_id);
                let mut builder = self
                    .spawn_at(position.x, position.y)
                    .with_label(&display_name, Team::Team(2))
                    .with_health(20.0 + level as f32 * 10.0)
                    .with_combat_loadout(CombatLoadout {
                        max_hit: 2.0 + level as f32 * 0.75,
                        attack_speed_ticks: 4,
                        attack_range: 24.0,
                        ..CombatLoadout::default()
                    })
                    .with_creature_identity(CreatureIdentity {
                        species_id: request.archetype_id.clone(),
                        species_name: display_name.clone(),
                        elemental_affinity: request.biome_id.clone(),
                        level,
                        temperament: CreatureTemperament::Neutral,
                        capture_difficulty: 0.4 + level as f32 * 0.05,
                        is_wild: true,
                    })
                    .with_encounter_profile(EncounterProfile {
                        table_id: request.encounter_table_id.clone(),
                        respawn_ticks: request.respawn_ticks,
                        ..EncounterProfile::default()
                    })
                    .with_spawn_profile(spawn_profile)
                    .with_movement(42.0)
                    .with_perception(280.0)
                    .with_color(26.0, 26.0, [0.38, 0.76, 0.42, 1.0]);
                if let Some(faction) = faction {
                    builder = builder.with_faction_affiliation(faction);
                }
                builder.build();
            }
            StreamingArchetypeKind::ResourceNode => {
                let mut builder = self
                    .spawn_at(position.x, position.y)
                    .with_label(&display_name, Team::None)
                    .with_resource_node(streaming_resource_node(&request.archetype_id))
                    .with_spawn_profile(spawn_profile)
                    .with_color(24.0, 24.0, [0.72, 0.58, 0.32, 1.0]);
                if let Some(faction) = faction {
                    builder = builder.with_faction_affiliation(faction);
                }
                builder.build();
            }
            StreamingArchetypeKind::LootContainer => {
                self.spawn_at(position.x, position.y)
                    .with_label(&display_name, Team::None)
                    .with_loot_container(streaming_loot_container(&request.archetype_id))
                    .with_spawn_profile(spawn_profile)
                    .with_color(24.0, 20.0, [0.74, 0.66, 0.32, 1.0])
                    .build();
            }
            StreamingArchetypeKind::Npc => {
                let mut builder = self
                    .spawn_at(position.x, position.y)
                    .with_label(&display_name, Team::Team(1))
                    .with_health(40.0)
                    .with_combat_loadout(CombatLoadout::default())
                    .with_spawn_profile(spawn_profile)
                    .with_movement(36.0)
                    .with_perception(240.0)
                    .with_color(28.0, 28.0, [0.30, 0.56, 0.84, 1.0]);
                if let Some(faction) = faction {
                    builder = builder.with_faction_affiliation(faction);
                }
                builder.build();
            }
        }
    }
}

fn classify_population_kind(
    world: &World,
    entity: hecs::Entity,
    controlled_entities: &HashSet<u64>,
) -> PopulationKind {
    let entity_id = entity.id() as u64;
    let has_resource = world.ecs.get::<&ResourceNode>(entity).is_ok();
    let has_loot = world.ecs.get::<&LootContainer>(entity).is_ok();
    let creature = world.ecs.get::<&CreatureIdentity>(entity).ok();
    let has_combat = world.ecs.get::<&CombatLoadout>(entity).is_ok();
    let has_health = world.ecs.get::<&Health>(entity).is_ok();

    if controlled_entities.contains(&entity_id) {
        return PopulationKind::Player;
    }
    if has_resource {
        return PopulationKind::ResourceNode;
    }
    if has_loot {
        return PopulationKind::LootContainer;
    }
    if let Some(creature) = creature {
        if creature.level > 0 {
            return PopulationKind::WildCreature;
        }
    }
    if world.ecs.get::<&CompanionRoster>(entity).is_ok() {
        return PopulationKind::Companion;
    }
    if has_combat || has_health {
        return PopulationKind::Npc;
    }
    PopulationKind::Scenery
}

fn encode_streaming_slot_key(chunk_key: &str, table_id: &str, slot_index: u32) -> String {
    format!("{chunk_key}|{table_id}|{slot_index}")
}

fn parse_streaming_slot_key(slot_key: &str) -> Option<(&str, &str, u32)> {
    let (chunk_key, remainder) = slot_key.split_once('|')?;
    let (table_id, slot_index) = remainder.rsplit_once('|')?;
    Some((chunk_key, table_id, slot_index.parse().ok()?))
}

fn choose_streaming_entry<'a>(
    table: &'a RegionEncounterTable,
    chunk_key: &str,
    slot_index: u32,
) -> Option<&'a EncounterSpawnEntry> {
    let candidates = table
        .entries
        .iter()
        .filter(|entry| {
            entry.required_stage_tags.is_empty() && entry.required_reputation_tiers.is_empty()
        })
        .collect::<Vec<_>>();
    let candidates = if candidates.is_empty() {
        table.entries.iter().collect::<Vec<_>>()
    } else {
        candidates
    };
    let total_weight = candidates
        .iter()
        .map(|entry| entry.weight.max(1) as u64)
        .sum::<u64>();
    if total_weight == 0 {
        return None;
    }

    let mut cursor = stable_streaming_hash(chunk_key, &table.table_id)
        .wrapping_add(slot_index as u64)
        % total_weight;
    for entry in candidates {
        let weight = entry.weight.max(1) as u64;
        if cursor < weight {
            return Some(entry);
        }
        cursor -= weight;
    }
    table.entries.first()
}

fn stable_streaming_hash(left: &str, right: &str) -> u64 {
    let mut hash = 1469598103934665603_u64;
    for byte in left.bytes().chain([0xff]).chain(right.bytes()) {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn parse_chunk_key(chunk_key: &str) -> Option<(i32, i32)> {
    let (x, y) = chunk_key.split_once(':')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

fn streaming_spawn_position(
    chunk_key: &str,
    chunk_size: f32,
    slot_index: u32,
    encounter_table_id: &str,
) -> Vec2 {
    let (chunk_x, chunk_y) = parse_chunk_key(chunk_key).unwrap_or((0, 0));
    let center = Vec2::new(
        (chunk_x as f32 + 0.5) * chunk_size,
        (chunk_y as f32 + 0.5) * chunk_size,
    );
    let pattern = [
        Vec2::new(-0.24, -0.18),
        Vec2::new(0.21, -0.22),
        Vec2::new(-0.18, 0.16),
        Vec2::new(0.24, 0.14),
        Vec2::new(0.00, -0.04),
        Vec2::new(-0.31, 0.02),
        Vec2::new(0.32, -0.01),
        Vec2::new(0.06, 0.28),
    ];
    let salt = stable_streaming_hash(chunk_key, encounter_table_id) as usize;
    let offset = pattern[(slot_index as usize + salt) % pattern.len()] * chunk_size;
    center + offset
}

fn streaming_respawn_ticks(archetype_id: &str, spawn_group: &str, entry_max_count: u8) -> u32 {
    let normalized = format!(
        "{}:{}",
        archetype_id.to_ascii_lowercase(),
        spawn_group.to_ascii_lowercase()
    );
    if normalized.contains("resource")
        || normalized.contains("vein")
        || normalized.contains("outcrop")
        || normalized.contains("tree")
    {
        60 * 12
    } else if normalized.contains("boss") || normalized.contains("beast") {
        60 * 20
    } else {
        let density_factor = entry_max_count.max(1) as u32;
        60 * (6 + density_factor * 2)
    }
}

fn classify_streaming_archetype(archetype_id: &str, spawn_group: &str) -> StreamingArchetypeKind {
    let normalized = format!(
        "{}:{}",
        archetype_id.to_ascii_lowercase(),
        spawn_group.to_ascii_lowercase()
    );
    if normalized.contains("resource")
        || normalized.contains("vein")
        || normalized.contains("outcrop")
        || normalized.contains("tree")
        || normalized.contains("ore")
    {
        StreamingArchetypeKind::ResourceNode
    } else if normalized.contains("chest")
        || normalized.contains("cache")
        || normalized.contains("crate")
        || normalized.contains("loot")
    {
        StreamingArchetypeKind::LootContainer
    } else if normalized.contains("npc")
        || normalized.contains("warden")
        || normalized.contains("merchant")
    {
        StreamingArchetypeKind::Npc
    } else {
        StreamingArchetypeKind::WildCreature
    }
}

fn archetype_display_name(archetype_id: &str) -> String {
    archetype_id
        .split(['-', '_'])
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => {
                    first.to_ascii_uppercase().to_string() + &chars.as_str().to_ascii_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn streaming_creature_level(archetype_id: &str) -> u16 {
    if archetype_id.contains("alpha") || archetype_id.contains("beast") {
        8
    } else if archetype_id.contains("spirit") || archetype_id.contains("guardian") {
        6
    } else {
        3
    }
}

fn streaming_resource_node(archetype_id: &str) -> ResourceNode {
    let normalized = archetype_id.to_ascii_lowercase();
    if normalized.contains("tree") || normalized.contains("wood") {
        ResourceNode {
            skill: SkillKind::Woodcutting,
            yield_item: ItemStack {
                item_id: "verdant-log".to_string(),
                display_name: "Verdant Log".to_string(),
                quantity: 1,
                stackable: true,
            },
            ..ResourceNode::default()
        }
    } else if normalized.contains("moonstone") {
        ResourceNode {
            tier: 2,
            experience: 42,
            yield_item: ItemStack {
                item_id: "moonstone-shard".to_string(),
                display_name: "Moonstone Shard".to_string(),
                quantity: 1,
                stackable: true,
            },
            ..ResourceNode::default()
        }
    } else {
        ResourceNode {
            yield_item: ItemStack {
                item_id: "copper-ore".to_string(),
                display_name: "Copper Ore".to_string(),
                quantity: 1,
                stackable: true,
            },
            ..ResourceNode::default()
        }
    }
}

fn streaming_loot_container(archetype_id: &str) -> LootContainer {
    LootContainer {
        coins: if archetype_id.contains("expedition") { 64 } else { 18 },
        items: vec![ItemStack {
            item_id: format!("{archetype_id}-salvage"),
            display_name: format!("{} Salvage", archetype_display_name(archetype_id)),
            quantity: 1,
            stackable: true,
        }],
        ..LootContainer::default()
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
            vec![
                RegionEncounterTable::new(
                    "verdant-heart-wildlife",
                    "verdant-hollow",
                    "wildlife",
                    vec![],
                ),
                RegionEncounterTable::new(
                    "spirewatch-encounters",
                    "spirewatch",
                    "spire",
                    vec![],
                ),
            ],
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
        assert_eq!(spire.biome_id.as_deref(), Some("spirewatch"));
    }

    #[test]
    fn world_population_state_tracks_region_and_chunk_density() {
        let mut world = World::new(42);

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

        let mut spire_region =
            WorldRegionDefinition::new("spirewatch", "Spirewatch", "spirewatch");
        spire_region.chunk_keys.push("0:1".into());
        spire_region.dominant_faction_track_id = "ancient-spirekeepers".into();
        spire_region
            .encounter_table_ids
            .push("spirewatch-encounters".into());

        world.set_streaming_metadata(
            8.0,
            vec![heart_chunk, spire_chunk],
            vec![heart_region, spire_region],
            vec![
                RegionEncounterTable {
                    ambient_cap: 6,
                    ..RegionEncounterTable::new(
                        "verdant-heart-wildlife",
                        "verdant-hollow",
                        "wildlife",
                        vec![],
                    )
                },
                RegionEncounterTable {
                    ambient_cap: 4,
                    ..RegionEncounterTable::new(
                        "spirewatch-encounters",
                        "spirewatch",
                        "spire",
                        vec![],
                    )
                },
            ],
        );

        let _tree = world.spawn_at(1.0, 1.0).with_label("Tree", Team::None).build();
        let _ore = world
            .spawn_at(2.0, 1.0)
            .with_label("Ore Vein", Team::None)
            .with_resource_node(ResourceNode {
                skill: SkillKind::Mining,
                tier: 2,
                remaining_uses: 6,
                respawn_ticks: 120,
                experience: 45,
                yield_item: ItemStack {
                    item_id: "iron-ore".into(),
                    display_name: "Iron Ore".into(),
                    quantity: 1,
                    stackable: true,
                },
            })
            .build();
        let _creature = world
            .spawn_at(3.0, 1.0)
            .with_label("Ember Lynx", Team::Team(2))
            .with_health(30.0)
            .with_combat_loadout(CombatLoadout::default())
            .with_creature_identity(CreatureIdentity {
                species_id: "ember-lynx".into(),
                species_name: "Ember Lynx".into(),
                elemental_affinity: "fire".into(),
                temperament: CreatureTemperament::Aggressive,
                level: 7,
                capture_difficulty: 0.72,
                is_wild: true,
            })
            .build();
        let _loot = world
            .spawn_at(2.0, 9.0)
            .with_label("Spire Chest", Team::None)
            .with_loot_container(LootContainer {
                coins: 28,
                items: vec![ItemStack {
                    item_id: "spire-relic".into(),
                    display_name: "Spire Relic".into(),
                    quantity: 1,
                    stackable: false,
                }],
                owner: None,
                claimed: false,
            })
            .build();
        let _ = world.add_agent(Box::new(crate::agent::HumanAgent::new()));

        let population = world.population_state();
        assert_eq!(population.tick, 0);
        assert_eq!(population.regions.len(), 2);
        assert_eq!(population.chunks.len(), 2);

        let heart = population
            .regions
            .iter()
            .find(|region| region.region_id == "verdant-heart")
            .expect("heart region should exist");
        assert_eq!(heart.active_chunk_count, 1);
        assert_eq!(heart.counts.players, 1);
        assert_eq!(heart.counts.wild_creatures, 1);
        assert_eq!(heart.counts.resource_nodes, 1);
        assert_eq!(heart.ambient_population_cap, 6);
        assert_eq!(heart.spawn_budget_remaining, 4);
        assert!(heart.population_pressure > 0.3);

        let spire = population
            .chunks
            .iter()
            .find(|chunk| chunk.chunk_key == "0:1")
            .expect("spire chunk should exist");
        assert_eq!(spire.region_id.as_deref(), Some("spirewatch"));
        assert_eq!(spire.counts.loot_containers, 1);
        assert_eq!(spire.ambient_population_cap, 4);
        assert_eq!(spire.spawn_budget_remaining, 4);
    }

    #[test]
    fn reconcile_streaming_population_spawns_and_rehomes_chunk_density() {
        let mut world = World::new(99);

        let mut heart_chunk = WorldChunkDefinition::new("0:0", "verdant-heart", "verdant-hollow");
        heart_chunk
            .encounter_table_ids
            .push("verdant-heart-wildlife".into());
        let mut steppe_chunk = WorldChunkDefinition::new("1:0", "ashen-steppe", "ashen-steppe");
        steppe_chunk
            .encounter_table_ids
            .push("ashen-steppe-wildlife".into());

        let mut heart_region =
            WorldRegionDefinition::new("verdant-heart", "Verdant Heart", "verdant-hollow");
        heart_region.chunk_keys.push("0:0".into());
        heart_region
            .encounter_table_ids
            .push("verdant-heart-wildlife".into());

        let mut steppe_region =
            WorldRegionDefinition::new("ashen-steppe", "Ashen Steppe", "ashen-steppe");
        steppe_region.chunk_keys.push("1:0".into());
        steppe_region
            .encounter_table_ids
            .push("ashen-steppe-wildlife".into());

        world.set_streaming_metadata(
            8.0,
            vec![heart_chunk, steppe_chunk],
            vec![heart_region, steppe_region],
            vec![
                RegionEncounterTable {
                    ambient_cap: 2,
                    ..RegionEncounterTable::new(
                        "verdant-heart-wildlife",
                        "verdant-hollow",
                        "wildlife",
                        vec![EncounterSpawnEntry {
                            archetype_id: "verdant-lynx".into(),
                            weight: 1,
                            min_count: 1,
                            max_count: 2,
                            required_stage_tags: Vec::new(),
                            required_reputation_tiers: Vec::new(),
                        }],
                    )
                },
                RegionEncounterTable {
                    ambient_cap: 1,
                    ..RegionEncounterTable::new(
                        "ashen-steppe-wildlife",
                        "ashen-steppe",
                        "wildlife",
                        vec![EncounterSpawnEntry {
                            archetype_id: "ashen-jackal".into(),
                            weight: 1,
                            min_count: 1,
                            max_count: 1,
                            required_stage_tags: Vec::new(),
                            required_reputation_tiers: Vec::new(),
                        }],
                    )
                },
            ],
        );

        let (_, player) = world.add_agent(Box::new(crate::agent::IdleAgent::new()));
        world
            .ecs
            .get::<&mut Transform>(player)
            .expect("player should exist")
            .position = Vec2::new(1.0, 1.0);
        world.reconcile_streaming_population();

        let population = world.population_state();
        let heart = population
            .chunks
            .iter()
            .find(|chunk| chunk.chunk_key == "0:0")
            .expect("heart chunk should exist");
        assert_eq!(heart.counts.wild_creatures, 2);

        world
            .ecs
            .get::<&mut Transform>(player)
            .expect("player should still exist")
            .position = Vec2::new(9.0, 1.0);
        world.reconcile_streaming_population();

        let moved = world.population_state();
        let heart_after = moved
            .chunks
            .iter()
            .find(|chunk| chunk.chunk_key == "0:0")
            .expect("heart chunk should still exist");
        let steppe_after = moved
            .chunks
            .iter()
            .find(|chunk| chunk.chunk_key == "1:0")
            .expect("steppe chunk should exist");
        assert_eq!(heart_after.counts.wild_creatures, 0);
        assert_eq!(steppe_after.counts.wild_creatures, 1);
    }

    #[test]
    fn reconcile_streaming_population_respects_respawn_deadlines() {
        let mut world = World::new(7);

        let mut chunk = WorldChunkDefinition::new("0:0", "verdant-heart", "verdant-hollow");
        chunk.encounter_table_ids.push("verdant-heart-wildlife".into());
        let mut region =
            WorldRegionDefinition::new("verdant-heart", "Verdant Heart", "verdant-hollow");
        region.chunk_keys.push("0:0".into());
        region
            .encounter_table_ids
            .push("verdant-heart-wildlife".into());

        world.set_streaming_metadata(
            8.0,
            vec![chunk],
            vec![region],
            vec![RegionEncounterTable {
                ambient_cap: 1,
                ..RegionEncounterTable::new(
                    "verdant-heart-wildlife",
                    "verdant-hollow",
                    "wildlife",
                    vec![EncounterSpawnEntry {
                        archetype_id: "verdant-lynx".into(),
                        weight: 1,
                        min_count: 1,
                        max_count: 1,
                        required_stage_tags: Vec::new(),
                        required_reputation_tiers: Vec::new(),
                    }],
                )
            }],
        );

        world.add_agent(Box::new(crate::agent::IdleAgent::new()));
        world.reconcile_streaming_population();
        let spawned_entity = world
            .ecs
            .query::<&SpawnProfile>()
            .iter()
            .map(|(entity, _)| entity)
            .next()
            .expect("streamed entity should spawn");
        let respawn_ticks = world
            .ecs
            .get::<&SpawnProfile>(spawned_entity)
            .expect("spawn profile exists")
            .respawn_ticks as u64;

        assert!(world.ecs.despawn(spawned_entity).is_ok());
        world.reconcile_streaming_population();

        let during_cooldown = world.population_state();
        let chunk_state = during_cooldown
            .chunks
            .iter()
            .find(|chunk| chunk.chunk_key == "0:0")
            .expect("chunk should exist");
        assert_eq!(chunk_state.counts.wild_creatures, 0);
        assert_eq!(chunk_state.pending_respawns, 1);
        assert_eq!(chunk_state.next_respawn_tick, Some(respawn_ticks));

        world.tick = respawn_ticks;
        world.reconcile_streaming_population();

        let after_respawn = world.population_state();
        let chunk_state = after_respawn
            .chunks
            .iter()
            .find(|chunk| chunk.chunk_key == "0:0")
            .expect("chunk should still exist");
        assert_eq!(chunk_state.counts.wild_creatures, 1);
        assert_eq!(chunk_state.pending_respawns, 0);
        assert_eq!(chunk_state.next_respawn_tick, None);
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
