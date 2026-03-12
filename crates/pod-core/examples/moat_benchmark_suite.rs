use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use pod_core::{
    run_flagship_mmo_acceptance, ActionLifecycleStage, FlagshipMmoAcceptanceConfig,
    FlagshipMmoAcceptanceResult,
};
use serde::Serialize;

#[derive(Debug)]
struct BenchmarkOptions {
    profile: String,
    monthly_host_cost_usd: Option<f64>,
    output: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    schema_version: u32,
    generated_at_unix_ms: u128,
    profile: String,
    config: FlagshipMmoAcceptanceConfig,
    deterministic_replay_fidelity: ReplayFidelityMetric,
    authoritative_tick_stability: TickStabilityMetric,
    agent_action_transparency: ActionTransparencyMetric,
    normalized_cost_model: CostMetric,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ReplayFidelityMetric {
    parity_passed: bool,
    matched_ticks: usize,
    observation_mismatches: usize,
    decision_mismatches: usize,
    fidelity_score: f64,
}

#[derive(Debug, Serialize)]
struct TickStabilityMetric {
    target_tps: u32,
    target_tick_budget_ms: f64,
    ticks_completed: u64,
    total_runtime_ms: f64,
    average_tick_runtime_ms: f64,
    p95_tick_runtime_ms: f64,
    max_tick_runtime_ms: f64,
    over_budget_ticks: usize,
    budget_compliance_ratio: f64,
}

#[derive(Debug, Serialize)]
struct ActionTransparencyMetric {
    actions_processed: usize,
    actions_rejected: usize,
    action_acceptance_rate: f64,
    action_rejection_rate: f64,
    rejected_action_traces: usize,
    rejected_action_traces_with_reason: usize,
    rejection_reason_coverage: f64,
    telemetry_frames: usize,
    tool_calls: usize,
    tool_call_errors: usize,
    tool_error_rate: f64,
}

#[derive(Debug, Serialize)]
struct CostMetric {
    measured_active_agents: usize,
    monthly_host_cost_usd: Option<f64>,
    normalized_cost_per_100_agents_usd: Option<f64>,
    normalized_cost_per_1000_agents_usd: Option<f64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args()?;
    let config = config_for_profile(&options.profile)?;
    let result = run_flagship_mmo_acceptance(config.clone())?;
    let report = build_report(&options, &result);
    let json = serde_json::to_string_pretty(&report)?;

    if let Some(output) = &options.output {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, json.as_bytes())?;
    }

    println!("{json}");
    Ok(())
}

fn parse_args() -> Result<BenchmarkOptions, Box<dyn std::error::Error>> {
    let mut options = BenchmarkOptions {
        profile: "ci-smoke".into(),
        monthly_host_cost_usd: None,
        output: None,
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
            "--monthly-host-cost-usd" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("missing value for --monthly-host-cost-usd")?
                    .parse::<f64>()?;
                options.monthly_host_cost_usd = Some(value);
            }
            "--output" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --output")?;
                options.output = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            unknown => {
                return Err(format!("unknown argument: {unknown}").into());
            }
        }
        index += 1;
    }

    Ok(options)
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p pod-core --example moat_benchmark_suite -- [--profile ci-smoke|shard-target] [--monthly-host-cost-usd VALUE] [--output PATH]"
    );
}

fn config_for_profile(profile: &str) -> Result<FlagshipMmoAcceptanceConfig, String> {
    match profile {
        "ci-smoke" => Ok(FlagshipMmoAcceptanceConfig::ci_smoke()),
        "shard-target" => Ok(FlagshipMmoAcceptanceConfig::shard_target()),
        unknown => Err(format!(
            "unsupported benchmark profile '{unknown}' (expected 'ci-smoke' or 'shard-target')"
        )),
    }
}

fn build_report(
    options: &BenchmarkOptions,
    result: &FlagshipMmoAcceptanceResult,
) -> BenchmarkReport {
    let mut notes = vec![
        "Run the shard-target profile in release mode for comparable economic baselines.".into(),
        "Use scripts/run_moat_benchmarks.ts to combine this core report with browser/native parity and creator-time tracking.".into(),
    ];
    if options.monthly_host_cost_usd.is_none() {
        notes.push(
            "Pass --monthly-host-cost-usd to normalize shard economics to cost per 100 and 1000 active agents."
                .into(),
        );
    }

    BenchmarkReport {
        schema_version: 1,
        generated_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        profile: options.profile.clone(),
        config: result.config.clone(),
        deterministic_replay_fidelity: build_replay_fidelity_metric(result),
        authoritative_tick_stability: build_tick_stability_metric(result),
        agent_action_transparency: build_action_transparency_metric(result),
        normalized_cost_model: build_cost_metric(result, options.monthly_host_cost_usd),
        notes,
    }
}

fn build_replay_fidelity_metric(result: &FlagshipMmoAcceptanceResult) -> ReplayFidelityMetric {
    let matched_ticks = result
        .parity_reports
        .iter()
        .map(|report| report.matched_ticks)
        .sum::<usize>();
    let observation_mismatches = result
        .parity_reports
        .iter()
        .map(|report| report.observation_mismatches)
        .sum::<usize>();
    let decision_mismatches = result
        .parity_reports
        .iter()
        .map(|report| report.decision_mismatches)
        .sum::<usize>();
    let total_signals = matched_ticks.saturating_mul(2);
    let mismatches = observation_mismatches + decision_mismatches;
    let fidelity_score = if total_signals == 0 {
        0.0
    } else {
        (total_signals.saturating_sub(mismatches)) as f64 / total_signals as f64
    };

    ReplayFidelityMetric {
        parity_passed: result.parity_passed(),
        matched_ticks,
        observation_mismatches,
        decision_mismatches,
        fidelity_score,
    }
}

fn build_tick_stability_metric(result: &FlagshipMmoAcceptanceResult) -> TickStabilityMetric {
    let durations = result.tick_durations_ms();
    let total_runtime_ms = durations.iter().sum::<f64>();
    let average_tick_runtime_ms = if durations.is_empty() {
        0.0
    } else {
        total_runtime_ms / durations.len() as f64
    };
    let target_tick_budget_ms = 1000.0 / result.config.scale_target.target_tps as f64;
    let max_tick_runtime_ms = durations.iter().copied().fold(0.0_f64, f64::max);
    let over_budget_ticks = durations
        .iter()
        .filter(|duration| **duration > target_tick_budget_ms)
        .count();
    let budget_compliance_ratio = if durations.is_empty() {
        0.0
    } else {
        (durations.len() - over_budget_ticks) as f64 / durations.len() as f64
    };

    TickStabilityMetric {
        target_tps: result.config.scale_target.target_tps,
        target_tick_budget_ms,
        ticks_completed: result.summary.ticks_completed,
        total_runtime_ms,
        average_tick_runtime_ms,
        p95_tick_runtime_ms: percentile(durations, 0.95),
        max_tick_runtime_ms,
        over_budget_ticks,
        budget_compliance_ratio,
    }
}

fn build_action_transparency_metric(
    result: &FlagshipMmoAcceptanceResult,
) -> ActionTransparencyMetric {
    let total_actions = result.summary.actions_processed + result.summary.actions_rejected;
    let action_acceptance_rate = if total_actions == 0 {
        0.0
    } else {
        result.summary.actions_processed as f64 / total_actions as f64
    };
    let action_rejection_rate = if total_actions == 0 {
        0.0
    } else {
        result.summary.actions_rejected as f64 / total_actions as f64
    };

    let mut rejected_action_traces = 0usize;
    let mut rejected_action_traces_with_reason = 0usize;
    for telemetry in result.telemetry_windows() {
        for agent in &telemetry.agents {
            for trace in &agent.action_trace {
                if trace.stage == ActionLifecycleStage::Rejected {
                    rejected_action_traces += 1;
                    if trace
                        .rejection_reason
                        .as_ref()
                        .is_some_and(|reason| !reason.is_empty())
                    {
                        rejected_action_traces_with_reason += 1;
                    }
                }
            }
        }
    }

    let rejection_reason_coverage = if rejected_action_traces == 0 {
        1.0
    } else {
        rejected_action_traces_with_reason as f64 / rejected_action_traces as f64
    };
    let tool_error_rate = if result.summary.tool_calls == 0 {
        0.0
    } else {
        result.summary.tool_call_errors as f64 / result.summary.tool_calls as f64
    };

    ActionTransparencyMetric {
        actions_processed: result.summary.actions_processed,
        actions_rejected: result.summary.actions_rejected,
        action_acceptance_rate,
        action_rejection_rate,
        rejected_action_traces,
        rejected_action_traces_with_reason,
        rejection_reason_coverage,
        telemetry_frames: result.summary.telemetry_frames,
        tool_calls: result.summary.tool_calls,
        tool_call_errors: result.summary.tool_call_errors,
        tool_error_rate,
    }
}

fn build_cost_metric(
    result: &FlagshipMmoAcceptanceResult,
    monthly_host_cost_usd: Option<f64>,
) -> CostMetric {
    let measured_active_agents = result.summary.total_agents;
    let (normalized_cost_per_100_agents_usd, normalized_cost_per_1000_agents_usd) =
        if let Some(monthly_cost) = monthly_host_cost_usd {
            if measured_active_agents == 0 {
                (None, None)
            } else {
                let per_agent_cost = monthly_cost / measured_active_agents as f64;
                (Some(per_agent_cost * 100.0), Some(per_agent_cost * 1000.0))
            }
        } else {
            (None, None)
        };

    CostMetric {
        measured_active_agents,
        monthly_host_cost_usd,
        normalized_cost_per_100_agents_usd,
        normalized_cost_per_1000_agents_usd,
    }
}

fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let index = ((sorted.len() - 1) as f64 * fraction.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
}
