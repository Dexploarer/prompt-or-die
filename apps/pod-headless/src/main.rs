use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pod_core::{
    build_remote_topology_bundle, build_remote_topology_parity_summary,
    build_world_admission_summary, build_world_control_plane_summary, build_world_quest_bindings,
    run_flagship_mmo_acceptance, AgentRewardSignal, AgentRuntimeProfile, AgentTeamDefinition,
    AgentType, AgentTypeCountSummary, AppliedWorldStateSummary, ControllerEvaluationSummary,
    CrossWorldEffect, CrossWorldLinkDefinition, CrossWorldPropagation, FlagshipMmoAcceptanceConfig,
    FlagshipMmoAcceptanceResult, FlagshipMmoAcceptanceSummary, NamedDeltaSummary,
    ObjectiveShiftSummary, QuestLineStateSummary, QuestStageApplicationSummary,
    QuestStageDefinition, QuestStateGraph, RemoteTopologyBundle, RemoteTopologyParitySummary,
    ReplayTrainingSample, RewardReason, ScenarioEvaluationSummary, TeamControlMode,
    TeamDeathMarkSummary, TeamDeltaSummary, TournamentEliminationMode, WorldAdmissionSummary,
    WorldControlPlaneSummary, WorldEvaluationSummary, WorldQuestBinding, WorldRealityDefinition,
    WorldRealityRole, WorldTournamentDefinition,
};
use serde::Serialize;

const REPORT_SCHEMA_VERSION: u32 = 2;
const DEFAULT_SCENARIO: &str = "deadman-neural-cup";

#[derive(Debug)]
struct HeadlessOptions {
    profile: String,
    scenario: String,
    output: Option<PathBuf>,
    dataset_output: Option<PathBuf>,
    topology_output: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ScenarioDefinition {
    tournament: WorldTournamentDefinition,
    teams: Vec<AgentTeamDefinition>,
    worlds: Vec<WorldRealityDefinition>,
    links: Vec<CrossWorldLinkDefinition>,
    quest_graphs: Vec<QuestStateGraph>,
    world_quest_graph_ids: BTreeMap<String, Vec<String>>,
    base_config: FlagshipMmoAcceptanceConfig,
}

#[derive(Debug)]
struct WorldExecution {
    world: WorldRealityDefinition,
    result: FlagshipMmoAcceptanceResult,
    report: WorldRunReport,
}

#[derive(Debug, Serialize)]
struct HeadlessAppReport {
    schema_version: u32,
    generated_at_unix_ms: u128,
    scenario: String,
    profile: String,
    tournament: WorldTournamentDefinition,
    teams: Vec<AgentTeamDefinition>,
    worlds: Vec<WorldRealityDefinition>,
    links: Vec<CrossWorldLinkDefinition>,
    quest_graphs: Vec<QuestStateGraph>,
    world_quest_bindings: Vec<WorldQuestBinding>,
    world_admissions: Vec<WorldAdmissionSummary>,
    world_control_planes: Vec<WorldControlPlaneSummary>,
    world_runs: Vec<WorldRunReport>,
    dataset_summary: RewardDatasetSummary,
    cross_world_projections: Vec<CrossWorldProjectionReport>,
    applied_world_states: Vec<AppliedWorldStateReport>,
    standings: Vec<TeamStandingReport>,
    evaluation: ScenarioEvaluationReport,
    topology_parity: TopologyParityReport,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct WorldRunReport {
    world_id: String,
    display_name: String,
    role: WorldRealityRole,
    ruleset_id: String,
    world_seed: u64,
    summary: FlagshipMmoAcceptanceSummary,
    runtime: WorldRuntimeReport,
    rewards: WorldRewardReport,
}

#[derive(Debug, Clone, Serialize)]
struct WorldRuntimeReport {
    total_runtime_ms: f64,
    average_tick_runtime_ms: f64,
    max_tick_runtime_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
struct WorldRewardReport {
    sample_count: usize,
    signal_count: usize,
    terminal_sample_count: usize,
    total: f32,
    positive_total: f32,
    negative_total: f32,
    reasons: Vec<RewardReasonStat>,
}

#[derive(Debug, Clone, Serialize)]
struct RewardDatasetSummary {
    row_count: usize,
    terminal_row_count: usize,
    total: f32,
    positive_total: f32,
    negative_total: f32,
    reasons: Vec<RewardReasonStat>,
}

#[derive(Debug, Clone, Serialize)]
struct RewardDatasetExport {
    schema_version: u32,
    generated_at_unix_ms: u128,
    scenario: String,
    profile: String,
    tournament_id: String,
    summary: RewardDatasetSummary,
    worlds: Vec<WorldDatasetExport>,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct WorldDatasetExport {
    world_id: String,
    display_name: String,
    role: WorldRealityRole,
    world_seed: u64,
    summary: RewardDatasetSummary,
    rows: Vec<RewardDatasetRow>,
}

#[derive(Debug, Clone, Serialize)]
struct RewardDatasetRow {
    world_id: String,
    world_role: WorldRealityRole,
    world_seed: u64,
    team_id: Option<String>,
    team_slot: Option<u16>,
    runtime_profile: AgentRuntimeProfile,
    sample: ReplayTrainingSample,
    reward_reasons: Vec<RewardReasonStat>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
struct RewardReasonStat {
    reason: String,
    count: usize,
    total_value: f32,
}

#[derive(Debug, Clone, Serialize)]
struct CrossWorldProjectionReport {
    link_id: String,
    source_world_id: String,
    target_world_id: String,
    trigger_tags: Vec<String>,
    propagation: CrossWorldPropagation,
    trigger_count: usize,
    application_count: usize,
    matched_tags: Vec<LinkTriggerMatch>,
    projected_effects: Vec<ProjectedCrossWorldEffect>,
}

type AppliedWorldStateReport = AppliedWorldStateSummary;
type TeamDeltaReport = TeamDeltaSummary;
type TeamDeathMarkReport = TeamDeathMarkSummary;
type NamedDeltaReport = NamedDeltaSummary;
type ObjectiveShiftReport = ObjectiveShiftSummary;
type QuestLineStateReport = QuestLineStateSummary;
type QuestStageApplicationReport = QuestStageApplicationSummary;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct LinkTriggerMatch {
    tag: String,
    matches: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ProjectedCrossWorldEffect {
    FactionReputationDelta {
        faction_id: String,
        per_application: i32,
        total_delta: i32,
    },
    EncounterWeightDelta {
        table_id: String,
        per_application: i16,
        total_delta: i32,
    },
    ResourceScarcityDelta {
        biome_id: String,
        per_application: i16,
        total_delta: i32,
    },
    TeamScoreDelta {
        team_id: String,
        per_application: i32,
        total_delta: i32,
    },
    DeathMark {
        team_id: String,
        per_application_duration_ticks: u32,
        total_duration_ticks: u64,
        applications: usize,
    },
    ObjectiveStateShift {
        quest_graph_id: String,
        stage_tag: String,
        applications: usize,
    },
}

#[derive(Debug, Clone, Serialize)]
struct TeamStandingReport {
    team_id: String,
    display_name: String,
    control_mode: TeamControlMode,
    home_world_id: String,
    participating_world_ids: Vec<String>,
    assigned_agent_count: usize,
    controller_mix: Vec<AgentTypeCountSummary>,
    dataset_row_count: usize,
    world_reward_total: f32,
    applied_score_delta: i32,
    active_death_marks: usize,
    active_death_mark_ticks: u64,
}

type ScenarioEvaluationReport = ScenarioEvaluationSummary;
type ControllerEvaluationReport = ControllerEvaluationSummary;
type WorldEvaluationReport = WorldEvaluationSummary;
type TopologyParityReport = RemoteTopologyParitySummary;

#[derive(Debug, Default, Clone)]
struct TeamStandingAccumulator {
    assigned_agents: BTreeSet<String>,
    runtime_profiles_by_agent: BTreeMap<String, AgentRuntimeProfile>,
    dataset_row_count: usize,
    world_reward_total: f32,
    applied_score_delta: i32,
    active_death_marks: usize,
    active_death_mark_ticks: u64,
}

#[derive(Debug)]
struct ScenarioRunOutputs {
    report: HeadlessAppReport,
    dataset: RewardDatasetExport,
    topology: RemoteTopologyBundle,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let options = parse_args()?;
    let scenario = build_scenario(&options.scenario, &options.profile)?;
    let outputs = run_scenario(&options, scenario)?;
    let json = serde_json::to_string_pretty(&outputs.report)?;

    if let Some(output) = &options.output {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, json.as_bytes())?;
    }
    if let Some(dataset_output) = &options.dataset_output {
        if let Some(parent) = dataset_output.parent() {
            fs::create_dir_all(parent)?;
        }
        let dataset_json = serde_json::to_string_pretty(&outputs.dataset)?;
        fs::write(dataset_output, dataset_json.as_bytes())?;
    }
    if let Some(topology_output) = &options.topology_output {
        if let Some(parent) = topology_output.parent() {
            fs::create_dir_all(parent)?;
        }
        let topology_json = serde_json::to_string_pretty(&outputs.topology)?;
        fs::write(topology_output, topology_json.as_bytes())?;
    }

    println!("{json}");
    Ok(())
}

fn parse_args() -> Result<HeadlessOptions, Box<dyn std::error::Error>> {
    parse_args_from(env::args().skip(1))
}

fn parse_args_from<I, S>(args: I) -> Result<HeadlessOptions, Box<dyn std::error::Error>>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut options = HeadlessOptions {
        profile: "ci-smoke".into(),
        scenario: DEFAULT_SCENARIO.into(),
        output: None,
        dataset_output: None,
        topology_output: None,
    };

    let args = args.into_iter().map(Into::into).collect::<Vec<String>>();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--profile" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --profile")?;
                options.profile = value.to_owned();
            }
            "--scenario" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --scenario")?;
                options.scenario = value.to_owned();
            }
            "--output" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --output")?;
                options.output = Some(PathBuf::from(value));
            }
            "--dataset-output" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("missing value for --dataset-output")?;
                options.dataset_output = Some(PathBuf::from(value));
            }
            "--topology-output" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("missing value for --topology-output")?;
                options.topology_output = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            unknown => return Err(format!("unknown argument: {unknown}").into()),
        }
        index += 1;
    }

    Ok(options)
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p pod-headless -- [--profile ci-smoke|shard-target] [--scenario deadman-neural-cup] [--output PATH] [--dataset-output PATH] [--topology-output PATH]"
    );
}

fn build_scenario(scenario: &str, profile: &str) -> Result<ScenarioDefinition, String> {
    let base_config = match profile {
        "ci-smoke" => FlagshipMmoAcceptanceConfig::ci_smoke(),
        "shard-target" => FlagshipMmoAcceptanceConfig::shard_target(),
        unknown => {
            return Err(format!(
                "unsupported profile '{unknown}' (expected 'ci-smoke' or 'shard-target')"
            ));
        }
    };

    match scenario {
        DEFAULT_SCENARIO => Ok(build_deadman_neural_cup(base_config)),
        unknown => Err(format!(
            "unsupported scenario '{unknown}' (expected '{DEFAULT_SCENARIO}')"
        )),
    }
}

fn build_deadman_neural_cup(base_config: FlagshipMmoAcceptanceConfig) -> ScenarioDefinition {
    let mut iron_sigil = AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime");
    iron_sigil.control_mode = TeamControlMode::HybridCommand;
    iron_sigil.allowed_world_ids = vec![
        "deadman-prime".into(),
        "deadman-shadow".into(),
        "sanctuary-echo".into(),
    ];
    iron_sigil.objective_tags = vec![
        "deadman-season".into(),
        "cross-world-pressure".into(),
        "reality-links".into(),
    ];

    let mut gloam_mesh = AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow");
    gloam_mesh.control_mode = TeamControlMode::AutonomousSwarm;
    gloam_mesh.allowed_world_ids = vec!["deadman-prime".into(), "deadman-shadow".into()];
    gloam_mesh.objective_tags = vec![
        "neural-swarm".into(),
        "alternate-reality".into(),
        "death-pressure".into(),
    ];

    let mut deadman_prime =
        WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
    deadman_prime.role = WorldRealityRole::Tournament;
    deadman_prime.linked_world_ids = vec!["deadman-shadow".into(), "sanctuary-echo".into()];
    deadman_prime.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];

    let mut deadman_shadow =
        WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "deadman-mirror");
    deadman_shadow.role = WorldRealityRole::Shadow;
    deadman_shadow.linked_world_ids = vec!["deadman-prime".into()];
    deadman_shadow.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];

    let mut sanctuary_echo =
        WorldRealityDefinition::new("sanctuary-echo", "Sanctuary Echo", "echo-sanctuary");
    sanctuary_echo.role = WorldRealityRole::Sanctuary;
    sanctuary_echo.linked_world_ids = vec!["deadman-prime".into()];
    sanctuary_echo.active_team_ids = vec!["iron-sigil".into()];

    let (quest_graphs, world_quest_graph_ids) = build_deadman_quest_graphs();

    let mut prime_to_shadow =
        CrossWorldLinkDefinition::new("prime-to-shadow", "deadman-prime", "deadman-shadow");
    prime_to_shadow.trigger_tags = vec![
        "kill-secured".into(),
        "creature-captured".into(),
        "death-pressure".into(),
    ];
    prime_to_shadow.propagation = CrossWorldPropagation::Delayed { ticks: 300 };
    prime_to_shadow.cooldown_ticks = 120;
    prime_to_shadow.max_applications_per_window = 8;
    prime_to_shadow.effects = vec![
        CrossWorldEffect::TeamScoreDelta {
            team_id: "iron-sigil".into(),
            delta: 5,
        },
        CrossWorldEffect::DeathMark {
            team_id: "gloam-mesh".into(),
            duration_ticks: 600,
        },
        CrossWorldEffect::ObjectiveStateShift {
            quest_graph_id: "deadman-shadow-hunt".into(),
            stage_tag: "marked-by-kills".into(),
        },
    ];

    let mut shadow_to_prime =
        CrossWorldLinkDefinition::new("shadow-to-prime", "deadman-shadow", "deadman-prime");
    shadow_to_prime.trigger_tags = vec!["death-taken".into(), "loot-claimed".into()];
    shadow_to_prime.propagation = CrossWorldPropagation::Threshold {
        required_triggers: 2,
    };
    shadow_to_prime.max_applications_per_window = 6;
    shadow_to_prime.effects = vec![
        CrossWorldEffect::TeamScoreDelta {
            team_id: "gloam-mesh".into(),
            delta: 4,
        },
        CrossWorldEffect::ResourceScarcityDelta {
            biome_id: "deadman-prime-wilds".into(),
            delta: 1,
        },
        CrossWorldEffect::ObjectiveStateShift {
            quest_graph_id: "deadman-prime-season".into(),
            stage_tag: "wilds-under-siege".into(),
        },
    ];

    let mut prime_to_sanctuary =
        CrossWorldLinkDefinition::new("prime-to-sanctuary", "deadman-prime", "sanctuary-echo");
    prime_to_sanctuary.trigger_tags = vec!["resource-gathered".into(), "skill-xp".into()];
    prime_to_sanctuary.propagation = CrossWorldPropagation::Threshold {
        required_triggers: 3,
    };
    prime_to_sanctuary.max_applications_per_window = 4;
    prime_to_sanctuary.effects = vec![
        CrossWorldEffect::FactionReputationDelta {
            faction_id: "echo-order".into(),
            delta: 2,
        },
        CrossWorldEffect::ObjectiveStateShift {
            quest_graph_id: "sanctuary-echo-uplift".into(),
            stage_tag: "uplifted".into(),
        },
    ];

    let teams = vec![iron_sigil, gloam_mesh];
    let worlds = vec![deadman_prime, deadman_shadow, sanctuary_echo];
    let links = vec![prime_to_shadow, shadow_to_prime, prime_to_sanctuary];

    let mut tournament = WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup");
    tournament.world_ids = worlds.iter().map(|world| world.world_id.clone()).collect();
    tournament.team_ids = teams.iter().map(|team| team.team_id.clone()).collect();
    tournament.cross_world_link_ids = links.iter().map(|link| link.link_id.clone()).collect();
    tournament.max_agents_per_team = 10;
    tournament.elimination_mode = TournamentEliminationMode::Permadeath;
    tournament.reward_tags = vec![
        "seasonal-deadman".into(),
        "alternate-reality".into(),
        "neural-swarm".into(),
    ];

    ScenarioDefinition {
        tournament,
        teams,
        worlds,
        links,
        quest_graphs,
        world_quest_graph_ids,
        base_config,
    }
}

fn build_deadman_quest_graphs() -> (Vec<QuestStateGraph>, BTreeMap<String, Vec<String>>) {
    let quest_graphs = vec![
        QuestStateGraph::new(
            "deadman-prime-season",
            "Deadman Prime: Blood Season",
            "enter-bracket",
            vec![
                QuestStageDefinition {
                    stage_id: "enter-bracket".into(),
                    title: "Enter the Bracket".into(),
                    objectives: vec!["Establish the team camp in Deadman Prime.".into()],
                    next_stage_ids: vec!["wilds-under-siege".into()],
                    reward_tags: vec!["season-open".into()],
                },
                QuestStageDefinition {
                    stage_id: "wilds-under-siege".into(),
                    title: "Wilds Under Siege".into(),
                    objectives: vec!["Survive pressure from the shadow world.".into()],
                    next_stage_ids: vec!["blood-round".into()],
                    reward_tags: vec!["wilds-under-siege".into()],
                },
                QuestStageDefinition {
                    stage_id: "blood-round".into(),
                    title: "Blood Round".into(),
                    objectives: vec!["Convert linked-world kills into a finals push.".into()],
                    next_stage_ids: vec!["crown-push".into()],
                    reward_tags: vec!["blood-round".into()],
                },
                QuestStageDefinition {
                    stage_id: "crown-push".into(),
                    title: "Crown Push".into(),
                    objectives: vec!["Hold the world long enough to close the season.".into()],
                    next_stage_ids: Vec::new(),
                    reward_tags: vec!["crown-push".into()],
                },
            ],
        ),
        QuestStateGraph::new(
            "deadman-shadow-hunt",
            "Deadman Shadow: Mirror Hunt",
            "shadow-observe",
            vec![
                QuestStageDefinition {
                    stage_id: "shadow-observe".into(),
                    title: "Observe the Prime World".into(),
                    objectives: vec!["Track rival teams from the mirror layer.".into()],
                    next_stage_ids: vec!["marked-by-kills".into()],
                    reward_tags: vec!["shadow-observe".into()],
                },
                QuestStageDefinition {
                    stage_id: "marked-by-kills".into(),
                    title: "Marked by Kills".into(),
                    objectives: vec!["Respond to kill pressure leaking in from Prime.".into()],
                    next_stage_ids: vec!["rift-collapse".into()],
                    reward_tags: vec!["marked-by-kills".into()],
                },
                QuestStageDefinition {
                    stage_id: "rift-collapse".into(),
                    title: "Rift Collapse".into(),
                    objectives: vec!["Lock down the breach before the swarm overruns it.".into()],
                    next_stage_ids: Vec::new(),
                    reward_tags: vec!["rift-collapse".into()],
                },
            ],
        ),
        QuestStateGraph::new(
            "sanctuary-echo-uplift",
            "Sanctuary Echo: Uplift",
            "attune-shrine",
            vec![
                QuestStageDefinition {
                    stage_id: "attune-shrine".into(),
                    title: "Attune the Shrine".into(),
                    objectives: vec!["Stabilize the sanctuary attunement anchor.".into()],
                    next_stage_ids: vec!["uplifted".into()],
                    reward_tags: vec!["attune-shrine".into()],
                },
                QuestStageDefinition {
                    stage_id: "uplifted".into(),
                    title: "Uplifted".into(),
                    objectives: vec!["Accept prosperity leaking in from Deadman Prime.".into()],
                    next_stage_ids: vec!["echo-stabilized".into()],
                    reward_tags: vec!["uplifted".into()],
                },
                QuestStageDefinition {
                    stage_id: "echo-stabilized".into(),
                    title: "Echo Stabilized".into(),
                    objectives: vec!["Lock the uplift into a permanent sanctuary boon.".into()],
                    next_stage_ids: Vec::new(),
                    reward_tags: vec!["echo-stabilized".into()],
                },
            ],
        ),
    ];

    let mut world_quest_graph_ids = BTreeMap::new();
    world_quest_graph_ids.insert("deadman-prime".into(), vec!["deadman-prime-season".into()]);
    world_quest_graph_ids.insert("deadman-shadow".into(), vec!["deadman-shadow-hunt".into()]);
    world_quest_graph_ids.insert(
        "sanctuary-echo".into(),
        vec!["sanctuary-echo-uplift".into()],
    );

    (quest_graphs, world_quest_graph_ids)
}

fn run_scenario(
    options: &HeadlessOptions,
    scenario: ScenarioDefinition,
) -> Result<ScenarioRunOutputs, Box<dyn std::error::Error>> {
    let mut executions = Vec::new();
    for (index, world) in scenario.worlds.iter().enumerate() {
        let config = world_config_for(world, &scenario.base_config, index);
        let result = run_flagship_mmo_acceptance(config)?;
        let report = build_world_run_report(world, &result);
        executions.push(WorldExecution {
            world: world.clone(),
            result,
            report,
        });
    }

    let world_admissions = executions
        .iter()
        .map(|execution| {
            world_admission_summary_for_result(&execution.world, &execution.result, &scenario.teams)
        })
        .collect::<Vec<_>>();
    let world_control_planes = executions
        .iter()
        .zip(world_admissions.iter())
        .map(|(execution, admissions)| {
            build_world_control_plane_for_result(admissions, &execution.result)
        })
        .collect::<Vec<_>>();
    let world_admissions_by_world = world_admissions
        .iter()
        .map(|summary| (summary.world_id.clone(), summary.clone()))
        .collect::<BTreeMap<_, _>>();

    let dataset_worlds = executions
        .iter()
        .map(|execution| {
            let admissions = world_admissions_by_world
                .get(&execution.world.world_id)
                .expect("world admissions should exist for every world execution");
            build_world_dataset_export(execution, admissions)
        })
        .collect::<Vec<_>>();
    let dataset_summary = summarize_dataset_rows(
        &dataset_worlds
            .iter()
            .flat_map(|world| world.rows.iter().cloned())
            .collect::<Vec<_>>(),
    );

    let mut cross_world_projections = Vec::new();
    for link in &scenario.links {
        cross_world_projections.push(build_projection_report(link, &executions)?);
    }
    let applied_world_states = build_applied_world_states(
        &scenario.worlds,
        &cross_world_projections,
        &scenario.quest_graphs,
        &scenario.world_quest_graph_ids,
    );

    let standings = build_team_standings(
        &scenario.teams,
        &scenario.worlds,
        &world_control_planes,
        &dataset_worlds,
        &applied_world_states,
    );
    let evaluation = build_scenario_evaluation(&dataset_worlds, &applied_world_states);

    let generated_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let tournament_id = scenario.tournament.tournament_id.clone();
    let notes = vec![
        "World runs are authoritative flagship acceptance simulations with deterministic per-world seed derivation.".into(),
        "Cross-world projections are derived from canonical reward reasons in replay telemetry, not browser-local heuristics.".into(),
        "World admissions and team controller mix now come from explicit per-world control-plane summaries derived from the authoritative runtime roster.".into(),
        "Dataset rows are replay-derived training samples enriched with authoritative reward reasons and runtime profile metadata.".into(),
    ];
    let world_quest_bindings = build_world_quest_bindings(&scenario.world_quest_graph_ids);
    let topology = build_remote_topology_bundle(
        &options.scenario,
        &options.profile,
        generated_at_unix_ms,
        &scenario.tournament,
        &scenario.teams,
        &scenario.worlds,
        &scenario.links,
        &scenario.quest_graphs,
        &scenario.world_quest_graph_ids,
        &world_admissions,
        &world_control_planes,
        &applied_world_states,
        &evaluation,
    );
    let topology_parity = build_remote_topology_parity_summary(
        &scenario.teams,
        &scenario.worlds,
        &scenario.links,
        &scenario.quest_graphs,
        &world_quest_bindings,
        &world_admissions,
        &world_control_planes,
        &applied_world_states,
        &evaluation,
        &topology,
    );

    Ok(ScenarioRunOutputs {
        report: HeadlessAppReport {
            schema_version: REPORT_SCHEMA_VERSION,
            generated_at_unix_ms,
            scenario: options.scenario.clone(),
            profile: options.profile.clone(),
            tournament: scenario.tournament,
            teams: scenario.teams,
            worlds: scenario.worlds,
            links: scenario.links,
            quest_graphs: scenario.quest_graphs,
            world_quest_bindings: topology.world_quest_bindings.clone(),
            world_admissions: topology.world_admissions.clone(),
            world_control_planes: topology.world_control_planes.clone(),
            world_runs: executions
                .into_iter()
                .map(|execution| execution.report)
                .collect(),
            dataset_summary: dataset_summary.clone(),
            cross_world_projections,
            applied_world_states,
            standings,
            evaluation,
            topology_parity,
            notes: notes.clone(),
        },
        dataset: RewardDatasetExport {
            schema_version: REPORT_SCHEMA_VERSION,
            generated_at_unix_ms,
            scenario: options.scenario.clone(),
            profile: options.profile.clone(),
            tournament_id,
            summary: dataset_summary,
            worlds: dataset_worlds,
            notes,
        },
        topology,
    })
}

fn world_config_for(
    world: &WorldRealityDefinition,
    base_config: &FlagshipMmoAcceptanceConfig,
    index: usize,
) -> FlagshipMmoAcceptanceConfig {
    let mut config = base_config.clone();
    config.world_seed = mix_world_seed(base_config.world_seed, &world.world_id, index);
    config
}

fn mix_world_seed(base_seed: u64, world_id: &str, index: usize) -> u64 {
    let mut hash = 0xcbf29ce484222325u64 ^ base_seed ^ index as u64;
    for byte in world_id.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn build_world_run_report(
    world: &WorldRealityDefinition,
    result: &FlagshipMmoAcceptanceResult,
) -> WorldRunReport {
    let total_runtime_ms = result.tick_durations_ms().iter().sum::<f64>();
    let average_tick_runtime_ms = if result.tick_durations_ms().is_empty() {
        0.0
    } else {
        total_runtime_ms / result.tick_durations_ms().len() as f64
    };
    let max_tick_runtime_ms = result
        .tick_durations_ms()
        .iter()
        .copied()
        .fold(0.0_f64, f64::max);

    WorldRunReport {
        world_id: world.world_id.clone(),
        display_name: world.display_name.clone(),
        role: world.role,
        ruleset_id: world.ruleset_id.clone(),
        world_seed: result.config.world_seed,
        summary: result.summary.clone(),
        runtime: WorldRuntimeReport {
            total_runtime_ms,
            average_tick_runtime_ms,
            max_tick_runtime_ms,
        },
        rewards: build_world_reward_report(result),
    }
}

fn build_world_reward_report(result: &FlagshipMmoAcceptanceResult) -> WorldRewardReport {
    let samples = result.training_samples();
    let terminal_sample_count = samples
        .iter()
        .filter(|sample| sample.reward_summary.terminal)
        .count();
    let total = samples
        .iter()
        .map(|sample| sample.reward_summary.total)
        .sum();
    let positive_total = samples
        .iter()
        .map(|sample| sample.reward_summary.positive_total)
        .sum();
    let negative_total = samples
        .iter()
        .map(|sample| sample.reward_summary.negative_total)
        .sum();

    let reasons = collect_reward_reason_stats(iter_reward_signals(result));
    let signal_count = reasons.iter().map(|stat| stat.count).sum();

    WorldRewardReport {
        sample_count: samples.len(),
        signal_count,
        terminal_sample_count,
        total,
        positive_total,
        negative_total,
        reasons,
    }
}

fn build_world_dataset_export(
    execution: &WorldExecution,
    admissions: &WorldAdmissionSummary,
) -> WorldDatasetExport {
    let rows = build_dataset_rows(&execution.world, &execution.result, admissions);
    let summary = summarize_dataset_rows(&rows);

    WorldDatasetExport {
        world_id: execution.world.world_id.clone(),
        display_name: execution.world.display_name.clone(),
        role: execution.world.role,
        world_seed: execution.result.config.world_seed,
        summary,
        rows,
    }
}

fn build_dataset_rows(
    world: &WorldRealityDefinition,
    result: &FlagshipMmoAcceptanceResult,
    admissions: &WorldAdmissionSummary,
) -> Vec<RewardDatasetRow> {
    let samples = result.training_samples();
    let mut sample_index = 0usize;
    let mut rows = Vec::with_capacity(samples.len());
    let admissions = admissions
        .assignments
        .iter()
        .cloned()
        .into_iter()
        .map(|assignment| (assignment.agent_id.clone(), assignment))
        .collect::<BTreeMap<_, _>>();

    for window in result.telemetry_windows() {
        for agent in &window.agents {
            let sample = samples[sample_index].clone();
            sample_index += 1;
            let admission = admissions.get(&agent.agent_id.0.to_string());
            rows.push(RewardDatasetRow {
                world_id: world.world_id.clone(),
                world_role: world.role,
                world_seed: result.config.world_seed,
                team_id: admission.map(|entry| entry.team_id.clone()),
                team_slot: admission.map(|entry| entry.slot_index),
                runtime_profile: agent.runtime_profile,
                sample,
                reward_reasons: collect_reward_reason_stats(agent.reward_signals.iter()),
            });
        }
    }

    rows
}

fn world_admission_summary_for_result(
    world: &WorldRealityDefinition,
    result: &FlagshipMmoAcceptanceResult,
    teams: &[AgentTeamDefinition],
) -> WorldAdmissionSummary {
    let roster = runtime_profiles_for_result(result)
        .into_keys()
        .collect::<Vec<_>>();
    build_world_admission_summary(&roster, world, teams)
}

fn build_world_control_plane_for_result(
    admissions: &WorldAdmissionSummary,
    result: &FlagshipMmoAcceptanceResult,
) -> WorldControlPlaneSummary {
    build_world_control_plane_summary(admissions, &runtime_profiles_for_result(result))
}

fn runtime_profiles_for_result(
    result: &FlagshipMmoAcceptanceResult,
) -> BTreeMap<String, AgentRuntimeProfile> {
    let mut profiles = BTreeMap::new();
    for window in result.telemetry_windows() {
        for agent in &window.agents {
            profiles
                .entry(agent.agent_id.0.to_string())
                .or_insert(agent.runtime_profile);
        }
    }
    profiles
}

fn summarize_dataset_rows(rows: &[RewardDatasetRow]) -> RewardDatasetSummary {
    let row_count = rows.len();
    let terminal_row_count = rows
        .iter()
        .filter(|row| row.sample.reward_summary.terminal)
        .count();
    let total = rows.iter().map(|row| row.sample.reward_summary.total).sum();
    let positive_total = rows
        .iter()
        .map(|row| row.sample.reward_summary.positive_total)
        .sum();
    let negative_total = rows
        .iter()
        .map(|row| row.sample.reward_summary.negative_total)
        .sum();

    let mut by_reason = BTreeMap::<String, (usize, f32)>::new();
    for row in rows {
        for stat in &row.reward_reasons {
            let entry = by_reason
                .entry(stat.reason.clone())
                .or_insert((0usize, 0.0));
            entry.0 += stat.count;
            entry.1 += stat.total_value;
        }
    }

    RewardDatasetSummary {
        row_count,
        terminal_row_count,
        total,
        positive_total,
        negative_total,
        reasons: by_reason
            .into_iter()
            .map(|(reason, (count, total_value))| RewardReasonStat {
                reason,
                count,
                total_value,
            })
            .collect(),
    }
}

fn build_projection_report(
    link: &CrossWorldLinkDefinition,
    executions: &[WorldExecution],
) -> Result<CrossWorldProjectionReport, String> {
    let source = executions
        .iter()
        .find(|execution| execution.world.world_id == link.source_world_id)
        .ok_or_else(|| {
            format!(
                "missing source world '{}' for link '{}'",
                link.source_world_id, link.link_id
            )
        })?;
    let trigger_summary = build_trigger_summary(link, &source.result);
    let application_count = derive_application_count(
        trigger_summary.trigger_count,
        link.propagation,
        link.max_applications_per_window,
    );

    Ok(CrossWorldProjectionReport {
        link_id: link.link_id.clone(),
        source_world_id: link.source_world_id.clone(),
        target_world_id: link.target_world_id.clone(),
        trigger_tags: link.trigger_tags.clone(),
        propagation: link.propagation.clone(),
        trigger_count: trigger_summary.trigger_count,
        application_count,
        matched_tags: trigger_summary.matched_tags,
        projected_effects: project_effects(link, application_count),
    })
}

#[derive(Debug)]
struct TriggerSummary {
    trigger_count: usize,
    matched_tags: Vec<LinkTriggerMatch>,
}

fn build_trigger_summary(
    link: &CrossWorldLinkDefinition,
    result: &FlagshipMmoAcceptanceResult,
) -> TriggerSummary {
    let mut matched_tags = BTreeMap::<String, usize>::new();
    let mut trigger_count = 0usize;

    for signal in iter_reward_signals(result) {
        let signal_tags = reward_signal_tags(signal);
        let mut matched_signal = false;
        for trigger_tag in &link.trigger_tags {
            if signal_tags.iter().any(|tag| *tag == trigger_tag.as_str()) {
                *matched_tags.entry(trigger_tag.clone()).or_insert(0) += 1;
                matched_signal = true;
            }
        }
        if matched_signal {
            trigger_count += 1;
        }
    }

    TriggerSummary {
        trigger_count,
        matched_tags: matched_tags
            .into_iter()
            .map(|(tag, matches)| LinkTriggerMatch { tag, matches })
            .collect(),
    }
}

fn derive_application_count(
    trigger_count: usize,
    propagation: CrossWorldPropagation,
    max_applications_per_window: u16,
) -> usize {
    if trigger_count == 0 || max_applications_per_window == 0 {
        return 0;
    }

    let uncapped = match propagation {
        CrossWorldPropagation::Immediate | CrossWorldPropagation::Delayed { .. } => trigger_count,
        CrossWorldPropagation::Threshold { required_triggers } => {
            let required = usize::from(required_triggers.max(1));
            trigger_count / required
        }
        CrossWorldPropagation::Scaled { basis_points } => {
            let numerator = trigger_count.saturating_mul(usize::from(basis_points));
            numerator.div_ceil(10_000)
        }
    };

    uncapped.min(usize::from(max_applications_per_window))
}

fn project_effects(
    link: &CrossWorldLinkDefinition,
    application_count: usize,
) -> Vec<ProjectedCrossWorldEffect> {
    link.effects
        .iter()
        .map(|effect| match effect {
            CrossWorldEffect::FactionReputationDelta { faction_id, delta } => {
                ProjectedCrossWorldEffect::FactionReputationDelta {
                    faction_id: faction_id.clone(),
                    per_application: *delta,
                    total_delta: *delta * application_count as i32,
                }
            }
            CrossWorldEffect::EncounterWeightDelta { table_id, delta } => {
                ProjectedCrossWorldEffect::EncounterWeightDelta {
                    table_id: table_id.clone(),
                    per_application: *delta,
                    total_delta: i32::from(*delta) * application_count as i32,
                }
            }
            CrossWorldEffect::ResourceScarcityDelta { biome_id, delta } => {
                ProjectedCrossWorldEffect::ResourceScarcityDelta {
                    biome_id: biome_id.clone(),
                    per_application: *delta,
                    total_delta: i32::from(*delta) * application_count as i32,
                }
            }
            CrossWorldEffect::TeamScoreDelta { team_id, delta } => {
                ProjectedCrossWorldEffect::TeamScoreDelta {
                    team_id: team_id.clone(),
                    per_application: *delta,
                    total_delta: *delta * application_count as i32,
                }
            }
            CrossWorldEffect::DeathMark {
                team_id,
                duration_ticks,
            } => ProjectedCrossWorldEffect::DeathMark {
                team_id: team_id.clone(),
                per_application_duration_ticks: *duration_ticks,
                total_duration_ticks: u64::from(*duration_ticks) * application_count as u64,
                applications: application_count,
            },
            CrossWorldEffect::ObjectiveStateShift {
                quest_graph_id,
                stage_tag,
            } => ProjectedCrossWorldEffect::ObjectiveStateShift {
                quest_graph_id: quest_graph_id.clone(),
                stage_tag: stage_tag.clone(),
                applications: application_count,
            },
        })
        .collect()
}

fn build_team_standings(
    teams: &[AgentTeamDefinition],
    worlds: &[WorldRealityDefinition],
    world_control_planes: &[WorldControlPlaneSummary],
    dataset_worlds: &[WorldDatasetExport],
    applied_world_states: &[AppliedWorldStateReport],
) -> Vec<TeamStandingReport> {
    let mut by_team = BTreeMap::<String, TeamStandingAccumulator>::new();
    for world_control_plane in world_control_planes {
        for team in &world_control_plane.teams {
            let entry = by_team.entry(team.team_id.clone()).or_default();
            for assignment in &team.assignments {
                entry.assigned_agents.insert(assignment.agent_id.clone());
                entry
                    .runtime_profiles_by_agent
                    .entry(assignment.agent_id.clone())
                    .or_insert(assignment.runtime_profile);
            }
        }
    }
    for world in dataset_worlds {
        for row in &world.rows {
            if let Some(team_id) = &row.team_id {
                let entry = by_team.entry(team_id.clone()).or_default();
                entry.dataset_row_count += 1;
                entry.world_reward_total += row.sample.reward_summary.total;
            }
        }
    }
    for state in applied_world_states {
        for team_score in &state.team_scores {
            by_team
                .entry(team_score.team_id.clone())
                .or_default()
                .applied_score_delta += team_score.total_delta;
        }
        for death_mark in &state.death_marks {
            let entry = by_team.entry(death_mark.team_id.clone()).or_default();
            entry.active_death_marks += death_mark.applications;
            entry.active_death_mark_ticks += death_mark.total_duration_ticks;
        }
    }

    teams
        .iter()
        .map(|team| {
            let totals = by_team.remove(&team.team_id).unwrap_or_default();
            let mut controller_mix = totals.runtime_profiles_by_agent.values().fold(
                BTreeMap::<String, usize>::new(),
                |mut by_type, runtime_profile| {
                    *by_type
                        .entry(agent_type_key(runtime_profile.agent_type).to_string())
                        .or_default() += 1;
                    by_type
                },
            );
            let participating_world_ids = worlds
                .iter()
                .filter(|world| {
                    world
                        .active_team_ids
                        .iter()
                        .any(|team_id| team_id == &team.team_id)
                })
                .map(|world| world.world_id.clone())
                .collect();

            TeamStandingReport {
                team_id: team.team_id.clone(),
                display_name: team.display_name.clone(),
                control_mode: team.control_mode,
                home_world_id: team.home_world_id.clone(),
                participating_world_ids,
                assigned_agent_count: totals.assigned_agents.len(),
                controller_mix: controller_mix
                    .iter_mut()
                    .map(|(agent_type, count)| AgentTypeCountSummary {
                        agent_type: agent_type.clone(),
                        count: *count,
                    })
                    .collect(),
                dataset_row_count: totals.dataset_row_count,
                world_reward_total: totals.world_reward_total,
                applied_score_delta: totals.applied_score_delta,
                active_death_marks: totals.active_death_marks,
                active_death_mark_ticks: totals.active_death_mark_ticks,
            }
        })
        .collect()
}

fn build_scenario_evaluation(
    dataset_worlds: &[WorldDatasetExport],
    applied_world_states: &[AppliedWorldStateReport],
) -> ScenarioEvaluationReport {
    let controller_mix = build_controller_evaluation_reports(
        &dataset_worlds
            .iter()
            .flat_map(|world| world.rows.iter().cloned())
            .collect::<Vec<_>>(),
    );
    let applied_world_lookup = applied_world_states
        .iter()
        .map(|state| (state.world_id.clone(), state))
        .collect::<BTreeMap<_, _>>();

    let worlds = dataset_worlds
        .iter()
        .map(|world| {
            let applied = applied_world_lookup
                .get(&world.world_id)
                .expect("applied world state should exist for every dataset world");
            let row_count = world.rows.len();
            let average_reward_per_row = if row_count == 0 {
                0.0
            } else {
                world.summary.total / row_count as f32
            };
            let quest_line_count = applied.quest_lines.len();
            let progressed_quest_line_count = applied
                .quest_lines
                .iter()
                .filter(|quest_line| {
                    !quest_line.completed_stage_ids.is_empty()
                        || !quest_line.stage_applications.is_empty()
                })
                .count();
            let average_quest_progress_basis_points = if quest_line_count == 0 {
                0
            } else {
                (applied
                    .quest_lines
                    .iter()
                    .map(|quest_line| u32::from(quest_line.progress_basis_points))
                    .sum::<u32>()
                    / quest_line_count as u32) as u16
            };

            WorldEvaluationReport {
                world_id: world.world_id.clone(),
                display_name: world.display_name.clone(),
                role: world.role,
                average_reward_per_row,
                controller_mix: build_controller_evaluation_reports(&world.rows),
                quest_line_count,
                progressed_quest_line_count,
                average_quest_progress_basis_points,
                applied_score_delta_total: applied
                    .team_scores
                    .iter()
                    .map(|score| score.total_delta)
                    .sum(),
                applied_death_mark_count: applied
                    .death_marks
                    .iter()
                    .map(|death_mark| death_mark.applications)
                    .sum(),
                applied_death_mark_ticks: applied
                    .death_marks
                    .iter()
                    .map(|death_mark| death_mark.total_duration_ticks)
                    .sum(),
                applied_objective_shift_count: applied
                    .objective_state_shifts
                    .iter()
                    .map(|shift| shift.applications)
                    .sum(),
                applied_reputation_delta_total: applied
                    .faction_reputation_deltas
                    .iter()
                    .map(|delta| delta.total_delta)
                    .sum(),
                applied_encounter_delta_total: applied
                    .encounter_weight_deltas
                    .iter()
                    .map(|delta| delta.total_delta)
                    .sum(),
                applied_resource_delta_total: applied
                    .resource_scarcity_deltas
                    .iter()
                    .map(|delta| delta.total_delta)
                    .sum(),
            }
        })
        .collect();

    ScenarioEvaluationReport {
        controller_mix,
        worlds,
    }
}

fn build_controller_evaluation_reports(
    rows: &[RewardDatasetRow],
) -> Vec<ControllerEvaluationReport> {
    let mut by_type = BTreeMap::<String, (usize, f32)>::new();
    for row in rows {
        let entry = by_type
            .entry(agent_type_key(row.runtime_profile.agent_type).to_string())
            .or_insert((0usize, 0.0));
        entry.0 += 1;
        entry.1 += row.sample.reward_summary.total;
    }

    by_type
        .into_iter()
        .map(
            |(agent_type, (row_count, reward_total))| ControllerEvaluationReport {
                agent_type,
                row_count,
                reward_total,
                average_reward_per_row: if row_count == 0 {
                    0.0
                } else {
                    reward_total / row_count as f32
                },
            },
        )
        .collect()
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

fn build_applied_world_states(
    worlds: &[WorldRealityDefinition],
    projections: &[CrossWorldProjectionReport],
    quest_graphs: &[QuestStateGraph],
    world_quest_graph_ids: &BTreeMap<String, Vec<String>>,
) -> Vec<AppliedWorldStateReport> {
    let quest_graph_lookup = quest_graphs
        .iter()
        .map(|graph| (graph.quest_id.clone(), graph))
        .collect::<BTreeMap<_, _>>();

    worlds
        .iter()
        .map(|world| {
            let mut team_scores = BTreeMap::<String, i32>::new();
            let mut death_marks = BTreeMap::<String, (usize, u64)>::new();
            let mut faction_reputation = BTreeMap::<String, i32>::new();
            let mut encounter_weights = BTreeMap::<String, i32>::new();
            let mut resource_scarcity = BTreeMap::<String, i32>::new();
            let mut objective_shifts = BTreeMap::<(String, String), usize>::new();

            for projection in projections
                .iter()
                .filter(|projection| projection.target_world_id == world.world_id)
            {
                for effect in &projection.projected_effects {
                    match effect {
                        ProjectedCrossWorldEffect::FactionReputationDelta {
                            faction_id,
                            total_delta,
                            ..
                        } => {
                            if *total_delta != 0 {
                                *faction_reputation.entry(faction_id.clone()).or_insert(0) +=
                                    *total_delta;
                            }
                        }
                        ProjectedCrossWorldEffect::EncounterWeightDelta {
                            table_id,
                            total_delta,
                            ..
                        } => {
                            if *total_delta != 0 {
                                *encounter_weights.entry(table_id.clone()).or_insert(0) +=
                                    *total_delta;
                            }
                        }
                        ProjectedCrossWorldEffect::ResourceScarcityDelta {
                            biome_id,
                            total_delta,
                            ..
                        } => {
                            if *total_delta != 0 {
                                *resource_scarcity.entry(biome_id.clone()).or_insert(0) +=
                                    *total_delta;
                            }
                        }
                        ProjectedCrossWorldEffect::TeamScoreDelta {
                            team_id,
                            total_delta,
                            ..
                        } => {
                            if *total_delta != 0 {
                                *team_scores.entry(team_id.clone()).or_insert(0) += *total_delta;
                            }
                        }
                        ProjectedCrossWorldEffect::DeathMark {
                            team_id,
                            applications,
                            total_duration_ticks,
                            ..
                        } => {
                            if *applications > 0 && *total_duration_ticks > 0 {
                                let entry =
                                    death_marks.entry(team_id.clone()).or_insert((0usize, 0));
                                entry.0 += *applications;
                                entry.1 += *total_duration_ticks;
                            }
                        }
                        ProjectedCrossWorldEffect::ObjectiveStateShift {
                            quest_graph_id,
                            stage_tag,
                            applications,
                        } => {
                            if *applications > 0 {
                                *objective_shifts
                                    .entry((quest_graph_id.clone(), stage_tag.clone()))
                                    .or_insert(0) += *applications;
                            }
                        }
                    }
                }
            }

            let objective_state_shifts = objective_shifts
                .iter()
                .map(
                    |((quest_graph_id, stage_tag), applications)| ObjectiveShiftReport {
                        quest_graph_id: quest_graph_id.clone(),
                        stage_tag: stage_tag.clone(),
                        applications: *applications,
                    },
                )
                .collect::<Vec<_>>();
            let mut unresolved_objective_state_shifts = objective_state_shifts
                .iter()
                .filter(|shift| {
                    !world_quest_graph_ids
                        .get(&world.world_id)
                        .is_some_and(|quest_ids| quest_ids.contains(&shift.quest_graph_id))
                })
                .cloned()
                .collect::<Vec<_>>();
            let mut quest_lines = Vec::new();

            for quest_graph_id in world_quest_graph_ids
                .get(&world.world_id)
                .into_iter()
                .flat_map(|quest_ids| quest_ids.iter())
            {
                let Some(graph) = quest_graph_lookup.get(quest_graph_id) else {
                    continue;
                };
                let shifts_for_graph = objective_state_shifts
                    .iter()
                    .filter(|shift| shift.quest_graph_id == graph.quest_id)
                    .cloned()
                    .collect::<Vec<_>>();
                let (quest_line, unresolved_shifts) =
                    build_quest_line_state(graph, &shifts_for_graph);
                quest_lines.push(quest_line);
                unresolved_objective_state_shifts.extend(unresolved_shifts);
            }

            AppliedWorldStateReport {
                world_id: world.world_id.clone(),
                display_name: world.display_name.clone(),
                role: world.role,
                team_scores: team_scores
                    .into_iter()
                    .map(|(team_id, total_delta)| TeamDeltaReport {
                        team_id,
                        total_delta,
                    })
                    .collect(),
                death_marks: death_marks
                    .into_iter()
                    .map(
                        |(team_id, (applications, total_duration_ticks))| TeamDeathMarkReport {
                            team_id,
                            applications,
                            total_duration_ticks,
                        },
                    )
                    .collect(),
                faction_reputation_deltas: faction_reputation
                    .into_iter()
                    .map(|(id, total_delta)| NamedDeltaReport { id, total_delta })
                    .collect(),
                encounter_weight_deltas: encounter_weights
                    .into_iter()
                    .map(|(id, total_delta)| NamedDeltaReport { id, total_delta })
                    .collect(),
                resource_scarcity_deltas: resource_scarcity
                    .into_iter()
                    .map(|(id, total_delta)| NamedDeltaReport { id, total_delta })
                    .collect(),
                objective_state_shifts,
                unresolved_objective_state_shifts,
                quest_lines,
            }
        })
        .collect()
}

fn build_quest_line_state(
    graph: &QuestStateGraph,
    shifts: &[ObjectiveShiftReport],
) -> (QuestLineStateReport, Vec<ObjectiveShiftReport>) {
    let mut stage_applications = BTreeMap::<String, usize>::new();
    let mut unresolved = Vec::new();

    for shift in shifts {
        let matching_stage_ids = graph
            .stages
            .iter()
            .filter(|stage| {
                stage.stage_id == shift.stage_tag
                    || stage
                        .reward_tags
                        .iter()
                        .any(|reward_tag| reward_tag == &shift.stage_tag)
            })
            .map(|stage| stage.stage_id.clone())
            .collect::<Vec<_>>();

        if matching_stage_ids.is_empty() {
            unresolved.push(shift.clone());
            continue;
        }

        for stage_id in matching_stage_ids {
            *stage_applications.entry(stage_id).or_insert(0) += shift.applications;
        }
    }

    let mut reached_stage_ids = BTreeSet::new();
    let mut current_stage_candidates = Vec::new();
    if stage_applications.is_empty() {
        reached_stage_ids.insert(graph.start_stage_id.clone());
        current_stage_candidates.push(graph.start_stage_id.clone());
    } else {
        for stage_id in stage_applications.keys() {
            if let Some(path) = quest_path_from_start(graph, stage_id) {
                for path_stage_id in path {
                    reached_stage_ids.insert(path_stage_id);
                }
                current_stage_candidates.push(stage_id.clone());
            } else {
                unresolved.push(ObjectiveShiftReport {
                    quest_graph_id: graph.quest_id.clone(),
                    stage_tag: stage_id.clone(),
                    applications: *stage_applications
                        .get(stage_id)
                        .expect("stage application exists"),
                });
            }
        }
        if reached_stage_ids.is_empty() {
            reached_stage_ids.insert(graph.start_stage_id.clone());
            current_stage_candidates.push(graph.start_stage_id.clone());
        }
    }

    let current_stage_ids = current_stage_candidates
        .iter()
        .filter(|candidate| {
            !current_stage_candidates.iter().any(|other| {
                *candidate != other && quest_stage_is_ancestor(graph, candidate, other)
            })
        })
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let all_stage_ids = graph
        .stages
        .iter()
        .map(|stage| stage.stage_id.clone())
        .collect::<BTreeSet<_>>();
    let completed_stage_ids = reached_stage_ids
        .iter()
        .filter(|stage_id| !current_stage_ids.iter().any(|current| current == *stage_id))
        .cloned()
        .collect::<Vec<_>>();
    let pending_stage_ids = all_stage_ids
        .iter()
        .filter(|stage_id| !reached_stage_ids.contains(*stage_id))
        .cloned()
        .collect::<Vec<_>>();
    let next_stage_ids = current_stage_ids
        .iter()
        .flat_map(|stage_id| {
            graph
                .stages
                .iter()
                .find(|stage| &stage.stage_id == stage_id)
                .into_iter()
                .flat_map(|stage| stage.next_stage_ids.iter().cloned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let progress_basis_points = if graph.stages.is_empty() {
        0
    } else {
        ((reached_stage_ids.len() as u32 * 10_000) / graph.stages.len() as u32) as u16
    };
    let stage_applications = graph
        .stages
        .iter()
        .filter_map(|stage| {
            stage_applications.get(&stage.stage_id).map(|applications| {
                QuestStageApplicationReport {
                    stage_id: stage.stage_id.clone(),
                    title: stage.title.clone(),
                    applications: *applications,
                }
            })
        })
        .collect::<Vec<_>>();
    let terminal = !current_stage_ids.is_empty()
        && current_stage_ids.iter().all(|stage_id| {
            graph
                .stages
                .iter()
                .find(|stage| &stage.stage_id == stage_id)
                .is_some_and(|stage| stage.next_stage_ids.is_empty())
        });

    (
        QuestLineStateReport {
            quest_graph_id: graph.quest_id.clone(),
            display_name: graph.display_name.clone(),
            current_stage_ids,
            completed_stage_ids,
            pending_stage_ids,
            next_stage_ids: next_stage_ids.clone(),
            progress_basis_points,
            terminal,
            stage_applications,
        },
        unresolved,
    )
}

fn quest_path_from_start(graph: &QuestStateGraph, target_stage_id: &str) -> Option<Vec<String>> {
    fn visit(
        graph: &QuestStateGraph,
        current_stage_id: &str,
        target_stage_id: &str,
        path: &mut Vec<String>,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if !visiting.insert(current_stage_id.to_string()) {
            return false;
        }

        path.push(current_stage_id.to_string());
        if current_stage_id == target_stage_id {
            visiting.remove(current_stage_id);
            return true;
        }

        let found = graph
            .stages
            .iter()
            .find(|stage| stage.stage_id == current_stage_id)
            .is_some_and(|stage| {
                stage.next_stage_ids.iter().any(|next_stage_id| {
                    visit(graph, next_stage_id, target_stage_id, path, visiting)
                })
            });

        if found {
            visiting.remove(current_stage_id);
            true
        } else {
            path.pop();
            visiting.remove(current_stage_id);
            false
        }
    }

    let mut path = Vec::new();
    let mut visiting = BTreeSet::new();
    visit(
        graph,
        &graph.start_stage_id,
        target_stage_id,
        &mut path,
        &mut visiting,
    )
    .then_some(path)
}

fn quest_stage_is_ancestor(graph: &QuestStateGraph, stage_id: &str, other_stage_id: &str) -> bool {
    quest_path_from_start(graph, other_stage_id).is_some_and(|path| {
        path.iter().any(|path_stage_id| path_stage_id == stage_id) && stage_id != other_stage_id
    })
}

fn collect_reward_reason_stats<'a, I>(signals: I) -> Vec<RewardReasonStat>
where
    I: IntoIterator<Item = &'a AgentRewardSignal>,
{
    let mut by_reason = BTreeMap::<String, (usize, f32)>::new();
    for signal in signals {
        let key = reward_reason_key(signal.reason).to_string();
        let entry = by_reason.entry(key).or_insert((0usize, 0.0));
        entry.0 += 1;
        entry.1 += signal.value;
    }

    by_reason
        .into_iter()
        .map(|(reason, (count, total_value))| RewardReasonStat {
            reason,
            count,
            total_value,
        })
        .collect()
}

fn iter_reward_signals(
    result: &FlagshipMmoAcceptanceResult,
) -> impl Iterator<Item = &AgentRewardSignal> {
    result
        .telemetry_windows()
        .iter()
        .flat_map(|window| window.agents.iter())
        .flat_map(|agent| agent.reward_signals.iter())
}

fn reward_signal_tags(signal: &AgentRewardSignal) -> Vec<&str> {
    let mut tags = reward_reason_tags(signal.reason).to_vec();
    if let Some(tag) = signal.tag.as_deref() {
        tags.push(tag);
    }
    tags
}

fn reward_reason_tags(reason: RewardReason) -> &'static [&'static str] {
    match reason {
        RewardReason::ActionExecuted => &["action-executed", "momentum"],
        RewardReason::ActionRejected => &["action-rejected", "friction"],
        RewardReason::ActionQueued => &["action-queued", "tempo"],
        RewardReason::DamageDealt => &["damage-dealt", "combat-pressure"],
        RewardReason::DamageTaken => &["damage-taken", "death-pressure"],
        RewardReason::KillSecured => &["kill-secured", "death-pressure"],
        RewardReason::DeathTaken => &["death-taken", "elimination"],
        RewardReason::SkillExperienceGained => &["skill-xp", "progression"],
        RewardReason::CreatureCaptured => &["creature-captured", "objective-progress"],
        RewardReason::CompanionSummoned => &["companion-summoned", "swarm-pressure"],
        RewardReason::CompanionCommandIssued => &["companion-commanded", "swarm-pressure"],
        RewardReason::ResourceGathered => &["resource-gathered", "economy"],
        RewardReason::LootClaimed => &["loot-claimed", "economy"],
    }
}

fn reward_reason_key(reason: RewardReason) -> &'static str {
    match reason {
        RewardReason::ActionExecuted => "action_executed",
        RewardReason::ActionRejected => "action_rejected",
        RewardReason::ActionQueued => "action_queued",
        RewardReason::DamageDealt => "damage_dealt",
        RewardReason::DamageTaken => "damage_taken",
        RewardReason::KillSecured => "kill_secured",
        RewardReason::DeathTaken => "death_taken",
        RewardReason::SkillExperienceGained => "skill_experience_gained",
        RewardReason::CreatureCaptured => "creature_captured",
        RewardReason::CompanionSummoned => "companion_summoned",
        RewardReason::CompanionCommandIssued => "companion_command_issued",
        RewardReason::ResourceGathered => "resource_gathered",
        RewardReason::LootClaimed => "loot_claimed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::{
        assign_roster_to_world_teams, ActionOutcomeSummary, AgentRole, AgentType,
        RewardAttributionSummary, RewardSource,
    };

    fn reward(reason: RewardReason, tag: Option<&str>, value: f32) -> AgentRewardSignal {
        AgentRewardSignal::new(
            7,
            RewardSource::WorldEvent,
            reason,
            value,
            matches!(reason, RewardReason::DeathTaken),
            tag.map(str::to_string),
        )
    }

    #[test]
    fn deadman_neural_cup_wires_expected_topology() {
        let scenario = build_deadman_neural_cup(FlagshipMmoAcceptanceConfig::ci_smoke());

        assert_eq!(scenario.teams.len(), 2);
        assert_eq!(scenario.worlds.len(), 3);
        assert_eq!(scenario.links.len(), 3);
        assert_eq!(scenario.quest_graphs.len(), 3);
        assert_eq!(scenario.tournament.tournament_id, "deadman-neural-cup");
        assert_eq!(scenario.tournament.world_ids.len(), 3);
        assert_eq!(scenario.tournament.team_ids.len(), 2);
        assert_eq!(scenario.tournament.cross_world_link_ids.len(), 3);
        assert_eq!(
            scenario.world_quest_graph_ids["deadman-prime"],
            vec!["deadman-prime-season".to_string()]
        );
    }

    #[test]
    fn world_seed_mix_is_stable_and_world_specific() {
        let first = mix_world_seed(0x50d0_2026, "deadman-prime", 0);
        let second = mix_world_seed(0x50d0_2026, "deadman-prime", 0);
        let other = mix_world_seed(0x50d0_2026, "deadman-shadow", 1);

        assert_eq!(first, second);
        assert_ne!(first, other);
    }

    #[test]
    fn scaled_and_threshold_propagation_are_bounded() {
        assert_eq!(
            derive_application_count(
                7,
                CrossWorldPropagation::Threshold {
                    required_triggers: 3
                },
                5
            ),
            2
        );
        assert_eq!(
            derive_application_count(12, CrossWorldPropagation::Scaled { basis_points: 2500 }, 9),
            3
        );
        assert_eq!(
            derive_application_count(4, CrossWorldPropagation::Immediate, 2),
            2
        );
    }

    #[test]
    fn reward_reason_tags_cover_cross_world_triggers() {
        let signal = reward(RewardReason::KillSecured, None, 3.0);
        let tags = reward_signal_tags(&signal);

        assert!(tags.contains(&"kill-secured"));
        assert!(tags.contains(&"death-pressure"));
    }

    #[test]
    fn project_effects_accumulate_team_score_and_death_marks() {
        let link = CrossWorldLinkDefinition {
            version: pod_core::RuntimeContractVersion::V1,
            link_id: "prime-to-shadow".into(),
            source_world_id: "deadman-prime".into(),
            target_world_id: "deadman-shadow".into(),
            trigger_tags: vec!["kill-secured".into()],
            propagation: CrossWorldPropagation::Immediate,
            effects: vec![
                CrossWorldEffect::TeamScoreDelta {
                    team_id: "iron-sigil".into(),
                    delta: 5,
                },
                CrossWorldEffect::DeathMark {
                    team_id: "gloam-mesh".into(),
                    duration_ticks: 600,
                },
            ],
            cooldown_ticks: 0,
            max_applications_per_window: 8,
        };

        let projected = project_effects(&link, 3);
        assert_eq!(
            projected[0],
            ProjectedCrossWorldEffect::TeamScoreDelta {
                team_id: "iron-sigil".into(),
                per_application: 5,
                total_delta: 15,
            }
        );
        assert_eq!(
            projected[1],
            ProjectedCrossWorldEffect::DeathMark {
                team_id: "gloam-mesh".into(),
                per_application_duration_ticks: 600,
                total_duration_ticks: 1800,
                applications: 3,
            }
        );
    }

    #[test]
    fn roster_assignment_round_robins_across_active_teams() {
        let mut iron_sigil = AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime");
        iron_sigil.allowed_world_ids = vec!["deadman-prime".into()];
        let mut gloam_mesh = AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow");
        gloam_mesh.allowed_world_ids = vec!["deadman-prime".into(), "deadman-shadow".into()];

        let world = WorldRealityDefinition {
            version: pod_core::RuntimeContractVersion::V1,
            world_id: "deadman-prime".into(),
            display_name: "Deadman Prime".into(),
            ruleset_id: "deadman".into(),
            role: WorldRealityRole::Tournament,
            linked_world_ids: vec![],
            active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
        };
        let teams = vec![iron_sigil, gloam_mesh];
        let roster = vec![
            "agent-a".to_string(),
            "agent-b".to_string(),
            "agent-c".to_string(),
            "agent-d".to_string(),
        ];
        let assignments = assign_roster_to_world_teams(&roster, &world, &teams)
            .iter()
            .map(|assignment| (assignment.agent_id.clone(), assignment.clone()))
            .collect::<BTreeMap<_, _>>();

        assert_eq!(assignments["agent-a"].team_id, "iron-sigil");
        assert_eq!(assignments["agent-b"].team_id, "gloam-mesh");
        assert_eq!(assignments["agent-c"].team_id, "iron-sigil");
        assert_eq!(assignments["agent-d"].team_id, "gloam-mesh");
        assert_eq!(assignments["agent-c"].slot_index, 1);
        assert_eq!(assignments["agent-d"].slot_index, 1);
    }

    #[test]
    fn applied_world_state_aggregates_target_effects() {
        let worlds = vec![
            WorldRealityDefinition {
                version: pod_core::RuntimeContractVersion::V1,
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                ruleset_id: "deadman".into(),
                role: WorldRealityRole::Tournament,
                linked_world_ids: vec!["deadman-shadow".into(), "sanctuary-echo".into()],
                active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
            },
            WorldRealityDefinition {
                version: pod_core::RuntimeContractVersion::V1,
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                ruleset_id: "shadow".into(),
                role: WorldRealityRole::Shadow,
                linked_world_ids: vec!["deadman-prime".into()],
                active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
            },
            WorldRealityDefinition {
                version: pod_core::RuntimeContractVersion::V1,
                world_id: "sanctuary-echo".into(),
                display_name: "Sanctuary Echo".into(),
                ruleset_id: "echo".into(),
                role: WorldRealityRole::Sanctuary,
                linked_world_ids: vec!["deadman-prime".into()],
                active_team_ids: vec!["iron-sigil".into()],
            },
        ];

        let projections = vec![
            CrossWorldProjectionReport {
                link_id: "prime-to-shadow".into(),
                source_world_id: "deadman-prime".into(),
                target_world_id: "deadman-shadow".into(),
                trigger_tags: vec!["kill-secured".into()],
                propagation: CrossWorldPropagation::Immediate,
                trigger_count: 2,
                application_count: 2,
                matched_tags: vec![],
                projected_effects: vec![
                    ProjectedCrossWorldEffect::TeamScoreDelta {
                        team_id: "iron-sigil".into(),
                        per_application: 5,
                        total_delta: 10,
                    },
                    ProjectedCrossWorldEffect::DeathMark {
                        team_id: "gloam-mesh".into(),
                        per_application_duration_ticks: 600,
                        total_duration_ticks: 1200,
                        applications: 2,
                    },
                    ProjectedCrossWorldEffect::ObjectiveStateShift {
                        quest_graph_id: "deadman-shadow-hunt".into(),
                        stage_tag: "marked-by-kills".into(),
                        applications: 2,
                    },
                ],
            },
            CrossWorldProjectionReport {
                link_id: "shadow-to-prime".into(),
                source_world_id: "deadman-shadow".into(),
                target_world_id: "deadman-prime".into(),
                trigger_tags: vec!["loot-claimed".into()],
                propagation: CrossWorldPropagation::Threshold {
                    required_triggers: 2,
                },
                trigger_count: 2,
                application_count: 1,
                matched_tags: vec![],
                projected_effects: vec![
                    ProjectedCrossWorldEffect::TeamScoreDelta {
                        team_id: "gloam-mesh".into(),
                        per_application: 4,
                        total_delta: 4,
                    },
                    ProjectedCrossWorldEffect::ObjectiveStateShift {
                        quest_graph_id: "deadman-prime-season".into(),
                        stage_tag: "wilds-under-siege".into(),
                        applications: 1,
                    },
                ],
            },
            CrossWorldProjectionReport {
                link_id: "prime-to-sanctuary".into(),
                source_world_id: "deadman-prime".into(),
                target_world_id: "sanctuary-echo".into(),
                trigger_tags: vec!["skill-xp".into()],
                propagation: CrossWorldPropagation::Threshold {
                    required_triggers: 3,
                },
                trigger_count: 3,
                application_count: 1,
                matched_tags: vec![],
                projected_effects: vec![
                    ProjectedCrossWorldEffect::FactionReputationDelta {
                        faction_id: "echo-order".into(),
                        per_application: 2,
                        total_delta: 2,
                    },
                    ProjectedCrossWorldEffect::ObjectiveStateShift {
                        quest_graph_id: "sanctuary-echo-uplift".into(),
                        stage_tag: "uplifted".into(),
                        applications: 1,
                    },
                ],
            },
        ];
        let (quest_graphs, world_quest_graph_ids) = build_deadman_quest_graphs();

        let applied = build_applied_world_states(
            &worlds,
            &projections,
            &quest_graphs,
            &world_quest_graph_ids,
        );
        let by_world = applied
            .into_iter()
            .map(|state| (state.world_id.clone(), state))
            .collect::<BTreeMap<_, _>>();

        assert_eq!(
            by_world["deadman-prime"].team_scores[0].team_id,
            "gloam-mesh"
        );
        assert_eq!(by_world["deadman-prime"].team_scores[0].total_delta, 4);
        assert_eq!(
            by_world["deadman-prime"].quest_lines[0].current_stage_ids,
            vec!["wilds-under-siege".to_string()]
        );
        assert_eq!(
            by_world["deadman-prime"].quest_lines[0].completed_stage_ids,
            vec!["enter-bracket".to_string()]
        );
        assert_eq!(
            by_world["deadman-shadow"].team_scores[0].team_id,
            "iron-sigil"
        );
        assert_eq!(by_world["deadman-shadow"].team_scores[0].total_delta, 10);
        assert_eq!(
            by_world["deadman-shadow"].death_marks[0].team_id,
            "gloam-mesh"
        );
        assert_eq!(by_world["deadman-shadow"].death_marks[0].applications, 2);
        assert_eq!(
            by_world["deadman-shadow"].quest_lines[0].current_stage_ids,
            vec!["marked-by-kills".to_string()]
        );
        assert_eq!(
            by_world["sanctuary-echo"].faction_reputation_deltas[0].id,
            "echo-order"
        );
        assert_eq!(
            by_world["sanctuary-echo"].objective_state_shifts[0].applications,
            1
        );
        assert_eq!(
            by_world["sanctuary-echo"].quest_lines[0].current_stage_ids,
            vec!["uplifted".to_string()]
        );
        assert_eq!(
            by_world["sanctuary-echo"].quest_lines[0].progress_basis_points,
            6666
        );
        assert!(by_world["sanctuary-echo"]
            .unresolved_objective_state_shifts
            .is_empty());
    }

    #[test]
    fn quest_line_state_resolves_reward_tag_targets() {
        let graph = QuestStateGraph::new(
            "echo-line",
            "Echo Line",
            "start",
            vec![
                QuestStageDefinition {
                    stage_id: "start".into(),
                    title: "Start".into(),
                    objectives: vec!["Spawn into the echo.".into()],
                    next_stage_ids: vec!["middle".into()],
                    reward_tags: vec!["spawned".into()],
                },
                QuestStageDefinition {
                    stage_id: "middle".into(),
                    title: "Middle".into(),
                    objectives: vec!["Reach the midpoint.".into()],
                    next_stage_ids: vec!["end".into()],
                    reward_tags: vec!["midpoint-reached".into()],
                },
                QuestStageDefinition {
                    stage_id: "end".into(),
                    title: "End".into(),
                    objectives: vec!["Close the line.".into()],
                    next_stage_ids: Vec::new(),
                    reward_tags: vec!["sealed".into()],
                },
            ],
        );

        let (quest_line, unresolved) = build_quest_line_state(
            &graph,
            &[ObjectiveShiftReport {
                quest_graph_id: "echo-line".into(),
                stage_tag: "midpoint-reached".into(),
                applications: 2,
            }],
        );

        assert!(unresolved.is_empty());
        assert_eq!(quest_line.current_stage_ids, vec!["middle".to_string()]);
        assert_eq!(quest_line.completed_stage_ids, vec!["start".to_string()]);
        assert_eq!(quest_line.pending_stage_ids, vec!["end".to_string()]);
        assert_eq!(quest_line.stage_applications[0].stage_id, "middle");
        assert_eq!(quest_line.stage_applications[0].applications, 2);
    }

    #[test]
    fn zero_application_objective_shifts_do_not_advance_quest_lines() {
        let worlds = vec![WorldRealityDefinition {
            version: pod_core::RuntimeContractVersion::V1,
            world_id: "sanctuary-echo".into(),
            display_name: "Sanctuary Echo".into(),
            ruleset_id: "echo".into(),
            role: WorldRealityRole::Sanctuary,
            linked_world_ids: vec!["deadman-prime".into()],
            active_team_ids: vec!["iron-sigil".into()],
        }];
        let projections = vec![CrossWorldProjectionReport {
            link_id: "prime-to-sanctuary".into(),
            source_world_id: "deadman-prime".into(),
            target_world_id: "sanctuary-echo".into(),
            trigger_tags: vec!["skill-xp".into()],
            propagation: CrossWorldPropagation::Threshold {
                required_triggers: 3,
            },
            trigger_count: 2,
            application_count: 0,
            matched_tags: vec![],
            projected_effects: vec![ProjectedCrossWorldEffect::ObjectiveStateShift {
                quest_graph_id: "sanctuary-echo-uplift".into(),
                stage_tag: "uplifted".into(),
                applications: 0,
            }],
        }];
        let (quest_graphs, world_quest_graph_ids) = build_deadman_quest_graphs();

        let applied = build_applied_world_states(
            &worlds,
            &projections,
            &quest_graphs,
            &world_quest_graph_ids,
        );

        assert!(applied[0].objective_state_shifts.is_empty());
        assert_eq!(
            applied[0].quest_lines[0].current_stage_ids,
            vec!["attune-shrine".to_string()]
        );
        assert!(applied[0].quest_lines[0].stage_applications.is_empty());
    }

    #[test]
    fn standings_accumulate_admissions_rewards_and_applied_effects() {
        let mut iron_sigil = AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime");
        iron_sigil.control_mode = TeamControlMode::HybridCommand;

        let mut gloam_mesh = AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow");
        gloam_mesh.control_mode = TeamControlMode::AutonomousSwarm;

        let worlds = vec![
            WorldRealityDefinition {
                version: pod_core::RuntimeContractVersion::V1,
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                ruleset_id: "deadman".into(),
                role: WorldRealityRole::Tournament,
                linked_world_ids: vec!["deadman-shadow".into()],
                active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
            },
            WorldRealityDefinition {
                version: pod_core::RuntimeContractVersion::V1,
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                ruleset_id: "shadow".into(),
                role: WorldRealityRole::Shadow,
                linked_world_ids: vec!["deadman-prime".into()],
                active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
            },
        ];

        let dataset_worlds = vec![WorldDatasetExport {
            world_id: "deadman-prime".into(),
            display_name: "Deadman Prime".into(),
            role: WorldRealityRole::Tournament,
            world_seed: 11,
            summary: RewardDatasetSummary {
                row_count: 2,
                terminal_row_count: 0,
                total: 3.0,
                positive_total: 3.0,
                negative_total: 0.0,
                reasons: vec![],
            },
            rows: vec![
                RewardDatasetRow {
                    world_id: "deadman-prime".into(),
                    world_role: WorldRealityRole::Tournament,
                    world_seed: 11,
                    team_id: Some("iron-sigil".into()),
                    team_slot: Some(0),
                    runtime_profile: AgentRuntimeProfile {
                        role: AgentRole::Player,
                        agent_type: AgentType::NeuralAgent,
                        ..Default::default()
                    },
                    sample: ReplayTrainingSample {
                        tick: 1,
                        agent_id: pod_core::AgentId::default(),
                        path_distance: 0.5,
                        action_outcomes: ActionOutcomeSummary::default(),
                        encounter_transition: None,
                        tool_call_latency_ms: 0,
                        tool_call_error_count: 0,
                        reward_summary: RewardAttributionSummary {
                            signal_count: 1,
                            total: 2.0,
                            positive_total: 2.0,
                            negative_total: 0.0,
                            terminal: false,
                        },
                    },
                    reward_reasons: vec![],
                },
                RewardDatasetRow {
                    world_id: "deadman-prime".into(),
                    world_role: WorldRealityRole::Tournament,
                    world_seed: 11,
                    team_id: Some("gloam-mesh".into()),
                    team_slot: Some(0),
                    runtime_profile: AgentRuntimeProfile {
                        role: AgentRole::Player,
                        agent_type: AgentType::LlmAgent,
                        ..Default::default()
                    },
                    sample: ReplayTrainingSample {
                        tick: 1,
                        agent_id: pod_core::AgentId::default(),
                        path_distance: 0.5,
                        action_outcomes: ActionOutcomeSummary::default(),
                        encounter_transition: None,
                        tool_call_latency_ms: 0,
                        tool_call_error_count: 0,
                        reward_summary: RewardAttributionSummary {
                            signal_count: 1,
                            total: 1.0,
                            positive_total: 1.0,
                            negative_total: 0.0,
                            terminal: false,
                        },
                    },
                    reward_reasons: vec![],
                },
            ],
        }];
        let applied_world_states = vec![
            AppliedWorldStateReport {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
                team_scores: vec![TeamDeltaReport {
                    team_id: "iron-sigil".into(),
                    total_delta: 15,
                }],
                death_marks: vec![TeamDeathMarkReport {
                    team_id: "gloam-mesh".into(),
                    applications: 2,
                    total_duration_ticks: 1200,
                }],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![],
            },
            AppliedWorldStateReport {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                team_scores: vec![TeamDeltaReport {
                    team_id: "gloam-mesh".into(),
                    total_delta: 8,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![],
            },
        ];
        let world_control_planes = vec![
            WorldControlPlaneSummary {
                world_id: "deadman-prime".into(),
                teams: vec![
                    pod_core::WorldTeamControlSummary {
                        team_id: "gloam-mesh".into(),
                        assignments: vec![pod_core::WorldControlAssignmentSummary {
                            agent_id: "agent-b".into(),
                            slot_index: 0,
                            runtime_profile: AgentRuntimeProfile {
                                role: AgentRole::Player,
                                agent_type: AgentType::LlmAgent,
                                ..Default::default()
                            },
                        }],
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "llm_agent".into(),
                            count: 1,
                        }],
                    },
                    pod_core::WorldTeamControlSummary {
                        team_id: "iron-sigil".into(),
                        assignments: vec![pod_core::WorldControlAssignmentSummary {
                            agent_id: "agent-a".into(),
                            slot_index: 0,
                            runtime_profile: AgentRuntimeProfile {
                                role: AgentRole::Player,
                                agent_type: AgentType::NeuralAgent,
                                ..Default::default()
                            },
                        }],
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "neural_agent".into(),
                            count: 1,
                        }],
                    },
                ],
            },
            WorldControlPlaneSummary {
                world_id: "deadman-shadow".into(),
                teams: vec![],
            },
        ];

        let standings = build_team_standings(
            &[iron_sigil, gloam_mesh],
            &worlds,
            &world_control_planes,
            &dataset_worlds,
            &applied_world_states,
        );
        let by_team = standings
            .into_iter()
            .map(|standing| (standing.team_id.clone(), standing))
            .collect::<BTreeMap<_, _>>();

        assert_eq!(by_team["gloam-mesh"].assigned_agent_count, 1);
        assert_eq!(by_team["gloam-mesh"].dataset_row_count, 1);
        assert_eq!(by_team["gloam-mesh"].world_reward_total, 1.0);
        assert_eq!(by_team["gloam-mesh"].applied_score_delta, 8);
        assert_eq!(by_team["gloam-mesh"].active_death_marks, 2);
        assert_eq!(
            by_team["gloam-mesh"].controller_mix,
            vec![AgentTypeCountSummary {
                agent_type: "llm_agent".into(),
                count: 1,
            }]
        );
        assert_eq!(by_team["iron-sigil"].assigned_agent_count, 1);
        assert_eq!(by_team["iron-sigil"].world_reward_total, 2.0);
        assert_eq!(by_team["iron-sigil"].applied_score_delta, 15);
        assert_eq!(by_team["iron-sigil"].active_death_marks, 0);
        assert_eq!(
            by_team["iron-sigil"].controller_mix,
            vec![AgentTypeCountSummary {
                agent_type: "neural_agent".into(),
                count: 1,
            }]
        );
    }

    #[test]
    fn scenario_evaluation_reports_controller_mix_and_quest_progress() {
        let dataset_worlds = vec![
            WorldDatasetExport {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                world_seed: 11,
                summary: RewardDatasetSummary {
                    row_count: 2,
                    terminal_row_count: 0,
                    total: 3.0,
                    positive_total: 3.0,
                    negative_total: 0.0,
                    reasons: vec![],
                },
                rows: vec![
                    RewardDatasetRow {
                        world_id: "deadman-prime".into(),
                        world_role: WorldRealityRole::Tournament,
                        world_seed: 11,
                        team_id: Some("iron-sigil".into()),
                        team_slot: Some(0),
                        runtime_profile: AgentRuntimeProfile {
                            role: AgentRole::Player,
                            agent_type: AgentType::NeuralAgent,
                            ..Default::default()
                        },
                        sample: ReplayTrainingSample {
                            tick: 1,
                            agent_id: pod_core::AgentId::default(),
                            path_distance: 0.5,
                            action_outcomes: ActionOutcomeSummary::default(),
                            encounter_transition: None,
                            tool_call_latency_ms: 0,
                            tool_call_error_count: 0,
                            reward_summary: RewardAttributionSummary {
                                signal_count: 1,
                                total: 2.0,
                                positive_total: 2.0,
                                negative_total: 0.0,
                                terminal: false,
                            },
                        },
                        reward_reasons: vec![],
                    },
                    RewardDatasetRow {
                        world_id: "deadman-prime".into(),
                        world_role: WorldRealityRole::Tournament,
                        world_seed: 11,
                        team_id: Some("gloam-mesh".into()),
                        team_slot: Some(0),
                        runtime_profile: AgentRuntimeProfile {
                            role: AgentRole::Player,
                            agent_type: AgentType::LlmAgent,
                            ..Default::default()
                        },
                        sample: ReplayTrainingSample {
                            tick: 1,
                            agent_id: pod_core::AgentId::default(),
                            path_distance: 0.5,
                            action_outcomes: ActionOutcomeSummary::default(),
                            encounter_transition: None,
                            tool_call_latency_ms: 0,
                            tool_call_error_count: 0,
                            reward_summary: RewardAttributionSummary {
                                signal_count: 1,
                                total: 1.0,
                                positive_total: 1.0,
                                negative_total: 0.0,
                                terminal: false,
                            },
                        },
                        reward_reasons: vec![],
                    },
                ],
            },
            WorldDatasetExport {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
                world_seed: 22,
                summary: RewardDatasetSummary {
                    row_count: 1,
                    terminal_row_count: 0,
                    total: 4.5,
                    positive_total: 4.5,
                    negative_total: 0.0,
                    reasons: vec![],
                },
                rows: vec![RewardDatasetRow {
                    world_id: "deadman-shadow".into(),
                    world_role: WorldRealityRole::Shadow,
                    world_seed: 22,
                    team_id: Some("gloam-mesh".into()),
                    team_slot: Some(1),
                    runtime_profile: AgentRuntimeProfile {
                        role: AgentRole::Player,
                        agent_type: AgentType::NeuralAgent,
                        ..Default::default()
                    },
                    sample: ReplayTrainingSample {
                        tick: 2,
                        agent_id: pod_core::AgentId::default(),
                        path_distance: 1.0,
                        action_outcomes: ActionOutcomeSummary::default(),
                        encounter_transition: None,
                        tool_call_latency_ms: 0,
                        tool_call_error_count: 0,
                        reward_summary: RewardAttributionSummary {
                            signal_count: 1,
                            total: 4.5,
                            positive_total: 4.5,
                            negative_total: 0.0,
                            terminal: false,
                        },
                    },
                    reward_reasons: vec![],
                }],
            },
        ];
        let applied_world_states = vec![
            AppliedWorldStateReport {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                team_scores: vec![TeamDeltaReport {
                    team_id: "gloam-mesh".into(),
                    total_delta: 8,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![QuestLineStateReport {
                    quest_graph_id: "deadman-prime-season".into(),
                    display_name: "Deadman Prime: Blood Season".into(),
                    current_stage_ids: vec!["wilds-under-siege".into()],
                    completed_stage_ids: vec!["enter-bracket".into()],
                    pending_stage_ids: vec!["blood-round".into(), "crown-push".into()],
                    next_stage_ids: vec!["blood-round".into()],
                    progress_basis_points: 5000,
                    terminal: false,
                    stage_applications: vec![QuestStageApplicationReport {
                        stage_id: "wilds-under-siege".into(),
                        title: "Wilds Under Siege".into(),
                        applications: 1,
                    }],
                }],
            },
            AppliedWorldStateReport {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
                team_scores: vec![TeamDeltaReport {
                    team_id: "iron-sigil".into(),
                    total_delta: 5,
                }],
                death_marks: vec![TeamDeathMarkReport {
                    team_id: "gloam-mesh".into(),
                    applications: 1,
                    total_duration_ticks: 600,
                }],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![ObjectiveShiftReport {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    stage_tag: "marked-by-kills".into(),
                    applications: 1,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![QuestLineStateReport {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    display_name: "Deadman Shadow: Mirror Hunt".into(),
                    current_stage_ids: vec!["marked-by-kills".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["rift-collapse".into()],
                    next_stage_ids: vec!["rift-collapse".into()],
                    progress_basis_points: 6666,
                    terminal: false,
                    stage_applications: vec![QuestStageApplicationReport {
                        stage_id: "marked-by-kills".into(),
                        title: "Marked by Kills".into(),
                        applications: 1,
                    }],
                }],
            },
        ];

        let evaluation = build_scenario_evaluation(&dataset_worlds, &applied_world_states);

        assert_eq!(evaluation.controller_mix.len(), 2);
        assert_eq!(evaluation.controller_mix[0].agent_type, "llm_agent");
        assert_eq!(evaluation.controller_mix[0].row_count, 1);
        assert_eq!(evaluation.controller_mix[1].agent_type, "neural_agent");
        assert_eq!(evaluation.controller_mix[1].row_count, 2);
        assert_eq!(evaluation.worlds[0].world_id, "deadman-prime");
        assert_eq!(evaluation.worlds[0].average_reward_per_row, 1.5);
        assert_eq!(evaluation.worlds[0].progressed_quest_line_count, 1);
        assert_eq!(
            evaluation.worlds[0].average_quest_progress_basis_points,
            5000
        );
        assert_eq!(evaluation.worlds[0].applied_score_delta_total, 8);
        assert_eq!(evaluation.worlds[1].world_id, "deadman-shadow");
        assert_eq!(evaluation.worlds[1].applied_death_mark_count, 1);
        assert_eq!(evaluation.worlds[1].applied_objective_shift_count, 1);
    }

    #[test]
    fn parse_args_accepts_topology_output() {
        let options = parse_args_from([
            "--profile",
            "shard-target",
            "--scenario",
            "deadman-neural-cup",
            "--topology-output",
            "/tmp/topology.json",
        ])
        .expect("args should parse");

        assert_eq!(options.profile, "shard-target");
        assert_eq!(options.scenario, "deadman-neural-cup");
        assert_eq!(
            options.topology_output,
            Some(PathBuf::from("/tmp/topology.json"))
        );
    }

    #[test]
    fn remote_topology_bundle_preserves_world_quest_bindings() {
        let mut tournament =
            WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup");
        tournament.world_ids = vec!["deadman-prime".into()];
        tournament.team_ids = vec!["iron-sigil".into()];

        let mut world =
            WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
        world.role = WorldRealityRole::Tournament;
        world.active_team_ids = vec!["iron-sigil".into()];

        let bundle = build_remote_topology_bundle(
            "deadman-neural-cup",
            "ci-smoke",
            42,
            &tournament,
            &[AgentTeamDefinition::new(
                "iron-sigil",
                "Iron Sigil",
                "deadman-prime",
            )],
            &[world],
            &[],
            &[QuestStateGraph::new(
                "deadman-prime-season",
                "Deadman Prime: Blood Season",
                "enter-bracket",
                vec![QuestStageDefinition {
                    stage_id: "enter-bracket".into(),
                    title: "Enter the Bracket".into(),
                    objectives: vec!["Establish camp.".into()],
                    next_stage_ids: vec!["wilds-under-siege".into()],
                    reward_tags: vec!["season-open".into()],
                }],
            )],
            &BTreeMap::from([("deadman-prime".into(), vec!["deadman-prime-season".into()])]),
            &[WorldAdmissionSummary {
                world_id: "deadman-prime".into(),
                assignments: vec![pod_core::WorldAdmissionAssignment {
                    agent_id: "agent-a".into(),
                    team_id: "iron-sigil".into(),
                    slot_index: 0,
                }],
            }],
            &[WorldControlPlaneSummary {
                world_id: "deadman-prime".into(),
                teams: vec![pod_core::WorldTeamControlSummary {
                    team_id: "iron-sigil".into(),
                    assignments: vec![pod_core::WorldControlAssignmentSummary {
                        agent_id: "agent-a".into(),
                        slot_index: 0,
                        runtime_profile: AgentRuntimeProfile {
                            role: AgentRole::Player,
                            agent_type: AgentType::NeuralAgent,
                            ..Default::default()
                        },
                    }],
                    controller_mix: vec![AgentTypeCountSummary {
                        agent_type: "neural_agent".into(),
                        count: 1,
                    }],
                }],
            }],
            &[AppliedWorldStateReport {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                team_scores: vec![TeamDeltaReport {
                    team_id: "iron-sigil".into(),
                    total_delta: 8,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![QuestLineStateReport {
                    quest_graph_id: "deadman-prime-season".into(),
                    display_name: "Deadman Prime: Blood Season".into(),
                    current_stage_ids: vec!["wilds-under-siege".into()],
                    completed_stage_ids: vec!["enter-bracket".into()],
                    pending_stage_ids: vec![],
                    next_stage_ids: vec![],
                    progress_basis_points: 5_000,
                    terminal: false,
                    stage_applications: vec![QuestStageApplicationReport {
                        stage_id: "wilds-under-siege".into(),
                        title: "Wilds Under Siege".into(),
                        applications: 1,
                    }],
                }],
            }],
            &ScenarioEvaluationReport {
                controller_mix: vec![ControllerEvaluationReport {
                    agent_type: "neural_agent".into(),
                    row_count: 1,
                    reward_total: 4.5,
                    average_reward_per_row: 4.5,
                }],
                worlds: vec![WorldEvaluationReport {
                    world_id: "deadman-prime".into(),
                    display_name: "Deadman Prime".into(),
                    role: WorldRealityRole::Tournament,
                    average_reward_per_row: 4.5,
                    controller_mix: vec![ControllerEvaluationReport {
                        agent_type: "neural_agent".into(),
                        row_count: 1,
                        reward_total: 4.5,
                        average_reward_per_row: 4.5,
                    }],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 5_000,
                    applied_score_delta_total: 8,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    applied_objective_shift_count: 0,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        );

        assert_eq!(bundle.scenario_id, "deadman-neural-cup");
        assert_eq!(bundle.profile_id, "ci-smoke");
        assert_eq!(bundle.world_quest_bindings.len(), 1);
        assert_eq!(bundle.world_quest_bindings[0].world_id, "deadman-prime");
        assert_eq!(
            bundle.world_admissions[0].assignments[0].team_id,
            "iron-sigil"
        );
        assert_eq!(
            bundle.world_control_planes[0].teams[0].controller_mix,
            vec![AgentTypeCountSummary {
                agent_type: "neural_agent".into(),
                count: 1,
            }]
        );
        assert_eq!(
            bundle.world_quest_bindings[0].quest_graph_ids,
            vec!["deadman-prime-season".to_string()]
        );
        assert_eq!(
            bundle.applied_world_states[0].quest_lines[0].current_stage_ids,
            vec!["wilds-under-siege".to_string()]
        );
        assert_eq!(bundle.evaluation.worlds[0].applied_score_delta_total, 8);
    }

    #[test]
    fn remote_topology_bundle_preserves_linked_world_evaluation_and_neural_swarm_progress() {
        let mut tournament =
            WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup");
        tournament.world_ids = vec!["deadman-prime".into(), "deadman-shadow".into()];
        tournament.team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
        tournament.cross_world_link_ids = vec!["prime-to-shadow".into()];

        let mut prime =
            WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
        prime.role = WorldRealityRole::Tournament;
        prime.linked_world_ids = vec!["deadman-shadow".into()];
        prime.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];

        let mut shadow =
            WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow-seasonal");
        shadow.role = WorldRealityRole::Shadow;
        shadow.linked_world_ids = vec!["deadman-prime".into()];
        shadow.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];

        let bundle = build_remote_topology_bundle(
            "deadman-neural-cup",
            "ci-smoke",
            42,
            &tournament,
            &[
                AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime"),
                AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow"),
            ],
            &[prime, shadow],
            &[CrossWorldLinkDefinition::new(
                "prime-to-shadow",
                "deadman-prime",
                "deadman-shadow",
            )],
            &[
                QuestStateGraph::new(
                    "deadman-prime-season",
                    "Deadman Prime: Blood Season",
                    "enter-bracket",
                    vec![QuestStageDefinition {
                        stage_id: "enter-bracket".into(),
                        title: "Enter the Bracket".into(),
                        objectives: vec!["Establish camp.".into()],
                        next_stage_ids: vec!["wilds-under-siege".into()],
                        reward_tags: vec!["season-open".into()],
                    }],
                ),
                QuestStateGraph::new(
                    "deadman-shadow-hunt",
                    "Deadman Shadow: Mirror Hunt",
                    "shadow-observe",
                    vec![QuestStageDefinition {
                        stage_id: "shadow-observe".into(),
                        title: "Shadow Observe".into(),
                        objectives: vec!["Scout the breach.".into()],
                        next_stage_ids: vec!["marked-by-kills".into()],
                        reward_tags: vec!["shadow-start".into()],
                    }],
                ),
            ],
            &BTreeMap::from([
                ("deadman-prime".into(), vec!["deadman-prime-season".into()]),
                ("deadman-shadow".into(), vec!["deadman-shadow-hunt".into()]),
            ]),
            &[
                WorldAdmissionSummary {
                    world_id: "deadman-prime".into(),
                    assignments: vec![pod_core::WorldAdmissionAssignment {
                        agent_id: "agent-a".into(),
                        team_id: "iron-sigil".into(),
                        slot_index: 0,
                    }],
                },
                WorldAdmissionSummary {
                    world_id: "deadman-shadow".into(),
                    assignments: vec![pod_core::WorldAdmissionAssignment {
                        agent_id: "agent-b".into(),
                        team_id: "gloam-mesh".into(),
                        slot_index: 0,
                    }],
                },
            ],
            &[
                WorldControlPlaneSummary {
                    world_id: "deadman-prime".into(),
                    teams: vec![pod_core::WorldTeamControlSummary {
                        team_id: "iron-sigil".into(),
                        assignments: vec![pod_core::WorldControlAssignmentSummary {
                            agent_id: "agent-a".into(),
                            slot_index: 0,
                            runtime_profile: AgentRuntimeProfile {
                                role: AgentRole::Player,
                                agent_type: AgentType::LlmAgent,
                                ..Default::default()
                            },
                        }],
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "llm_agent".into(),
                            count: 1,
                        }],
                    }],
                },
                WorldControlPlaneSummary {
                    world_id: "deadman-shadow".into(),
                    teams: vec![pod_core::WorldTeamControlSummary {
                        team_id: "gloam-mesh".into(),
                        assignments: vec![pod_core::WorldControlAssignmentSummary {
                            agent_id: "agent-b".into(),
                            slot_index: 0,
                            runtime_profile: AgentRuntimeProfile {
                                role: AgentRole::Player,
                                agent_type: AgentType::NeuralAgent,
                                ..Default::default()
                            },
                        }],
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "neural_agent".into(),
                            count: 1,
                        }],
                    }],
                },
            ],
            &[
                AppliedWorldStateReport {
                    world_id: "deadman-prime".into(),
                    display_name: "Deadman Prime".into(),
                    role: WorldRealityRole::Tournament,
                    team_scores: vec![TeamDeltaReport {
                        team_id: "gloam-mesh".into(),
                        total_delta: 8,
                    }],
                    death_marks: vec![],
                    faction_reputation_deltas: vec![],
                    encounter_weight_deltas: vec![],
                    resource_scarcity_deltas: vec![],
                    objective_state_shifts: vec![],
                    unresolved_objective_state_shifts: vec![],
                    quest_lines: vec![QuestLineStateReport {
                        quest_graph_id: "deadman-prime-season".into(),
                        display_name: "Deadman Prime: Blood Season".into(),
                        current_stage_ids: vec!["wilds-under-siege".into()],
                        completed_stage_ids: vec!["enter-bracket".into()],
                        pending_stage_ids: vec![],
                        next_stage_ids: vec![],
                        progress_basis_points: 5_000,
                        terminal: false,
                        stage_applications: vec![QuestStageApplicationReport {
                            stage_id: "wilds-under-siege".into(),
                            title: "Wilds Under Siege".into(),
                            applications: 1,
                        }],
                    }],
                },
                AppliedWorldStateReport {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: WorldRealityRole::Shadow,
                    team_scores: vec![TeamDeltaReport {
                        team_id: "iron-sigil".into(),
                        total_delta: 10,
                    }],
                    death_marks: vec![TeamDeathMarkReport {
                        team_id: "gloam-mesh".into(),
                        applications: 2,
                        total_duration_ticks: 1200,
                    }],
                    faction_reputation_deltas: vec![],
                    encounter_weight_deltas: vec![],
                    resource_scarcity_deltas: vec![],
                    objective_state_shifts: vec![ObjectiveShiftReport {
                        quest_graph_id: "deadman-shadow-hunt".into(),
                        stage_tag: "marked-by-kills".into(),
                        applications: 2,
                    }],
                    unresolved_objective_state_shifts: vec![],
                    quest_lines: vec![QuestLineStateReport {
                        quest_graph_id: "deadman-shadow-hunt".into(),
                        display_name: "Deadman Shadow: Mirror Hunt".into(),
                        current_stage_ids: vec!["marked-by-kills".into()],
                        completed_stage_ids: vec!["shadow-observe".into()],
                        pending_stage_ids: vec!["rift-collapse".into()],
                        next_stage_ids: vec!["rift-collapse".into()],
                        progress_basis_points: 6666,
                        terminal: false,
                        stage_applications: vec![QuestStageApplicationReport {
                            stage_id: "marked-by-kills".into(),
                            title: "Marked by Kills".into(),
                            applications: 2,
                        }],
                    }],
                },
            ],
            &ScenarioEvaluationReport {
                controller_mix: vec![
                    ControllerEvaluationReport {
                        agent_type: "llm_agent".into(),
                        row_count: 1,
                        reward_total: 1.0,
                        average_reward_per_row: 1.0,
                    },
                    ControllerEvaluationReport {
                        agent_type: "neural_agent".into(),
                        row_count: 3,
                        reward_total: 13.5,
                        average_reward_per_row: 4.5,
                    },
                ],
                worlds: vec![
                    WorldEvaluationReport {
                        world_id: "deadman-prime".into(),
                        display_name: "Deadman Prime".into(),
                        role: WorldRealityRole::Tournament,
                        average_reward_per_row: 1.0,
                        controller_mix: vec![ControllerEvaluationReport {
                            agent_type: "llm_agent".into(),
                            row_count: 1,
                            reward_total: 1.0,
                            average_reward_per_row: 1.0,
                        }],
                        quest_line_count: 1,
                        progressed_quest_line_count: 1,
                        average_quest_progress_basis_points: 5_000,
                        applied_score_delta_total: 8,
                        applied_death_mark_count: 0,
                        applied_death_mark_ticks: 0,
                        applied_objective_shift_count: 0,
                        applied_reputation_delta_total: 0,
                        applied_encounter_delta_total: 0,
                        applied_resource_delta_total: 0,
                    },
                    WorldEvaluationReport {
                        world_id: "deadman-shadow".into(),
                        display_name: "Deadman Shadow".into(),
                        role: WorldRealityRole::Shadow,
                        average_reward_per_row: 4.5,
                        controller_mix: vec![ControllerEvaluationReport {
                            agent_type: "neural_agent".into(),
                            row_count: 3,
                            reward_total: 13.5,
                            average_reward_per_row: 4.5,
                        }],
                        quest_line_count: 1,
                        progressed_quest_line_count: 1,
                        average_quest_progress_basis_points: 6666,
                        applied_score_delta_total: 10,
                        applied_death_mark_count: 2,
                        applied_death_mark_ticks: 1200,
                        applied_objective_shift_count: 2,
                        applied_reputation_delta_total: 0,
                        applied_encounter_delta_total: 0,
                        applied_resource_delta_total: 0,
                    },
                ],
            },
        );

        assert_eq!(bundle.links.len(), 1);
        assert_eq!(bundle.world_quest_bindings.len(), 2);
        let shadow_state = bundle
            .applied_world_states
            .iter()
            .find(|state| state.world_id == "deadman-shadow")
            .expect("shadow world state present");
        assert_eq!(
            shadow_state.quest_lines[0].current_stage_ids,
            vec!["marked-by-kills"]
        );
        assert_eq!(shadow_state.death_marks[0].applications, 2);

        let shadow_eval = bundle
            .evaluation
            .worlds
            .iter()
            .find(|world| world.world_id == "deadman-shadow")
            .expect("shadow evaluation present");
        assert_eq!(shadow_eval.controller_mix[0].agent_type, "neural_agent");
        assert_eq!(shadow_eval.controller_mix[0].row_count, 3);
        assert_eq!(shadow_eval.applied_objective_shift_count, 2);
        assert_eq!(shadow_eval.average_quest_progress_basis_points, 6666);
    }

    #[test]
    fn topology_parity_report_confirms_matching_bundle_components() {
        let mut tournament =
            WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup");
        tournament.world_ids = vec!["deadman-prime".into()];
        tournament.team_ids = vec!["iron-sigil".into()];

        let mut world =
            WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
        world.role = WorldRealityRole::Tournament;
        world.active_team_ids = vec!["iron-sigil".into()];

        let teams = vec![AgentTeamDefinition::new(
            "iron-sigil",
            "Iron Sigil",
            "deadman-prime",
        )];
        let links = vec![];
        let quest_graphs = vec![QuestStateGraph::new(
            "deadman-prime-season",
            "Deadman Prime: Blood Season",
            "enter-bracket",
            vec![QuestStageDefinition {
                stage_id: "enter-bracket".into(),
                title: "Enter the Bracket".into(),
                objectives: vec!["Establish camp.".into()],
                next_stage_ids: vec!["wilds-under-siege".into()],
                reward_tags: vec!["season-open".into()],
            }],
        )];
        let world_quest_graph_ids =
            BTreeMap::from([("deadman-prime".into(), vec!["deadman-prime-season".into()])]);
        let applied_world_states = vec![AppliedWorldStateReport {
            world_id: "deadman-prime".into(),
            display_name: "Deadman Prime".into(),
            role: WorldRealityRole::Tournament,
            team_scores: vec![TeamDeltaReport {
                team_id: "iron-sigil".into(),
                total_delta: 8,
            }],
            death_marks: vec![],
            faction_reputation_deltas: vec![],
            encounter_weight_deltas: vec![],
            resource_scarcity_deltas: vec![],
            objective_state_shifts: vec![],
            unresolved_objective_state_shifts: vec![],
            quest_lines: vec![QuestLineStateReport {
                quest_graph_id: "deadman-prime-season".into(),
                display_name: "Deadman Prime: Blood Season".into(),
                current_stage_ids: vec!["wilds-under-siege".into()],
                completed_stage_ids: vec!["enter-bracket".into()],
                pending_stage_ids: vec![],
                next_stage_ids: vec![],
                progress_basis_points: 5_000,
                terminal: false,
                stage_applications: vec![QuestStageApplicationReport {
                    stage_id: "wilds-under-siege".into(),
                    title: "Wilds Under Siege".into(),
                    applications: 1,
                }],
            }],
        }];
        let world_admissions = vec![WorldAdmissionSummary {
            world_id: "deadman-prime".into(),
            assignments: vec![pod_core::WorldAdmissionAssignment {
                agent_id: "agent-a".into(),
                team_id: "iron-sigil".into(),
                slot_index: 0,
            }],
        }];
        let world_control_planes = vec![WorldControlPlaneSummary {
            world_id: "deadman-prime".into(),
            teams: vec![pod_core::WorldTeamControlSummary {
                team_id: "iron-sigil".into(),
                assignments: vec![pod_core::WorldControlAssignmentSummary {
                    agent_id: "agent-a".into(),
                    slot_index: 0,
                    runtime_profile: AgentRuntimeProfile {
                        role: AgentRole::Player,
                        agent_type: AgentType::NeuralAgent,
                        ..Default::default()
                    },
                }],
                controller_mix: vec![AgentTypeCountSummary {
                    agent_type: "neural_agent".into(),
                    count: 1,
                }],
            }],
        }];
        let evaluation = ScenarioEvaluationReport {
            controller_mix: vec![ControllerEvaluationReport {
                agent_type: "neural_agent".into(),
                row_count: 1,
                reward_total: 4.5,
                average_reward_per_row: 4.5,
            }],
            worlds: vec![WorldEvaluationReport {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                average_reward_per_row: 4.5,
                controller_mix: vec![ControllerEvaluationReport {
                    agent_type: "neural_agent".into(),
                    row_count: 1,
                    reward_total: 4.5,
                    average_reward_per_row: 4.5,
                }],
                quest_line_count: 1,
                progressed_quest_line_count: 1,
                average_quest_progress_basis_points: 5_000,
                applied_score_delta_total: 8,
                applied_death_mark_count: 0,
                applied_death_mark_ticks: 0,
                applied_objective_shift_count: 0,
                applied_reputation_delta_total: 0,
                applied_encounter_delta_total: 0,
                applied_resource_delta_total: 0,
            }],
        };
        let topology = build_remote_topology_bundle(
            "deadman-neural-cup",
            "ci-smoke",
            42,
            &tournament,
            &teams,
            &[world.clone()],
            &links,
            &quest_graphs,
            &world_quest_graph_ids,
            &world_admissions,
            &world_control_planes,
            &applied_world_states,
            &evaluation,
        );

        let parity = build_remote_topology_parity_summary(
            &teams,
            &[world],
            &links,
            &quest_graphs,
            &build_world_quest_bindings(&world_quest_graph_ids),
            &world_admissions,
            &world_control_planes,
            &applied_world_states,
            &evaluation,
            &topology,
        );

        assert!(parity.consistent);
        assert!(parity.world_quest_bindings_match);
        assert!(parity.world_control_planes_match);
        assert!(parity.applied_world_states_match);
        assert!(parity.evaluation_match);
        assert!(parity.missing_world_quest_binding_ids.is_empty());
        assert!(parity.missing_world_control_plane_ids.is_empty());
        assert!(parity.missing_applied_world_ids.is_empty());
        assert!(parity.missing_evaluation_world_ids.is_empty());
    }

    #[test]
    fn topology_parity_report_flags_missing_evaluation_and_binding_ids() {
        let teams = vec![AgentTeamDefinition::new(
            "iron-sigil",
            "Iron Sigil",
            "deadman-prime",
        )];
        let worlds = vec![{
            let mut world =
                WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
            world.role = WorldRealityRole::Tournament;
            world.active_team_ids = vec!["iron-sigil".into()];
            world
        }];
        let quest_graphs = vec![QuestStateGraph::new(
            "deadman-prime-season",
            "Deadman Prime: Blood Season",
            "enter-bracket",
            vec![QuestStageDefinition {
                stage_id: "enter-bracket".into(),
                title: "Enter the Bracket".into(),
                objectives: vec!["Establish camp.".into()],
                next_stage_ids: vec!["wilds-under-siege".into()],
                reward_tags: vec!["season-open".into()],
            }],
        )];
        let world_quest_graph_ids =
            BTreeMap::from([("deadman-prime".into(), vec!["deadman-prime-season".into()])]);
        let applied_world_states = vec![AppliedWorldStateReport {
            world_id: "deadman-prime".into(),
            display_name: "Deadman Prime".into(),
            role: WorldRealityRole::Tournament,
            team_scores: vec![],
            death_marks: vec![],
            faction_reputation_deltas: vec![],
            encounter_weight_deltas: vec![],
            resource_scarcity_deltas: vec![],
            objective_state_shifts: vec![],
            unresolved_objective_state_shifts: vec![],
            quest_lines: vec![],
        }];
        let world_admissions = vec![WorldAdmissionSummary {
            world_id: "deadman-prime".into(),
            assignments: vec![pod_core::WorldAdmissionAssignment {
                agent_id: "agent-a".into(),
                team_id: "iron-sigil".into(),
                slot_index: 0,
            }],
        }];
        let world_control_planes = vec![WorldControlPlaneSummary {
            world_id: "deadman-prime".into(),
            teams: vec![pod_core::WorldTeamControlSummary {
                team_id: "iron-sigil".into(),
                assignments: vec![pod_core::WorldControlAssignmentSummary {
                    agent_id: "agent-a".into(),
                    slot_index: 0,
                    runtime_profile: AgentRuntimeProfile {
                        role: AgentRole::Player,
                        agent_type: AgentType::NeuralAgent,
                        ..Default::default()
                    },
                }],
                controller_mix: vec![AgentTypeCountSummary {
                    agent_type: "neural_agent".into(),
                    count: 1,
                }],
            }],
        }];
        let evaluation = ScenarioEvaluationReport {
            controller_mix: vec![],
            worlds: vec![WorldEvaluationReport {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                average_reward_per_row: 0.0,
                controller_mix: vec![],
                quest_line_count: 0,
                progressed_quest_line_count: 0,
                average_quest_progress_basis_points: 0,
                applied_score_delta_total: 0,
                applied_death_mark_count: 0,
                applied_death_mark_ticks: 0,
                applied_objective_shift_count: 0,
                applied_reputation_delta_total: 0,
                applied_encounter_delta_total: 0,
                applied_resource_delta_total: 0,
            }],
        };
        let mut topology = build_remote_topology_bundle(
            "deadman-neural-cup",
            "ci-smoke",
            42,
            &WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            &teams,
            &worlds,
            &[],
            &quest_graphs,
            &world_quest_graph_ids,
            &world_admissions,
            &world_control_planes,
            &applied_world_states,
            &evaluation,
        );
        topology.world_quest_bindings.clear();
        topology.world_admissions.clear();
        topology.world_control_planes.clear();
        topology.evaluation.worlds.clear();

        let parity = build_remote_topology_parity_summary(
            &teams,
            &worlds,
            &[],
            &quest_graphs,
            &build_world_quest_bindings(&world_quest_graph_ids),
            &world_admissions,
            &world_control_planes,
            &applied_world_states,
            &evaluation,
            &topology,
        );

        assert!(!parity.consistent);
        assert!(!parity.world_quest_bindings_match);
        assert!(!parity.world_admissions_match);
        assert!(!parity.world_control_planes_match);
        assert!(!parity.evaluation_match);
        assert_eq!(
            parity.missing_world_quest_binding_ids,
            vec!["deadman-prime".to_string()]
        );
        assert_eq!(
            parity.missing_world_admission_ids,
            vec!["deadman-prime".to_string()]
        );
        assert_eq!(
            parity.missing_world_control_plane_ids,
            vec!["deadman-prime".to_string()]
        );
        assert_eq!(
            parity.missing_evaluation_world_ids,
            vec!["deadman-prime".to_string()]
        );
    }

    #[test]
    fn collect_reward_reason_stats_aggregates_counts_and_totals() {
        let signals = vec![
            reward(RewardReason::DamageDealt, None, 1.5),
            reward(RewardReason::DamageDealt, None, 2.0),
            reward(RewardReason::LootClaimed, None, 0.75),
        ];

        let stats = collect_reward_reason_stats(signals.iter());

        assert_eq!(
            stats,
            vec![
                RewardReasonStat {
                    reason: "damage_dealt".into(),
                    count: 2,
                    total_value: 3.5,
                },
                RewardReasonStat {
                    reason: "loot_claimed".into(),
                    count: 1,
                    total_value: 0.75,
                },
            ]
        );
    }

    #[test]
    fn dataset_summary_aggregates_rows_across_worlds() {
        let rows = vec![
            RewardDatasetRow {
                world_id: "deadman-prime".into(),
                world_role: WorldRealityRole::Tournament,
                world_seed: 11,
                team_id: Some("iron-sigil".into()),
                team_slot: Some(0),
                runtime_profile: AgentRuntimeProfile {
                    role: AgentRole::Player,
                    agent_type: AgentType::NeuralAgent,
                    ..Default::default()
                },
                sample: ReplayTrainingSample {
                    tick: 3,
                    agent_id: pod_core::AgentId::default(),
                    path_distance: 1.25,
                    action_outcomes: ActionOutcomeSummary {
                        submitted: 1,
                        executed: 1,
                        rejected: 0,
                        queued: 0,
                    },
                    encounter_transition: None,
                    tool_call_latency_ms: 0,
                    tool_call_error_count: 0,
                    reward_summary: RewardAttributionSummary {
                        signal_count: 2,
                        total: 2.25,
                        positive_total: 2.25,
                        negative_total: 0.0,
                        terminal: false,
                    },
                },
                reward_reasons: vec![RewardReasonStat {
                    reason: "damage_dealt".into(),
                    count: 2,
                    total_value: 2.25,
                }],
            },
            RewardDatasetRow {
                world_id: "deadman-shadow".into(),
                world_role: WorldRealityRole::Shadow,
                world_seed: 22,
                team_id: Some("gloam-mesh".into()),
                team_slot: Some(0),
                runtime_profile: AgentRuntimeProfile {
                    role: AgentRole::Player,
                    agent_type: AgentType::LlmAgent,
                    ..Default::default()
                },
                sample: ReplayTrainingSample {
                    tick: 4,
                    agent_id: pod_core::AgentId::default(),
                    path_distance: 0.5,
                    action_outcomes: ActionOutcomeSummary {
                        submitted: 1,
                        executed: 0,
                        rejected: 1,
                        queued: 0,
                    },
                    encounter_transition: None,
                    tool_call_latency_ms: 30,
                    tool_call_error_count: 1,
                    reward_summary: RewardAttributionSummary {
                        signal_count: 2,
                        total: -3.25,
                        positive_total: 0.0,
                        negative_total: -3.25,
                        terminal: true,
                    },
                },
                reward_reasons: vec![RewardReasonStat {
                    reason: "death_taken".into(),
                    count: 1,
                    total_value: -3.25,
                }],
            },
        ];

        let summary = summarize_dataset_rows(&rows);

        assert_eq!(summary.row_count, 2);
        assert_eq!(summary.terminal_row_count, 1);
        assert!((summary.total + 1.0).abs() < f32::EPSILON);
        assert!((summary.positive_total - 2.25).abs() < f32::EPSILON);
        assert!((summary.negative_total + 3.25).abs() < f32::EPSILON);
        assert_eq!(summary.reasons.len(), 2);
        assert_eq!(summary.reasons[0].reason, "damage_dealt");
        assert_eq!(summary.reasons[1].reason, "death_taken");
    }
}
