use crate::action::{validate_action, Action, ActionResult, AgentAction, CompanionCommand};
use crate::agent::AgentSlot;
use crate::component::*;
use crate::event::{Event, EventBus};
use crate::id::EntityId;
use crate::observation::*;
use crate::telemetry::{
    ActionLifecycleStage, ActionSource, AgentTelemetryFrame, TickTelemetryFrame, TrajectorySample,
};
use crate::TICK_DURATION_SECS;
use glam::{Vec2, Vec3};
use std::collections::HashMap;

/// Result of a single tick
#[derive(Debug, Clone)]
pub struct TickResult {
    pub tick: u64,
    pub events: Vec<crate::event::GameEvent>,
    pub entity_count: usize,
    pub actions_processed: usize,
    pub actions_rejected: usize,
    pub telemetry: TickTelemetryFrame,
}

/// Execute one tick of the simulation
pub fn execute_tick(
    ecs: &mut hecs::World,
    agents: &mut Vec<AgentSlot>,
    events: &mut EventBus,
    tick: u64,
    external_actions: Vec<AgentAction>,
    _next_entity_id: &mut u64,
) -> TickResult {
    struct PendingAction {
        agent_action: AgentAction,
        source: ActionSource,
    }

    let elapsed = tick as f32 * TICK_DURATION_SECS;
    let mut actions_processed = 0u32;
    let mut actions_rejected = 0u32;

    // Cooldowns advance once per authoritative tick.
    for slot in agents.iter_mut() {
        slot.attack_cooldown_remaining = slot.attack_cooldown_remaining.saturating_sub(1);
    }

    // ========================================
    // PHASE 1: BUILD OBSERVATIONS
    // ========================================
    let observations = build_observations(ecs, agents, events, tick, elapsed);

    // ========================================
    // PHASE 2: DELIVER OBSERVATIONS & COLLECT DECISIONS
    // ========================================
    let mut all_actions: Vec<PendingAction> = Vec::new();
    let mut telemetry_frames = Vec::<AgentTelemetryFrame>::new();
    let mut telemetry_indices = HashMap::<crate::id::AgentId, usize>::new();

    for (slot, obs) in agents.iter_mut().zip(observations.into_iter()) {
        if !slot.connected {
            continue;
        }
        let entity_id = slot.entity_id.map(|entity| EntityId(entity.id() as u64));
        let trajectory_start = entity_id.map(|_| {
            TrajectorySample::new(
                tick,
                elapsed,
                obs.self_state.position,
                obs.self_state.velocity,
                obs.self_state.rotation,
            )
        });
        let telemetry_index = telemetry_frames.len();
        telemetry_frames.push(AgentTelemetryFrame::new(
            tick,
            slot.agent.id(),
            entity_id,
            slot.agent.runtime_profile(),
            obs.visible_entities.len(),
            obs.audible_events.len(),
            obs.messages.len(),
            obs.available_actions.len(),
            obs.objectives.len(),
            obs.self_state.encounter.clone(),
            trajectory_start,
        ));
        telemetry_indices.insert(slot.agent.id(), telemetry_index);
        slot.agent.observe(obs);
        let decisions = slot.agent.decide();
        if let Some(index) = telemetry_indices.get(&slot.agent.id()).copied() {
            for trace in slot.agent.drain_tool_calls() {
                telemetry_frames[index].record_tool_call(trace);
            }
        }
        for action in decisions {
            if let Some(index) = telemetry_indices.get(&slot.agent.id()).copied() {
                telemetry_frames[index].record_action(
                    ActionSource::AgentDecision,
                    ActionLifecycleStage::Submitted,
                    action.clone(),
                    None,
                );
            }
            all_actions.push(PendingAction {
                agent_action: AgentAction {
                    agent_id: slot.agent.id(),
                    tick,
                    action,
                },
                source: ActionSource::AgentDecision,
            });
        }
    }
    for agent_action in external_actions {
        if let Some(index) = telemetry_indices.get(&agent_action.agent_id).copied() {
            telemetry_frames[index].record_action(
                ActionSource::ExternalSubmission,
                ActionLifecycleStage::Submitted,
                agent_action.action.clone(),
                None,
            );
        }
        all_actions.push(PendingAction {
            agent_action,
            source: ActionSource::ExternalSubmission,
        });
    }

    // ========================================
    // PHASE 3: VALIDATE & EXECUTE ACTIONS
    // ========================================
    for pending in &all_actions {
        let agent_action = &pending.agent_action;
        // Find the agent's constraints
        let constraints = agents
            .iter()
            .find(|s| s.agent.id() == agent_action.agent_id)
            .map(|s| s.agent.constraints().clone());

        let Some(constraints) = constraints else {
            continue;
        };

        match validate_action(agent_action, &constraints, tick) {
            ActionResult::Valid => {
                execute_action(ecs, agents, events, agent_action);
                actions_processed += 1;
                if let Some(index) = telemetry_indices.get(&agent_action.agent_id).copied() {
                    telemetry_frames[index].record_action(
                        pending.source,
                        ActionLifecycleStage::Executed,
                        agent_action.action.clone(),
                        None,
                    );
                }
            }
            ActionResult::Rejected(reason) => {
                log::debug!("Action rejected for {}: {}", agent_action.agent_id, reason);
                actions_rejected += 1;
                if let Some(index) = telemetry_indices.get(&agent_action.agent_id).copied() {
                    telemetry_frames[index].record_action(
                        pending.source,
                        ActionLifecycleStage::Rejected,
                        agent_action.action.clone(),
                        Some(reason),
                    );
                }
            }
            ActionResult::Queued => {
                log::debug!(
                    "Action deferred for {} at tick {}",
                    agent_action.agent_id,
                    tick
                );
                actions_rejected += 1;
                if let Some(index) = telemetry_indices.get(&agent_action.agent_id).copied() {
                    telemetry_frames[index].record_action(
                        pending.source,
                        ActionLifecycleStage::Queued,
                        agent_action.action.clone(),
                        None,
                    );
                }
            }
        }
    }

    // ========================================
    // PHASE 4: PHYSICS / MOVEMENT
    // ========================================
    step_camera_controllers(ecs);
    step_movement(ecs);

    // ========================================
    // PHASE 5: FLUSH EVENTS
    // ========================================
    for slot in agents.iter() {
        let Some(index) = telemetry_indices.get(&slot.agent.id()).copied() else {
            continue;
        };
        let Some(entity) = slot.entity_id else {
            continue;
        };
        let Ok(transform) = ecs.get::<&Transform>(entity) else {
            continue;
        };
        let velocity = ecs
            .get::<&Velocity>(entity)
            .map(|value| value.linear)
            .unwrap_or(Vec2::ZERO);
        telemetry_frames[index].update_trajectory_end(TrajectorySample::new(
            tick,
            elapsed + TICK_DURATION_SECS,
            transform.position,
            velocity,
            transform.rotation,
        ));
    }

    let tick_events = events.current_events().to_vec();
    events.flush(tick + 1);

    TickResult {
        tick,
        events: tick_events,
        entity_count: ecs.len() as usize,
        actions_processed: actions_processed as usize,
        actions_rejected: actions_rejected as usize,
        telemetry: TickTelemetryFrame {
            tick,
            agents: telemetry_frames,
        },
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
        let combat_loadout = ecs
            .get::<&CombatLoadout>(entity)
            .ok()
            .map(|loadout| (*loadout).clone());
        let skill_book = ecs
            .get::<&SkillBook>(entity)
            .ok()
            .map(|book| (*book).clone());
        let inventory = ecs
            .get::<&Inventory>(entity)
            .ok()
            .map(|inventory| (*inventory).clone());
        let companion_roster = ecs
            .get::<&CompanionRoster>(entity)
            .ok()
            .map(|roster| (*roster).clone());
        let encounter = ecs
            .get::<&EncounterState>(entity)
            .ok()
            .map(|encounter| (*encounter).clone());

        let my_team = label.as_ref().map(|l| l.team).unwrap_or(Team::None);
        let my_pos = transform.position;
        let my_rot = transform.rotation;

        let self_state = SelfState {
            agent_id: slot.agent.id(),
            entity_id: EntityId(entity.id() as u64),
            runtime_profile: slot.agent.runtime_profile(),
            position: my_pos,
            rotation: my_rot,
            velocity,
            health: health.as_ref().map(|h| h.current),
            max_health: health.as_ref().map(|h| h.max),
            team: my_team,
            cooldowns: build_cooldowns(slot),
            combat_loadout,
            skills: skill_book.map(|book| book.skills).unwrap_or_default(),
            inventory,
            companion_roster,
            encounter,
        };

        let event_source = if events.current_events().is_empty() {
            events.last_events()
        } else {
            events.current_events()
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
                combat_style: ecs
                    .get::<&CombatLoadout>(other_entity)
                    .ok()
                    .map(|loadout| loadout.style),
                creature: ecs
                    .get::<&CreatureIdentity>(other_entity)
                    .ok()
                    .map(|creature| (*creature).clone()),
            });
        }

        // Sort by distance (closest first)
        visible_entities.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());

        // Audible events
        let audible_events: Vec<AudibleEvent> = events
            .last_events()
            .iter()
            .filter(|e| e.origin.distance(my_pos) <= perception.hearing_range)
            .map(|e| {
                let dir = (e.origin - my_pos).normalize_or_zero();
                let dist = e.origin.distance(my_pos);
                AudibleEvent {
                    event_type: format!("{:?}", e.event)
                        .split('{')
                        .next()
                        .unwrap_or("unknown")
                        .trim()
                        .to_string(),
                    direction: dir,
                    distance: dist,
                    intensity: 1.0 - (dist / perception.hearing_range).min(1.0),
                }
            })
            .collect();

        let messages = collect_messages(event_source, my_pos, perception.hearing_range);
        let objectives = collect_objectives(event_source);

        observations.push(Observation {
            tick,
            elapsed_secs: elapsed,
            self_state,
            visible_entities,
            audible_events,
            messages,
            available_actions: vec![
                "Move",
                "Stop",
                "Rotate",
                "LookAt",
                "Attack",
                "UseAbility",
                "CaptureCreature",
                "SummonCompanion",
                "CommandCompanion",
                "Interact",
                "Pickup",
                "Drop",
                "UseItem",
                "GatherResource",
                "Loot",
                "Speak",
                "Signal",
                "SetAutoRetaliate",
                "Idle",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            objectives,
        });
    }

    observations
}

fn build_cooldowns(slot: &AgentSlot) -> Vec<CooldownState> {
    let constraints = slot.agent.constraints();
    vec![
        CooldownState {
            name: "attack".to_string(),
            remaining_ticks: slot.attack_cooldown_remaining,
            total_ticks: constraints.attack_cooldown,
        },
        CooldownState {
            name: "actions_per_tick".to_string(),
            remaining_ticks: 0,
            total_ticks: constraints.actions_per_tick as u32,
        },
    ]
}

fn collect_messages(
    events: &[crate::event::GameEvent],
    my_pos: Vec2,
    hearing_range: f32,
) -> Vec<AgentMessage> {
    events
        .iter()
        .filter(|e| e.origin.distance(my_pos) <= hearing_range)
        .filter_map(|e| match &e.event {
            Event::AgentSpoke {
                agent_id, message, ..
            } => Some(AgentMessage {
                from: *agent_id,
                content: message.clone(),
                channel: MessageChannel::Proximity,
            }),
            _ => None,
        })
        .collect()
}

fn collect_objectives(events: &[crate::event::GameEvent]) -> Vec<Objective> {
    events
        .iter()
        .filter_map(|e| match &e.event {
            Event::ObjectiveUpdated { id, progress } => Some(Objective {
                id: id.clone(),
                description: id.clone(),
                progress: progress.clamp(0.0, 1.0),
                completed: false,
            }),
            Event::ObjectiveCompleted { id } => Some(Objective {
                id: id.clone(),
                description: id.clone(),
                progress: 1.0,
                completed: true,
            }),
            _ => None,
        })
        .collect()
}

fn empty_observation(tick: u64, elapsed: f32, slot: &AgentSlot) -> Observation {
    Observation {
        tick,
        elapsed_secs: elapsed,
        self_state: SelfState {
            agent_id: slot.agent.id(),
            entity_id: EntityId(0),
            runtime_profile: slot.agent.runtime_profile(),
            position: Vec2::ZERO,
            rotation: 0.0,
            velocity: Vec2::ZERO,
            health: None,
            max_health: None,
            team: Team::None,
            cooldowns: build_cooldowns(slot),
            combat_loadout: None,
            skills: vec![],
            inventory: None,
            companion_roster: None,
            encounter: None,
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
    let Some(actor_slot_index) = agents
        .iter()
        .position(|s| s.agent.id() == agent_action.agent_id)
    else {
        return;
    };
    let Some(entity) = agents[actor_slot_index].entity_id else {
        return;
    };

    let mut set_attack_cooldown = false;
    let attack_range = attack_range_for(ecs, entity);

    match &agent_action.action {
        Action::Move { direction } => {
            if let Ok(mut vel) = ecs.get::<&mut Velocity>(entity) {
                let movement = ecs.get::<&Movement>(entity).map(|m| *m).unwrap_or_default();
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
            if agents[actor_slot_index].attack_cooldown_remaining > 0 {
                return;
            }

            if let Some(target_entity) = select_attack_target(ecs, entity, None, attack_range) {
                let applied = apply_attack(
                    ecs,
                    agents,
                    events,
                    entity,
                    target_entity,
                    agent_action.agent_id,
                );
                if applied > 0.0 {
                    set_attack_cooldown = true;
                }
            }
        }
        Action::AttackTarget { target } => {
            if agents[actor_slot_index].attack_cooldown_remaining > 0 {
                return;
            }

            if let Some(target_entity) = find_entity_by_id(ecs, target.0) {
                if target_entity != entity && in_range(ecs, entity, target_entity, attack_range) {
                    let applied = apply_attack(
                        ecs,
                        agents,
                        events,
                        entity,
                        target_entity,
                        agent_action.agent_id,
                    );
                    if applied > 0.0 {
                        set_attack_cooldown = true;
                    }
                }
            }
        }
        Action::CaptureCreature { target, tool_slot } => {
            if let Some(target_entity) = find_entity_by_id(ecs, target.0) {
                let _ = capture_creature(
                    ecs,
                    events,
                    entity,
                    target_entity,
                    agent_action.agent_id,
                    *tool_slot,
                );
            }
        }
        Action::SummonCompanion { slot } => {
            summon_companion(ecs, events, entity, agent_action.agent_id, *slot);
        }
        Action::CommandCompanion {
            slot,
            command,
            target,
        } => {
            command_companion(
                ecs,
                agents,
                events,
                entity,
                agent_action.agent_id,
                *slot,
                *command,
                *target,
            );
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
            if let Some(target_entity) = select_interaction_target(ecs, entity, None) {
                let target_id = EntityId(target_entity.id() as u64);
                if let Some(slot) = agents.get_mut(actor_slot_index) {
                    slot.agent.on_interact(target_id);
                }
                if let Ok(transform) = ecs.get::<&Transform>(entity) {
                    events.emit(
                        transform.position,
                        Event::Custom {
                            name: "interact".into(),
                            data: target_id.0.to_string(),
                        },
                    );
                }
            }
        }
        Action::InteractWith { target } => {
            if let Some(target_entity) = find_entity_by_id(ecs, target.0) {
                if in_range(ecs, entity, target_entity, INTERACT_RANGE) {
                    if let Some(slot) = agents.get_mut(actor_slot_index) {
                        slot.agent.on_interact(*target);
                    }
                    if let Ok(transform) = ecs.get::<&Transform>(entity) {
                        events.emit(
                            transform.position,
                            Event::Custom {
                                name: "interact".into(),
                                data: target.0.to_string(),
                            },
                        );
                    }
                }
            }
        }
        Action::GatherResource { target, skill } => {
            if let Some(target_entity) = find_entity_by_id(ecs, target.0) {
                let _ = gather_resource(ecs, events, entity, target_entity, *skill);
            }
        }
        Action::Loot { target } => {
            if let Some(target_entity) = find_entity_by_id(ecs, target.0) {
                let _ = loot_container(ecs, events, entity, target_entity);
            }
        }
        Action::SetAutoRetaliate { enabled } => {
            if let Ok(mut loadout) = ecs.get::<&mut CombatLoadout>(entity) {
                loadout.auto_retaliate = *enabled;
                if let Ok(transform) = ecs.get::<&Transform>(entity) {
                    events.emit(
                        transform.position,
                        Event::AutoRetaliateSet {
                            entity: EntityId(entity.id() as u64),
                            enabled: *enabled,
                        },
                    );
                }
            }
        }
        Action::Idle => {}
        _ => {
            log::debug!("Unhandled action: {:?}", agent_action.action);
        }
    }

    if set_attack_cooldown {
        let duration = attack_cooldown_for(
            ecs,
            entity,
            agents[actor_slot_index].agent.constraints().attack_cooldown,
        );
        agents[actor_slot_index].attack_cooldown_remaining = duration;
    }
}

const ATTACK_RANGE: f32 = 80.0;
const INTERACT_RANGE: f32 = 50.0;
const CAPTURE_RANGE: f32 = 60.0;
const COMPANION_COMMAND_RANGE: f32 = 160.0;
const CAPTURE_HEALTH_THRESHOLD: f32 = 0.35;
const BASE_ATTACK_DAMAGE: f32 = 10.0;

fn find_entity_by_id(ecs: &hecs::World, target_id: u64) -> Option<hecs::Entity> {
    ecs.query::<()>()
        .iter()
        .find_map(|(entity, _)| (entity.id() as u64 == target_id).then_some(entity))
}

fn attack_range_for(ecs: &hecs::World, entity: hecs::Entity) -> f32 {
    ecs.get::<&CombatLoadout>(entity)
        .map(|loadout| loadout.attack_range.max(1.0))
        .unwrap_or(ATTACK_RANGE)
}

fn attack_cooldown_for(ecs: &hecs::World, entity: hecs::Entity, fallback: u32) -> u32 {
    ecs.get::<&CombatLoadout>(entity)
        .map(|loadout| loadout.attack_speed_ticks.max(1))
        .unwrap_or(fallback.max(1))
}

fn attack_damage_for(ecs: &hecs::World, entity: hecs::Entity) -> f32 {
    ecs.get::<&CombatLoadout>(entity)
        .map(|loadout| loadout.max_hit.max(1.0))
        .unwrap_or(BASE_ATTACK_DAMAGE)
}

fn in_range(ecs: &hecs::World, source: hecs::Entity, target: hecs::Entity, range: f32) -> bool {
    let Ok(source_transform) = ecs.get::<&Transform>(source) else {
        return false;
    };
    let Ok(target_transform) = ecs.get::<&Transform>(target) else {
        return false;
    };
    source_transform
        .position
        .distance(target_transform.position)
        <= range
}

fn is_hostile_target(ecs: &hecs::World, source: hecs::Entity, target: hecs::Entity) -> bool {
    let source_team = ecs
        .get::<&Label>(source)
        .map(|label| label.team)
        .unwrap_or(Team::None);
    let target_team = ecs
        .get::<&Label>(target)
        .map(|label| label.team)
        .unwrap_or(Team::None);

    if source_team == Team::None || target_team == Team::None {
        return true;
    }
    source_team.is_hostile_to(&target_team)
}

fn select_attack_target(
    ecs: &hecs::World,
    source: hecs::Entity,
    explicit_target: Option<hecs::Entity>,
    attack_range: f32,
) -> Option<hecs::Entity> {
    if let Some(target) = explicit_target {
        if target != source
            && in_range(ecs, source, target, attack_range)
            && is_hostile_target(ecs, source, target)
        {
            return Some(target);
        }
        return None;
    }

    let source_pos = ecs.get::<&Transform>(source).ok()?.position;
    let mut candidates: Vec<(hecs::Entity, f32)> = ecs
        .query::<(&Transform, &Health)>()
        .iter()
        .filter_map(|(entity, (transform, health))| {
            if entity == source || health.is_dead() {
                return None;
            }
            let distance = source_pos.distance(transform.position);
            if distance > attack_range || !is_hostile_target(ecs, source, entity) {
                return None;
            }
            Some((entity, distance))
        })
        .collect();

    candidates.sort_by(|(a_entity, a_distance), (b_entity, b_distance)| {
        a_distance
            .partial_cmp(b_distance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a_entity.id().cmp(&b_entity.id()))
    });

    candidates.first().map(|(entity, _)| *entity)
}

fn add_item_to_inventory(inventory: &mut Inventory, item: ItemStack) -> bool {
    if item.stackable {
        if let Some(existing) = inventory
            .items
            .iter_mut()
            .find(|existing| existing.item_id == item.item_id)
        {
            existing.quantity = existing.quantity.saturating_add(item.quantity);
            return true;
        }
    }

    if inventory.items.len() >= inventory.capacity as usize {
        return false;
    }

    inventory.items.push(item);
    true
}

fn consume_inventory_slot(inventory: &mut Inventory, slot: u8) -> bool {
    let index = slot as usize;
    if index >= inventory.items.len() {
        return false;
    }

    let item = &mut inventory.items[index];
    if item.quantity == 0 {
        return false;
    }

    if item.stackable && item.quantity > 1 {
        item.quantity -= 1;
    } else {
        inventory.items.remove(index);
    }

    true
}

fn next_level_xp(level: u16) -> u32 {
    83 + u32::from(level.saturating_sub(1)) * 97
}

fn recompute_skillbook_totals(skill_book: &mut SkillBook) {
    skill_book.total_level = skill_book.skills.iter().map(|skill| skill.level).sum();

    let combat_skills = [
        SkillKind::Attack,
        SkillKind::Strength,
        SkillKind::Defence,
        SkillKind::Ranged,
        SkillKind::Magic,
        SkillKind::Constitution,
    ];
    let combat_sum: u32 = skill_book
        .skills
        .iter()
        .filter(|skill| combat_skills.contains(&skill.kind))
        .map(|skill| u32::from(skill.level))
        .sum();
    skill_book.combat_level = (combat_sum / combat_skills.len() as u32).max(3) as u16;
}

fn award_skill_experience(
    skill_book: &mut SkillBook,
    skill: SkillKind,
    amount: u32,
) -> Option<u16> {
    let progress_index = skill_book
        .skills
        .iter()
        .position(|entry| entry.kind == skill)?;
    let mut level_changed = false;

    let new_level = {
        let progress = &mut skill_book.skills[progress_index];
        progress.experience = progress.experience.saturating_add(amount);

        while progress.experience >= progress.xp_to_next_level {
            progress.experience -= progress.xp_to_next_level;
            progress.level = progress.level.saturating_add(1);
            progress.xp_to_next_level = next_level_xp(progress.level);
            level_changed = true;
        }

        level_changed.then_some(progress.level)
    };

    recompute_skillbook_totals(skill_book);
    new_level
}

fn select_interaction_target(
    ecs: &hecs::World,
    source: hecs::Entity,
    explicit_target: Option<hecs::Entity>,
) -> Option<hecs::Entity> {
    if let Some(target) = explicit_target {
        return (target != source && in_range(ecs, source, target, INTERACT_RANGE))
            .then_some(target);
    }

    let source_pos = ecs.get::<&Transform>(source).ok()?.position;
    let mut candidates: Vec<(hecs::Entity, f32)> = ecs
        .query::<(&Transform,)>()
        .iter()
        .filter_map(|(entity, (transform,))| {
            if entity == source {
                return None;
            }
            let distance = source_pos.distance(transform.position);
            (distance <= INTERACT_RANGE).then_some((entity, distance))
        })
        .collect();
    candidates.sort_by(|(a_entity, a_distance), (b_entity, b_distance)| {
        a_distance
            .partial_cmp(b_distance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a_entity.id().cmp(&b_entity.id()))
    });
    candidates.first().map(|(entity, _)| *entity)
}

fn summon_companion(
    ecs: &mut hecs::World,
    events: &mut EventBus,
    entity: hecs::Entity,
    agent_id: crate::id::AgentId,
    slot: u8,
) -> bool {
    let Some(companion) = ecs
        .get::<&CompanionRoster>(entity)
        .ok()
        .and_then(|roster| roster.creatures.get(slot as usize).cloned())
    else {
        return false;
    };

    if let Ok(mut roster) = ecs.get::<&mut CompanionRoster>(entity) {
        roster.active_slot = Some(slot);
    } else {
        return false;
    }

    if let Ok(transform) = ecs.get::<&Transform>(entity) {
        events.emit(
            transform.position,
            Event::CompanionSummoned {
                agent_id,
                species_id: companion.creature.species_id,
            },
        );
    }

    true
}

fn command_companion(
    ecs: &mut hecs::World,
    agents: &mut [AgentSlot],
    events: &mut EventBus,
    entity: hecs::Entity,
    agent_id: crate::id::AgentId,
    slot: u8,
    command: CompanionCommand,
    target: Option<EntityId>,
) -> bool {
    let Some(companion) = ecs
        .get::<&CompanionRoster>(entity)
        .ok()
        .and_then(|roster| roster.creatures.get(slot as usize).cloned())
    else {
        return false;
    };

    if let Ok(mut roster) = ecs.get::<&mut CompanionRoster>(entity) {
        roster.active_slot = match command {
            CompanionCommand::Recall => None,
            _ => Some(slot),
        };
    } else {
        return false;
    }

    let entity_id = EntityId(entity.id() as u64);
    let origin = ecs
        .get::<&Transform>(entity)
        .map(|transform| transform.position)
        .unwrap_or(Vec2::ZERO);
    events.emit(
        origin,
        Event::CompanionCommandIssued {
            agent_id,
            slot,
            command: format!("{command:?}"),
            target,
        },
    );

    if matches!(command, CompanionCommand::Attack) {
        let Some(target_id) = target else {
            return true;
        };
        let Some(target_entity) = find_entity_by_id(ecs, target_id.0) else {
            return false;
        };
        if !in_range(ecs, entity, target_entity, COMPANION_COMMAND_RANGE) {
            return false;
        }

        let companion_damage = f32::from(companion.creature.level).max(4.0);
        let _ = apply_damage_from_source(
            ecs,
            agents,
            events,
            entity,
            target_entity,
            agent_id,
            companion_damage,
            "companion_attack",
        );
    } else if matches!(command, CompanionCommand::Recall) {
        events.emit(
            origin,
            Event::EncounterEnded {
                encounter_id: 0,
                victory: false,
            },
        );
    }

    let _ = entity_id;
    true
}

fn capture_creature(
    ecs: &mut hecs::World,
    events: &mut EventBus,
    entity: hecs::Entity,
    target_entity: hecs::Entity,
    agent_id: crate::id::AgentId,
    tool_slot: Option<u8>,
) -> bool {
    if !in_range(ecs, entity, target_entity, CAPTURE_RANGE) {
        return false;
    }

    let Some(creature) = ecs
        .get::<&CreatureIdentity>(target_entity)
        .ok()
        .map(|creature| (*creature).clone())
    else {
        return false;
    };
    if !creature.is_wild {
        return false;
    }

    let target_health_fraction = ecs
        .get::<&Health>(target_entity)
        .map(|health| health.current / health.max.max(1.0))
        .unwrap_or(1.0);
    if target_health_fraction > CAPTURE_HEALTH_THRESHOLD {
        return false;
    }

    let capture_allowed = ecs
        .get::<&EncounterState>(entity)
        .map(|encounter| encounter.capture_allowed)
        .unwrap_or(true);
    if !capture_allowed {
        return false;
    }

    if let Some(slot) = tool_slot {
        let Ok(mut inventory) = ecs.get::<&mut Inventory>(entity) else {
            return false;
        };
        if !consume_inventory_slot(&mut inventory, slot) {
            return false;
        }
    }

    let current_max_health = ecs
        .get::<&Health>(target_entity)
        .map(|health| health.max)
        .unwrap_or(10.0);
    let current_health = ecs
        .get::<&Health>(target_entity)
        .map(|health| health.current.max(1.0))
        .unwrap_or(current_max_health);
    let combat_style = ecs
        .get::<&CombatLoadout>(target_entity)
        .map(|loadout| loadout.style)
        .unwrap_or(CombatStyle::Summoning);
    let captured = CompanionCreature {
        creature: creature.clone(),
        nickname: None,
        current_health,
        max_health: current_max_health,
        combat_style,
        mood: 1.0,
    };

    let next_slot = {
        let Ok(mut roster) = ecs.get::<&mut CompanionRoster>(entity) else {
            return false;
        };
        if roster.creatures.len() >= roster.party_capacity as usize {
            return false;
        }
        let slot = roster.creatures.len() as u8;
        roster.creatures.push(captured);
        roster.active_slot.get_or_insert(slot);
        slot
    };

    if let Ok(mut encounter) = ecs.get::<&mut EncounterState>(entity) {
        encounter.capture_allowed = false;
        encounter.in_combat = false;
    }

    let origin = ecs
        .get::<&Transform>(entity)
        .map(|transform| transform.position)
        .unwrap_or(Vec2::ZERO);
    let encounter_id = ecs
        .get::<&EncounterState>(entity)
        .map(|encounter| encounter.encounter_id)
        .unwrap_or(0);
    let species_id = creature.species_id.clone();
    events.emit(
        origin,
        Event::CreatureCaptured {
            agent_id,
            species_id: species_id.clone(),
            nickname: None,
        },
    );
    events.emit(
        origin,
        Event::CompanionSummoned {
            agent_id,
            species_id: species_id.clone(),
        },
    );
    if encounter_id != 0 {
        events.emit(
            origin,
            Event::EncounterEnded {
                encounter_id,
                victory: true,
            },
        );
    }

    let _ = next_slot;
    ecs.despawn(target_entity).is_ok()
}

fn gather_resource(
    ecs: &mut hecs::World,
    events: &mut EventBus,
    entity: hecs::Entity,
    target_entity: hecs::Entity,
    skill: SkillKind,
) -> bool {
    if !in_range(ecs, entity, target_entity, INTERACT_RANGE) {
        return false;
    }

    let Some(resource) = ecs
        .get::<&ResourceNode>(target_entity)
        .ok()
        .map(|resource| (*resource).clone())
    else {
        return false;
    };
    if resource.skill != skill || resource.remaining_uses == 0 {
        return false;
    }

    let Ok(mut inventory) = ecs.get::<&mut Inventory>(entity) else {
        return false;
    };
    if !add_item_to_inventory(&mut inventory, resource.yield_item.clone()) {
        return false;
    }

    if let Ok(mut node) = ecs.get::<&mut ResourceNode>(target_entity) {
        node.remaining_uses = node.remaining_uses.saturating_sub(1);
    }

    let new_level = if let Ok(mut skill_book) = ecs.get::<&mut SkillBook>(entity) {
        let level = award_skill_experience(&mut skill_book, skill, resource.experience)
            .or_else(|| {
                skill_book
                    .skills
                    .iter()
                    .find(|entry| entry.kind == skill)
                    .map(|entry| entry.level)
            })
            .unwrap_or(1);
        Some(level)
    } else {
        None
    };

    let origin = ecs
        .get::<&Transform>(entity)
        .map(|transform| transform.position)
        .unwrap_or(Vec2::ZERO);
    events.emit(
        origin,
        Event::ResourceGathered {
            entity: EntityId(entity.id() as u64),
            resource: EntityId(target_entity.id() as u64),
            skill: format!("{skill:?}"),
            item_id: resource.yield_item.item_id.clone(),
            quantity: resource.yield_item.quantity,
        },
    );
    if let Some(level) = new_level {
        events.emit(
            origin,
            Event::SkillXpGained {
                entity: EntityId(entity.id() as u64),
                skill: format!("{skill:?}"),
                amount: resource.experience,
                new_level: level,
            },
        );
    }

    true
}

fn loot_container(
    ecs: &mut hecs::World,
    events: &mut EventBus,
    entity: hecs::Entity,
    target_entity: hecs::Entity,
) -> bool {
    if !in_range(ecs, entity, target_entity, INTERACT_RANGE) {
        return false;
    }

    let Some(loot) = ecs
        .get::<&LootContainer>(target_entity)
        .ok()
        .map(|loot| (*loot).clone())
    else {
        return false;
    };
    if loot.claimed {
        return false;
    }

    let Ok(mut inventory) = ecs.get::<&mut Inventory>(entity) else {
        return false;
    };
    let mut preview = (*inventory).clone();
    for item in loot.items.iter().cloned() {
        if !add_item_to_inventory(&mut preview, item) {
            return false;
        }
    }
    preview.coins = preview.coins.saturating_add(loot.coins);
    *inventory = preview;

    if let Ok(mut loot_container) = ecs.get::<&mut LootContainer>(target_entity) {
        loot_container.claimed = true;
        loot_container.items.clear();
        loot_container.coins = 0;
    }

    let origin = ecs
        .get::<&Transform>(entity)
        .map(|transform| transform.position)
        .unwrap_or(Vec2::ZERO);
    events.emit(
        origin,
        Event::LootClaimed {
            entity: EntityId(entity.id() as u64),
            source: EntityId(target_entity.id() as u64),
            coins: loot.coins,
            item_count: loot.items.len(),
        },
    );

    true
}

fn apply_attack(
    ecs: &mut hecs::World,
    agents: &mut [AgentSlot],
    events: &mut EventBus,
    attacker_entity: hecs::Entity,
    target_entity: hecs::Entity,
    attacker_agent_id: crate::id::AgentId,
) -> f32 {
    let damage = attack_damage_for(ecs, attacker_entity);
    apply_damage_from_source(
        ecs,
        agents,
        events,
        attacker_entity,
        target_entity,
        attacker_agent_id,
        damage,
        "attack",
    )
}

fn apply_damage_from_source(
    ecs: &mut hecs::World,
    agents: &mut [AgentSlot],
    events: &mut EventBus,
    attacker_entity: hecs::Entity,
    target_entity: hecs::Entity,
    attacker_agent_id: crate::id::AgentId,
    damage: f32,
    sound_name: &str,
) -> f32 {
    let attacker_pos = ecs
        .get::<&Transform>(attacker_entity)
        .map(|transform| transform.position)
        .unwrap_or(Vec2::ZERO);
    if !is_hostile_target(ecs, attacker_entity, target_entity) {
        return 0.0;
    }
    events.emit(
        attacker_pos,
        Event::Sound {
            name: sound_name.into(),
            intensity: 1.0,
        },
    );

    let target_pos = ecs
        .get::<&Transform>(target_entity)
        .map(|transform| transform.position)
        .unwrap_or(attacker_pos);

    let mut applied_damage = 0.0;
    let mut target_dead = false;

    if let Ok(mut health) = ecs.get::<&mut Health>(target_entity) {
        applied_damage = health.damage(damage);
        target_dead = health.is_dead();
    }

    if applied_damage <= 0.0 {
        return 0.0;
    }

    let attacker_id = EntityId(attacker_entity.id() as u64);
    let target_id = EntityId(target_entity.id() as u64);
    events.emit(
        target_pos,
        Event::Damage {
            source: Some(attacker_id),
            target: target_id,
            amount: applied_damage,
        },
    );

    if target_dead {
        events.emit(
            target_pos,
            Event::Kill {
                killer: Some(attacker_id),
                victim: target_id,
            },
        );
    }

    for slot in agents.iter_mut() {
        if slot.entity_id == Some(target_entity) {
            slot.agent
                .on_damage(applied_damage, Some(attacker_agent_id));
            if target_dead {
                slot.agent.on_death();
            }
            break;
        }
    }

    applied_damage
}

/// Simple movement integration (before full physics)
fn step_movement(ecs: &mut hecs::World) {
    for (_, (transform, velocity)) in ecs.query_mut::<(&mut Transform, &Velocity)>() {
        transform.position += velocity.linear * TICK_DURATION_SECS;
        transform.rotation += velocity.angular * TICK_DURATION_SECS;
    }
}

fn step_camera_controllers(ecs: &mut hecs::World) {
    step_orbit_camera_controllers(ecs);
    step_follow_camera_controllers(ecs);
    step_fly_camera_controllers(ecs);
}

fn step_orbit_camera_controllers(ecs: &mut hecs::World) {
    for (_, (camera, controller)) in ecs.query_mut::<(&mut Camera3D, &mut OrbitCameraController)>()
    {
        let target = Vec3::from_array(controller.target);
        let yaw = controller.yaw + controller.yaw_speed * TICK_DURATION_SECS;
        let pitch_delta = controller.pitch_speed * TICK_DURATION_SECS;

        controller.yaw = yaw;
        controller.pitch = (controller.pitch + pitch_delta).clamp(-1.47, 1.47);
        let radius = controller
            .radius
            .clamp(controller.min_radius, controller.max_radius);
        let yaw_sin = yaw.sin();
        let yaw_cos = yaw.cos();
        let pitch_cos = controller.pitch.cos();
        let pitch_sin = controller.pitch.sin();

        let offset = Vec3::new(
            radius * pitch_cos * yaw_sin,
            -radius * pitch_sin,
            radius * pitch_cos * yaw_cos,
        );

        camera.target = target;
        camera.position = target + offset;
        camera.near_plane = 0.05_f32.max(camera.near_plane);
        camera.fov_y_radians = camera.fov_y_radians.max(0.2);
        camera.far_plane = 10_000.0_f32.max(camera.far_plane);
    }
}

fn step_follow_camera_controllers(ecs: &mut hecs::World) {
    let mut target_positions = std::collections::HashMap::<u32, Vec3>::new();

    for (entity, (transform,)) in ecs.query::<(&Transform3D,)>().iter() {
        target_positions.insert(entity.id(), transform.position);
    }

    for (_, (camera, controller)) in ecs.query_mut::<(&mut Camera3D, &mut FollowCameraController)>()
    {
        if controller.target == FollowCameraController::default().target {
            continue;
        }

        let target = match target_positions.get(&(controller.target as u32)) {
            Some(position) => *position,
            None => continue,
        };

        let offset = Vec3::from_array(controller.offset);
        let desired_position = target + offset;
        let follow_t = (controller.follow_speed * TICK_DURATION_SECS).clamp(0.0, 1.0);
        camera.position = camera.position.lerp(desired_position, follow_t);
        camera.target = camera.target.lerp(target, follow_t);
    }
}

fn step_fly_camera_controllers(ecs: &mut hecs::World) {
    for (_, (camera, controller)) in ecs.query_mut::<(&mut Camera3D, &mut FlyCameraController)>() {
        let move_speed = controller.move_speed.max(0.0);
        let input = Vec3::from_array(controller.move_input);
        if input.length_squared() <= 0.00001
            && controller.yaw_delta.abs() <= 0.0001
            && controller.pitch_delta.abs() <= 0.0001
        {
            continue;
        }

        let yaw = controller.yaw + controller.yaw_delta * TICK_DURATION_SECS;
        let pitch =
            (controller.pitch + controller.pitch_delta * TICK_DURATION_SECS).clamp(-1.45, 1.45);
        let distance = (camera.target - camera.position).length().max(0.1);

        let forward = Vec3::new(
            yaw.cos() * pitch.cos(),
            pitch.sin(),
            yaw.sin() * pitch.cos(),
        );
        let right = forward.cross(camera.up).normalize_or_zero();
        let up = camera.up;

        camera.position += (right * input.x + up * input.y + forward * input.z)
            * (move_speed * TICK_DURATION_SECS);
        camera.target = camera.position + forward * distance;
        controller.yaw = yaw;
        controller.pitch = pitch;

        let damping = controller.damping.clamp(0.0, 1.0);
        controller.move_input = [
            input.x * (1.0 - damping * TICK_DURATION_SECS),
            input.y * (1.0 - damping * TICK_DURATION_SECS),
            input.z * (1.0 - damping * TICK_DURATION_SECS),
        ];
        controller.yaw_delta = 0.0;
        controller.pitch_delta = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::AgentConstraints;
    use crate::agent::{Agent, AgentType};
    use crate::id::{AgentId, EntityId};
    use crate::telemetry::{AgentToolCallTrace, ToolCallStatus};
    use std::sync::{Arc, Mutex};

    struct RecordingAgent {
        id: AgentId,
        constraints: AgentConstraints,
        planned_actions: Vec<Action>,
        observations: Arc<Mutex<Vec<Observation>>>,
    }

    impl RecordingAgent {
        fn new(
            planned_actions: Vec<Action>,
            observations: Arc<Mutex<Vec<Observation>>>,
        ) -> (Self, AgentId) {
            let id = AgentId::new();
            (
                Self {
                    id,
                    constraints: AgentConstraints::default(),
                    planned_actions,
                    observations,
                },
                id,
            )
        }
    }

    impl Agent for RecordingAgent {
        fn id(&self) -> AgentId {
            self.id
        }

        fn agent_type(&self) -> AgentType {
            AgentType::ScriptedNpc
        }

        fn observe(&mut self, observation: Observation) {
            self.observations.lock().unwrap().push(observation);
        }

        fn decide(&mut self) -> Vec<Action> {
            std::mem::take(&mut self.planned_actions)
        }

        fn constraints(&self) -> &AgentConstraints {
            &self.constraints
        }

        fn constraints_mut(&mut self) -> &mut AgentConstraints {
            &mut self.constraints
        }
    }

    struct ToolTracingAgent {
        id: AgentId,
        constraints: AgentConstraints,
        tool_calls: Vec<AgentToolCallTrace>,
    }

    impl ToolTracingAgent {
        fn new(tool_calls: Vec<AgentToolCallTrace>) -> Self {
            Self {
                id: AgentId::new(),
                constraints: AgentConstraints::default(),
                tool_calls,
            }
        }
    }

    impl Agent for ToolTracingAgent {
        fn id(&self) -> AgentId {
            self.id
        }

        fn agent_type(&self) -> AgentType {
            AgentType::LlmAgent
        }

        fn observe(&mut self, _observation: Observation) {}

        fn decide(&mut self) -> Vec<Action> {
            vec![Action::Idle]
        }

        fn constraints(&self) -> &AgentConstraints {
            &self.constraints
        }

        fn constraints_mut(&mut self) -> &mut AgentConstraints {
            &mut self.constraints
        }

        fn drain_tool_calls(&mut self) -> Vec<AgentToolCallTrace> {
            std::mem::take(&mut self.tool_calls)
        }
    }

    fn spawn_actor(ecs: &mut hecs::World, pos: Vec2, team: Team, health: Health) -> hecs::Entity {
        ecs.spawn((
            Transform {
                position: pos,
                rotation: 0.0,
                scale: Vec2::ONE,
            },
            Velocity::default(),
            Movement::default(),
            Label {
                name: "actor".to_string(),
                team,
            },
            health,
            Perception::default(),
        ))
    }

    #[test]
    fn attack_target_applies_damage_and_sets_cooldown() {
        let mut ecs = hecs::World::new();
        let attacker_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        let target_entity = spawn_actor(
            &mut ecs,
            Vec2::new(20.0, 0.0),
            Team::Team(2),
            Health::new(100.0),
        );
        let target_id = EntityId(target_entity.id() as u64);

        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(attacker_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::AttackTarget { target: target_id },
            }],
            &mut next_entity_id,
        );

        let target_health = ecs.get::<&Health>(target_entity).unwrap();
        assert_eq!(target_health.current, 90.0);
        assert_eq!(result.actions_processed, 1);
        assert_eq!(
            agents[0].attack_cooldown_remaining,
            agents[0].agent.constraints().attack_cooldown
        );
        assert!(result.events.iter().any(|event| matches!(
            event.event,
            Event::Damage {
                target,
                amount,
                ..
            } if target == target_id && (amount - BASE_ATTACK_DAMAGE).abs() < f32::EPSILON
        )));
    }

    #[test]
    fn attack_target_rejects_self_target_without_damage() {
        let mut ecs = hecs::World::new();
        let attacker_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        let self_target_id = EntityId(attacker_entity.id() as u64);

        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(attacker_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::AttackTarget {
                    target: self_target_id,
                },
            }],
            &mut next_entity_id,
        );

        let self_health = ecs.get::<&Health>(attacker_entity).unwrap();
        assert_eq!(self_health.current, 100.0);
        assert_eq!(agents[0].attack_cooldown_remaining, 0);
        assert!(events
            .last_events()
            .iter()
            .chain(events.current_events().iter())
            .all(|event| !matches!(event.event, Event::Damage { .. })));
    }

    #[test]
    fn attack_target_respects_invulnerability() {
        let mut ecs = hecs::World::new();
        let attacker_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        let mut invulnerable_target = Health::new(100.0);
        invulnerable_target.invulnerable = true;
        let target_entity = spawn_actor(
            &mut ecs,
            Vec2::new(20.0, 0.0),
            Team::Team(2),
            invulnerable_target,
        );
        let target_id = EntityId(target_entity.id() as u64);

        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(attacker_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::AttackTarget { target: target_id },
            }],
            &mut next_entity_id,
        );

        let target_health = ecs.get::<&Health>(target_entity).unwrap();
        assert_eq!(target_health.current, 100.0);
        assert_eq!(agents[0].attack_cooldown_remaining, 0);
        assert!(result
            .events
            .iter()
            .all(|event| !matches!(event.event, Event::Damage { .. })));
    }

    #[test]
    fn observations_include_cooldowns_messages_and_objectives() {
        let mut ecs = hecs::World::new();
        let observer_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );

        let observed_messages = Arc::new(Mutex::new(Vec::<Observation>::new()));
        let (agent, _agent_id) = RecordingAgent::new(vec![], observed_messages.clone());
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(observer_entity);
        slot.attack_cooldown_remaining = 5;
        let mut agents = vec![slot];

        let speaker_id = AgentId::new();
        let mut events = EventBus::new();
        events.emit(
            Vec2::new(0.0, 0.0),
            Event::AgentSpoke {
                agent_id: speaker_id,
                message: "ready".to_string(),
                volume: 1.0,
            },
        );
        events.emit(
            Vec2::new(0.0, 0.0),
            Event::ObjectiveUpdated {
                id: "capture-point".to_string(),
                progress: 0.25,
            },
        );
        events.flush(1);

        let mut next_entity_id = 1;
        execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![],
            &mut next_entity_id,
        );

        let snapshots = observed_messages.lock().unwrap();
        let observation = snapshots.last().expect("expected an observation");

        let attack_cd = observation
            .self_state
            .cooldowns
            .iter()
            .find(|cd| cd.name == "attack")
            .expect("attack cooldown should be present");
        assert_eq!(attack_cd.remaining_ticks, 4);

        assert!(observation
            .messages
            .iter()
            .any(|message| { message.content == "ready" && message.from == speaker_id }));
        assert!(observation.objectives.iter().any(|objective| {
            objective.id == "capture-point"
                && (objective.progress - 0.25).abs() < f32::EPSILON
                && !objective.completed
        }));
    }

    #[test]
    fn observations_include_runtime_profile_and_mmo_state() {
        let mut ecs = hecs::World::new();
        let observer_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        let wild_entity = spawn_actor(
            &mut ecs,
            Vec2::new(16.0, 0.0),
            Team::None,
            Health::new(24.0),
        );

        ecs.insert(
            observer_entity,
            (
                CombatLoadout {
                    style: CombatStyle::Magic,
                    attack_range: 240.0,
                    attack_speed_ticks: 24,
                    max_hit: 18.0,
                    auto_retaliate: true,
                    equipped_weapon: Some("oak-staff".to_string()),
                    offhand_item: Some("capture-focus".to_string()),
                    active_ability_bar: vec!["wind-strike".to_string(), "bind".to_string()],
                },
                SkillBook {
                    combat_level: 12,
                    total_level: 38,
                    skills: vec![
                        SkillProgress::new(SkillKind::Magic, 12, 1_980, 2_744),
                        SkillProgress::new(SkillKind::Taming, 7, 650, 801),
                    ],
                },
                Inventory {
                    capacity: 28,
                    carried_weight: 3.5,
                    coins: 420,
                    items: vec![ItemStack {
                        item_id: "capture-orb".to_string(),
                        display_name: "Capture Orb".to_string(),
                        quantity: 3,
                        stackable: true,
                    }],
                },
                CompanionRoster {
                    active_slot: Some(0),
                    party_capacity: 6,
                    creatures: vec![CompanionCreature {
                        creature: CreatureIdentity {
                            species_id: "ember-fox".to_string(),
                            species_name: "Ember Fox".to_string(),
                            elemental_affinity: "fire".to_string(),
                            level: 9,
                            temperament: CreatureTemperament::Loyal,
                            capture_difficulty: 0.3,
                            is_wild: false,
                        },
                        nickname: Some("Cinder".to_string()),
                        current_health: 22.0,
                        max_health: 24.0,
                        combat_style: CombatStyle::Summoning,
                        mood: 0.9,
                    }],
                },
                EncounterState {
                    encounter_id: 55,
                    kind: EncounterKind::WildCreature,
                    threat_level: 0.7,
                    primary_target: Some(EntityId(wild_entity.id() as u64)),
                    active_turn_owner: Some(EntityId(observer_entity.id() as u64)),
                    capture_allowed: true,
                    in_combat: true,
                },
            ),
        )
        .unwrap();

        ecs.insert(
            wild_entity,
            (
                CombatLoadout {
                    style: CombatStyle::Summoning,
                    attack_range: 64.0,
                    attack_speed_ticks: 30,
                    max_hit: 8.0,
                    auto_retaliate: true,
                    equipped_weapon: None,
                    offhand_item: None,
                    active_ability_bar: vec!["scratch".to_string()],
                },
                CreatureIdentity {
                    species_id: "moss-turtle".to_string(),
                    species_name: "Moss Turtle".to_string(),
                    elemental_affinity: "nature".to_string(),
                    level: 6,
                    temperament: CreatureTemperament::Timid,
                    capture_difficulty: 0.45,
                    is_wild: true,
                },
            ),
        )
        .unwrap();

        let observed_messages = Arc::new(Mutex::new(Vec::<Observation>::new()));
        let (agent, _agent_id) = RecordingAgent::new(vec![], observed_messages.clone());
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(observer_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![],
            &mut next_entity_id,
        );

        let snapshots = observed_messages.lock().unwrap();
        let observation = snapshots.last().expect("expected an observation");
        assert_eq!(
            observation.self_state.runtime_profile.role,
            crate::AgentRole::Npc
        );
        assert_eq!(
            observation
                .self_state
                .combat_loadout
                .as_ref()
                .expect("combat loadout")
                .style,
            CombatStyle::Magic
        );
        assert_eq!(observation.self_state.skills.len(), 2);
        assert_eq!(
            observation
                .self_state
                .inventory
                .as_ref()
                .expect("inventory")
                .coins,
            420
        );
        assert_eq!(
            observation
                .self_state
                .companion_roster
                .as_ref()
                .expect("roster")
                .creatures[0]
                .creature
                .species_name,
            "Ember Fox"
        );
        assert!(
            observation
                .self_state
                .encounter
                .as_ref()
                .expect("encounter")
                .capture_allowed
        );

        let visible_creature = observation
            .visible_entities
            .iter()
            .find(|entity| entity.entity_id == EntityId(wild_entity.id() as u64))
            .expect("wild creature should be visible");
        assert_eq!(visible_creature.combat_style, Some(CombatStyle::Summoning));
        assert_eq!(
            visible_creature
                .creature
                .as_ref()
                .expect("creature identity")
                .species_name,
            "Moss Turtle"
        );
    }

    #[test]
    fn tick_result_records_authoritative_agent_trajectory_and_action_trace() {
        let mut ecs = hecs::World::new();
        let mover_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );

        let observed_messages = Arc::new(Mutex::new(Vec::<Observation>::new()));
        let (agent, agent_id) =
            RecordingAgent::new(vec![Action::Move { direction: Vec2::X }], observed_messages);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(mover_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            3,
            vec![],
            &mut next_entity_id,
        );

        assert_eq!(result.telemetry.tick, 3);
        assert_eq!(result.telemetry.agents.len(), 1);

        let telemetry = &result.telemetry.agents[0];
        assert_eq!(telemetry.agent_id, agent_id);
        assert_eq!(telemetry.runtime_profile.agent_type, AgentType::ScriptedNpc);
        let trajectory = telemetry.trajectory.as_ref().expect("trajectory frame");
        assert_eq!(trajectory.start.position, Vec2::ZERO);
        assert!(trajectory.end.position.x > 0.0);
        assert!(trajectory.distance_travelled > 0.0);
        assert!(telemetry.action_trace.iter().any(|trace| {
            trace.stage == ActionLifecycleStage::Submitted
                && matches!(trace.source, ActionSource::AgentDecision)
                && matches!(trace.action, Action::Move { .. })
        }));
        assert!(telemetry.action_trace.iter().any(|trace| {
            trace.stage == ActionLifecycleStage::Executed
                && matches!(trace.source, ActionSource::AgentDecision)
                && matches!(trace.action, Action::Move { .. })
        }));
    }

    #[test]
    fn rejected_external_actions_are_captured_in_tick_telemetry() {
        let mut ecs = hecs::World::new();
        let observer_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );

        let observed_messages = Arc::new(Mutex::new(Vec::<Observation>::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], observed_messages);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(observer_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            4,
            vec![AgentAction {
                agent_id,
                tick: 4,
                action: Action::Move {
                    direction: Vec2::splat(10.0),
                },
            }],
            &mut next_entity_id,
        );

        let telemetry = &result.telemetry.agents[0];
        assert!(telemetry.action_trace.iter().any(|trace| {
            trace.stage == ActionLifecycleStage::Submitted
                && matches!(trace.source, ActionSource::ExternalSubmission)
        }));
        let rejected = telemetry
            .action_trace
            .iter()
            .find(|trace| trace.stage == ActionLifecycleStage::Rejected)
            .expect("rejected action trace");
        assert!(matches!(rejected.source, ActionSource::ExternalSubmission));
        assert!(rejected
            .rejection_reason
            .as_deref()
            .expect("rejection reason")
            .contains("magnitude too large"));
    }

    #[test]
    fn tick_result_captures_tool_call_traces_from_agents() {
        let mut ecs = hecs::World::new();
        let actor_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );

        let tool_trace = AgentToolCallTrace::new(
            8,
            "llm.complete",
            "mock",
            ToolCallStatus::ParseError,
            24,
            128,
            64,
            Some("invalid response".into()),
        );
        let mut slot = AgentSlot::new(Box::new(ToolTracingAgent::new(vec![tool_trace])));
        slot.entity_id = Some(actor_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            8,
            vec![],
            &mut next_entity_id,
        );

        assert_eq!(result.telemetry.agents.len(), 1);
        let telemetry = &result.telemetry.agents[0];
        assert_eq!(telemetry.tool_calls.len(), 1);
        assert_eq!(telemetry.tool_calls[0].status, ToolCallStatus::ParseError);
        assert_eq!(telemetry.tool_calls[0].request_units, 128);
        assert_eq!(
            telemetry.tool_calls[0].error_message.as_deref(),
            Some("invalid response")
        );
    }

    #[test]
    fn attack_target_uses_combat_loadout_damage_range_and_cooldown() {
        let mut ecs = hecs::World::new();
        let attacker_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        let target_entity = spawn_actor(
            &mut ecs,
            Vec2::new(140.0, 0.0),
            Team::Team(2),
            Health::new(100.0),
        );
        ecs.insert(
            attacker_entity,
            (CombatLoadout {
                style: CombatStyle::Ranged,
                attack_range: 180.0,
                attack_speed_ticks: 42,
                max_hit: 17.0,
                auto_retaliate: true,
                equipped_weapon: Some("oak-shortbow".to_string()),
                offhand_item: None,
                active_ability_bar: vec!["rapid-shot".to_string()],
            },),
        )
        .unwrap();

        let target_id = EntityId(target_entity.id() as u64);
        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(attacker_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::AttackTarget { target: target_id },
            }],
            &mut next_entity_id,
        );

        let target_health = ecs.get::<&Health>(target_entity).unwrap();
        assert_eq!(target_health.current, 83.0);
        assert_eq!(agents[0].attack_cooldown_remaining, 42);
        assert!(result.events.iter().any(|event| matches!(
            event.event,
            Event::Damage {
                target,
                amount,
                ..
            } if target == target_id && (amount - 17.0).abs() < f32::EPSILON
        )));
    }

    #[test]
    fn capture_creature_consumes_tool_and_adds_companion() {
        let mut ecs = hecs::World::new();
        let player_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        let wild_entity = spawn_actor(
            &mut ecs,
            Vec2::new(12.0, 0.0),
            Team::None,
            Health {
                current: 6.0,
                max: 30.0,
                armor: 0.0,
                invulnerable: false,
            },
        );
        ecs.insert(
            player_entity,
            (
                Inventory {
                    capacity: 28,
                    carried_weight: 0.0,
                    coins: 0,
                    items: vec![ItemStack {
                        item_id: "capture-orb".to_string(),
                        display_name: "Capture Orb".to_string(),
                        quantity: 2,
                        stackable: true,
                    }],
                },
                CompanionRoster::default(),
                EncounterState {
                    encounter_id: 7,
                    kind: EncounterKind::WildCreature,
                    threat_level: 0.4,
                    primary_target: Some(EntityId(wild_entity.id() as u64)),
                    active_turn_owner: Some(EntityId(player_entity.id() as u64)),
                    capture_allowed: true,
                    in_combat: true,
                },
            ),
        )
        .unwrap();
        ecs.insert(
            wild_entity,
            (
                CreatureIdentity {
                    species_id: "spark-mouse".to_string(),
                    species_name: "Spark Mouse".to_string(),
                    elemental_affinity: "storm".to_string(),
                    level: 5,
                    temperament: CreatureTemperament::Timid,
                    capture_difficulty: 0.25,
                    is_wild: true,
                },
                CombatLoadout {
                    style: CombatStyle::Summoning,
                    ..CombatLoadout::default()
                },
            ),
        )
        .unwrap();

        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(player_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::CaptureCreature {
                    target: EntityId(wild_entity.id() as u64),
                    tool_slot: Some(0),
                },
            }],
            &mut next_entity_id,
        );

        let roster = ecs.get::<&CompanionRoster>(player_entity).unwrap();
        assert_eq!(roster.active_slot, Some(0));
        assert_eq!(roster.creatures.len(), 1);
        assert_eq!(roster.creatures[0].creature.species_id, "spark-mouse");
        let inventory = ecs.get::<&Inventory>(player_entity).unwrap();
        assert_eq!(inventory.items[0].quantity, 1);
        let encounter = ecs.get::<&EncounterState>(player_entity).unwrap();
        assert!(!encounter.capture_allowed);
        assert!(!encounter.in_combat);
        assert!(ecs.get::<&Transform>(wild_entity).is_err());
        assert!(result.events.iter().any(|event| matches!(
            &event.event,
            Event::CreatureCaptured { species_id, .. } if species_id == "spark-mouse"
        )));
    }

    #[test]
    fn summon_companion_sets_active_slot_and_emits_event() {
        let mut ecs = hecs::World::new();
        let player_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        ecs.insert(
            player_entity,
            (CompanionRoster {
                active_slot: None,
                party_capacity: 6,
                creatures: vec![CompanionCreature {
                    creature: CreatureIdentity {
                        species_id: "ember-fox".to_string(),
                        species_name: "Ember Fox".to_string(),
                        elemental_affinity: "fire".to_string(),
                        level: 9,
                        temperament: CreatureTemperament::Loyal,
                        capture_difficulty: 0.2,
                        is_wild: false,
                    },
                    nickname: Some("Cinder".to_string()),
                    current_health: 20.0,
                    max_health: 20.0,
                    combat_style: CombatStyle::Summoning,
                    mood: 1.0,
                }],
            },),
        )
        .unwrap();

        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(player_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::SummonCompanion { slot: 0 },
            }],
            &mut next_entity_id,
        );

        let roster = ecs.get::<&CompanionRoster>(player_entity).unwrap();
        assert_eq!(roster.active_slot, Some(0));
        assert!(result.events.iter().any(|event| matches!(
            &event.event,
            Event::CompanionSummoned { species_id, .. } if species_id == "ember-fox"
        )));
    }

    #[test]
    fn command_companion_attack_damages_hostile_target() {
        let mut ecs = hecs::World::new();
        let player_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        let target_entity = spawn_actor(
            &mut ecs,
            Vec2::new(20.0, 0.0),
            Team::Team(2),
            Health::new(50.0),
        );
        ecs.insert(
            player_entity,
            (CompanionRoster {
                active_slot: None,
                party_capacity: 6,
                creatures: vec![CompanionCreature {
                    creature: CreatureIdentity {
                        species_id: "river-drake".to_string(),
                        species_name: "River Drake".to_string(),
                        elemental_affinity: "water".to_string(),
                        level: 11,
                        temperament: CreatureTemperament::Loyal,
                        capture_difficulty: 0.1,
                        is_wild: false,
                    },
                    nickname: None,
                    current_health: 28.0,
                    max_health: 28.0,
                    combat_style: CombatStyle::Summoning,
                    mood: 1.0,
                }],
            },),
        )
        .unwrap();

        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(player_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::CommandCompanion {
                    slot: 0,
                    command: CompanionCommand::Attack,
                    target: Some(EntityId(target_entity.id() as u64)),
                },
            }],
            &mut next_entity_id,
        );

        let target_health = ecs.get::<&Health>(target_entity).unwrap();
        assert_eq!(target_health.current, 39.0);
        let roster = ecs.get::<&CompanionRoster>(player_entity).unwrap();
        assert_eq!(roster.active_slot, Some(0));
        assert!(result.events.iter().any(|event| matches!(
            event.event,
            Event::CompanionCommandIssued { slot, ref command, .. }
                if slot == 0 && command == "Attack"
        )));
    }

    #[test]
    fn gather_resource_adds_item_and_awards_skill_xp() {
        let mut ecs = hecs::World::new();
        let player_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        let resource_entity = ecs.spawn((
            Transform {
                position: Vec2::new(10.0, 0.0),
                rotation: 0.0,
                scale: Vec2::ONE,
            },
            ResourceNode {
                skill: SkillKind::Mining,
                tier: 1,
                remaining_uses: 1,
                respawn_ticks: 300,
                experience: 10,
                yield_item: ItemStack {
                    item_id: "copper-ore".to_string(),
                    display_name: "Copper Ore".to_string(),
                    quantity: 2,
                    stackable: true,
                },
            },
        ));
        ecs.insert(
            player_entity,
            (
                Inventory::default(),
                SkillBook {
                    combat_level: 3,
                    total_level: 17,
                    skills: vec![
                        SkillProgress::new(SkillKind::Attack, 1, 0, 83),
                        SkillProgress::new(SkillKind::Strength, 1, 0, 83),
                        SkillProgress::new(SkillKind::Defence, 1, 0, 83),
                        SkillProgress::new(SkillKind::Ranged, 1, 0, 83),
                        SkillProgress::new(SkillKind::Magic, 1, 0, 83),
                        SkillProgress::new(SkillKind::Constitution, 10, 1_154, 1_358),
                        SkillProgress::new(SkillKind::Mining, 1, 80, 83),
                        SkillProgress::new(SkillKind::Taming, 1, 0, 83),
                        SkillProgress::new(SkillKind::Bonding, 1, 0, 83),
                    ],
                },
            ),
        )
        .unwrap();

        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(player_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::GatherResource {
                    target: EntityId(resource_entity.id() as u64),
                    skill: SkillKind::Mining,
                },
            }],
            &mut next_entity_id,
        );

        let inventory = ecs.get::<&Inventory>(player_entity).unwrap();
        assert_eq!(inventory.items.len(), 1);
        assert_eq!(inventory.items[0].item_id, "copper-ore");
        assert_eq!(inventory.items[0].quantity, 2);
        let skill_book = ecs.get::<&SkillBook>(player_entity).unwrap();
        let mining = skill_book
            .skills
            .iter()
            .find(|progress| progress.kind == SkillKind::Mining)
            .unwrap();
        assert_eq!(mining.level, 2);
        assert_eq!(mining.experience, 7);
        let node = ecs.get::<&ResourceNode>(resource_entity).unwrap();
        assert_eq!(node.remaining_uses, 0);
        assert!(result.events.iter().any(|event| matches!(
            &event.event,
            Event::SkillXpGained { skill, amount, new_level, .. }
                if skill == "Mining" && *amount == 10 && *new_level == 2
        )));
    }

    #[test]
    fn loot_claims_container_and_transfers_contents() {
        let mut ecs = hecs::World::new();
        let player_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        let chest_entity = ecs.spawn((
            Transform {
                position: Vec2::new(8.0, 0.0),
                rotation: 0.0,
                scale: Vec2::ONE,
            },
            LootContainer {
                coins: 125,
                items: vec![ItemStack {
                    item_id: "salmon".to_string(),
                    display_name: "Salmon".to_string(),
                    quantity: 3,
                    stackable: true,
                }],
                owner: None,
                claimed: false,
            },
        ));
        ecs.insert(player_entity, (Inventory::default(),)).unwrap();

        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(player_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::Loot {
                    target: EntityId(chest_entity.id() as u64),
                },
            }],
            &mut next_entity_id,
        );

        let inventory = ecs.get::<&Inventory>(player_entity).unwrap();
        assert_eq!(inventory.coins, 125);
        assert_eq!(inventory.items.len(), 1);
        assert_eq!(inventory.items[0].item_id, "salmon");
        let loot = ecs.get::<&LootContainer>(chest_entity).unwrap();
        assert!(loot.claimed);
        assert_eq!(loot.coins, 0);
        assert!(loot.items.is_empty());
        assert!(result.events.iter().any(|event| matches!(
            event.event,
            Event::LootClaimed {
                coins,
                item_count,
                ..
            } if coins == 125 && item_count == 1
        )));
    }

    #[test]
    fn set_auto_retaliate_updates_combat_loadout() {
        let mut ecs = hecs::World::new();
        let player_entity = spawn_actor(
            &mut ecs,
            Vec2::new(0.0, 0.0),
            Team::Team(1),
            Health::new(100.0),
        );
        ecs.insert(
            player_entity,
            (CombatLoadout {
                auto_retaliate: true,
                ..CombatLoadout::default()
            },),
        )
        .unwrap();

        let obs_store = Arc::new(Mutex::new(Vec::new()));
        let (agent, agent_id) = RecordingAgent::new(vec![], obs_store);
        let mut slot = AgentSlot::new(Box::new(agent));
        slot.entity_id = Some(player_entity);
        let mut agents = vec![slot];
        let mut events = EventBus::new();
        let mut next_entity_id = 1;

        let result = execute_tick(
            &mut ecs,
            &mut agents,
            &mut events,
            1,
            vec![AgentAction {
                agent_id,
                tick: 1,
                action: Action::SetAutoRetaliate { enabled: false },
            }],
            &mut next_entity_id,
        );

        let loadout = ecs.get::<&CombatLoadout>(player_entity).unwrap();
        assert!(!loadout.auto_retaliate);
        assert!(result.events.iter().any(|event| matches!(
            event.event,
            Event::AutoRetaliateSet { enabled, .. } if !enabled
        )));
    }
}
