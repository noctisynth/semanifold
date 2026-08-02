use std::{collections::BTreeMap, path::Path};

use camino::Utf8PathBuf;
use minijinja::{Environment, UndefinedBehavior, context};
use semifold_core::{CiContext, Ecosystem, PackageId, RepositoryContext};
use semifold_resolver::config::{Asset, CommandConfig, Config, PreCheckConfig, StdioType};
use semver::Version;
use serde::Serialize;

use crate::workspace::load_workspace_graph;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct PublishContext {
    pub package: PublishPackageContext,
    pub repository: Option<RepositoryContext>,
    pub ci: Option<CiContext>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct PublishPackageContext {
    pub id: PackageId,
    pub name: String,
    pub ecosystem: Ecosystem,
    pub version: Version,
    pub tag: String,
    pub path: Utf8PathBuf,
    pub private: bool,
}

#[derive(Debug)]
pub(crate) struct PublishPlan {
    pub packages: Vec<PackagePublish>,
}

#[derive(Debug)]
pub(crate) struct PackagePublish {
    pub context: PublishContext,
    pub preflight: Option<PlannedRegistryCheck>,
    pub commands: Vec<CommandSpec>,
    pub assets: Vec<ReleaseAsset>,
    pub skip_reason: Option<PublishSkipReason>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReleaseAsset {
    pub path: std::path::PathBuf,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlannedRegistryCheck {
    pub url: String,
    pub extra_headers: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublishSkipReason {
    Private,
    RegistryVersionExists,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandPhase {
    Prepublish,
    Publish,
    PostVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StdioPolicy {
    Inherit,
    Pipe,
    Null,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommandSpec {
    pub executable: String,
    pub args: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub working_directory: Utf8PathBuf,
    pub phase: CommandPhase,
    pub stdout: StdioPolicy,
    pub stderr: StdioPolicy,
    pub run_in_dry_run: bool,
}

pub(crate) fn plan_publish(root: &Path, config: &Config) -> anyhow::Result<PublishPlan> {
    let graph = load_workspace_graph(root, config)?;
    let order = graph.topological_order()?;
    let packages = order
        .into_iter()
        .map(|id| {
            let snapshot = graph
                .package(&id)
                .expect("workspace topological order only contains graph packages");
            let package_config = config
                .packages
                .get(id.as_str())
                .expect("workspace graph is constructed from configured packages");
            let resolver_config =
                config
                    .resolver
                    .get(&package_config.resolver)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "resolver config is missing for {}",
                            package_config.resolver
                        )
                    })?;
            let package = PublishPackageContext {
                id: id.clone(),
                name: snapshot.manifest_name.clone(),
                ecosystem: snapshot.ecosystem,
                version: snapshot.version.clone(),
                tag: format!("{}-v{}", snapshot.manifest_name, snapshot.version),
                path: snapshot.path.clone(),
                private: !snapshot.publishable,
            };
            let tag_reference = format!("refs/tags/{}", package.tag);
            anyhow::ensure!(
                git2::Reference::is_valid_name(&tag_reference),
                "planned package tag is not a valid Git reference: {}",
                package.tag
            );
            let context = PublishContext {
                package,
                repository: None,
                ci: None,
            };
            let working_directory = Utf8PathBuf::from_path_buf(root.join(&context.package.path))
                .map_err(|_| anyhow::anyhow!("publish working directory is not valid UTF-8"))?;
            let commands = resolver_config
                .prepublish
                .iter()
                .map(|command| {
                    render_command(
                        command,
                        &context,
                        &working_directory,
                        CommandPhase::Prepublish,
                    )
                })
                .chain(resolver_config.publish.iter().map(|command| {
                    render_command(command, &context, &working_directory, CommandPhase::Publish)
                }))
                .collect::<anyhow::Result<Vec<_>>>()?;

            Ok(PackagePublish {
                preflight: resolver_config
                    .pre_check
                    .as_ref()
                    .map(|preflight| render_preflight(preflight, &context))
                    .transpose()?,
                commands,
                assets: resolve_assets(root, &package_config.assets)?,
                skip_reason: context
                    .package
                    .private
                    .then_some(PublishSkipReason::Private),
                context,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(PublishPlan { packages })
}

fn resolve_assets(root: &Path, configured: &[Asset]) -> anyhow::Result<Vec<ReleaseAsset>> {
    let mut assets = Vec::new();
    for asset in configured {
        match asset {
            Asset::Asset(asset) => {
                let path = root.join(&asset.path);
                if path.is_file() {
                    assets.push(ReleaseAsset {
                        path,
                        name: asset.name.clone(),
                    });
                }
            }
            Asset::String(pattern) => {
                let pattern = root.join(pattern).to_string_lossy().to_string();
                for path in glob::glob(&pattern)?
                    .flatten()
                    .filter(|path| path.is_file())
                {
                    let name = path.file_name().map_or_else(
                        || path.to_string_lossy().to_string(),
                        |name| name.to_string_lossy().to_string(),
                    );
                    assets.push(ReleaseAsset { path, name });
                }
            }
        }
    }
    assets.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(assets)
}

fn render_preflight(
    preflight: &PreCheckConfig,
    context: &PublishContext,
) -> anyhow::Result<PlannedRegistryCheck> {
    let url = render_template(&preflight.url, context)?;
    let parsed = reqwest::Url::parse(&url)?;
    anyhow::ensure!(
        matches!(parsed.scheme(), "http" | "https"),
        "registry preflight URL must use HTTP or HTTPS"
    );
    Ok(PlannedRegistryCheck {
        url,
        extra_headers: preflight.extra_headers.clone(),
    })
}

fn render_command(
    command: &CommandConfig,
    context: &PublishContext,
    working_directory: &Utf8PathBuf,
    phase: CommandPhase,
) -> anyhow::Result<CommandSpec> {
    let executable = render_template(&command.command, context)?;
    anyhow::ensure!(!executable.is_empty(), "rendered command must not be empty");
    let args = command
        .args
        .as_ref()
        .map(|args| {
            args.iter()
                .map(|argument| render_template(argument, context))
                .collect::<anyhow::Result<Vec<_>>>()
        })
        .transpose()?;
    anyhow::ensure!(
        !executable.contains('\0')
            && args
                .iter()
                .flatten()
                .all(|argument| !argument.contains('\0')),
        "rendered command contains a null byte"
    );

    Ok(CommandSpec {
        executable,
        args: args.unwrap_or_default(),
        environment: command.extra_env.clone(),
        working_directory: working_directory.clone(),
        phase,
        stdout: stdio_policy(command.stdout),
        stderr: stdio_policy(command.stderr),
        run_in_dry_run: command.dry_run.unwrap_or(false),
    })
}

const fn stdio_policy(stdio: StdioType) -> StdioPolicy {
    match stdio {
        StdioType::Inherit => StdioPolicy::Inherit,
        StdioType::Pipe => StdioPolicy::Pipe,
        StdioType::Null => StdioPolicy::Null,
    }
}

fn render_template(template: &str, publish: &PublishContext) -> anyhow::Result<String> {
    let mut environment = Environment::new();
    environment.set_undefined_behavior(UndefinedBehavior::Strict);
    Ok(environment.render_str(template, context!(package => &publish.package))?)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use semifold_resolver::{
        config::{BranchesConfig, PackageConfig, ReleaseChannel, ResolverConfig, StdioType},
        resolver::ResolverType,
    };

    use super::*;

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn temporary_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "semifold-publish-plan-{}-{nonce}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn package(path: &str) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Rust,
            channel: ReleaseChannel::Stable,
            channel_bump: None,
            assets: Vec::new(),
            depends_on: Vec::new(),
        }
    }

    fn resolver(pre_check: &str) -> ResolverConfig {
        ResolverConfig {
            pre_check: Some(PreCheckConfig {
                url: pre_check.to_string(),
                extra_headers: BTreeMap::new(),
            }),
            prepublish: Vec::new(),
            publish: vec![CommandConfig {
                command: "cargo".to_string(),
                args: Some(vec![
                    "publish".to_string(),
                    "--tag={{ package.tag }}".to_string(),
                ]),
                extra_env: BTreeMap::new(),
                stdout: StdioType::Inherit,
                stderr: StdioType::Inherit,
                dry_run: None,
            }],
            post_version: Vec::new(),
        }
    }

    fn config(pre_check: &str) -> Config {
        Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("app".to_string(), package("app")),
                ("core".to_string(), package("core")),
            ]),
            resolver: BTreeMap::from([(ResolverType::Rust, resolver(pre_check))]),
        }
    }

    fn write_workspace(root: &Path) {
        for (name, dependency) in [
            ("core", ""),
            (
                "app",
                "\n[dependencies]\ncore = { version = \"1\", path = \"../core\" }\n",
            ),
        ] {
            let package_root = root.join(name);
            fs::create_dir_all(&package_root).unwrap();
            fs::write(
                package_root.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"1.2.3\"\n{dependency}"),
            )
            .unwrap();
        }
    }

    #[test]
    fn publish_plan_is_rebuilt_from_current_packages_in_topological_order() {
        let root = temporary_root();
        write_workspace(&root);

        let plan = plan_publish(
            &root,
            &config("https://registry.test/{{ package.name }}/{{ package.version }}"),
        )
        .unwrap();

        assert_eq!(
            plan.packages
                .iter()
                .map(|package| package.context.package.id.as_str())
                .collect::<Vec<_>>(),
            ["core", "app"]
        );
        assert_eq!(
            plan.packages[0].preflight.as_ref().unwrap().url,
            "https://registry.test/core/1.2.3"
        );
        assert_eq!(plan.packages[0].commands[0].args[1], "--tag=core-v1.2.3");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn publish_templates_reject_release_scope_and_unknown_package_fields() {
        let root = temporary_root();
        write_workspace(&root);

        assert!(plan_publish(&root, &config("{{ release.plan.fingerprint }}")).is_err());
        assert!(plan_publish(&root, &config("{{ package.next_version }}")).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn publish_plan_rejects_invalid_preflight_urls() {
        let root = temporary_root();
        write_workspace(&root);

        assert!(plan_publish(&root, &config("not a URL")).is_err());
        assert!(plan_publish(&root, &config("file:///tmp/package")).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
