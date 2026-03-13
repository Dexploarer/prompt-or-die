use std::env;
use std::fs;
use std::path::PathBuf;

use pod_agents::run_controller_parity_harness;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = None::<PathBuf>;
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
            "--fail-on-checks" => fail_on_checks = true,
            other => return Err(format!("unsupported argument: {other}").into()),
        }
    }

    let report = run_controller_parity_harness();
    let json = serde_json::to_string_pretty(&report)?;

    if let Some(path) = output {
        fs::write(path, json)?;
    } else {
        println!("{json}");
    }

    if fail_on_checks && !report.passed() {
        return Err(format!(
            "controller parity harness failed checks: {}",
            report.failed_checks.join("; ")
        )
        .into());
    }

    Ok(())
}
