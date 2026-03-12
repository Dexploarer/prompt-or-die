use pod_assets::{
    build_runtime_bundle_manifest, import_asset, materialize_runtime_bundle_manifest, AssetCache,
    AssetImport, RuntimeBundleSpec,
};
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let program = env::args()
        .next()
        .unwrap_or_else(|| "cargo run -p pod-assets --example stage_import --".to_string());
    let mut args = env::args().skip(1);
    let mut json_output = false;
    let mut materialize_runtime = false;
    let mut output_root = PathBuf::from("artifacts/staged-assets");
    let mut base_dir: Option<PathBuf> = None;
    let mut bundle_spec_path: Option<PathBuf> = None;
    let mut source_paths = Vec::<PathBuf>::new();

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--json" => {
                json_output = true;
            }
            "--materialize-runtime" => {
                materialize_runtime = true;
            }
            "--output-root" => {
                let Some(value) = args.next() else {
                    eprintln!("{}", usage(&program));
                    return ExitCode::from(64);
                };
                output_root = PathBuf::from(value);
            }
            "--base-dir" => {
                let Some(value) = args.next() else {
                    eprintln!("{}", usage(&program));
                    return ExitCode::from(64);
                };
                base_dir = Some(PathBuf::from(value));
            }
            "--bundle-spec" => {
                let Some(value) = args.next() else {
                    eprintln!("{}", usage(&program));
                    return ExitCode::from(64);
                };
                bundle_spec_path = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                println!("{}", usage(&program));
                return ExitCode::SUCCESS;
            }
            value if value.starts_with("--") => {
                eprintln!("Unknown flag: {value}\n\n{}", usage(&program));
                return ExitCode::from(64);
            }
            value => source_paths.push(PathBuf::from(value)),
        }
    }

    if source_paths.is_empty() {
        eprintln!("{}", usage(&program));
        return ExitCode::from(64);
    }
    if materialize_runtime && bundle_spec_path.is_none() {
        eprintln!(
            "stage_import failed: --materialize-runtime requires --bundle-spec\n\n{}",
            usage(&program)
        );
        return ExitCode::from(64);
    }

    let bundle_spec = match bundle_spec_path {
        Some(path) => match fs::read_to_string(&path)
            .ok()
            .and_then(|contents| serde_json::from_str::<RuntimeBundleSpec>(&contents).ok())
        {
            Some(spec) => Some(spec),
            None => {
                eprintln!("stage_import failed: could not read bundle spec at {}", path.display());
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    let mut cache = AssetCache::new();
    let mut imports = Vec::<AssetImport>::new();
    for source_path in source_paths {
        match import_asset(&mut cache, &source_path, &output_root) {
            Ok(import) => imports.push(import),
            Err(error) => {
                eprintln!("stage_import failed: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    let bundle_base_dir =
        base_dir.unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let bundle_manifest = match bundle_spec.as_ref() {
        Some(spec) => match build_runtime_bundle_manifest(spec, &imports, &bundle_base_dir) {
            Ok(manifest) => Some(manifest),
            Err(error) => {
                eprintln!("stage_import failed: {error}");
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };

    if materialize_runtime {
        let Some(bundle_manifest) = bundle_manifest.as_ref() else {
            eprintln!("stage_import failed: --materialize-runtime requires --bundle-spec");
            return ExitCode::from(64);
        };
        if let Err(error) = materialize_runtime_bundle_manifest(bundle_manifest, &bundle_base_dir) {
            eprintln!("stage_import failed: {error}");
            return ExitCode::FAILURE;
        }
    }

    if json_output {
        println!(
            "{}",
            json!({
                "imports": imports.iter().map(import_to_json).collect::<Vec<_>>(),
                "bundleManifest": bundle_manifest
            })
        );
    } else {
        for (index, import) in imports.iter().enumerate() {
            if index > 0 {
                println!();
            }
            println!("id={}", import.id);
            println!("format={}", import.format);
            println!("source={}", import.source_path.display());
            println!("imported={}", import.imported_path.display());
            println!("bytes={}", import.byte_len);
        }
    }

    ExitCode::SUCCESS
}

fn import_to_json(import: &AssetImport) -> serde_json::Value {
    json!({
        "id": import.id.to_string(),
        "format": import.format.to_string(),
        "source": import.source_path.display().to_string(),
        "imported": import.imported_path.display().to_string(),
        "bytes": import.byte_len,
        "checksum": import.checksum
    })
}

fn usage(program: &str) -> String {
    format!(
        "Usage: {program} [--json] [--materialize-runtime] [--output-root <dir>] [--base-dir <dir>] [--bundle-spec <json>] <source-path> [<source-path> ...]\n\nStages one or more supported authoring assets into a content-addressed import directory."
    )
}
