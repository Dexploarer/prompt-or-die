use std::env;
use std::fs;
use std::path::PathBuf;

use pod_core::RemoteTopologyBundle;
use pod_net::{
    build_topology_feed_measurements, build_topology_feed_measurements_with_options,
    LiveGeneratedSdkTopologyFeedConfig, TopologyFeedGeneratedRuntimeMode,
    TopologyFeedMeasurementsOptions,
};

#[derive(Debug)]
struct Options {
    topology_input: PathBuf,
    output: Option<PathBuf>,
    fail_on_checks: bool,
    generated_sdk_host: Option<String>,
    generated_sdk_auth_token: Option<String>,
    generated_sdk_timeout_ms: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args()?;
    let topology =
        serde_json::from_slice::<RemoteTopologyBundle>(&fs::read(&options.topology_input)?)?;
    let report = if let Some(host) = options.generated_sdk_host.clone() {
        build_topology_feed_measurements_with_options(
            &topology,
            &TopologyFeedMeasurementsOptions {
                generated_runtime_mode: TopologyFeedGeneratedRuntimeMode::LiveSdk(
                    LiveGeneratedSdkTopologyFeedConfig {
                        host,
                        auth_token: options.generated_sdk_auth_token.clone(),
                        timeout_ms: options.generated_sdk_timeout_ms,
                        poll_interval_ms: 10,
                    },
                ),
            },
        )?
    } else {
        build_topology_feed_measurements(&topology)?
    };
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

fn parse_args_from(args: Vec<String>) -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options {
        topology_input: PathBuf::new(),
        output: None,
        fail_on_checks: false,
        generated_sdk_host: None,
        generated_sdk_auth_token: None,
        generated_sdk_timeout_ms: 5_000,
    };

    let args = args.into_iter().skip(1).collect::<Vec<_>>();
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
            "--generated-sdk-host" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("missing value for --generated-sdk-host")?;
                options.generated_sdk_host = Some(value.clone());
            }
            "--generated-sdk-auth-token" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("missing value for --generated-sdk-auth-token")?;
                options.generated_sdk_auth_token = Some(value.clone());
            }
            "--generated-sdk-timeout-ms" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("missing value for --generated-sdk-timeout-ms")?;
                options.generated_sdk_timeout_ms = value
                    .parse::<u64>()
                    .map_err(|error| format!("invalid --generated-sdk-timeout-ms: {error}"))?;
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

fn parse_args() -> Result<Options, Box<dyn std::error::Error>> {
    parse_args_from(env::args().collect())
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p pod-net --example topology_feed_benchmark_suite -- --topology-input PATH [--output PATH] [--fail-on-checks] [--generated-sdk-host URL] [--generated-sdk-auth-token TOKEN] [--generated-sdk-timeout-ms MS]"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_generated_sdk_flags() {
        let options = parse_args_from(vec![
            "topology_feed_benchmark_suite".into(),
            "--topology-input".into(),
            "/tmp/topology.json".into(),
            "--generated-sdk-host".into(),
            "http://127.0.0.1:3000".into(),
            "--generated-sdk-auth-token".into(),
            "tok-live".into(),
            "--generated-sdk-timeout-ms".into(),
            "2500".into(),
            "--fail-on-checks".into(),
        ])
        .expect("arguments should parse");

        assert_eq!(options.topology_input, PathBuf::from("/tmp/topology.json"));
        assert_eq!(
            options.generated_sdk_host.as_deref(),
            Some("http://127.0.0.1:3000")
        );
        assert_eq!(
            options.generated_sdk_auth_token.as_deref(),
            Some("tok-live")
        );
        assert_eq!(options.generated_sdk_timeout_ms, 2500);
        assert!(options.fail_on_checks);
    }
}
