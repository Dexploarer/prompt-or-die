use std::env;
use std::fs;
use std::path::PathBuf;

use pod_core::build_rust_sdk_handoff_fixture;

#[derive(Debug, Clone, Copy)]
enum OutputFormat {
    Json,
    Toon,
}

#[derive(Debug)]
struct Options {
    format: OutputFormat,
    output: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args()?;
    let fixture = build_rust_sdk_handoff_fixture();
    let output = match options.format {
        OutputFormat::Json => serde_json::to_string_pretty(&fixture)?,
        OutputFormat::Toon => fixture.to_toon_document(),
    };

    if let Some(path) = &options.output {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, output.as_bytes())?;
    }

    println!("{output}");
    Ok(())
}

fn parse_args() -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options {
        format: OutputFormat::Json,
        output: None,
    };

    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--format" => {
                index += 1;
                let value = args.get(index).ok_or("missing value for --format")?;
                options.format = match value.as_str() {
                    "json" => OutputFormat::Json,
                    "toon" => OutputFormat::Toon,
                    unknown => {
                        return Err(format!(
                            "unsupported format '{unknown}' (expected 'json' or 'toon')"
                        )
                        .into());
                    }
                };
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
            unknown => return Err(format!("unknown argument: {unknown}").into()),
        }
        index += 1;
    }

    Ok(options)
}

fn print_help() {
    eprintln!(
        "Usage: cargo run -p pod-core --example rust_sdk_handoff_fixture -- [--format json|toon] [--output PATH]"
    );
}
