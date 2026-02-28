//! # Observation Space Optimization
//!
//! Compresses observations to fit LLM token budgets while preserving critical information.
//!
//! LLMs charge by token count. An observation with 50 visible entities can easily
//! exceed the token budget. This system:
//! - Scores entities by salience (threat, distance, relevance)
//! - Keeps only the most salient entities
//! - Encodes positions relative to the agent
//! - Estimates token count and compresses if needed
//! - Maintains a sliding window of history for context

use crate::observation::{Observation, VisibleEntity, AudibleEvent, Relationship};
use serde::{Deserialize, Serialize};

/// Salience score for an entity (higher = more important)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct SalienceScore(pub f32);

impl SalienceScore {
    /// Compute salience based on threat, distance, and relevance
    pub fn compute(entity: &VisibleEntity, self_health: f32, self_max_health: f32) -> Self {
        let mut score = 0.0f32;

        // Hostile entities are critical
        match entity.relationship {
            Relationship::Hostile => score += 100.0,
            Relationship::Friendly => score += 10.0,
            Relationship::Neutral => score += 1.0,
            Relationship::Unknown => score += 0.5,
        }

        // Threat multiplier: low-health targets are higher priority
        if let Some(health) = entity.health_fraction {
            if health < 0.3 {
                score *= 1.5; // weak enemy = higher priority
            }
        }

        // Distance factor: inverse (closer = higher score)
        let distance_factor = 1000.0 / (entity.distance + 10.0);
        score *= distance_factor;

        // Being at low health makes ALL threats more relevant
        let health_ratio = self_health / self_max_health;
        if health_ratio < 0.5 {
            score *= 2.0;
        }

        SalienceScore(score)
    }

    pub fn value(&self) -> f32 {
        self.0
    }
}

impl Ord for SalienceScore {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(std::cmp::Ordering::Equal)
    }
}

impl Eq for SalienceScore {}

impl PartialEq<f32> for SalienceScore {
    fn eq(&self, other: &f32) -> bool {
        self.0 == *other
    }
}

/// Filters observations to reduce token count
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationFilter {
    /// Maximum entities to include in observation
    pub max_entities: usize,
    /// Maximum audible events to include
    pub max_events: usize,
    /// Token budget for observation (0 = no limit)
    pub token_budget: u32,
    /// Use relative coordinates instead of absolute
    pub use_relative_coords: bool,
    /// How many past observations to remember
    pub history_window: usize,
}

impl ObservationFilter {
    pub fn new() -> Self {
        Self {
            max_entities: 10,
            max_events: 5,
            token_budget: 2048,
            use_relative_coords: true,
            history_window: 3,
        }
    }

    /// Aggressively compress for token-limited scenarios
    pub fn aggressive() -> Self {
        Self {
            max_entities: 5,
            max_events: 2,
            token_budget: 512,
            use_relative_coords: true,
            history_window: 1,
        }
    }

    /// Balanced compression
    pub fn balanced() -> Self {
        Self {
            max_entities: 10,
            max_events: 5,
            token_budget: 2048,
            use_relative_coords: true,
            history_window: 3,
        }
    }

    /// No compression (full information)
    pub fn none() -> Self {
        Self {
            max_entities: 1000,
            max_events: 100,
            token_budget: 0,
            use_relative_coords: false,
            history_window: 0,
        }
    }

    /// Apply filtering to an observation
    pub fn filter(&self, obs: Observation) -> FilteredObservation {
        let self_health = obs.self_state.health.unwrap_or(100.0);
        let self_max_health = obs.self_state.max_health.unwrap_or(100.0);

        // Score entities by salience
        let mut scored_entities: Vec<_> = obs
            .visible_entities
            .iter()
            .map(|e| {
                let score = SalienceScore::compute(e, self_health, self_max_health);
                (score, e.clone())
            })
            .collect();

        // Sort by salience (descending)
        scored_entities.sort_by(|a, b| b.0.cmp(&a.0));

        // Keep top N
        let filtered_entities: Vec<VisibleEntity> = scored_entities
            .into_iter()
            .take(self.max_entities)
            .map(|(_, e)| e)
            .collect();

        // Convert to relative coordinates if enabled
        let visible_entities = if self.use_relative_coords {
            filtered_entities
                .into_iter()
                .map(|mut e| {
                    e.position = e.position - obs.self_state.position;
                    e
                })
                .collect()
        } else {
            filtered_entities
        };

        // Keep top N audible events
        let audible_events: Vec<AudibleEvent> = obs
            .audible_events
            .into_iter()
            .take(self.max_events)
            .collect();

        // Estimate token count
        let estimated_tokens = estimate_token_count(&obs, &visible_entities, &audible_events);

        // Build filtered observation
        let mut filtered = FilteredObservation {
            observation: Observation {
                visible_entities,
                audible_events,
                ..obs
            },
            estimated_tokens,
            was_compressed: false,
            original_entity_count: 0,
            original_event_count: 0,
        };

        // Track compression
        filtered.original_entity_count = obs.visible_entities.len();
        filtered.original_event_count = obs.audible_events.len();
        filtered.was_compressed = filtered.estimated_tokens > self.token_budget as usize && self.token_budget > 0;

        filtered
    }
}

impl Default for ObservationFilter {
    fn default() -> Self {
        Self::balanced()
    }
}

/// Result of filtering, with metadata
#[derive(Debug, Clone)]
pub struct FilteredObservation {
    /// The filtered observation
    pub observation: Observation,
    /// Estimated token count of filtered observation
    pub estimated_tokens: usize,
    /// Was this observation compressed due to token budget?
    pub was_compressed: bool,
    /// How many entities were removed?
    pub original_entity_count: usize,
    /// How many events were removed?
    pub original_event_count: usize,
}

impl FilteredObservation {
    /// How much compression happened
    pub fn compression_ratio(&self) -> f32 {
        if self.original_entity_count == 0 {
            return 1.0;
        }
        self.observation.visible_entities.len() as f32 / self.original_entity_count as f32
    }

    /// Serialize to agent-friendly string
    pub fn to_agent_prompt(&self) -> String {
        self.observation.to_agent_prompt()
    }
}

/// Rough estimation of tokens an observation will use
fn estimate_token_count(
    obs: &Observation,
    entities: &[VisibleEntity],
    events: &[AudibleEvent],
) -> usize {
    let mut tokens = 0usize;

    // Header info: ~20 tokens
    tokens += 20;

    // Self state: ~30 tokens
    tokens += 30;

    // Visible entities: ~15 tokens each
    tokens += entities.len() * 15;

    // Audible events: ~10 tokens each
    tokens += events.len() * 10;

    // Messages: ~5 tokens each
    tokens += obs.messages.len() * 5;

    // Objectives: ~10 tokens each
    tokens += obs.objectives.len() * 10;

    tokens
}

/// Tracks observation history for context windows
#[derive(Debug, Clone)]
pub struct ObservationHistory {
    /// Past observations (most recent first)
    observations: Vec<FilteredObservation>,
    /// Maximum history size
    max_size: usize,
}

impl ObservationHistory {
    pub fn new(max_size: usize) -> Self {
        Self {
            observations: Vec::with_capacity(max_size),
            max_size,
        }
    }

    /// Add an observation to history
    pub fn push(&mut self, obs: FilteredObservation) {
        self.observations.push(obs);
        if self.observations.len() > self.max_size {
            self.observations.remove(0);
        }
    }

    /// Get current observation (most recent)
    pub fn current(&self) -> Option<&FilteredObservation> {
        self.observations.last()
    }

    /// Get previous observations (by recency)
    pub fn previous(&self, steps_back: usize) -> Option<&FilteredObservation> {
        if steps_back == 0 {
            return self.current();
        }
        let index = self.observations.len().saturating_sub(steps_back + 1);
        self.observations.get(index)
    }

    /// Get all observations in order (oldest first)
    pub fn all(&self) -> &[FilteredObservation] {
        &self.observations
    }

    /// Clear history
    pub fn clear(&mut self) {
        self.observations.clear();
    }
}

impl Default for ObservationHistory {
    fn default() -> Self {
        Self::new(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::EntityId;
    use crate::component::Team;
    use glam::Vec2;

    #[test]
    fn test_salience_scoring() {
        let entity = VisibleEntity {
            entity_id: EntityId(1),
            entity_type: "enemy".to_string(),
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            rotation: 0.0,
            distance: 50.0,
            relationship: Relationship::Hostile,
            health_fraction: Some(0.2),
        };

        let score = SalienceScore::compute(&entity, 100.0, 100.0);
        assert!(score.value() > 0.0);

        let friendly = VisibleEntity {
            relationship: Relationship::Friendly,
            ..entity.clone()
        };

        let friendly_score = SalienceScore::compute(&friendly, 100.0, 100.0);
        assert!(score.value() > friendly_score.value());
    }

    #[test]
    fn test_filter_reduces_entities() {
        let mut obs = Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: crate::observation::SelfState {
                agent_id: crate::id::AgentId::new(),
                entity_id: EntityId(0),
                position: Vec2::ZERO,
                rotation: 0.0,
                velocity: Vec2::ZERO,
                health: Some(100.0),
                max_health: Some(100.0),
                team: Team::None,
                cooldowns: vec![],
            },
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        };

        // Add 20 entities
        for i in 0..20 {
            obs.visible_entities.push(VisibleEntity {
                entity_id: EntityId(i),
                entity_type: "enemy".to_string(),
                position: Vec2::new(i as f32, 0.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                distance: 50.0 + i as f32,
                relationship: Relationship::Hostile,
                health_fraction: Some(0.5),
            });
        }

        let filter = ObservationFilter::aggressive();
        let filtered = filter.filter(obs.clone());

        assert!(filtered.observation.visible_entities.len() <= filter.max_entities);
        assert!(filtered.compression_ratio() < 1.0);
    }

    #[test]
    fn test_observation_history() {
        let mut history = ObservationHistory::new(3);

        for i in 0..5 {
            let filtered = FilteredObservation {
                observation: Observation {
                    tick: i,
                    ..Default::default()
                },
                estimated_tokens: 100,
                was_compressed: false,
                original_entity_count: 0,
                original_event_count: 0,
            };
            history.push(filtered);
        }

        assert_eq!(history.all().len(), 3); // limited by max_size
        assert_eq!(history.current().unwrap().observation.tick, 4);
    }
}
