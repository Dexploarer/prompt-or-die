use std::env;
use std::fs;
use std::path::PathBuf;

use pod_sdk::run_rust_sdk_benchmark_suite;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = None::<PathBuf>;
    let mut replay_output = None::<PathBuf>;
    let mut training_output = None::<PathBuf>;
    let mut fail_on_checks = false;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--output" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--output requires a path".to_string())?;
                output = Some(PathBuf::from(value));
            }
            "--replay-output" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--replay-output requires a path".to_string())?;
                replay_output = Some(PathBuf::from(value));
            }
            "--training-output" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--training-output requires a path".to_string())?;
                training_output = Some(PathBuf::from(value));
            }
            "--fail-on-checks" => fail_on_checks = true,
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other => return Err(format!("unsupported argument: {other}").into()),
        }
    }

    let run = run_rust_sdk_benchmark_suite()?;
    let report_json = serde_json::to_string_pretty(&run.report)?;

    if let Some(path) = output {
        fs::write(path, report_json)?;
    } else {
        println!("{report_json}");
    }

    if let Some(path) = replay_output {
        fs::write(path, run.replay.to_toon_document())?;
    }

    if let Some(path) = training_output {
        fs::write(path, run.replay.training_samples_to_toon_document())?;
    }

    if fail_on_checks && !run.report.passed() {
        return Err("Rust SDK benchmark checks failed".into());
    }

    Ok(())
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p pod-sdk --example rust_sdk_benchmark_suite -- [--output PATH] [--replay-output PATH] [--training-output PATH] [--fail-on-checks]"
    );
}
