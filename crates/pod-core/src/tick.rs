use crate::action::{validate_action, Action, ActionResult, AgentAction};
use crate::agent::AgentSlot;
use crate::component::*;
use crate::event::{Event, EventBus};
use crate::id::EntityId;
use crate::observation::*;
use crate::TICK_DURATION_SECS;
use glam::{Vec2, Vec3};

/// Result of a single tick
#[derive(Debug, Clone)]
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
    external_actions: Vec<AgentAction>,
    _next_entity_id: &mut u64,
) -> TickResult {
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
    all_actions.extend(external_actions);

    // ========================================
    // PHASE 3: VALIDATE & EXECUTE ACTIONS
    // ========================================
    for agent_action in &all_actions {
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
            }
            ActionResult::Rejected(reason) => {
                log::debug!("Action rejected for {}: {}", agent_action.agent_id, reason);
                actions_rejected += 1;
            }
            ActionResult::Queued => {
                log::debug!(
                    "Action deferred for {} at tick {}",
                    agent_action.agent_id,
                    tick
                );
                actions_rejected += 1;
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
                "Interact",
                "Pickup",
                "Drop",
                "UseItem",
                "Speak",
                "Signal",
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

            if let Some(target_entity) = select_attack_target(ecs, entity, None) {
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
                if target_entity != entity && in_range(ecs, entity, target_entity, ATTACK_RANGE) {
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
        Action::Idle => {}
        _ => {
            log::debug!("Unhandled action: {:?}", agent_action.action);
        }
    }

    if set_attack_cooldown {
        let duration = agents[actor_slot_index].agent.constraints().attack_cooldown;
        agents[actor_slot_index].attack_cooldown_remaining = duration;
    }
}

const ATTACK_RANGE: f32 = 80.0;
const INTERACT_RANGE: f32 = 50.0;
const BASE_ATTACK_DAMAGE: f32 = 10.0;

fn find_entity_by_id(ecs: &hecs::World, target_id: u64) -> Option<hecs::Entity> {
    ecs.query::<()>()
        .iter()
        .find_map(|(entity, _)| (entity.id() as u64 == target_id).then_some(entity))
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
) -> Option<hecs::Entity> {
    if let Some(target) = explicit_target {
        if target != source
            && in_range(ecs, source, target, ATTACK_RANGE)
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
            if distance > ATTACK_RANGE || !is_hostile_target(ecs, source, entity) {
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

fn apply_attack(
    ecs: &mut hecs::World,
    agents: &mut [AgentSlot],
    events: &mut EventBus,
    attacker_entity: hecs::Entity,
    target_entity: hecs::Entity,
    attacker_agent_id: crate::id::AgentId,
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
            name: "attack".into(),
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
        applied_damage = health.damage(BASE_ATTACK_DAMAGE);
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
}
