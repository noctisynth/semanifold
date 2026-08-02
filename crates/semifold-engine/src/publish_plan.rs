use std::{collections::BTreeMap, path::Path};

use camino::Utf8PathBuf;
use minijinja::{Environment, UndefinedBehavior, context};
use semifold_changelog::read_latest_changelog;
use semifold_core::{CiContext, Ecosystem, PackageId, RepositoryContext};
use semifold_resolver::config::{Asset, CommandConfig, Config, PreCheckConfig, StdioType};
use semver::Version;
use serde::Serialize;

use crate::workspace::load_workspace_graph;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublishContext {
    pub package: PublishPackageContext,
    pub repository: Option<RepositoryContext>,
    pub ci: Option<CiContext>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublishPackageContext {
    pub id: PackageId,
    pub name: String,
    pub ecosystem: Ecosystem,
    pub version: Version,
    pub tag: String,
    pub path: Utf8PathBuf,
    pub private: bool,
}

#[derive(Debug)]
pub struct PublishPlan {
    pub project_root: Utf8PathBuf,
    pub packages: Vec<PackagePublish>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PublishOptions {
    pub create_forge_release: bool,
    pub repository: Option<RepositoryContext>,
}

#[derive(Debug)]
pub struct PackagePublish {
    pub context: PublishContext,
    pub preflight: Option<PlannedRegistryCheck>,
    pub commands: Vec<CommandSpec>,
    pub assets: Vec<AssetDeclaration>,
    pub forge: Option<PackageForgePlan>,
    pub skip_reason: Option<PublishSkipReason>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageForgePlan {
    pub release: ForgeRelease,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForgeRelease {
    pub owner: String,
    pub repository: String,
    pub tag: String,
    pub title: String,
    pub body: String,
    pub prerelease: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetDeclaration {
    Path {
        path: std::path::PathBuf,
        name: String,
    },
    Glob {
        pattern: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedRegistryCheck {
    pub url: String,
    pub extra_headers: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishSkipReason {
    Private,
    MissingChangelog,
    RegistryVersionExists,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandPhase {
    Prepublish,
    Publish,
    PostVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StdioPolicy {
    Inherit,
    Pipe,
    Null,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub executable: String,
    pub args: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub working_directory: Utf8PathBuf,
    pub phase: CommandPhase,
    pub stdout: StdioPolicy,
    pub stderr: StdioPolicy,
    pub run_in_dry_run: bool,
}

pub async fn plan_publish(
    root: &Path,
    config: &Config,
    options: &PublishOptions,
) -> anyhow::Result<PublishPlan> {
    if options.create_forge_release && options.repository.is_none() {
        anyhow::bail!("repository context is required to create Forge releases");
    }
    let graph = load_workspace_graph(root, config)?;
    let order = graph.topological_order()?;
    let mut packages = Vec::with_capacity(order.len());
    for id in order {
        let snapshot = graph
            .package(&id)
            .ok_or_else(|| anyhow::anyhow!("workspace graph package {id} is missing"))?;
        let package_config = config
            .packages
            .get(id.as_str())
            .ok_or_else(|| anyhow::anyhow!("configured package {id} is missing"))?;
        let resolver_config = config
            .resolver
            .get(&package_config.resolver)
            .ok_or_else(|| {
                anyhow::anyhow!("resolver config is missing for {}", package_config.resolver)
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
            repository: options.repository.clone(),
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

        let skip_reason = if context.package.private {
            Some(PublishSkipReason::Private)
        } else if !root
            .join(context.package.path.as_std_path())
            .join("CHANGELOG.md")
            .is_file()
        {
            Some(PublishSkipReason::MissingChangelog)
        } else {
            None
        };
        let forge = if options.create_forge_release && skip_reason.is_none() {
            let repository = options.repository.as_ref().ok_or_else(|| {
                anyhow::anyhow!("repository context is required to plan a Forge release")
            })?;
            let changelog_path = root
                .join(context.package.path.as_std_path())
                .join("CHANGELOG.md");
            let changelog = read_latest_changelog(changelog_path).await?;
            Some(PackageForgePlan {
                release: ForgeRelease {
                    owner: repository.owner.clone(),
                    repository: repository.name.clone(),
                    tag: context.package.tag.clone(),
                    title: format!("{} {}", context.package.name, changelog.version),
                    body: changelog.body,
                    prerelease: !context.package.version.pre.is_empty(),
                },
            })
        } else {
            None
        };

        packages.push(PackagePublish {
            preflight: resolver_config
                .pre_check
                .as_ref()
                .map(|preflight| render_preflight(preflight, &context))
                .transpose()?,
            commands,
            assets: plan_assets(&package_config.assets)?,
            forge,
            skip_reason,
            context,
        });
    }

    let project_root = Utf8PathBuf::from_path_buf(root.to_path_buf())
        .map_err(|_| anyhow::anyhow!("project root is not valid UTF-8"))?;
    Ok(PublishPlan {
        project_root,
        packages,
    })
}

fn plan_assets(configured: &[Asset]) -> anyhow::Result<Vec<AssetDeclaration>> {
    let mut assets = Vec::new();
    for asset in configured {
        match asset {
            Asset::Asset(asset) => {
                validate_asset_path(&asset.path)?;
                anyhow::ensure!(
                    !asset.name.is_empty(),
                    "release asset name must not be empty"
                );
                assets.push(AssetDeclaration::Path {
                    path: asset.path.clone(),
                    name: asset.name.clone(),
                });
            }
            Asset::String(pattern) => {
                validate_asset_path(Path::new(pattern))?;
                glob::Pattern::new(pattern).map_err(|error| {
                    anyhow::anyhow!("invalid release asset glob {pattern}: {error}")
                })?;
                assets.push(AssetDeclaration::Glob {
                    pattern: pattern.clone(),
                });
            }
        }
    }
    Ok(assets)
}

fn validate_asset_path(path: &Path) -> anyhow::Result<()> {
    let valid = !path.as_os_str().is_empty()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        });
    anyhow::ensure!(
        valid,
        "release asset path must stay within the project: {}",
        path.display()
    );
    Ok(())
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
            fs::write(
                package_root.join("CHANGELOG.md"),
                "# Changelog\n\n## v1.2.3\n\n- Changes\n",
            )
            .unwrap();
        }
    }

    #[tokio::test]
    async fn publish_plan_is_rebuilt_from_current_packages_in_topological_order() {
        let root = temporary_root();
        write_workspace(&root);

        let plan = plan_publish(
            &root,
            &config("https://registry.test/{{ package.name }}/{{ package.version }}"),
            &PublishOptions::default(),
        )
        .await
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

    #[tokio::test]
    async fn publish_templates_reject_release_scope_and_unknown_package_fields() {
        let root = temporary_root();
        write_workspace(&root);

        assert!(
            plan_publish(
                &root,
                &config("{{ release.plan.fingerprint }}"),
                &PublishOptions::default()
            )
            .await
            .is_err()
        );
        assert!(
            plan_publish(
                &root,
                &config("{{ package.next_version }}"),
                &PublishOptions::default()
            )
            .await
            .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn publish_plan_rejects_invalid_preflight_urls() {
        let root = temporary_root();
        write_workspace(&root);

        assert!(
            plan_publish(&root, &config("not a URL"), &PublishOptions::default())
                .await
                .is_err()
        );
        assert!(
            plan_publish(
                &root,
                &config("file:///tmp/package"),
                &PublishOptions::default()
            )
            .await
            .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn publish_plan_rejects_assets_outside_the_project() {
        let root = temporary_root();
        write_workspace(&root);
        let mut config = config("https://registry.test/{{ package.name }}/{{ package.version }}");
        config
            .packages
            .get_mut("core")
            .expect("core package exists in the test configuration")
            .assets = vec![Asset::String("../artifact.tar.gz".to_string())];

        assert!(
            plan_publish(&root, &config, &PublishOptions::default())
                .await
                .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn publish_plan_skips_packages_without_changelogs() {
        let root = temporary_root();
        write_workspace(&root);
        fs::remove_file(root.join("core/CHANGELOG.md")).unwrap();

        let plan = plan_publish(
            &root,
            &config("https://registry.test/{{ package.name }}/{{ package.version }}"),
            &PublishOptions::default(),
        )
        .await
        .unwrap();

        assert_eq!(
            plan.packages[0].skip_reason,
            Some(PublishSkipReason::MissingChangelog)
        );
        assert_eq!(plan.packages[1].skip_reason, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn forge_release_is_fully_planned_from_the_latest_changelog() {
        let root = temporary_root();
        write_workspace(&root);
        let repository = RepositoryContext {
            host: "https://github.com".to_string(),
            owner: "semifold".to_string(),
            name: "semifold".to_string(),
            web_url: "https://github.com/semifold/semifold".to_string(),
            commit: None,
        };

        let plan = plan_publish(
            &root,
            &config("https://registry.test/{{ package.name }}/{{ package.version }}"),
            &PublishOptions {
                create_forge_release: true,
                repository: Some(repository),
            },
        )
        .await
        .expect("Forge publish plan must be created");

        let core = plan
            .packages
            .first()
            .expect("core package must be first in the publish plan");
        let forge = core
            .forge
            .as_ref()
            .expect("publishable package must contain a Forge plan");
        assert_eq!(forge.release.owner, "semifold");
        assert_eq!(forge.release.tag, "core-v1.2.3");
        assert_eq!(forge.release.title, "core v1.2.3");
        assert_eq!(forge.release.body, "## v1.2.3\n\n- Changes");
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn forge_planning_requires_repository_context() {
        let root = temporary_root();
        write_workspace(&root);

        let error = plan_publish(
            &root,
            &config("https://registry.test/{{ package.name }}/{{ package.version }}"),
            &PublishOptions {
                create_forge_release: true,
                repository: None,
            },
        )
        .await
        .expect_err("Forge planning without repository context must fail");

        assert!(error.to_string().contains("repository context"));
        fs::remove_dir_all(root).unwrap();
    }
}
