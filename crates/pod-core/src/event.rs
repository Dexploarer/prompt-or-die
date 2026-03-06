use crate::id::{AgentId, EntityId};
use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Game events — things that happen in the world.
/// Events are broadcast to agents based on their perception.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameEvent {
    pub tick: u64,
    pub origin: Vec2,
    pub event: Event,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    // === LIFECYCLE ===
    EntitySpawned {
        entity: EntityId,
        entity_type: String,
    },
    EntityDestroyed {
        entity: EntityId,
    },

    // === COMBAT ===
    Damage {
        source: Option<EntityId>,
        target: EntityId,
        amount: f32,
    },
    Kill {
        killer: Option<EntityId>,
        victim: EntityId,
    },
    Heal {
        source: Option<EntityId>,
        target: EntityId,
        amount: f32,
    },
    SkillXpGained {
        entity: EntityId,
        skill: String,
        amount: u32,
        new_level: u16,
    },
    CreatureCaptured {
        agent_id: AgentId,
        species_id: String,
        nickname: Option<String>,
    },
    CompanionSummoned {
        agent_id: AgentId,
        species_id: String,
    },
    CompanionCommandIssued {
        agent_id: AgentId,
        slot: u8,
        command: String,
        target: Option<EntityId>,
    },
    EncounterStarted {
        encounter_id: u64,
        kind: String,
    },
    EncounterEnded {
        encounter_id: u64,
        victory: bool,
    },
    ResourceGathered {
        entity: EntityId,
        resource: EntityId,
        skill: String,
        item_id: String,
        quantity: u32,
    },
    LootClaimed {
        entity: EntityId,
        source: EntityId,
        coins: u64,
        item_count: usize,
    },
    AutoRetaliateSet {
        entity: EntityId,
        enabled: bool,
    },

    // === PHYSICS ===
    Collision {
        entity_a: EntityId,
        entity_b: EntityId,
        point: Vec2,
        normal: Vec2,
    },
    TriggerEnter {
        trigger: EntityId,
        entity: EntityId,
    },
    TriggerExit {
        trigger: EntityId,
        entity: EntityId,
    },

    // === AGENT ===
    AgentJoined {
        agent_id: AgentId,
        agent_type: String,
    },
    AgentLeft {
        agent_id: AgentId,
    },
    AgentSpoke {
        agent_id: AgentId,
        message: String,
        volume: f32,
    },

    // === INTERACTION ===
    ItemPickedUp {
        entity: EntityId,
        item: EntityId,
    },
    ItemDropped {
        entity: EntityId,
        item: EntityId,
    },
    ItemUsed {
        entity: EntityId,
        item: String,
    },

    // === GAME ===
    ObjectiveUpdated {
        id: String,
        progress: f32,
    },
    ObjectiveCompleted {
        id: String,
    },
    GameStateChanged {
        new_state: String,
    },
    ScoreChanged {
        team: u8,
        score: i32,
    },

    // === AUDIO CUE ===
    /// Sound event — used for both actual audio AND agent perception
    Sound {
        name: String,
        intensity: f32,
    },

    // === CUSTOM ===
    Custom {
        name: String,
        data: String,
    },
}

/// Collects events during a tick, then broadcasts them
pub struct EventBus {
    /// Events from the current tick
    events: Vec<GameEvent>,
    /// Events from the previous tick (for queries)
    last_tick_events: Vec<GameEvent>,
    /// Current tick number
    tick: u64,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            events: Vec::with_capacity(256),
            last_tick_events: Vec::new(),
            tick: 0,
        }
    }

    /// Emit an event at a world position
    pub fn emit(&mut self, origin: Vec2, event: Event) {
        self.events.push(GameEvent {
            tick: self.tick,
            origin,
            event,
        });
    }

    /// Emit an event with no specific origin
    pub fn emit_global(&mut self, event: Event) {
        self.emit(Vec2::ZERO, event);
    }

    /// Get all events from the current tick
    pub fn current_events(&self) -> &[GameEvent] {
        &self.events
    }

    /// Get all events from the previous tick
    pub fn last_events(&self) -> &[GameEvent] {
        &self.last_tick_events
    }

    /// Get events near a position within a range
    pub fn events_near(&self, position: Vec2, range: f32) -> Vec<&GameEvent> {
        self.events
            .iter()
            .filter(|e| e.origin.distance(position) <= range)
            .collect()
    }

    /// Advance to next tick — moves current events to last_tick
    pub fn flush(&mut self, new_tick: u64) {
        self.last_tick_events = std::mem::take(&mut self.events);
        self.tick = new_tick;
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
