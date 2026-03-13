use std::env;
use std::fs;
use std::path::PathBuf;

use pod_core::RemoteTopologyBundle;
use pod_net::build_topology_feed_measurements;

#[derive(Debug)]
struct Options {
    topology_input: PathBuf,
    output: Option<PathBuf>,
    fail_on_checks: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args()?;
    let topology =
        serde_json::from_slice::<RemoteTopologyBundle>(&fs::read(&options.topology_input)?)?;
    let report = build_topology_feed_measurements(&topology)?;
    let json = serde_json::to_string_pretty(&report)?;

    if let Some(output) = &options.output {
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output, json.as_bytes())?;
    }

    println!("{json}");

    if options.fail_on_checks && !report.all_checks_passed() {
        return Err("topology feed benchmark checks failed".into());
    }

    Ok(())
}

fn parse_args() -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options {
        topology_input: PathBuf::new(),
        output: None,
        fail_on_checks: false,
    };

    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--topology-input" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("missing value for --topology-input")?;
                options.topology_input = PathBuf::from(value);
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

    if options.topology_input.as_os_str().is_empty() {
        return Err("missing required --topology-input".into());
    }

    Ok(options)
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p pod-net --example topology_feed_benchmark_suite -- --topology-input PATH [--output PATH] [--fail-on-checks]"
    );
}
