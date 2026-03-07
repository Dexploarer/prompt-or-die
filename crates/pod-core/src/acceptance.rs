use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::action::{Action, AgentConstraints, CompanionCommand, SpeakVolume};
use crate::agent::{Agent, AgentType};
use crate::app::App;
use crate::component::{
    AgentControlled, CombatLoadout, CombatStyle, CreatureIdentity, EncounterKind, EncounterState,
    Health, ItemStack, Label, LootContainer, Movement, ResourceNode, SkillKind, Team, Transform,
};
use crate::event::Event;
use crate::id::{AgentId, EntityId};
use crate::observation::{MessageChannel, Observation};
use crate::replay::{ReplayFile, ReplayHeader, ReplayRecorder, ReplayTrainingSample};
use crate::telemetry::{AgentToolCallTrace, TickTelemetryFrame, ToolCallStatus};
use crate::tick::TickResult;
use crate::TICKS_PER_SECOND;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagshipMmoScaleTarget {
    pub min_human_sessions: usize,
    pub max_human_sessions: usize,
    pub min_autonomous_agents: usize,
    pub max_autonomous_agents: usize,
    pub target_tps: u32,
}

impl Default for FlagshipMmoScaleTarget {
    fn default() -> Self {
        Self {
            min_human_sessions: 32,
            max_human_sessions: 64,
            min_autonomous_agents: 256,
            max_autonomous_agents: 512,
            target_tps: 60,
        }
    }
}

impl FlagshipMmoScaleTarget {
    pub fn satisfied_by(self, human_sessions: usize, autonomous_agents: usize) -> bool {
        human_sessions >= self.min_human_sessions
            && human_sessions <= self.max_human_sessions
            && autonomous_agents >= self.min_autonomous_agents
            && autonomous_agents <= self.max_autonomous_agents
            && self.target_tps == TICKS_PER_SECOND
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlagshipMmoAcceptanceConfig {
    pub world_seed: u64,
    pub total_ticks: u64,
    pub human_sessions: usize,
    pub autonomous_agents: usize,
    pub scale_target: FlagshipMmoScaleTarget,
}

impl Default for FlagshipMmoAcceptanceConfig {
    fn default() -> Self {
        Self::shard_target()
    }
}

impl FlagshipMmoAcceptanceConfig {
    pub fn shard_target() -> Self {
        Self {
            world_seed: 0x50D0_2026,
            total_ticks: 12,
            human_sessions: 32,
            autonomous_agents: 256,
            scale_target: FlagshipMmoScaleTarget::default(),
        }
    }

    pub fn ci_smoke() -> Self {
        Self {
            world_seed: 0x50D0_2026,
            total_ticks: 12,
            human_sessions: 2,
            autonomous_agents: 4,
            scale_target: FlagshipMmoScaleTarget::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptanceParityReport {
    pub human_agent_id: AgentId,
    pub autonomous_agent_id: AgentId,
    pub matched_ticks: usize,
    pub observation_mismatches: usize,
    pub decision_mismatches: usize,
}

impl AcceptanceParityReport {
    pub fn passed(&self) -> bool {
        self.observation_mismatches == 0 && self.decision_mismatches == 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlagshipMmoAcceptanceSummary {
    pub ticks_completed: u64,
    pub human_sessions: usize,
    pub autonomous_agents: usize,
    pub total_agents: usize,
    pub peak_entities: usize,
    pub actions_processed: usize,
    pub actions_rejected: usize,
    pub chat_messages: usize,
    pub damage_events: usize,
    pub captures: usize,
    pub capture_actions: usize,
    pub summons: usize,
    pub summon_actions: usize,
    pub companion_commands: usize,
    pub resource_gathers: usize,
    pub gather_actions: usize,
    pub loot_claims: usize,
    pub loot_actions: usize,
    pub tool_calls: usize,
    pub tool_call_errors: usize,
    pub telemetry_frames: usize,
    pub replay_training_samples: usize,
    pub average_path_distance: f32,
    pub scale_target: FlagshipMmoScaleTarget,
    pub scale_target_satisfied: bool,
}

impl FlagshipMmoAcceptanceSummary {
    pub fn parity_passed(&self, reports: &[AcceptanceParityReport]) -> bool {
        !reports.is_empty() && reports.iter().all(AcceptanceParityReport::passed)
    }
}

#[derive(Debug, Clone)]
pub struct FlagshipMmoAcceptanceResult {
    pub config: FlagshipMmoAcceptanceConfig,
    pub summary: FlagshipMmoAcceptanceSummary,
    pub parity_reports: Vec<AcceptanceParityReport>,
    pub tick_results: Vec<TickResult>,
    pub replay: ReplayFile,
    pub training_samples: Vec<ReplayTrainingSample>,
}

impl FlagshipMmoAcceptanceResult {
    pub fn training_samples(&self) -> &[ReplayTrainingSample] {
        &self.training_samples
    }

    pub fn telemetry_windows(&self) -> &[TickTelemetryFrame] {
        &self.replay.telemetry_windows
    }

    pub fn parity_passed(&self) -> bool {
        self.summary.parity_passed(&self.parity_reports)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ObservationSignature {
    tick: u64,
    visible_entity_count: usize,
    hostile_visible_count: usize,
    friendly_visible_count: usize,
    neutral_visible_count: usize,
    creature_species: Vec<String>,
    audible_events: Vec<AudibleEventSignature>,
    messages: Vec<String>,
    available_actions: Vec<String>,
    encounter: Option<EncounterSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AudibleEventSignature {
    event_type: String,
    direction_x_milli: i32,
    direction_y_milli: i32,
    distance_milli: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct EncounterSignature {
    encounter_id: u64,
    kind: String,
    capture_allowed: bool,
    in_combat: bool,
    primary_target: Option<u64>,
}

#[derive(Debug, Clone)]
struct DecisionRecord {
    tick: u64,
    actions: Vec<Action>,
    tool_calls: Vec<AgentToolCallTrace>,
}

#[derive(Debug, Clone)]
struct AcceptanceAgentAudit {
    observations: Vec<Observation>,
    decisions: Vec<DecisionRecord>,
}

impl AcceptanceAgentAudit {
    fn new() -> Self {
        Self {
            observations: Vec::new(),
            decisions: Vec::new(),
        }
    }
}

struct ScriptedAcceptanceAgent {
    id: AgentId,
    agent_type: AgentType,
    constraints: AgentConstraints,
    scheduled_actions: BTreeMap<u64, Vec<Action>>,
    scheduled_tool_calls: BTreeMap<u64, Vec<AgentToolCallTrace>>,
    current_tick: u64,
    audit: Arc<Mutex<AcceptanceAgentAudit>>,
    pending_tool_calls: Vec<AgentToolCallTrace>,
}

impl ScriptedAcceptanceAgent {
    fn new(
        id: AgentId,
        label: impl Into<String>,
        agent_type: AgentType,
        scheduled_actions: BTreeMap<u64, Vec<Action>>,
        scheduled_tool_calls: BTreeMap<u64, Vec<AgentToolCallTrace>>,
        attack_cooldown: u32,
    ) -> (Self, Arc<Mutex<AcceptanceAgentAudit>>) {
        let _ = label.into();
        let audit = Arc::new(Mutex::new(AcceptanceAgentAudit::new()));
        let mut constraints = AgentConstraints::default();
        constraints.attack_cooldown = attack_cooldown.max(1);
        (
            Self {
                id,
                agent_type,
                constraints,
                scheduled_actions,
                scheduled_tool_calls,
                current_tick: 0,
                audit: Arc::clone(&audit),
                pending_tool_calls: Vec::new(),
            },
            audit,
        )
    }
}

impl Agent for ScriptedAcceptanceAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn agent_type(&self) -> AgentType {
        self.agent_type
    }

    fn observe(&mut self, observation: Observation) {
        self.current_tick = observation.tick;
        self.audit.lock().unwrap().observations.push(observation);
    }

    fn decide(&mut self) -> Vec<Action> {
        let actions = self
            .scheduled_actions
            .remove(&self.current_tick)
            .unwrap_or_default();
        self.pending_tool_calls = self
            .scheduled_tool_calls
            .remove(&self.current_tick)
            .unwrap_or_default();
        self.audit.lock().unwrap().decisions.push(DecisionRecord {
            tick: self.current_tick,
            actions: actions.clone(),
            tool_calls: self.pending_tool_calls.clone(),
        });
        actions
    }

    fn constraints(&self) -> &AgentConstraints {
        &self.constraints
    }

    fn constraints_mut(&mut self) -> &mut AgentConstraints {
        &mut self.constraints
    }

    fn drain_tool_calls(&mut self) -> Vec<AgentToolCallTrace> {
        std::mem::take(&mut self.pending_tool_calls)
    }
}

struct ScenarioActors {
    parity_human_id: AgentId,
    parity_human_audit: Arc<Mutex<AcceptanceAgentAudit>>,
    parity_ai_id: AgentId,
    parity_ai_audit: Arc<Mutex<AcceptanceAgentAudit>>,
    audits: Vec<(AgentId, Arc<Mutex<AcceptanceAgentAudit>>)>,
}

pub fn run_flagship_mmo_acceptance(
    config: FlagshipMmoAcceptanceConfig,
) -> Result<FlagshipMmoAcceptanceResult, String> {
    if config.human_sessions < 2 {
        return Err("flagship MMO acceptance requires at least two human-capable sessions".into());
    }
    if config.autonomous_agents < 2 {
        return Err("flagship MMO acceptance requires at least two autonomous agents".into());
    }
    if config.total_ticks < 8 {
        return Err("flagship MMO acceptance requires at least eight ticks".into());
    }

    let mut app = App::new(config.world_seed);
    let actors = build_flagship_scenario(app.world_mut(), &config);

    let mut tick_results = Vec::with_capacity(config.total_ticks as usize);
    for _ in 0..config.total_ticks {
        tick_results.push(app.update());
    }

    let telemetry_windows: Vec<TickTelemetryFrame> = tick_results
        .iter()
        .map(|result| result.telemetry.clone())
        .collect();
    let replay = build_replay(
        config.world_seed,
        config.total_ticks,
        config.human_sessions + config.autonomous_agents,
        &actors.audits,
        &telemetry_windows,
    );
    let training_samples = replay.training_samples();
    let parity_reports = vec![build_parity_report(
        actors.parity_human_id,
        &actors.parity_human_audit,
        actors.parity_ai_id,
        &actors.parity_ai_audit,
    )];

    let summary = build_summary(
        &config,
        &tick_results,
        &telemetry_windows,
        &training_samples,
    );

    Ok(FlagshipMmoAcceptanceResult {
        config,
        summary,
        parity_reports,
        tick_results,
        replay,
        training_samples,
    })
}

fn build_flagship_scenario(
    world: &mut crate::World,
    config: &FlagshipMmoAcceptanceConfig,
) -> ScenarioActors {
    let _anchor = world
        .spawn_at(1_000.0, 1_000.0)
        .with_label("Shard Anchor", Team::None)
        .build();
    let resource_entity = world
        .spawn_at(20.0, 0.0)
        .with_label("Copper Vein", Team::None)
        .with_resource_node(ResourceNode {
            skill: SkillKind::Mining,
            tier: 1,
            remaining_uses: 2,
            respawn_ticks: 300,
            experience: 30,
            yield_item: ItemStack {
                item_id: "copper-ore".into(),
                display_name: "Copper Ore".into(),
                quantity: 1,
                stackable: true,
            },
        })
        .build();
    let loot_entity = world
        .spawn_at(28.0, 0.0)
        .with_label("Bronze Chest", Team::None)
        .with_loot_container(LootContainer {
            coins: 120,
            items: vec![ItemStack {
                item_id: "bronze-shield".into(),
                display_name: "Bronze Shield".into(),
                quantity: 1,
                stackable: false,
            }],
            owner: None,
            claimed: false,
        })
        .build();
    let dummy_entity = world
        .spawn_at(50.0, 0.0)
        .with_label("Training Dummy", Team::Team(2))
        .with_health(48.0)
        .with_combat_loadout(CombatLoadout {
            style: CombatStyle::Melee,
            attack_range: 0.0,
            attack_speed_ticks: 4,
            max_hit: 0.0,
            auto_retaliate: false,
            equipped_weapon: None,
            offhand_item: None,
            active_ability_bar: vec![],
        })
        .build();
    let wild_entity = world
        .spawn_at(40.0, 30.0)
        .with_label("Wild Embercub", Team::None)
        .with_health(10.0)
        .with_creature_identity(CreatureIdentity {
            species_id: "embercub".into(),
            species_name: "Embercub".into(),
            elemental_affinity: "fire".into(),
            level: 3,
            temperament: crate::component::CreatureTemperament::Timid,
            capture_difficulty: 0.25,
            is_wild: true,
        })
        .with_combat_loadout(CombatLoadout {
            style: CombatStyle::Summoning,
            attack_range: 40.0,
            attack_speed_ticks: 3,
            max_hit: 3.0,
            auto_retaliate: false,
            equipped_weapon: None,
            offhand_item: None,
            active_ability_bar: vec!["ember-pounce".into()],
        })
        .build();

    let resource_id = EntityId(resource_entity.id() as u64);
    let loot_id = EntityId(loot_entity.id() as u64);
    let dummy_id = EntityId(dummy_entity.id() as u64);
    let wild_id = EntityId(wild_entity.id() as u64);

    let mut audits = Vec::with_capacity(config.human_sessions + config.autonomous_agents);

    let parity_human_id = deterministic_agent_id(1);
    let (parity_human_agent, parity_human_audit) = ScriptedAcceptanceAgent::new(
        parity_human_id,
        "parity-human",
        AgentType::Human,
        parity_script(dummy_id),
        BTreeMap::new(),
        4,
    );
    let (_, parity_human_entity) = world.add_agent(Box::new(parity_human_agent));
    configure_agent_entity(
        world,
        parity_human_entity,
        parity_human_id,
        "Parity Human",
        Vec2::new(0.0, 0.0),
        Team::Team(1),
        CombatLoadout {
            style: CombatStyle::Melee,
            attack_range: 80.0,
            attack_speed_ticks: 4,
            max_hit: 4.0,
            auto_retaliate: true,
            equipped_weapon: Some("bronze-scimitar".into()),
            offhand_item: None,
            active_ability_bar: vec!["slash".into()],
        },
        None,
    );
    audits.push((parity_human_id, Arc::clone(&parity_human_audit)));

    let gatherer_id = deterministic_agent_id(2);
    let (gatherer_agent, gatherer_audit) = ScriptedAcceptanceAgent::new(
        gatherer_id,
        "gatherer-human",
        AgentType::Human,
        gatherer_script(resource_id, loot_id),
        BTreeMap::new(),
        3,
    );
    let (_, gatherer_entity) = world.add_agent(Box::new(gatherer_agent));
    configure_agent_entity(
        world,
        gatherer_entity,
        gatherer_id,
        "Gatherer Human",
        Vec2::new(8.0, 0.0),
        Team::Team(1),
        CombatLoadout::default(),
        None,
    );
    audits.push((gatherer_id, gatherer_audit));

    let parity_ai_id = deterministic_agent_id(10_001);
    let (parity_ai_agent, parity_ai_audit) = ScriptedAcceptanceAgent::new(
        parity_ai_id,
        "parity-ai",
        AgentType::LlmAgent,
        parity_script(dummy_id),
        tool_call_schedule(vec![(
            0,
            AgentToolCallTrace::success(0, "llm.complete", "acceptance-mock", 14, 72, 18),
        )]),
        4,
    );
    let (_, parity_ai_entity) = world.add_agent(Box::new(parity_ai_agent));
    configure_agent_entity(
        world,
        parity_ai_entity,
        parity_ai_id,
        "Parity AI",
        Vec2::new(0.0, 0.0),
        Team::Team(1),
        CombatLoadout {
            style: CombatStyle::Melee,
            attack_range: 80.0,
            attack_speed_ticks: 4,
            max_hit: 4.0,
            auto_retaliate: true,
            equipped_weapon: Some("bronze-scimitar".into()),
            offhand_item: None,
            active_ability_bar: vec!["slash".into()],
        },
        None,
    );
    audits.push((parity_ai_id, Arc::clone(&parity_ai_audit)));

    let tamer_id = deterministic_agent_id(10_002);
    let (tamer_agent, tamer_audit) = ScriptedAcceptanceAgent::new(
        tamer_id,
        "tamer-ai",
        AgentType::ScriptedNpc,
        tamer_script(wild_id, dummy_id),
        tool_call_schedule(vec![(
            4,
            AgentToolCallTrace::new(
                4,
                "llm.complete",
                "acceptance-mock",
                ToolCallStatus::ParseError,
                9,
                48,
                0,
                Some("capture intent parse failed once".into()),
            ),
        )]),
        2,
    );
    let (_, tamer_entity) = world.add_agent(Box::new(tamer_agent));
    configure_agent_entity(
        world,
        tamer_entity,
        tamer_id,
        "Tamer AI",
        Vec2::new(0.0, 30.0),
        Team::Team(1),
        CombatLoadout {
            style: CombatStyle::Summoning,
            attack_range: 80.0,
            attack_speed_ticks: 2,
            max_hit: 4.0,
            auto_retaliate: false,
            equipped_weapon: Some("capture-staff".into()),
            offhand_item: None,
            active_ability_bar: vec!["bind".into(), "capture".into()],
        },
        Some(EncounterState {
            encounter_id: 77,
            kind: EncounterKind::WildCreature,
            threat_level: 2.5,
            primary_target: Some(wild_id),
            active_turn_owner: Some(EntityId(tamer_entity.id() as u64)),
            capture_allowed: true,
            in_combat: true,
        }),
    );
    audits.push((tamer_id, tamer_audit));

    for human_index in 2..config.human_sessions {
        let agent_id = deterministic_agent_id((human_index + 1) as u128);
        let (agent, audit) = ScriptedAcceptanceAgent::new(
            agent_id,
            format!("human-filler-{human_index}"),
            AgentType::Human,
            filler_script(human_index),
            BTreeMap::new(),
            4,
        );
        let (_, entity) = world.add_agent(Box::new(agent));
        configure_agent_entity(
            world,
            entity,
            agent_id,
            &format!("Human {human_index}"),
            filler_position(human_index, true),
            Team::Team(1),
            CombatLoadout::default(),
            None,
        );
        audits.push((agent_id, audit));
    }

    for auto_index in 2..config.autonomous_agents {
        let agent_id = deterministic_agent_id(10_000 + (auto_index + 1) as u128);
        let agent_type = if auto_index % 2 == 0 {
            AgentType::ScriptedNpc
        } else {
            AgentType::NeuralAgent
        };
        let (agent, audit) = ScriptedAcceptanceAgent::new(
            agent_id,
            format!("ai-filler-{auto_index}"),
            agent_type,
            filler_script(auto_index),
            if auto_index == 2 {
                tool_call_schedule(vec![(
                    2,
                    AgentToolCallTrace::success(2, "llm.complete", "acceptance-mock", 11, 32, 14),
                )])
            } else {
                BTreeMap::new()
            },
            4,
        );
        let (_, entity) = world.add_agent(Box::new(agent));
        configure_agent_entity(
            world,
            entity,
            agent_id,
            &format!("AI {auto_index}"),
            filler_position(auto_index, false),
            Team::Team(1),
            CombatLoadout::default(),
            None,
        );
        audits.push((agent_id, audit));
    }

    ScenarioActors {
        parity_human_id,
        parity_human_audit,
        parity_ai_id,
        parity_ai_audit,
        audits,
    }
}

fn build_replay(
    world_seed: u64,
    tick_count: u64,
    agent_count: usize,
    audits: &[(AgentId, Arc<Mutex<AcceptanceAgentAudit>>)],
    telemetry_windows: &[TickTelemetryFrame],
) -> ReplayFile {
    let mut recorder = ReplayRecorder::new();
    for (agent_id, audit) in audits {
        let audit = audit.lock().unwrap();
        for (observation, decision) in audit.observations.iter().zip(audit.decisions.iter()) {
            recorder.record_decision(
                decision.tick,
                *agent_id,
                observation,
                observation.to_agent_prompt(),
                format!("acceptance-script:{:?}", decision.actions),
                decision.actions.clone(),
                decision.tool_calls.clone(),
                0,
            );
        }
    }

    recorder.finalize_with_telemetry(
        ReplayHeader {
            name: "flagship-mmo-acceptance".into(),
            timestamp: 0,
            world_seed,
            tick_count,
            agent_count,
            notes: "RuneScape-style MMO acceptance loop with human/AI parity, capture, summon, gather, loot, and telemetry".into(),
        },
        telemetry_windows.to_vec(),
    )
}

fn build_summary(
    config: &FlagshipMmoAcceptanceConfig,
    tick_results: &[TickResult],
    telemetry_windows: &[TickTelemetryFrame],
    training_samples: &[ReplayTrainingSample],
) -> FlagshipMmoAcceptanceSummary {
    let mut peak_entities = 0usize;
    let mut actions_processed = 0usize;
    let mut actions_rejected = 0usize;
    let mut chat_messages = 0usize;
    let mut damage_events = 0usize;
    let mut captures = 0usize;
    let mut capture_actions = 0usize;
    let mut summons = 0usize;
    let mut summon_actions = 0usize;
    let mut companion_commands = 0usize;
    let mut resource_gathers = 0usize;
    let mut gather_actions = 0usize;
    let mut loot_claims = 0usize;
    let mut loot_actions = 0usize;

    for tick in tick_results {
        peak_entities = peak_entities.max(tick.entity_count);
        actions_processed += tick.actions_processed;
        actions_rejected += tick.actions_rejected;

        for event in &tick.events {
            match &event.event {
                Event::AgentSpoke { .. } => chat_messages += 1,
                Event::Damage { .. } => damage_events += 1,
                Event::CreatureCaptured { .. } => captures += 1,
                Event::CompanionSummoned { .. } => summons += 1,
                Event::CompanionCommandIssued { .. } => companion_commands += 1,
                Event::ResourceGathered { .. } => resource_gathers += 1,
                Event::LootClaimed { .. } => loot_claims += 1,
                _ => {}
            }
        }
    }

    let mut tool_calls = 0usize;
    let mut tool_call_errors = 0usize;
    let mut total_path_distance = 0.0f32;
    let mut trajectory_samples = 0usize;

    for frame in telemetry_windows {
        for agent in &frame.agents {
            for trace in &agent.action_trace {
                if trace.stage != crate::telemetry::ActionLifecycleStage::Executed {
                    continue;
                }
                match trace.action {
                    Action::CaptureCreature { .. } => capture_actions += 1,
                    Action::SummonCompanion { .. } => summon_actions += 1,
                    Action::GatherResource { .. } => gather_actions += 1,
                    Action::Loot { .. } => loot_actions += 1,
                    _ => {}
                }
            }
            tool_calls += agent.tool_calls.len();
            tool_call_errors += agent
                .tool_calls
                .iter()
                .filter(|trace| {
                    !matches!(
                        trace.status,
                        ToolCallStatus::Requested | ToolCallStatus::Succeeded
                    )
                })
                .count();
            if let Some(trajectory) = &agent.trajectory {
                total_path_distance += trajectory.distance_travelled;
                trajectory_samples += 1;
            }
        }
    }

    FlagshipMmoAcceptanceSummary {
        ticks_completed: tick_results.len() as u64,
        human_sessions: config.human_sessions,
        autonomous_agents: config.autonomous_agents,
        total_agents: config.human_sessions + config.autonomous_agents,
        peak_entities,
        actions_processed,
        actions_rejected,
        chat_messages,
        damage_events,
        captures,
        capture_actions,
        summons,
        summon_actions,
        companion_commands,
        resource_gathers,
        gather_actions,
        loot_claims,
        loot_actions,
        tool_calls,
        tool_call_errors,
        telemetry_frames: telemetry_windows.len(),
        replay_training_samples: training_samples.len(),
        average_path_distance: if trajectory_samples == 0 {
            0.0
        } else {
            total_path_distance / trajectory_samples as f32
        },
        scale_target: config.scale_target,
        scale_target_satisfied: config
            .scale_target
            .satisfied_by(config.human_sessions, config.autonomous_agents),
    }
}

fn build_parity_report(
    human_agent_id: AgentId,
    human_audit: &Arc<Mutex<AcceptanceAgentAudit>>,
    autonomous_agent_id: AgentId,
    autonomous_audit: &Arc<Mutex<AcceptanceAgentAudit>>,
) -> AcceptanceParityReport {
    let human = human_audit.lock().unwrap();
    let autonomous = autonomous_audit.lock().unwrap();

    let human_observations = observations_by_tick(&human.observations);
    let autonomous_observations = observations_by_tick(&autonomous.observations);
    let human_decisions = decisions_by_tick(&human.decisions);
    let autonomous_decisions = decisions_by_tick(&autonomous.decisions);

    let matched_ticks = human_observations
        .keys()
        .filter(|tick| autonomous_observations.contains_key(tick))
        .count();

    let observation_mismatches = human_observations
        .iter()
        .filter(|(tick, signature)| autonomous_observations.get(*tick) != Some(*signature))
        .count();

    let decision_mismatches = human_decisions
        .iter()
        .filter(|(tick, signature)| autonomous_decisions.get(*tick) != Some(*signature))
        .count();

    AcceptanceParityReport {
        human_agent_id,
        autonomous_agent_id,
        matched_ticks,
        observation_mismatches,
        decision_mismatches,
    }
}

fn observations_by_tick(observations: &[Observation]) -> HashMap<u64, String> {
    observations
        .iter()
        .map(|observation| {
            (
                observation.tick,
                serde_json::to_string(&observation_signature(observation)).unwrap(),
            )
        })
        .collect()
}

fn decisions_by_tick(decisions: &[DecisionRecord]) -> HashMap<u64, String> {
    decisions
        .iter()
        .map(|record| {
            (
                record.tick,
                serde_json::to_string(&record.actions).unwrap_or_default(),
            )
        })
        .collect()
}

fn observation_signature(observation: &Observation) -> ObservationSignature {
    let hostile_visible_count = observation
        .visible_entities
        .iter()
        .filter(|entity| {
            matches!(
                entity.relationship,
                crate::observation::Relationship::Hostile
            )
        })
        .count();
    let friendly_visible_count = observation
        .visible_entities
        .iter()
        .filter(|entity| {
            matches!(
                entity.relationship,
                crate::observation::Relationship::Friendly
            )
        })
        .count();
    let neutral_visible_count =
        observation.visible_entities.len() - hostile_visible_count - friendly_visible_count;

    ObservationSignature {
        tick: observation.tick,
        visible_entity_count: observation.visible_entities.len(),
        hostile_visible_count,
        friendly_visible_count,
        neutral_visible_count,
        creature_species: observation
            .visible_entities
            .iter()
            .filter_map(|entity| {
                entity
                    .creature
                    .as_ref()
                    .map(|creature| creature.species_name.clone())
            })
            .collect(),
        audible_events: observation
            .audible_events
            .iter()
            .map(|event| AudibleEventSignature {
                event_type: event.event_type.clone(),
                direction_x_milli: (event.direction.x * 1000.0).round() as i32,
                direction_y_milli: (event.direction.y * 1000.0).round() as i32,
                distance_milli: (event.distance * 1000.0).round() as i32,
            })
            .collect(),
        messages: observation
            .messages
            .iter()
            .map(|message| {
                let channel = match message.channel {
                    MessageChannel::Proximity => "Proximity",
                    MessageChannel::Team => "Team",
                    MessageChannel::Direct => "Direct",
                    MessageChannel::Global => "Global",
                };
                format!("{channel}:{}", message.content)
            })
            .collect(),
        available_actions: observation.available_actions.clone(),
        encounter: observation
            .self_state
            .encounter
            .as_ref()
            .map(|encounter| EncounterSignature {
                encounter_id: encounter.encounter_id,
                kind: format!("{:?}", encounter.kind),
                capture_allowed: encounter.capture_allowed,
                in_combat: encounter.in_combat,
                primary_target: encounter.primary_target.map(|target| target.0),
            }),
    }
}

fn configure_agent_entity(
    world: &mut crate::World,
    entity: hecs::Entity,
    agent_id: AgentId,
    label: &str,
    position: Vec2,
    team: Team,
    loadout: CombatLoadout,
    encounter: Option<EncounterState>,
) {
    if let Ok(mut transform) = world.ecs.get::<&mut Transform>(entity) {
        transform.position = position;
    }
    if let Ok(mut label_component) = world.ecs.get::<&mut Label>(entity) {
        label_component.name = label.to_string();
        label_component.team = team;
    }
    if let Ok(mut movement) = world.ecs.get::<&mut Movement>(entity) {
        movement.max_speed = 180.0;
        movement.acceleration = 720.0;
        movement.deceleration = 600.0;
    }
    if let Ok(mut health) = world.ecs.get::<&mut Health>(entity) {
        health.current = 100.0;
        health.max = 100.0;
    }
    if let Ok(mut combat_loadout) = world.ecs.get::<&mut CombatLoadout>(entity) {
        *combat_loadout = loadout;
    }
    let _ = world.ecs.insert_one(entity, AgentControlled { agent_id });
    if let Some(encounter) = encounter {
        let _ = world.ecs.insert_one(entity, encounter);
    }
}

fn deterministic_agent_id(seed: u128) -> AgentId {
    AgentId(Uuid::from_u128(seed))
}

fn parity_script(dummy_id: EntityId) -> BTreeMap<u64, Vec<Action>> {
    schedule(vec![
        (
            0,
            vec![Action::Speak {
                message: "sync ready".into(),
                volume: SpeakVolume::Normal,
            }],
        ),
        (
            1,
            vec![Action::Move {
                direction: Vec2::new(1.0, 0.0),
            }],
        ),
        (2, vec![Action::Stop]),
        (3, vec![Action::AttackTarget { target: dummy_id }]),
        (4, vec![Action::AttackTarget { target: dummy_id }]),
        (7, vec![Action::AttackTarget { target: dummy_id }]),
        (
            8,
            vec![Action::Speak {
                message: "combat cadence held".into(),
                volume: SpeakVolume::Normal,
            }],
        ),
    ])
}

fn gatherer_script(resource_id: EntityId, loot_id: EntityId) -> BTreeMap<u64, Vec<Action>> {
    schedule(vec![
        (
            0,
            vec![Action::Speak {
                message: "gather loop online".into(),
                volume: SpeakVolume::Normal,
            }],
        ),
        (
            1,
            vec![
                Action::GatherResource {
                    target: resource_id,
                    skill: SkillKind::Mining,
                },
                Action::Move {
                    direction: Vec2::new(1.0, 0.0),
                },
            ],
        ),
        (
            2,
            vec![
                Action::Stop,
                Action::Loot { target: loot_id },
                Action::Speak {
                    message: "loot secured".into(),
                    volume: SpeakVolume::Normal,
                },
            ],
        ),
    ])
}

fn tamer_script(wild_id: EntityId, dummy_id: EntityId) -> BTreeMap<u64, Vec<Action>> {
    schedule(vec![
        (
            0,
            vec![Action::Speak {
                message: "wild encounter engaged".into(),
                volume: SpeakVolume::Normal,
            }],
        ),
        (1, vec![Action::AttackTarget { target: wild_id }]),
        (3, vec![Action::AttackTarget { target: wild_id }]),
        (
            4,
            vec![Action::CaptureCreature {
                target: wild_id,
                tool_slot: None,
            }],
        ),
        (5, vec![Action::SummonCompanion { slot: 0 }]),
        (
            6,
            vec![Action::CommandCompanion {
                slot: 0,
                command: CompanionCommand::Attack,
                target: Some(dummy_id),
            }],
        ),
    ])
}

fn filler_script(index: usize) -> BTreeMap<u64, Vec<Action>> {
    let start = (index % 4) as u64;
    let x = if index % 2 == 0 { 1.0 } else { -1.0 };
    let y = if index % 3 == 0 { 0.5 } else { 0.0 };
    schedule(vec![
        (
            start,
            vec![Action::Move {
                direction: Vec2::new(x, y),
            }],
        ),
        (start + 2, vec![Action::Stop]),
    ])
}

fn filler_position(index: usize, human: bool) -> Vec2 {
    let row = (index / 12) as f32;
    let col = (index % 12) as f32;
    let x = if human { -30.0 } else { -10.0 } + col * 5.0;
    let y = if human { -40.0 } else { -70.0 } + row * 5.0;
    Vec2::new(x, y)
}

fn schedule(entries: Vec<(u64, Vec<Action>)>) -> BTreeMap<u64, Vec<Action>> {
    entries.into_iter().collect()
}

fn tool_call_schedule(
    entries: Vec<(u64, AgentToolCallTrace)>,
) -> BTreeMap<u64, Vec<AgentToolCallTrace>> {
    let mut schedule = BTreeMap::new();
    for (tick, trace) in entries {
        schedule.entry(tick).or_insert_with(Vec::new).push(trace);
    }
    schedule
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::EncounterTransition;

    #[test]
    fn shard_target_configuration_matches_mmo_goal() {
        let config = FlagshipMmoAcceptanceConfig::shard_target();
        assert!(config
            .scale_target
            .satisfied_by(config.human_sessions, config.autonomous_agents));
    }

    #[test]
    fn flagship_acceptance_harness_covers_core_mmo_loop() {
        let result = run_flagship_mmo_acceptance(FlagshipMmoAcceptanceConfig::ci_smoke()).unwrap();

        assert_eq!(result.summary.human_sessions, 2);
        assert_eq!(result.summary.autonomous_agents, 4);
        assert_eq!(
            result.summary.telemetry_frames,
            result.config.total_ticks as usize
        );
        assert!(result.summary.chat_messages >= 4);
        assert!(result.summary.damage_events >= 4);
        assert_eq!(result.summary.captures, 1);
        assert_eq!(result.summary.capture_actions, 1);
        assert!(result.summary.summons >= 2);
        assert_eq!(result.summary.summon_actions, 1);
        assert_eq!(result.summary.companion_commands, 1);
        assert_eq!(result.summary.resource_gathers, 1);
        assert_eq!(result.summary.gather_actions, 1);
        assert_eq!(result.summary.loot_claims, 1);
        assert_eq!(result.summary.loot_actions, 1);
        assert!(result.summary.tool_calls >= 2);
        assert!(result.summary.tool_call_errors >= 1);
        assert!(result.summary.average_path_distance > 0.0);
        assert!(result.parity_passed());

        let transitions = result
            .replay
            .training_samples()
            .into_iter()
            .filter_map(|sample| sample.encounter_transition)
            .collect::<Vec<_>>();
        assert!(!transitions.is_empty());
        assert!(transitions.iter().any(|transition| matches!(
            transition,
            EncounterTransition::CombatStateChanged {
                in_combat: false,
                ..
            }
        )));
    }
}
