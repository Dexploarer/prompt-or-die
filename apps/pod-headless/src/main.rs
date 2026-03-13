use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pod_core::{
    run_flagship_mmo_acceptance, AgentRewardSignal, AgentRuntimeProfile, AgentTeamDefinition,
    CrossWorldEffect, CrossWorldLinkDefinition, CrossWorldPropagation, FlagshipMmoAcceptanceConfig,
    FlagshipMmoAcceptanceResult, FlagshipMmoAcceptanceSummary, ReplayTrainingSample, RewardReason,
    TeamControlMode, TournamentEliminationMode, WorldRealityDefinition, WorldRealityRole,
    WorldTournamentDefinition,
};
use serde::Serialize;

const REPORT_SCHEMA_VERSION: u32 = 1;
const DEFAULT_SCENARIO: &str = "deadman-neural-cup";

#[derive(Debug)]
struct HeadlessOptions {
    profile: String,
    scenario: String,
    output: Option<PathBuf>,
    dataset_output: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct ScenarioDefinition {
    tournament: WorldTournamentDefinition,
    teams: Vec<AgentTeamDefinition>,
    worlds: Vec<WorldRealityDefinition>,
    links: Vec<CrossWorldLinkDefinition>,
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
    world_runs: Vec<WorldRunReport>,
    dataset_summary: RewardDatasetSummary,
    cross_world_projections: Vec<CrossWorldProjectionReport>,
    applied_world_states: Vec<AppliedWorldStateReport>,
    standings: Vec<TeamStandingReport>,
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

#[derive(Debug, Clone, Serialize)]
struct AppliedWorldStateReport {
    world_id: String,
    display_name: String,
    role: WorldRealityRole,
    team_scores: Vec<TeamDeltaReport>,
    death_marks: Vec<TeamDeathMarkReport>,
    faction_reputation_deltas: Vec<NamedDeltaReport>,
    encounter_weight_deltas: Vec<NamedDeltaReport>,
    resource_scarcity_deltas: Vec<NamedDeltaReport>,
    objective_state_shifts: Vec<ObjectiveShiftReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct TeamDeltaReport {
    team_id: String,
    total_delta: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct TeamDeathMarkReport {
    team_id: String,
    applications: usize,
    total_duration_ticks: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct NamedDeltaReport {
    id: String,
    total_delta: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ObjectiveShiftReport {
    quest_graph_id: String,
    stage_tag: String,
    applications: usize,
}

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
    dataset_row_count: usize,
    world_reward_total: f32,
    applied_score_delta: i32,
    active_death_marks: usize,
    active_death_mark_ticks: u64,
}

#[derive(Debug, Default, Clone)]
struct TeamStandingAccumulator {
    assigned_agents: BTreeSet<String>,
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

    println!("{json}");
    Ok(())
}

fn parse_args() -> Result<HeadlessOptions, Box<dyn std::error::Error>> {
    let mut options = HeadlessOptions {
        profile: "ci-smoke".into(),
        scenario: DEFAULT_SCENARIO.into(),
        output: None,
        dataset_output: None,
    };

    let args = env::args().skip(1).collect::<Vec<_>>();
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
        "Usage: cargo run -p pod-headless -- [--profile ci-smoke|shard-target] [--scenario deadman-neural-cup] [--output PATH] [--dataset-output PATH]"
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
        base_config,
    }
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

    let dataset_worlds = executions
        .iter()
        .map(|execution| build_world_dataset_export(execution, &scenario.teams))
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
    let applied_world_states =
        build_applied_world_states(&scenario.worlds, &cross_world_projections);

    let standings = build_team_standings(
        &scenario.teams,
        &scenario.worlds,
        &dataset_worlds,
        &applied_world_states,
    );

    let generated_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let tournament_id = scenario.tournament.tournament_id.clone();
    let notes = vec![
        "World runs are authoritative flagship acceptance simulations with deterministic per-world seed derivation.".into(),
        "Cross-world projections are derived from canonical reward reasons in replay telemetry, not browser-local heuristics.".into(),
        "Team standings are projection totals from cross-world links; per-team in-world attribution still needs admission-aware runtime wiring.".into(),
        "Dataset rows are replay-derived training samples enriched with authoritative reward reasons and runtime profile metadata.".into(),
    ];

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
            world_runs: executions
                .into_iter()
                .map(|execution| execution.report)
                .collect(),
            dataset_summary: dataset_summary.clone(),
            cross_world_projections,
            applied_world_states,
            standings,
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
    teams: &[AgentTeamDefinition],
) -> WorldDatasetExport {
    let rows = build_dataset_rows(&execution.world, &execution.result, teams);
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
    teams: &[AgentTeamDefinition],
) -> Vec<RewardDatasetRow> {
    let samples = result.training_samples();
    let mut sample_index = 0usize;
    let mut rows = Vec::with_capacity(samples.len());
    let admissions = build_world_admissions(world, result, teams);

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

#[derive(Debug, Clone)]
struct TeamAdmissionAssignment {
    team_id: String,
    slot_index: u16,
}

fn build_world_admissions(
    world: &WorldRealityDefinition,
    result: &FlagshipMmoAcceptanceResult,
    teams: &[AgentTeamDefinition],
) -> BTreeMap<String, TeamAdmissionAssignment> {
    let Some(first_window) = result.telemetry_windows().first() else {
        return BTreeMap::new();
    };

    let mut roster = first_window
        .agents
        .iter()
        .map(|agent| agent.agent_id.0.to_string())
        .collect::<Vec<_>>();
    roster.sort();

    let team_lookup = teams
        .iter()
        .map(|team| (team.team_id.clone(), team))
        .collect::<BTreeMap<_, _>>();

    assign_roster_to_world_teams(&roster, world, &team_lookup)
}

fn assign_roster_to_world_teams(
    roster: &[String],
    world: &WorldRealityDefinition,
    team_lookup: &BTreeMap<String, &AgentTeamDefinition>,
) -> BTreeMap<String, TeamAdmissionAssignment> {
    let active_teams = world
        .active_team_ids
        .iter()
        .filter_map(|team_id| team_lookup.get(team_id).copied())
        .filter(|team| {
            team.allowed_world_ids
                .iter()
                .any(|world_id| world_id == &world.world_id)
        })
        .collect::<Vec<_>>();
    if active_teams.is_empty() {
        return BTreeMap::new();
    }

    let mut assignments = BTreeMap::new();
    let mut team_slots = active_teams
        .iter()
        .map(|team| (team.team_id.clone(), 0u16))
        .collect::<BTreeMap<_, _>>();
    let mut team_index = 0usize;

    for agent_id in roster {
        let mut selected = None;
        for offset in 0..active_teams.len() {
            let candidate_index = (team_index + offset) % active_teams.len();
            let candidate = active_teams[candidate_index];
            let next_slot = *team_slots
                .get(&candidate.team_id)
                .expect("candidate team has slot entry");
            if next_slot < candidate.max_agents {
                selected = Some((candidate_index, candidate.team_id.clone(), next_slot));
                break;
            }
        }

        if let Some((selected_index, team_id, slot_index)) = selected {
            assignments.insert(
                agent_id.clone(),
                TeamAdmissionAssignment {
                    team_id,
                    slot_index,
                },
            );
            if let Some(slot) = team_slots.get_mut(
                &assignments
                    .get(agent_id)
                    .expect("assignment inserted")
                    .team_id,
            ) {
                *slot += 1;
            }
            team_index = (selected_index + 1) % active_teams.len();
        }
    }

    assignments
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
    dataset_worlds: &[WorldDatasetExport],
    applied_world_states: &[AppliedWorldStateReport],
) -> Vec<TeamStandingReport> {
    let mut by_team = BTreeMap::<String, TeamStandingAccumulator>::new();
    for world in dataset_worlds {
        for row in &world.rows {
            if let Some(team_id) = &row.team_id {
                let entry = by_team.entry(team_id.clone()).or_default();
                entry.dataset_row_count += 1;
                entry.world_reward_total += row.sample.reward_summary.total;
                entry
                    .assigned_agents
                    .insert(row.sample.agent_id.0.to_string());
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
                dataset_row_count: totals.dataset_row_count,
                world_reward_total: totals.world_reward_total,
                applied_score_delta: totals.applied_score_delta,
                active_death_marks: totals.active_death_marks,
                active_death_mark_ticks: totals.active_death_mark_ticks,
            }
        })
        .collect()
}

fn build_applied_world_states(
    worlds: &[WorldRealityDefinition],
    projections: &[CrossWorldProjectionReport],
) -> Vec<AppliedWorldStateReport> {
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
                            *faction_reputation.entry(faction_id.clone()).or_insert(0) +=
                                *total_delta;
                        }
                        ProjectedCrossWorldEffect::EncounterWeightDelta {
                            table_id,
                            total_delta,
                            ..
                        } => {
                            *encounter_weights.entry(table_id.clone()).or_insert(0) += *total_delta;
                        }
                        ProjectedCrossWorldEffect::ResourceScarcityDelta {
                            biome_id,
                            total_delta,
                            ..
                        } => {
                            *resource_scarcity.entry(biome_id.clone()).or_insert(0) += *total_delta;
                        }
                        ProjectedCrossWorldEffect::TeamScoreDelta {
                            team_id,
                            total_delta,
                            ..
                        } => {
                            *team_scores.entry(team_id.clone()).or_insert(0) += *total_delta;
                        }
                        ProjectedCrossWorldEffect::DeathMark {
                            team_id,
                            applications,
                            total_duration_ticks,
                            ..
                        } => {
                            let entry = death_marks.entry(team_id.clone()).or_insert((0usize, 0));
                            entry.0 += *applications;
                            entry.1 += *total_duration_ticks;
                        }
                        ProjectedCrossWorldEffect::ObjectiveStateShift {
                            quest_graph_id,
                            stage_tag,
                            applications,
                        } => {
                            *objective_shifts
                                .entry((quest_graph_id.clone(), stage_tag.clone()))
                                .or_insert(0) += *applications;
                        }
                    }
                }
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
                objective_state_shifts: objective_shifts
                    .into_iter()
                    .map(
                        |((quest_graph_id, stage_tag), applications)| ObjectiveShiftReport {
                            quest_graph_id,
                            stage_tag,
                            applications,
                        },
                    )
                    .collect(),
            }
        })
        .collect()
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
        ActionOutcomeSummary, AgentRole, AgentType, RewardAttributionSummary, RewardSource,
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
        assert_eq!(scenario.tournament.tournament_id, "deadman-neural-cup");
        assert_eq!(scenario.tournament.world_ids.len(), 3);
        assert_eq!(scenario.tournament.team_ids.len(), 2);
        assert_eq!(scenario.tournament.cross_world_link_ids.len(), 3);
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
        let team_lookup = teams
            .iter()
            .map(|team| (team.team_id.clone(), team))
            .collect::<BTreeMap<_, _>>();
        let roster = vec![
            "agent-a".to_string(),
            "agent-b".to_string(),
            "agent-c".to_string(),
            "agent-d".to_string(),
        ];

        let assignments = assign_roster_to_world_teams(&roster, &world, &team_lookup);

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

        let applied = build_applied_world_states(&worlds, &projections);

        assert_eq!(applied[0].team_scores[0].team_id, "iron-sigil");
        assert_eq!(applied[0].team_scores[0].total_delta, 10);
        assert_eq!(applied[0].death_marks[0].team_id, "gloam-mesh");
        assert_eq!(applied[0].death_marks[0].applications, 2);
        assert_eq!(applied[1].faction_reputation_deltas[0].id, "echo-order");
        assert_eq!(applied[1].objective_state_shifts[0].applications, 1);
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
            },
        ];

        let standings = build_team_standings(
            &[iron_sigil, gloam_mesh],
            &worlds,
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
        assert_eq!(by_team["iron-sigil"].assigned_agent_count, 1);
        assert_eq!(by_team["iron-sigil"].world_reward_total, 2.0);
        assert_eq!(by_team["iron-sigil"].applied_score_delta, 15);
        assert_eq!(by_team["iron-sigil"].active_death_marks, 0);
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
