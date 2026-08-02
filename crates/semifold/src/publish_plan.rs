use std::{collections::BTreeMap, path::Path};

use camino::Utf8PathBuf;
use minijinja::{Environment, UndefinedBehavior, context};
use semifold_core::{CiContext, Ecosystem, PackageId, RepositoryContext};
use semifold_resolver::config::{Asset, CommandConfig, Config, PreCheckConfig};
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
    pub preflight: PlannedRegistryCheck,
    pub prepublish: Vec<CommandConfig>,
    pub publish: Vec<CommandConfig>,
    pub assets: Vec<Asset>,
    pub skip_reason: Option<PublishSkipReason>,
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

            Ok(PackagePublish {
                preflight: render_preflight(&resolver_config.pre_check, &context)?,
                prepublish: resolver_config
                    .prepublish
                    .iter()
                    .map(|command| render_command(command, &context))
                    .collect::<anyhow::Result<_>>()?,
                publish: resolver_config
                    .publish
                    .iter()
                    .map(|command| render_command(command, &context))
                    .collect::<anyhow::Result<_>>()?,
                assets: package_config.assets.clone(),
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
) -> anyhow::Result<CommandConfig> {
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

    Ok(CommandConfig {
        command: executable,
        args,
        extra_env: command.extra_env.clone(),
        stdout: command.stdout,
        stderr: command.stderr,
        dry_run: command.dry_run,
    })
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
            pre_check: PreCheckConfig {
                url: pre_check.to_string(),
                extra_headers: BTreeMap::new(),
            },
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
            plan.packages[0].preflight.url,
            "https://registry.test/core/1.2.3"
        );
        assert_eq!(
            plan.packages[0].publish[0].args.as_ref().unwrap()[1],
            "--tag=core-v1.2.3"
        );
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
