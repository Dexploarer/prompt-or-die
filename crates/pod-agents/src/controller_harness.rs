use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use glam::Vec2;
use pod_core::action::{validate_action, Action, ActionResult, AgentAction};
use pod_core::agent::{Agent, AgentType};
use pod_core::component::EncounterState;
use pod_core::contract::AgentRuntimeProfile;
use pod_core::id::{AgentId, EntityId};
use pod_core::observation::{Objective, Observation, Relationship, SelfState, VisibleEntity};
use serde::{Deserialize, Serialize};

use crate::hybrid_agent::{HybridAgent, HybridAgentConfig};
use crate::llm_agent::{LlmAgent, LlmAgentConfig};
use crate::llm_provider::{
    CompletionRequest, CompletionResponse, LlmError, LlmProvider, TokenBudget, TokenUsage,
};
use crate::neural_agent::{GreedyActionSelector, NeuralAgent, PolicyNetwork};
use crate::scripted_agent::{
    attack_nearest, flee_from, guard, patrol, BehaviorNode, BehaviorTree, ScriptedAgent,
};
use crate::{FallbackParser, MemoryConfig, ToonTemplate};

const HARNESS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerHarnessStepReport {
    pub controller_id: String,
    pub agent_type: String,
    pub scenario_id: String,
    pub scenario_description: String,
    pub action_keys: Vec<String>,
    pub representative_action_key: String,
    pub valid_action: bool,
    pub objective_aligned: bool,
    pub encounter_success: bool,
    pub latency_ms: f64,
    pub tool_call_count: usize,
    pub tool_call_error_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerHarnessSummary {
    pub controller_id: String,
    pub agent_type: String,
    pub step_count: usize,
    pub valid_action_count: usize,
    pub valid_action_basis_points: u16,
    pub objective_step_count: usize,
    pub objective_aligned_count: usize,
    pub objective_alignment_basis_points: u16,
    pub encounter_step_count: usize,
    pub encounter_success_count: usize,
    pub encounter_success_basis_points: u16,
    pub average_latency_ms: f64,
    pub max_latency_ms: f64,
    pub tool_call_steps: usize,
    pub tool_call_reliance_basis_points: u16,
    pub tool_call_error_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerParitySummary {
    pub baseline_controller_id: String,
    pub candidate_controller_id: String,
    pub compared_step_count: usize,
    pub matched_step_count: usize,
    pub action_parity_basis_points: u16,
    pub mismatched_scenario_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerHarnessReport {
    pub schema_version: u32,
    pub generated_at_unix_ms: u128,
    pub harness_id: String,
    pub steps: Vec<ControllerHarnessStepReport>,
    pub controllers: Vec<ControllerHarnessSummary>,
    pub parity_checks: Vec<ControllerParitySummary>,
    pub failed_checks: Vec<String>,
}

impl ControllerHarnessReport {
    pub fn passed(&self) -> bool {
        self.failed_checks.is_empty()
    }
}

#[derive(Debug, Clone)]
struct HarnessScenario {
    id: &'static str,
    description: &'static str,
    observation: Observation,
    objective_action_keys: &'static [&'static str],
    encounter_action_keys: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HarnessControllerKind {
    ScriptedBaseline,
    Llm,
    Hybrid,
    Neural,
}

impl HarnessControllerKind {
    fn controller_id(self) -> &'static str {
        match self {
            Self::ScriptedBaseline => "scripted_baseline",
            Self::Llm => "llm_agent",
            Self::Hybrid => "hybrid_agent",
            Self::Neural => "neural_agent",
        }
    }

    fn agent_type(self) -> AgentType {
        match self {
            Self::ScriptedBaseline => AgentType::ScriptedNpc,
            Self::Llm | Self::Hybrid => AgentType::LlmAgent,
            Self::Neural => AgentType::NeuralAgent,
        }
    }
}

pub fn run_controller_parity_harness() -> ControllerHarnessReport {
    let scenarios = curated_scenarios();
    let generated_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let mut steps = Vec::new();
    for kind in [
        HarnessControllerKind::ScriptedBaseline,
        HarnessControllerKind::Llm,
        HarnessControllerKind::Hybrid,
        HarnessControllerKind::Neural,
    ] {
        for scenario in &scenarios {
            steps.push(run_controller_step(kind, scenario));
        }
    }

    let controllers = summarize_controllers(&steps);
    let parity_checks = summarize_parity(&steps, &scenarios);
    let failed_checks = collect_failures(&controllers, &parity_checks);

    ControllerHarnessReport {
        schema_version: HARNESS_SCHEMA_VERSION,
        generated_at_unix_ms,
        harness_id: "curated-controller-parity".to_string(),
        steps,
        controllers,
        parity_checks,
        failed_checks,
    }
}

fn run_controller_step(
    kind: HarnessControllerKind,
    scenario: &HarnessScenario,
) -> ControllerHarnessStepReport {
    let started = Instant::now();
    let (actions, agent_id, constraints, tool_call_count, tool_call_error_count) = match kind {
        HarnessControllerKind::ScriptedBaseline => {
            let mut agent = build_scripted_agent(scenario.id);
            agent.observe(scenario.observation.clone());
            let actions = agent.decide();
            let tool_calls = agent.drain_tool_calls();
            (
                actions,
                agent.id(),
                agent.constraints().clone(),
                tool_calls.len(),
                tool_calls
                    .iter()
                    .filter(|trace| tool_call_status_is_error(trace.status))
                    .count(),
            )
        }
        HarnessControllerKind::Llm => {
            let mut agent = build_llm_agent(scenario.id);
            agent.observe(scenario.observation.clone());
            let ready = agent.wait_for_request_completion(Duration::from_millis(250));
            let actions = agent.decide();
            let tool_calls = agent.drain_tool_calls();
            let final_actions = if ready { actions } else { vec![Action::Idle] };
            (
                final_actions,
                agent.id(),
                agent.constraints().clone(),
                tool_calls.len(),
                tool_calls
                    .iter()
                    .filter(|trace| tool_call_status_is_error(trace.status))
                    .count(),
            )
        }
        HarnessControllerKind::Hybrid => {
            let mut agent = build_hybrid_agent(scenario.id);
            agent.observe(scenario.observation.clone());
            let ready = agent.wait_for_strategy_completion(Duration::from_millis(250));
            let actions = agent.decide();
            let tool_calls = agent.drain_tool_calls();
            let final_actions = if ready { actions } else { vec![Action::Idle] };
            (
                final_actions,
                agent.id(),
                agent.constraints().clone(),
                tool_calls.len(),
                tool_calls
                    .iter()
                    .filter(|trace| tool_call_status_is_error(trace.status))
                    .count(),
            )
        }
        HarnessControllerKind::Neural => {
            let mut agent = build_neural_agent();
            agent.observe(scenario.observation.clone());
            let actions = agent.decide();
            (actions, agent.id(), agent.constraints().clone(), 0, 0)
        }
    };
    let latency_ms = started.elapsed().as_secs_f64() * 1000.0;

    let action_keys = actions
        .iter()
        .map(|action| action_semantic_key(action).to_string())
        .collect::<Vec<_>>();
    let representative_action_key = pick_representative_action_key(
        &action_keys,
        scenario.objective_action_keys,
        scenario.encounter_action_keys,
    )
    .to_string();
    let valid_action = actions.iter().all(|action| {
        scenario
            .observation
            .available_actions
            .iter()
            .any(|candidate| candidate == action_variant_name(action))
            && matches!(
                validate_action(
                    &AgentAction {
                        agent_id,
                        tick: scenario.observation.tick,
                        action: action.clone(),
                    },
                    &constraints,
                    scenario.observation.tick,
                ),
                ActionResult::Valid
            )
    });
    let objective_aligned = scenario
        .objective_action_keys
        .iter()
        .any(|expected| action_keys.iter().any(|actual| actual == expected));
    let encounter_success = if scenario.encounter_action_keys.is_empty() {
        false
    } else {
        scenario
            .encounter_action_keys
            .iter()
            .any(|expected| action_keys.iter().any(|actual| actual == expected))
    };

    ControllerHarnessStepReport {
        controller_id: kind.controller_id().to_string(),
        agent_type: agent_type_key(kind.agent_type()).to_string(),
        scenario_id: scenario.id.to_string(),
        scenario_description: scenario.description.to_string(),
        action_keys,
        representative_action_key,
        valid_action,
        objective_aligned,
        encounter_success,
        latency_ms,
        tool_call_count,
        tool_call_error_count,
    }
}

fn summarize_controllers(steps: &[ControllerHarnessStepReport]) -> Vec<ControllerHarnessSummary> {
    let mut grouped = BTreeMap::<String, Vec<&ControllerHarnessStepReport>>::new();
    for step in steps {
        grouped
            .entry(step.controller_id.clone())
            .or_default()
            .push(step);
    }

    grouped
        .into_iter()
        .map(|(controller_id, rows)| {
            let step_count = rows.len();
            let valid_action_count = rows.iter().filter(|row| row.valid_action).count();
            let objective_step_count = rows.len();
            let objective_aligned_count = rows.iter().filter(|row| row.objective_aligned).count();
            let encounter_step_count = rows
                .iter()
                .filter(|row| {
                    matches!(
                        row.scenario_id.as_str(),
                        "engage_hostile" | "retreat_low_health"
                    )
                })
                .count();
            let encounter_success_count = rows.iter().filter(|row| row.encounter_success).count();
            let total_latency_ms = rows.iter().map(|row| row.latency_ms).sum::<f64>();
            let max_latency_ms = rows.iter().map(|row| row.latency_ms).fold(0.0f64, f64::max);
            let tool_call_steps = rows.iter().filter(|row| row.tool_call_count > 0).count();
            let tool_call_error_count = rows
                .iter()
                .map(|row| row.tool_call_error_count)
                .sum::<usize>();

            ControllerHarnessSummary {
                controller_id,
                agent_type: rows[0].agent_type.clone(),
                step_count,
                valid_action_count,
                valid_action_basis_points: ratio_basis_points(valid_action_count, step_count),
                objective_step_count,
                objective_aligned_count,
                objective_alignment_basis_points: ratio_basis_points(
                    objective_aligned_count,
                    objective_step_count,
                ),
                encounter_step_count,
                encounter_success_count,
                encounter_success_basis_points: ratio_basis_points(
                    encounter_success_count,
                    encounter_step_count,
                ),
                average_latency_ms: if step_count == 0 {
                    0.0
                } else {
                    total_latency_ms / step_count as f64
                },
                max_latency_ms,
                tool_call_steps,
                tool_call_reliance_basis_points: ratio_basis_points(tool_call_steps, step_count),
                tool_call_error_count,
            }
        })
        .collect()
}

fn summarize_parity(
    steps: &[ControllerHarnessStepReport],
    scenarios: &[HarnessScenario],
) -> Vec<ControllerParitySummary> {
    let baseline = steps
        .iter()
        .filter(|step| {
            step.controller_id == HarnessControllerKind::ScriptedBaseline.controller_id()
        })
        .map(|step| {
            (
                step.scenario_id.clone(),
                step.representative_action_key.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    [
        HarnessControllerKind::Llm,
        HarnessControllerKind::Hybrid,
        HarnessControllerKind::Neural,
    ]
    .into_iter()
    .map(|kind| {
        let candidate_steps = steps
            .iter()
            .filter(|step| step.controller_id == kind.controller_id())
            .collect::<Vec<_>>();
        let mut matched = 0usize;
        let mut mismatched = Vec::new();
        for scenario in scenarios {
            let candidate = candidate_steps
                .iter()
                .find(|step| step.scenario_id == scenario.id)
                .expect("candidate step should exist");
            let baseline_action = baseline
                .get(scenario.id)
                .expect("baseline action should exist");
            if &candidate.representative_action_key == baseline_action {
                matched += 1;
            } else {
                mismatched.push(scenario.id.to_string());
            }
        }
        let compared_step_count = scenarios.len();
        ControllerParitySummary {
            baseline_controller_id: HarnessControllerKind::ScriptedBaseline
                .controller_id()
                .to_string(),
            candidate_controller_id: kind.controller_id().to_string(),
            compared_step_count,
            matched_step_count: matched,
            action_parity_basis_points: ratio_basis_points(matched, compared_step_count),
            mismatched_scenario_ids: mismatched,
        }
    })
    .collect()
}

fn collect_failures(
    controllers: &[ControllerHarnessSummary],
    parity_checks: &[ControllerParitySummary],
) -> Vec<String> {
    let mut failures = Vec::new();

    for controller in controllers {
        if controller.valid_action_count != controller.step_count {
            failures.push(format!(
                "{} produced invalid actions on {}/{} curated steps",
                controller.controller_id,
                controller.step_count - controller.valid_action_count,
                controller.step_count
            ));
        }
        if controller.objective_aligned_count != controller.objective_step_count {
            failures.push(format!(
                "{} missed objective alignment on {}/{} curated steps",
                controller.controller_id,
                controller.objective_step_count - controller.objective_aligned_count,
                controller.objective_step_count
            ));
        }
        if controller.encounter_success_count != controller.encounter_step_count {
            failures.push(format!(
                "{} missed encounter outcomes on {}/{} curated encounter steps",
                controller.controller_id,
                controller.encounter_step_count - controller.encounter_success_count,
                controller.encounter_step_count
            ));
        }
    }

    for parity in parity_checks {
        if parity.matched_step_count != parity.compared_step_count {
            failures.push(format!(
                "{} diverged from {} on {} curated steps",
                parity.candidate_controller_id,
                parity.baseline_controller_id,
                parity.compared_step_count - parity.matched_step_count
            ));
        }
    }

    failures
}

fn build_scripted_agent(scenario_id: &str) -> ScriptedAgent {
    let tree = match scenario_id {
        "engage_hostile" => BehaviorTree::new(attack_nearest()),
        "retreat_low_health" => BehaviorTree::new(flee_from(Vec2::new(1.0, 0.0))),
        "advance_objective" => BehaviorTree::new(patrol(vec![Vec2::new(1.0, 0.0)])),
        "hold_position" => BehaviorTree::new(guard(Vec2::ZERO, 40.0)),
        other => panic!("unsupported scripted scenario: {other}"),
    };
    ScriptedAgent::new_with_behavior_tree(tree)
}

fn build_llm_agent(scenario_id: &str) -> LlmAgent {
    let response = match scenario_id {
        "engage_hostile" => r#"{"actions":["attack"],"reasoning":"engage nearby hostile"}"#,
        "retreat_low_health" => {
            r#"{"actions":["move west"],"reasoning":"retreat while low health"}"#
        }
        "advance_objective" => {
            r#"{"actions":["move east"],"reasoning":"advance toward objective"}"#
        }
        "hold_position" => r#"{"actions":["idle"],"reasoning":"hold the line"}"#,
        other => panic!("unsupported llm scenario: {other}"),
    };

    LlmAgent::with_components(
        LlmAgentConfig {
            reaction_time_ms: 0,
            ..Default::default()
        },
        Arc::new(SequenceProvider::new(
            "controller-harness-llm",
            vec![response.to_string()],
        )),
        Arc::new(ToonTemplate),
        Arc::new(FallbackParser::default_chain()),
        MemoryConfig::default(),
        TokenBudget::unlimited(),
    )
}

fn build_hybrid_agent(scenario_id: &str) -> HybridAgent {
    let response = match scenario_id {
        "engage_hostile" => {
            r#"{"strategy":"attack","urgency":0.9,"reasoning":"engage nearby hostile"}"#
        }
        "retreat_low_health" => {
            r#"{"strategy":"flee","urgency":1.0,"reasoning":"retreat while low health"}"#
        }
        "advance_objective" => {
            r#"{"strategy":"explore","urgency":0.5,"reasoning":"advance toward objective"}"#
        }
        "hold_position" => r#"{"strategy":"hold","urgency":0.4,"reasoning":"hold current ground"}"#,
        other => panic!("unsupported hybrid scenario: {other}"),
    };

    HybridAgent::new(HybridAgentConfig {
        strategy_interval_ticks: 0,
        ..Default::default()
    })
    .with_provider(Arc::new(SequenceProvider::new(
        "controller-harness-hybrid",
        vec![response.to_string()],
    )))
    .with_tree(BehaviorTree::new(BehaviorNode::Failure))
}

fn build_neural_agent() -> NeuralAgent {
    NeuralAgent::with_selector_and_network(
        Box::new(GreedyActionSelector),
        Box::new(HeuristicParityPolicy),
    )
}

fn curated_scenarios() -> Vec<HarnessScenario> {
    vec![
        HarnessScenario {
            id: "engage_hostile",
            description: "High-health agent should engage a nearby hostile.",
            observation: make_observation(
                1,
                0.05,
                1.0,
                Some((EntityId(41), Vec2::new(50.0, 0.0), 50.0)),
            ),
            objective_action_keys: &["attack"],
            encounter_action_keys: &["attack"],
        },
        HarnessScenario {
            id: "retreat_low_health",
            description: "Low-health agent should move away from nearby hostile pressure.",
            observation: make_observation(
                2,
                0.10,
                0.2,
                Some((EntityId(42), Vec2::new(50.0, 0.0), 50.0)),
            ),
            objective_action_keys: &["move"],
            encounter_action_keys: &["move"],
        },
        HarnessScenario {
            id: "advance_objective",
            description: "No hostiles visible; controller should advance the active objective.",
            observation: make_observation(3, 0.35, 1.0, None),
            objective_action_keys: &["move"],
            encounter_action_keys: &[],
        },
        HarnessScenario {
            id: "hold_position",
            description: "Completed objective and no hostiles should yield a hold action.",
            observation: make_completed_observation(4, 1.0),
            objective_action_keys: &["idle", "stop"],
            encounter_action_keys: &[],
        },
    ]
}

fn make_observation(
    tick: u64,
    objective_progress: f32,
    health_ratio: f32,
    hostile: Option<(EntityId, Vec2, f32)>,
) -> Observation {
    let mut visible_entities = Vec::new();
    let encounter = hostile.as_ref().map(|(entity_id, _, _)| EncounterState {
        encounter_id: 900 + tick,
        kind: pod_core::component::EncounterKind::OpenWorld,
        threat_level: 0.75,
        primary_target: Some(*entity_id),
        active_turn_owner: Some(EntityId(7)),
        capture_allowed: false,
        in_combat: true,
    });

    if let Some((entity_id, position, distance)) = hostile {
        visible_entities.push(VisibleEntity {
            entity_id,
            entity_type: "hostile".to_string(),
            position,
            velocity: Vec2::ZERO,
            rotation: 0.0,
            distance,
            relationship: Relationship::Hostile,
            health_fraction: Some(0.6),
            combat_style: None,
            creature: None,
        });
    }

    Observation {
        tick,
        elapsed_secs: tick as f32 / 60.0,
        self_state: SelfState {
            agent_id: AgentId::new(),
            entity_id: EntityId(7),
            runtime_profile: AgentRuntimeProfile::for_agent_type(AgentType::Human),
            position: Vec2::ZERO,
            rotation: 0.0,
            velocity: Vec2::ZERO,
            health: Some(health_ratio * 100.0),
            max_health: Some(100.0),
            team: pod_core::component::Team::Team(1),
            cooldowns: vec![],
            combat_loadout: None,
            skills: vec![],
            inventory: None,
            companion_roster: None,
            encounter,
        },
        visible_entities,
        audible_events: vec![],
        messages: vec![],
        available_actions: vec![
            "Move".to_string(),
            "Stop".to_string(),
            "Rotate".to_string(),
            "LookAt".to_string(),
            "Attack".to_string(),
            "Interact".to_string(),
            "Idle".to_string(),
        ],
        objectives: vec![Objective {
            id: format!("objective-{tick}"),
            description: "Curated harness objective".to_string(),
            progress: objective_progress,
            completed: false,
        }],
    }
}

fn make_completed_observation(tick: u64, health_ratio: f32) -> Observation {
    let mut observation = make_observation(tick, 1.0, health_ratio, None);
    observation.objectives[0].completed = true;
    observation
}

fn action_variant_name(action: &Action) -> &'static str {
    match action {
        Action::Move { .. } => "Move",
        Action::Stop => "Stop",
        Action::Rotate { .. } => "Rotate",
        Action::LookAt { .. } => "LookAt",
        Action::Attack | Action::AttackTarget { .. } => "Attack",
        Action::UseAbility { .. } => "UseAbility",
        Action::CaptureCreature { .. } => "CaptureCreature",
        Action::SummonCompanion { .. } => "SummonCompanion",
        Action::CommandCompanion { .. } => "CommandCompanion",
        Action::Interact | Action::InteractWith { .. } => "Interact",
        Action::Pickup { .. } => "Pickup",
        Action::Drop { .. } => "Drop",
        Action::UseItem { .. } => "UseItem",
        Action::GatherResource { .. } => "GatherResource",
        Action::Loot { .. } => "Loot",
        Action::Speak { .. } => "Speak",
        Action::Signal { .. } => "Signal",
        Action::SetAutoRetaliate { .. } => "SetAutoRetaliate",
        Action::Idle => "Idle",
        Action::Spawn { .. } => "Spawn",
    }
}

fn action_semantic_key(action: &Action) -> &'static str {
    match action {
        Action::Move { .. } => "move",
        Action::Stop => "stop",
        Action::Rotate { .. } => "rotate",
        Action::LookAt { .. } => "look_at",
        Action::Attack | Action::AttackTarget { .. } => "attack",
        Action::UseAbility { .. } => "use_ability",
        Action::CaptureCreature { .. } => "capture",
        Action::SummonCompanion { .. } => "summon",
        Action::CommandCompanion { .. } => "command_companion",
        Action::Interact | Action::InteractWith { .. } => "interact",
        Action::Pickup { .. } => "pickup",
        Action::Drop { .. } => "drop",
        Action::UseItem { .. } => "use_item",
        Action::GatherResource { .. } => "gather_resource",
        Action::Loot { .. } => "loot",
        Action::Speak { .. } => "speak",
        Action::Signal { .. } => "signal",
        Action::SetAutoRetaliate { .. } => "set_auto_retaliate",
        Action::Idle => "idle",
        Action::Spawn { .. } => "spawn",
    }
}

fn pick_representative_action_key<'a>(
    action_keys: &'a [String],
    objective_keys: &[&str],
    encounter_keys: &[&str],
) -> &'a str {
    for expected in objective_keys.iter().chain(encounter_keys.iter()) {
        if let Some(found) = action_keys
            .iter()
            .find(|candidate| candidate.as_str() == *expected)
        {
            return found.as_str();
        }
    }

    action_keys.first().map(String::as_str).unwrap_or("idle")
}

fn ratio_basis_points(numerator: usize, denominator: usize) -> u16 {
    if denominator == 0 {
        return 0;
    }
    ((numerator * 10_000) / denominator) as u16
}

fn agent_type_key(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::Human => "human",
        AgentType::ScriptedNpc => "scripted_npc",
        AgentType::LlmAgent => "llm_agent",
        AgentType::NeuralAgent => "neural_agent",
        AgentType::System => "system",
    }
}

fn tool_call_status_is_error(status: pod_core::ToolCallStatus) -> bool {
    !matches!(
        status,
        pod_core::ToolCallStatus::Requested | pod_core::ToolCallStatus::Succeeded
    )
}

struct SequenceProvider {
    name: &'static str,
    responses: Mutex<Vec<String>>,
}

impl SequenceProvider {
    fn new(name: &'static str, responses: Vec<String>) -> Self {
        Self {
            name,
            responses: Mutex::new(responses),
        }
    }
}

impl LlmProvider for SequenceProvider {
    fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut responses = self.responses.lock().unwrap();
        let content = if responses.is_empty() {
            r#"{"actions":["idle"],"reasoning":"provider exhausted"}"#.to_string()
        } else {
            responses.remove(0)
        };
        let prompt_tokens = self.estimate_tokens(&request.user_prompt);
        let completion_tokens = self.estimate_tokens(&content);
        Ok(CompletionResponse {
            content,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            model: self.name.to_string(),
        })
    }

    fn name(&self) -> &str {
        self.name
    }
}

struct HeuristicParityPolicy;

impl PolicyNetwork for HeuristicParityPolicy {
    fn forward(&self, features: &[f32]) -> Vec<f32> {
        let health_ratio = features.get(5).copied().unwrap_or(1.0);
        let hostile_count = features.get(6).copied().unwrap_or(0.0);
        let objective_progress = features.get(24).copied().unwrap_or(1.0);

        let mut logits = vec![0.0; 10];
        if hostile_count > 0.0 {
            if health_ratio < 0.35 {
                logits[3] = 1.0;
            } else {
                logits[6] = 1.0;
            }
        } else if objective_progress < 0.95 {
            logits[4] = 1.0;
        } else {
            logits[0] = 1.0;
        }
        logits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controller_parity_harness_reports_expected_metrics() {
        let report = run_controller_parity_harness();

        assert!(
            report.passed(),
            "unexpected failures: {:?}",
            report.failed_checks
        );
        assert_eq!(report.controllers.len(), 4);
        assert_eq!(report.parity_checks.len(), 3);

        let llm = report
            .controllers
            .iter()
            .find(|summary| summary.controller_id == "llm_agent")
            .expect("llm summary");
        assert_eq!(llm.step_count, 4);
        assert_eq!(llm.valid_action_basis_points, 10_000);
        assert_eq!(llm.tool_call_reliance_basis_points, 10_000);

        let neural_parity = report
            .parity_checks
            .iter()
            .find(|summary| summary.candidate_controller_id == "neural_agent")
            .expect("neural parity");
        assert_eq!(neural_parity.action_parity_basis_points, 10_000);

        let hold_step = report
            .steps
            .iter()
            .find(|step| {
                step.controller_id == "scripted_baseline" && step.scenario_id == "hold_position"
            })
            .expect("hold step");
        assert_eq!(hold_step.representative_action_key, "idle");
    }

    #[test]
    fn heuristic_parity_policy_matches_curated_expectations() {
        let scenarios = curated_scenarios();
        let engage = scenarios
            .iter()
            .find(|scenario| scenario.id == "engage_hostile")
            .expect("engage scenario");
        let retreat = scenarios
            .iter()
            .find(|scenario| scenario.id == "retreat_low_health")
            .expect("retreat scenario");
        let hold = scenarios
            .iter()
            .find(|scenario| scenario.id == "hold_position")
            .expect("hold scenario");

        let policy = HeuristicParityPolicy;
        let engage_logits =
            policy.forward(&NeuralAgent::observation_to_features(&engage.observation));
        let retreat_logits =
            policy.forward(&NeuralAgent::observation_to_features(&retreat.observation));
        let hold_logits = policy.forward(&NeuralAgent::observation_to_features(&hold.observation));

        assert_eq!(
            engage_logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(index, _)| index),
            Some(6)
        );
        assert_eq!(
            retreat_logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(index, _)| index),
            Some(3)
        );
        assert_eq!(
            hold_logits
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .map(|(index, _)| index),
            Some(0)
        );
    }
}
