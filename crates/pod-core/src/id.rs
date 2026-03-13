use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

/// Unique identifier for an entity in the world
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub u64);

/// Unique identifier for an agent (player, AI, NPC)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

/// Unique identifier for a game event
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub u64);

impl AgentId {
    pub fn new() -> Self {
        static NEXT_AGENT_ID: AtomicU64 = AtomicU64::new(1);

        // POD agent ids only need to be unique within authoritative runtime
        // state. Keeping generation deterministic avoids platform RNG backends
        // in wasm module builds.
        let sequence = NEXT_AGENT_ID.fetch_add(1, Ordering::Relaxed) as u128;
        Self(Uuid::from_u128(
            0x504f445f4147454e_5400000000000000 | sequence,
        ))
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Agent({})", &self.0.to_string()[..8])
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "E({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::AgentId;

    #[test]
    fn agent_ids_are_unique() {
        assert_ne!(AgentId::new(), AgentId::new());
    }
}
