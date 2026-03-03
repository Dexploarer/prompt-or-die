//! # Multi-Agent Orchestrator
//!
//! Efficiently manages large numbers of LLM-powered agents through batching
//! and priority-based scheduling.
//!
//! When you have 50+ agents, making sequential API calls is inefficient.
//! This orchestrator:
//! - Groups agents into batches for parallel processing
//! - Prioritizes agents near action (combat, interaction)
//! - Tracks decision freshness to avoid stale decisions
//! - Maintains a decision budget to keep framerate consistent

use crate::agent::AgentSlot;
use crate::id::AgentId;
use crate::observation::Observation;
use std::collections::HashMap;

/// Priority score for agent decision-making
/// Higher scores = higher priority for this tick
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriorityScore(pub f32);

impl PriorityScore {
    /// Score based on proximity to combat/interaction
    pub fn from_observation(obs: &Observation) -> Self {
        let mut score = 0.0f32;

        // Enemies nearby = high priority
        for entity in &obs.visible_entities {
            use crate::observation::Relationship;
            if matches!(entity.relationship, Relationship::Hostile) {
                let proximity_boost = 1000.0 / (entity.distance + 1.0);
                score += proximity_boost;
            }
        }

        // Being attacked = max priority
        if obs.self_state.health.unwrap_or(100.0) < obs.self_state.max_health.unwrap_or(100.0) {
            score += 500.0;
        }

        // Hearing combat = medium priority
        for event in &obs.audible_events {
            if event.event_type.contains("attack") || event.event_type.contains("damage") {
                score += 100.0;
            }
        }

        // Idle agents have low priority
        if obs.visible_entities.is_empty() && obs.audible_events.is_empty() {
            score += 1.0;
        }

        PriorityScore(score)
    }
}

impl PartialOrd for PriorityScore {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

/// Tracks the freshness of an agent's last decision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionFreshness {
    /// This tick's observation has been decided on
    Fresh,
    /// Last tick's decision is being reused (one tick old)
    Stale { ticks_old: u32 },
}

impl DecisionFreshness {
    pub fn is_fresh(&self) -> bool {
        matches!(self, DecisionFreshness::Fresh)
    }

    pub fn ticks_old(&self) -> u32 {
        match self {
            DecisionFreshness::Fresh => 0,
            DecisionFreshness::Stale { ticks_old } => *ticks_old,
        }
    }
}

/// An agent scheduled for decision-making
#[derive(Debug, Clone)]
struct ScheduledAgent {
    agent_id: AgentId,
    priority: PriorityScore,
    slot_index: usize,
    freshness: DecisionFreshness,
}

/// Batch of agents to be processed together
#[derive(Debug, Clone)]
pub struct AgentBatch {
    /// Agent IDs in this batch
    pub agents: Vec<AgentId>,
    /// Which slot index each maps to
    pub slot_indices: Vec<usize>,
    /// Total priority of all agents in batch
    pub total_priority: f32,
}

/// Orchestrates decisions for many agents
pub struct AgentOrchestrator {
    /// Per-agent decision tracking
    agent_state: HashMap<AgentId, AgentDecisionState>,
    /// Decision budget for this tick (max API calls)
    pub decision_budget: u32,
    /// How many decisions we've made this tick
    decisions_made_this_tick: u32,
}

/// Tracks decision state per agent
#[derive(Debug, Clone)]
struct AgentDecisionState {
    last_decision_tick: u64,
    freshness: DecisionFreshness,
}

impl AgentOrchestrator {
    /// Create with a decision budget (e.g., 8 agents per tick max)
    pub fn new(decision_budget: u32) -> Self {
        Self {
            agent_state: HashMap::new(),
            decision_budget,
            decisions_made_this_tick: 0,
        }
    }

    /// Update agent state at the start of a tick
    pub fn update_tick(&mut self, agents: &[AgentSlot], tick: u64) {
        for (_slot_idx, slot) in agents.iter().enumerate() {
            if !slot.connected {
                continue;
            }

            let agent_id = slot.agent.id();
            let state = self
                .agent_state
                .entry(agent_id)
                .or_insert_with(|| AgentDecisionState {
                    last_decision_tick: tick,
                    freshness: DecisionFreshness::Fresh,
                });

            if state.last_decision_tick < tick {
                let ticks_old = (tick - state.last_decision_tick) as u32;
                state.freshness = DecisionFreshness::Stale { ticks_old };
            }
        }
    }

    /// Schedule agents for decision-making this tick
    /// Returns batches ready for processing
    pub fn schedule_batches(&mut self, agents: &[AgentSlot], batch_size: usize) -> Vec<AgentBatch> {
        self.decisions_made_this_tick = 0;

        // Build list of agents to schedule
        let mut scheduled: Vec<ScheduledAgent> = agents
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.connected && !slot.last_observation.is_none())
            .map(|(slot_idx, slot)| {
                let obs = slot.last_observation.as_ref().unwrap();
                let priority = PriorityScore::from_observation(obs);
                let state = self
                    .agent_state
                    .get(&slot.agent.id())
                    .cloned()
                    .unwrap_or_else(|| AgentDecisionState {
                        last_decision_tick: 0,
                        freshness: DecisionFreshness::Fresh,
                    });

                ScheduledAgent {
                    agent_id: slot.agent.id(),
                    priority,
                    slot_index: slot_idx,
                    freshness: state.freshness,
                }
            })
            .collect();

        // Sort by priority (fresh decisions first, then by priority score)
        scheduled.sort_by(|a, b| {
            match (a.freshness.is_fresh(), b.freshness.is_fresh()) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal),
            }
        });

        // Create batches
        let mut batches = Vec::new();
        let mut current_batch = AgentBatch {
            agents: Vec::new(),
            slot_indices: Vec::new(),
            total_priority: 0.0,
        };

        for agent in scheduled {
            // Respect budget
            if self.decisions_made_this_tick >= self.decision_budget {
                break;
            }

            // Fill current batch
            if current_batch.agents.len() >= batch_size {
                if !current_batch.agents.is_empty() {
                    batches.push(current_batch);
                    current_batch = AgentBatch {
                        agents: Vec::new(),
                        slot_indices: Vec::new(),
                        total_priority: 0.0,
                    };
                }
            }

            current_batch.agents.push(agent.agent_id);
            current_batch.slot_indices.push(agent.slot_index);
            current_batch.total_priority += agent.priority.0;
        }

        if !current_batch.agents.is_empty() {
            batches.push(current_batch);
        }

        batches
    }

    /// Mark that a decision was made
    pub fn mark_decision_made(&mut self, agent_id: AgentId, tick: u64) {
        if let Some(state) = self.agent_state.get_mut(&agent_id) {
            state.last_decision_tick = tick;
            state.freshness = DecisionFreshness::Fresh;
        }
        self.decisions_made_this_tick += 1;
    }

    /// Get freshness for an agent
    pub fn get_freshness(&self, agent_id: AgentId) -> DecisionFreshness {
        self.agent_state
            .get(&agent_id)
            .map(|s| s.freshness)
            .unwrap_or(DecisionFreshness::Fresh)
    }

    /// How many decisions left in budget
    pub fn decisions_remaining(&self) -> u32 {
        self.decision_budget.saturating_sub(self.decisions_made_this_tick)
    }

    /// Reset budget for new tick
    pub fn reset_tick_budget(&mut self) {
        self.decisions_made_this_tick = 0;
    }
}

impl Default for AgentOrchestrator {
    fn default() -> Self {
        Self::new(8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::*;

    #[test]
    fn test_priority_scoring() {
        let mut obs = Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: SelfState {
                agent_id: AgentId::new(),
                entity_id: crate::id::EntityId(0),
                position: glam::Vec2::ZERO,
                rotation: 0.0,
                velocity: glam::Vec2::ZERO,
                health: Some(100.0),
                max_health: Some(100.0),
                team: crate::component::Team::None,
                cooldowns: vec![],
            },
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        };

        // No threat = low priority
        let score = PriorityScore::from_observation(&obs);
        assert!(score.0 < 10.0);

        // Add hostile entity = high priority
        obs.visible_entities.push(VisibleEntity {
            entity_id: crate::id::EntityId(1),
            entity_type: "enemy".to_string(),
            position: glam::Vec2::new(10.0, 0.0),
            velocity: glam::Vec2::ZERO,
            rotation: 0.0,
            distance: 10.0,
            relationship: Relationship::Hostile,
            health_fraction: Some(0.5),
        });

        let score = PriorityScore::from_observation(&obs);
        assert!(score.0 > 80.0);
    }

    #[test]
    fn test_orchestrator_batching() {
        let orchestrator = AgentOrchestrator::new(8);
        assert_eq!(orchestrator.decisions_remaining(), 8);
    }
}
