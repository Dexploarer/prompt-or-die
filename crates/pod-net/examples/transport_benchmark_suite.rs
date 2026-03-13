use std::env;
use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use pod_net::{run_transport_benchmark_suite, TransportBenchmarkProfile};

#[derive(Debug)]
struct Options {
    profile: TransportBenchmarkProfile,
    output: Option<PathBuf>,
    fail_on_checks: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args()?;
    let report = run_transport_benchmark_suite(options.profile).await;
    let json = serde_json::to_string_pretty(&report)?;

    if let Some(output) = &options.output {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, json.as_bytes())?;
    }

    println!("{json}");

    if options.fail_on_checks && !report.all_checks_passed() {
        return Err("transport benchmark checks failed".into());
    }

    Ok(())
}

fn parse_args() -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options {
        profile: TransportBenchmarkProfile::CiSmoke,
        output: None,
        fail_on_checks: false,
    };

    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--profile" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --profile")?;
                options.profile = TransportBenchmarkProfile::from_str(value)?;
            }
            "--output" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --output")?;
                options.output = Some(PathBuf::from(value));
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

fn print_help() {
    eprintln!(
        "Usage: cargo run -p pod-net --example transport_benchmark_suite -- [--profile ci-smoke|shard-target] [--output PATH] [--fail-on-checks]"
    );
}
