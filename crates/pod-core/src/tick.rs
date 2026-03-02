use crate::action::{validate_action, Action, AgentAction, ActionResult};
use crate::agent::AgentSlot;
use crate::component::*;
use crate::event::{Event, EventBus};
use crate::id::EntityId;
use crate::observation::*;
use crate::TICK_DURATION_SECS;
use glam::Vec2;

/// Result of a single tick
#[derive(Debug)]
pub struct TickResult {
    pub tick: u64,
    pub events: Vec<crate::event::GameEvent>,
    pub entity_count: usize,
    pub actions_processed: usize,
    pub actions_rejected: usize,
}

/// Execute one tick of the simulation
pub fn execute_tick(
    ecs: &mut hecs::World,
    agents: &mut Vec<AgentSlot>,
    events: &mut EventBus,
    tick: u64,
    _next_entity_id: &mut u64,
) -> TickResult {
    let elapsed = tick as f32 * TICK_DURATION_SECS;
    let mut actions_processed = 0u32;
    let mut actions_rejected = 0u32;

    // ========================================
    // PHASE 1: BUILD OBSERVATIONS
    // ========================================
    let observations = build_observations(ecs, agents, events, tick, elapsed);

    // ========================================
    // PHASE 2: DELIVER OBSERVATIONS & COLLECT DECISIONS
    // ========================================
    let mut all_actions: Vec<AgentAction> = Vec::new();

    for (slot, obs) in agents.iter_mut().zip(observations.into_iter()) {
        if !slot.connected {
            continue;
        }
        slot.agent.observe(obs);
        let decisions = slot.agent.decide();
        for action in decisions {
            all_actions.push(AgentAction {
                agent_id: slot.agent.id(),
                tick,
                action,
            });
        }
    }

    // ========================================
    // PHASE 3: VALIDATE & EXECUTE ACTIONS
    // ========================================
    for agent_action in &all_actions {
        // Find the agent's constraints
        let constraints = agents
            .iter()
            .find(|s| s.agent.id() == agent_action.agent_id)
            .map(|s| s.agent.constraints().clone());

        let Some(constraints) = constraints else { continue };

        match validate_action(agent_action, &constraints, tick) {
            ActionResult::Valid => {
                execute_action(ecs, agents, events, agent_action);
                actions_processed += 1;
            }
            ActionResult::Rejected(reason) => {
                log::debug!(
                    "Action rejected for {}: {}",
                    agent_action.agent_id,
                    reason
                );
                actions_rejected += 1;
            }
            ActionResult::Queued => {
                // TODO: action queue for next tick
            }
        }
    }

    // ========================================
    // PHASE 4: PHYSICS / MOVEMENT
    // ========================================
    step_movement(ecs);

    // ========================================
    // PHASE 5: FLUSH EVENTS
    // ========================================
    let tick_events = events.current_events().to_vec();
    events.flush(tick + 1);

    TickResult {
        tick,
        events: tick_events,
        entity_count: ecs.len() as usize,
        actions_processed: actions_processed as usize,
        actions_rejected: actions_rejected as usize,
    }
}

/// Build observations for each agent based on their perception
fn build_observations(
    ecs: &hecs::World,
    agents: &[AgentSlot],
    events: &EventBus,
    tick: u64,
    elapsed: f32,
) -> Vec<Observation> {
    let mut observations = Vec::with_capacity(agents.len());

    for slot in agents.iter() {
        if !slot.connected {
            observations.push(empty_observation(tick, elapsed, slot));
            continue;
        }

        let Some(entity) = slot.entity_id else {
            observations.push(empty_observation(tick, elapsed, slot));
            continue;
        };

        // Get agent's own state
        let Ok(transform) = ecs.get::<&Transform>(entity) else {
            observations.push(empty_observation(tick, elapsed, slot));
            continue;
        };

        let velocity = ecs
            .get::<&Velocity>(entity)
            .map(|v| v.linear)
            .unwrap_or(Vec2::ZERO);
        let health = ecs.get::<&Health>(entity).ok();
        let label = ecs.get::<&Label>(entity).ok();
        let perception = ecs
            .get::<&Perception>(entity)
            .map(|p| *p)
            .unwrap_or_default();

        let my_team = label.as_ref().map(|l| l.team).unwrap_or(Team::None);
        let my_pos = transform.position;
        let my_rot = transform.rotation;

        let self_state = SelfState {
            agent_id: slot.agent.id(),
            entity_id: EntityId(entity.id() as u64),
            position: my_pos,
            rotation: my_rot,
            velocity,
            health: health.as_ref().map(|h| h.current),
            max_health: health.as_ref().map(|h| h.max),
            team: my_team,
            cooldowns: vec![], // TODO: populate from constraint tracker
        };

        // Find visible entities
        let mut visible_entities = Vec::new();
        for (other_entity, (other_transform,)) in ecs.query::<(&Transform,)>().iter() {
            if other_entity == entity {
                continue; // skip self
            }

            let other_pos = other_transform.position;
            let distance = my_pos.distance(other_pos);

            // Range check
            if distance > perception.vision_range {
                continue;
            }

            // FOV check
            if perception.vision_fov < std::f32::consts::TAU - 0.01 {
                let to_other = (other_pos - my_pos).normalize_or_zero();
                let facing = Vec2::new(my_rot.cos(), my_rot.sin());
                let angle = facing.dot(to_other).acos();
                if angle > perception.vision_fov / 2.0 {
                    continue;
                }
            }

            // Determine relationship
            let other_label = ecs.get::<&Label>(other_entity).ok();
            let other_team = other_label.as_ref().map(|l| l.team).unwrap_or(Team::None);
            let relationship = if my_team.is_hostile_to(&other_team) {
                Relationship::Hostile
            } else if my_team == other_team && my_team != Team::None {
                Relationship::Friendly
            } else {
                Relationship::Neutral
            };

            let other_health = ecs
                .get::<&Health>(other_entity)
                .ok()
                .filter(|_| distance < 100.0) // only see health up close
                .map(|h| h.current / h.max);

            let entity_type = other_label
                .as_ref()
                .map(|l| l.name.clone())
                .unwrap_or_else(|| "unknown".to_string());

            visible_entities.push(VisibleEntity {
                entity_id: EntityId(other_entity.id() as u64),
                entity_type,
                position: other_pos,
                velocity: ecs
                    .get::<&Velocity>(other_entity)
                    .map(|v| v.linear)
                    .unwrap_or(Vec2::ZERO),
                rotation: other_transform.rotation,
                distance,
                relationship,
                health_fraction: other_health,
            });
        }

        // Sort by distance (closest first)
        visible_entities.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());

        // Audible events
        let audible_events: Vec<AudibleEvent> = events
            .current_events()
            .iter()
            .filter(|e| e.origin.distance(my_pos) <= perception.hearing_range)
            .map(|e| {
                let dir = (e.origin - my_pos).normalize_or_zero();
                let dist = e.origin.distance(my_pos);
                AudibleEvent {
                    event_type: format!("{:?}", e.event).split('{').next().unwrap_or("unknown").trim().to_string(),
                    direction: dir,
                    distance: dist,
                    intensity: 1.0 - (dist / perception.hearing_range).min(1.0),
                }
            })
            .collect();

        observations.push(Observation {
            tick,
            elapsed_secs: elapsed,
            self_state,
            visible_entities,
            audible_events,
            messages: vec![], // TODO: collect from speak events
            available_actions: vec![
                "Move", "Stop", "Rotate", "LookAt", "Attack", "Interact", "Speak", "Idle",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            objectives: vec![], // TODO: game-specific
        });
    }

    observations
}

fn empty_observation(tick: u64, elapsed: f32, slot: &AgentSlot) -> Observation {
    Observation {
        tick,
        elapsed_secs: elapsed,
        self_state: SelfState {
            agent_id: slot.agent.id(),
            entity_id: EntityId(0),
            position: Vec2::ZERO,
            rotation: 0.0,
            velocity: Vec2::ZERO,
            health: None,
            max_health: None,
            team: Team::None,
            cooldowns: vec![],
        },
        visible_entities: vec![],
        audible_events: vec![],
        messages: vec![],
        available_actions: vec![],
        objectives: vec![],
    }
}

/// Execute a validated action on the world
fn execute_action(
    ecs: &mut hecs::World,
    agents: &mut [AgentSlot],
    events: &mut EventBus,
    agent_action: &AgentAction,
) {
    // Find the entity controlled by this agent
    let entity = agents
        .iter()
        .find(|s| s.agent.id() == agent_action.agent_id)
        .and_then(|s| s.entity_id);

    let Some(entity) = entity else { return };

    match &agent_action.action {
        Action::Move { direction } => {
            if let Ok(mut vel) = ecs.get::<&mut Velocity>(entity) {
                let movement = ecs
                    .get::<&Movement>(entity)
                    .map(|m| *m)
                    .unwrap_or_default();
                vel.linear = direction.normalize_or_zero() * movement.max_speed;
            }
        }
        Action::Stop => {
            if let Ok(mut vel) = ecs.get::<&mut Velocity>(entity) {
                vel.linear = Vec2::ZERO;
            }
        }
        Action::Rotate { angle } => {
            if let Ok(mut transform) = ecs.get::<&mut Transform>(entity) {
                transform.rotation = *angle;
            }
        }
        Action::LookAt { target } => {
            if let Ok(mut transform) = ecs.get::<&mut Transform>(entity) {
                let dir = *target - transform.position;
                transform.rotation = dir.y.atan2(dir.x);
            }
        }
        Action::Attack => {
            if let Ok(transform) = ecs.get::<&Transform>(entity) {
                events.emit(
                    transform.position,
                    Event::Sound {
                        name: "attack".into(),
                        intensity: 1.0,
                    },
                );
            }
            // TODO: raycast/hitbox for damage
        }
        Action::AttackTarget { target } => {
            let damage = 10.0; // TODO: configurable damage

            // Emit attack sound from attacker
            if let Ok(transform) = ecs.get::<&Transform>(entity) {
                events.emit(
                    transform.position,
                    Event::Sound {
                        name: "attack".into(),
                        intensity: 1.0,
                    },
                );
            }

            // Apply damage and call on_damage hook on target agent
            for slot in agents.iter_mut() {
                if let Some(e) = slot.entity_id {
                    if EntityId(e.id() as u64) == *target {
                        if let Ok(mut health) = ecs.get::<&mut Health>(e) {
                            health.current = (health.current - damage).max(0.0);
                        }
                        slot.agent.on_damage(damage, Some(agent_action.agent_id));
                        break;
                    }
                }
            }
        }
        Action::Speak { message, volume } => {
            if let Ok(transform) = ecs.get::<&Transform>(entity) {
                events.emit(
                    transform.position,
                    Event::AgentSpoke {
                        agent_id: agent_action.agent_id,
                        message: message.clone(),
                        volume: volume.range(),
                    },
                );
            }
        }
        Action::Interact => {
            // Find nearest entity within interaction range and interact with it
            if let Ok(transform) = ecs.get::<&Transform>(entity) {
                let my_pos = transform.position;
                let mut nearest: Option<(hecs::Entity, f32)> = None;

                for (other, (other_t,)) in ecs.query::<(&Transform,)>().iter() {
                    if other == entity {
                        continue;
                    }
                    let dist = my_pos.distance(other_t.position);
                    if dist < 50.0 {
                        if nearest.is_none() || dist < nearest.unwrap().1 {
                            nearest = Some((other, dist));
                        }
                    }
                }

                if let Some((target_entity, _)) = nearest {
                    let target_id = EntityId(target_entity.id() as u64);
                    // Call on_interact on the acting agent
                    if let Some(slot) = agents.iter_mut().find(|s| s.agent.id() == agent_action.agent_id) {
                        slot.agent.on_interact(target_id);
                    }
                }
            }
        }
        Action::InteractWith { target } => {
            // Call on_interact on the acting agent with the specified target
            if let Some(slot) = agents.iter_mut().find(|s| s.agent.id() == agent_action.agent_id) {
                slot.agent.on_interact(*target);
            }
        }
        Action::Idle => {} // explicit no-op
        _ => {
            log::debug!("Unhandled action: {:?}", agent_action.action);
        }
    }
}

/// Simple movement integration (before full physics)
fn step_movement(ecs: &mut hecs::World) {
    for (_, (transform, velocity)) in ecs.query_mut::<(&mut Transform, &Velocity)>() {
        transform.position += velocity.linear * TICK_DURATION_SECS;
        transform.rotation += velocity.angular * TICK_DURATION_SECS;
    }
}
