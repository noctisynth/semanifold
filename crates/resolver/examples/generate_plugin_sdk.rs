use std::process::ExitCode;

#[cfg(feature = "ts-rs")]
mod generator {
    use std::{env, error::Error, fs, io, path::PathBuf};

    use semifold_resolver::plugin::protocol::{
        PLUGIN_OPERATIONS, PLUGIN_PROTOCOL_SCHEMA_VERSION, PluginCallV1, PluginDependencyKindV1,
        PluginDependencySourceV1, PluginDependencyV1, PluginDiagnosticSeverityV1,
        PluginDiagnosticV1, PluginDiscoverInputV1, PluginEditSourceV1, PluginFileEditExpectationV1,
        PluginFileEditV1, PluginInspectInputV1, PluginManifestDependencyV1, PluginMetadataV1,
        PluginOperation, PluginOutcomeV1, PluginOutputV1, PluginPackageInspectionV1,
        PluginPackageLocationV1, PluginPackageSnapshotV1, PluginPlanEditsInputV1, PluginRequestV1,
        PluginResponseV1, PluginSharedVersionEditV1, PluginVersionSourceV1,
    };
    use ts_rs::{Config, TS};

    const GENERATED_PATH: &str = "../../packages/plugin-sdk/src/generated/protocol.ts";

    pub fn run() -> Result<(), Box<dyn Error>> {
        let check = parse_mode()?;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GENERATED_PATH);
        let generated = generate()?;

        if check {
            let current = fs::read_to_string(&path).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!(
                        "failed to read generated plugin SDK types at {}: {error}",
                        path.display()
                    ),
                )
            })?;
            if current != generated {
                return Err(io::Error::other(format!(
                    "generated plugin SDK types are stale; run `cargo run -p semifold-resolver --example generate_plugin_sdk --features ts-rs` to update {}",
                    path.display()
                ))
                .into());
            }
            return Ok(());
        }

        let parent = path.parent().ok_or_else(|| {
            io::Error::other(format!(
                "generated plugin SDK path has no parent: {}",
                path.display()
            ))
        })?;
        fs::create_dir_all(parent)?;
        fs::write(&path, generated)?;
        Ok(())
    }

    fn parse_mode() -> Result<bool, io::Error> {
        let arguments = env::args().skip(1).collect::<Vec<_>>();
        match arguments.as_slice() {
            [] => Ok(false),
            [argument] if argument == "--check" => Ok(true),
            _ => Err(io::Error::other("usage: generate_plugin_sdk [--check]")),
        }
    }

    fn generate() -> Result<String, serde_json::Error> {
        let config = Config::default();
        let operations = serde_json::to_string(&PLUGIN_OPERATIONS)?;
        let mut output = format!(
            "// This file is generated from crates/resolver/src/plugin/protocol.rs.\n\
             // Do not edit it directly.\n\n\
             export const PLUGIN_PROTOCOL_SCHEMA_VERSION = {PLUGIN_PROTOCOL_SCHEMA_VERSION} as const;\n\n\
             export const PLUGIN_OPERATIONS: readonly PluginOperationV1[] = Object.freeze({operations});\n\n"
        );

        append::<PluginOperation>(&config, &mut output);
        append::<PluginMetadataV1>(&config, &mut output);
        append::<PluginDiscoverInputV1>(&config, &mut output);
        append::<PluginPackageLocationV1>(&config, &mut output);
        append::<PluginInspectInputV1>(&config, &mut output);
        append::<PluginDependencyKindV1>(&config, &mut output);
        append::<PluginDependencySourceV1>(&config, &mut output);
        append::<PluginManifestDependencyV1>(&config, &mut output);
        append::<PluginVersionSourceV1>(&config, &mut output);
        append::<PluginPackageInspectionV1>(&config, &mut output);
        append::<PluginDependencyV1>(&config, &mut output);
        append::<PluginPackageSnapshotV1>(&config, &mut output);
        append::<PluginPlanEditsInputV1>(&config, &mut output);
        append::<PluginCallV1>(&config, &mut output);
        append::<PluginRequestV1>(&config, &mut output);
        append::<PluginDiagnosticSeverityV1>(&config, &mut output);
        append::<PluginDiagnosticV1>(&config, &mut output);
        append::<PluginFileEditExpectationV1>(&config, &mut output);
        append::<PluginSharedVersionEditV1>(&config, &mut output);
        append::<PluginEditSourceV1>(&config, &mut output);
        append::<PluginFileEditV1>(&config, &mut output);
        append::<PluginOutputV1>(&config, &mut output);
        append::<PluginOutcomeV1>(&config, &mut output);
        append::<PluginResponseV1>(&config, &mut output);

        Ok(output)
    }

    fn append<T: TS>(config: &Config, output: &mut String) {
        output.push_str("export ");
        output.push_str(&T::decl(config));
        output.push_str("\n\n");
    }
}

fn main() -> ExitCode {
    #[cfg(feature = "ts-rs")]
    {
        return match generator::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("failed to generate plugin SDK types: {error}");
                ExitCode::FAILURE
            }
        };
    }

    #[cfg(not(feature = "ts-rs"))]
    {
        eprintln!("plugin SDK generation requires `--features ts-rs`");
        ExitCode::FAILURE
    }
}
