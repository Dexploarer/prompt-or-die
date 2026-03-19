use std::env;
use std::fs;
use std::path::PathBuf;

use pod_sdk::{run_rust_sdk_live_smoke, RustSdkLiveSmokeConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    host: String,
    db_name: String,
    auth_token: Option<String>,
    timeout_ms: u64,
    output: Option<PathBuf>,
    replay_output: Option<PathBuf>,
    training_output: Option<PathBuf>,
    fail_on_checks: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args()?;
    let run = run_rust_sdk_live_smoke(&RustSdkLiveSmokeConfig {
        host: options.host.clone(),
        db_name: options.db_name.clone(),
        auth_token: options.auth_token.clone(),
        timeout_ms: options.timeout_ms,
        poll_interval_ms: 10,
    })?;
    let report_json = serde_json::to_string_pretty(&run.report)?;

    if let Some(path) = &options.output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, report_json.as_bytes())?;
    }

    if let Some(path) = &options.replay_output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, run.replay.to_toon_document())?;
    }

    if let Some(path) = &options.training_output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, run.replay.training_samples_to_toon_document())?;
    }

    println!("{report_json}");

    if options.fail_on_checks && !run.report.passed() {
        return Err("Rust SDK live smoke checks failed".into());
    }

    Ok(())
}

fn parse_args_from(args: Vec<String>) -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options {
        host: "http://localhost:3000".into(),
        db_name: "prompt-or-die".into(),
        auth_token: None,
        timeout_ms: 5_000,
        output: None,
        replay_output: None,
        training_output: None,
        fail_on_checks: false,
    };

    let args = args.into_iter().skip(1).collect::<Vec<_>>();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--host" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --host")?;
                options.host = value.clone();
            }
            "--db-name" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --db-name")?;
                options.db_name = value.clone();
            }
            "--auth-token" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --auth-token")?;
                options.auth_token = Some(value.clone());
            }
            "--timeout-ms" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --timeout-ms")?;
                options.timeout_ms = value
                    .parse::<u64>()
                    .map_err(|error| format!("invalid --timeout-ms: {error}"))?;
            }
            "--output" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --output")?;
                options.output = Some(PathBuf::from(value));
            }
            "--replay-output" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --replay-output")?;
                options.replay_output = Some(PathBuf::from(value));
            }
            "--training-output" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("missing value for --training-output")?;
                options.training_output = Some(PathBuf::from(value));
            }
            "--fail-on-checks" => {
                options.fail_on_checks = true;
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

fn parse_args() -> Result<Options, Box<dyn std::error::Error>> {
    parse_args_from(env::args().collect())
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p pod-sdk --example rust_sdk_live_smoke -- [--host URL] [--db-name NAME] [--auth-token TOKEN] [--timeout-ms MS] [--output PATH] [--replay-output PATH] [--training-output PATH] [--fail-on-checks]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_live_smoke_flags() {
        let options = parse_args_from(vec![
            "rust_sdk_live_smoke".into(),
            "--host".into(),
            "http://127.0.0.1:3100".into(),
            "--db-name".into(),
            "deadman-prime".into(),
            "--auth-token".into(),
            "tok-live".into(),
            "--timeout-ms".into(),
            "2500".into(),
            "--output".into(),
            "/tmp/live-smoke.json".into(),
            "--replay-output".into(),
            "/tmp/live-smoke.toon".into(),
            "--training-output".into(),
            "/tmp/live-smoke-training.toon".into(),
            "--fail-on-checks".into(),
        ])
        .expect("arguments should parse");

        assert_eq!(options.host, "http://127.0.0.1:3100");
        assert_eq!(options.db_name, "deadman-prime");
        assert_eq!(options.auth_token.as_deref(), Some("tok-live"));
        assert_eq!(options.timeout_ms, 2500);
        assert_eq!(options.output, Some(PathBuf::from("/tmp/live-smoke.json")));
        assert_eq!(
            options.replay_output,
            Some(PathBuf::from("/tmp/live-smoke.toon"))
        );
        assert_eq!(
            options.training_output,
            Some(PathBuf::from("/tmp/live-smoke-training.toon"))
        );
        assert!(options.fail_on_checks);
    }
}
