use std::{fs, io::Read};

use bytes::Bytes;
use clap::Parser;
use colored::Colorize;
use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use rust_i18n::t;
use serde::Serialize;

use semifold_changelog::read_latest_changelog;
use semifold_resolver::{
    adapter::PackageLocation,
    config::{Config, PackageConfig, ResolverConfig},
    context::Context,
    utils,
};

use crate::{
    discovery::ResolverRegistry, package_path::normalize_package_path,
    workspace::load_workspace_graph,
};

#[derive(Debug, Serialize)]
struct PublishPackage {
    name: String,
    version: semver::Version,
    path: std::path::PathBuf,
    private: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct Publish {
    #[clap(short = 'r', long, default_value_t = true, help = t!("cli.publish.flags.github_release"))]
    github_release: bool,
    #[clap(short = 'd', long, default_value_t = false, help = t!("cli.publish.flags.allow_dirty"))]
    allow_dirty: bool,
}

pub(crate) fn is_release_exists_error(errors: &[serde_json::Value]) -> anyhow::Result<bool> {
    let Some(error) = errors.first() else {
        return Ok(false);
    };
    let error = error
        .as_object()
        .ok_or(anyhow::anyhow!("Invalid error format"))?;
    let Some(code) = error.get("code") else {
        return Ok(false);
    };
    let code = code.as_str().ok_or(anyhow::anyhow!("Invalid error code"))?;
    Ok(code == "already_exists")
}

pub(crate) async fn create_github_release(
    ctx: &Context,
    octocrab: &octocrab::Octocrab,
    package_name: &str,
    package_config: &PackageConfig,
) -> anyhow::Result<Option<octocrab::models::repos::Release>> {
    let Some(repo_info) = &ctx.repo_info else {
        return Err(anyhow::anyhow!("Repo info not found"));
    };

    let changelog_path = package_config.path.join("CHANGELOG.md");
    if !changelog_path.exists() {
        log::warn!(
            "Changelog file not found for package {}, skip create GitHub release",
            package_name.cyan()
        );
        return Ok(None);
    }

    let changelog = read_latest_changelog(&changelog_path).await?;
    let tag_name = format!("{}-{}", package_name, changelog.version);
    let release_title = format!("{} {}", package_name, changelog.version);
    let version = semver::Version::parse(&changelog.version[1..])?;

    log::debug!("Tag name: {}", tag_name);
    log::debug!("Changelog for {}:\n\n{}", package_name, changelog.body);

    match octocrab
        .repos(&repo_info.owner, &repo_info.repo_name)
        .releases()
        .create(&tag_name)
        .name(&release_title)
        .body(&changelog.body)
        .prerelease(!version.pre.is_empty())
        .send()
        .await
    {
        Ok(release) => Ok(Some(release)),
        Err(octocrab::Error::GitHub { source, .. }) => {
            if source.status_code == StatusCode::UNPROCESSABLE_ENTITY {
                if is_release_exists_error(&source.errors.clone().unwrap_or_default())? {
                    log::warn!("GitHub release already exists, skip create");
                    return Ok(None);
                }
                log::warn!("Failed to create GitHub release: {:?}", source);
                Ok(None)
            } else {
                Err(anyhow::anyhow!(
                    "Failed to create GitHub release: {:?}",
                    source
                ))
            }
        }
        Err(e) => Err(anyhow::anyhow!("Failed to create GitHub release: {:?}", e)),
    }
}

async fn pre_check(
    resolver_config: &ResolverConfig,
    resolved_package: &PublishPackage,
) -> anyhow::Result<bool> {
    let url = minijinja::render!(
        &resolver_config.pre_check.url,
        package => &resolved_package,
    );
    log::debug!("Pre-check URL: {}", url);
    let client = reqwest::Client::new();
    let headers = resolver_config.pre_check.extra_headers.iter().try_fold(
        HeaderMap::new(),
        |mut acc, (key, value)| {
            let header_name = HeaderName::from_bytes(key.as_bytes())
                .map_err(|e| anyhow::anyhow!("Invalid header name: {:?}", e))?;
            let header_value = HeaderValue::from_str(value)
                .map_err(|e| anyhow::anyhow!("Invalid header value: {:?}", e))?;
            acc.insert(header_name, header_value);
            Ok::<_, anyhow::Error>(acc)
        },
    )?;
    let resp = client.get(url).headers(headers).send().await?;
    log::debug!("Pre-check response: {:?}", resp);
    Ok(resp.status() == StatusCode::OK)
}

fn package_publish_order(root: &std::path::Path, config: &Config) -> anyhow::Result<Vec<String>> {
    Ok(load_workspace_graph(root, config)?
        .topological_order()?
        .into_iter()
        .map(|package| package.to_string())
        .collect())
}

fn command_display(command: &semifold_resolver::config::CommandConfig) -> String {
    let args = command.args.as_deref().unwrap_or_default();
    format!("{} {}", command.command, args.join(" "))
        .trim_end()
        .to_string()
}

fn run_publish_commands(
    package: &PublishPackage,
    resolver_config: &ResolverConfig,
    dry_run: bool,
) -> Result<(), semifold_resolver::error::ResolveError> {
    log::info!(
        "{}",
        t!(
            "cli.publish.run_prepublish_commands",
            package = package.name.cyan()
        )
    );
    for command in &resolver_config.prepublish {
        let display = command_display(command);
        if dry_run && !command.dry_run.unwrap_or(false) {
            log::warn!(
                "{}",
                t!(
                    "cli.publish.skip_prepublish_command",
                    command = display.magenta(),
                    package = package.name.cyan()
                )
            );
            continue;
        }
        log::info!(
            "{}",
            t!(
                "cli.publish.run_prepublish_command",
                command = display.magenta(),
                package = package.name.cyan()
            )
        );
        utils::run_command(command, &package.path)?;
    }

    log::info!(
        "{}",
        t!(
            "cli.publish.run_publish_commands",
            package = package.name.cyan()
        )
    );
    for command in &resolver_config.publish {
        let display = command_display(command);
        if dry_run && !command.dry_run.unwrap_or(false) {
            log::warn!(
                "{}",
                t!(
                    "cli.publish.skip_publish_command",
                    command = display.magenta(),
                    package = package.name.cyan()
                )
            );
            continue;
        }
        log::info!(
            "{}",
            t!(
                "cli.publish.run_publish_command",
                command = display.magenta(),
                package = package.name.cyan()
            )
        );
        utils::run_command(command, &package.path)?;
    }
    Ok(())
}

pub(crate) async fn publish(ctx: &Context, github_release: bool) -> anyhow::Result<()> {
    let config = ctx
        .config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(t!("cli.not_initialized")))?;

    log::debug!(
        "Packages to publish: {:?}",
        config.packages.keys().collect::<Vec<_>>()
    );

    let should_create_github_release = ctx.is_ci() && github_release;

    let octocrab = if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        octocrab::Octocrab::builder()
            .personal_token(token)
            .build()?
    } else {
        octocrab::Octocrab::default()
    };

    let root = ctx.repo_root.clone().unwrap_or(std::env::current_dir()?);
    let project_root = camino::Utf8PathBuf::from_path_buf(root.clone())
        .map_err(|_| anyhow::anyhow!(t!("cli.publish.non_utf8_project_root")))?;
    let registry = ResolverRegistry;
    let publish_order = package_publish_order(&root, config)?;
    log::debug!("Package publish order: {:?}", publish_order);

    for package_name in &publish_order {
        let package = config
            .packages
            .get(package_name)
            .expect("publish order is derived from configured packages");
        let resolver_config = config
            .resolver
            .get(&package.resolver)
            .ok_or(anyhow::anyhow!(
                "Config for resolver {} not found",
                package.resolver
            ))?;
        log::debug!("Resolver config: {:?}", resolver_config);

        let inspection = registry
            .create_adapter(package.resolver)
            .inspect(&PackageLocation {
                id: semifold_core::PackageId::new(package_name),
                project_root: project_root.clone(),
                path: normalize_package_path(&root, &package.path)?,
            })?;
        let resolved_package = PublishPackage {
            name: inspection.manifest_name,
            version: inspection.version,
            path: inspection.path.into_std_path_buf(),
            private: !inspection.publishable,
        };
        log::debug!("Resolved package: {}", resolved_package.name);

        if pre_check(resolver_config, &resolved_package).await? {
            log::warn!(
                "{}",
                t!(
                    "cli.publish.pre_check",
                    package = package_name.cyan(),
                    version = format!("v{}", resolved_package.version).green()
                )
            );
            continue;
        }

        if !resolved_package.private {
            run_publish_commands(&resolved_package, resolver_config, ctx.dry_run)?;
        } else {
            log::warn!(
                "{}",
                t!(
                    "cli.publish.skip_private",
                    package = package_name.cyan(),
                    version = format!("v{}", resolved_package.version).green()
                )
            );
        }

        let assets = ctx.get_assets(package_name)?;
        log::debug!("Assets: {:?}", assets);

        if should_create_github_release {
            let Some(repo_info) = &ctx.repo_info else {
                return Err(anyhow::anyhow!("Repo info not found"));
            };

            if !ctx.dry_run {
                let Some(release) =
                    create_github_release(ctx, &octocrab, package_name, package).await?
                else {
                    log::warn!(
                        "Failed to create GitHub release for {} {}",
                        package_name.cyan(),
                        format!("v{}", resolved_package.version).green()
                    );
                    continue;
                };

                for asset in assets {
                    log::info!(
                        "Uploading asset: {} from {}",
                        asset.name,
                        asset.path.display()
                    );
                    if asset.path.exists() && asset.path.is_file() {
                        let mut file = fs::File::open(&asset.path)?;
                        let mut bytes = Vec::new();
                        file.read_to_end(&mut bytes)?;
                        let bytes = Bytes::from(bytes);
                        octocrab
                            .repos(&repo_info.owner, &repo_info.repo_name)
                            .releases()
                            .upload_asset(release.id.0, &asset.name, bytes)
                            .send()
                            .await?;
                    } else if !asset.path.is_file() {
                        log::warn!("Asset {} is not a file, skip upload", asset.path.display());
                    } else {
                        log::warn!("Asset {} not found, skip upload", asset.path.display());
                    }
                }
            } else {
                log::warn!(
                    "Skipped creating GitHub release for {} {} due to dry run",
                    package_name.cyan(),
                    format!("v{}", resolved_package.version).green()
                );
                log::warn!("Skipped uploading assets: {:?}", assets);
            }
        }
    }

    Ok(())
}

pub(crate) async fn run(opts: &Publish, ctx: &Context) -> anyhow::Result<()> {
    if !ctx.is_initialized() {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    if !opts.allow_dirty && !ctx.is_git_repo_clean() {
        return Err(anyhow::anyhow!(t!("cli.dirty_repo")));
    }

    publish(ctx, opts.github_release).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use semifold_resolver::{
        config::{
            BranchesConfig, CommandConfig, PackageConfig, PreCheckConfig, ReleaseChannel,
            ResolverConfig, StdioType,
        },
        resolver::ResolverType,
    };

    use super::{Config, PublishPackage, package_publish_order, run_publish_commands};

    fn temporary_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "semifold-publish-order-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn package(path: &str) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Rust,
            channel: ReleaseChannel::Stable,
            assets: vec![],
            depends_on: vec![],
        }
    }

    #[test]
    fn publishes_each_dependency_before_its_transitive_dependents() {
        let root = temporary_root();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n",
        )
        .unwrap();
        for (name, dependencies) in [
            ("core", ""),
            (
                "resolver",
                "\n[dependencies]\ncore = { version = \"1\", path = \"../core\" }\n",
            ),
            (
                "changelog",
                "\n[dependencies]\nresolver = { version = \"1\", path = \"../resolver\" }\n",
            ),
            (
                "semifold",
                "\n[dependencies]\nchangelog = { version = \"1\", path = \"../changelog\" }\n",
            ),
        ] {
            let package_root = root.join("crates").join(name);
            fs::create_dir_all(&package_root).unwrap();
            fs::write(
                package_root.join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"1.0.0\"\n{dependencies}"),
            )
            .unwrap();
        }
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("changelog".to_string(), package("crates/changelog")),
                ("core".to_string(), package("crates/core")),
                ("resolver".to_string(), package("crates/resolver")),
                ("semifold".to_string(), package("crates/semifold")),
            ]),
            resolver: BTreeMap::new(),
        };

        assert_eq!(
            package_publish_order(&root, &config).unwrap(),
            ["core", "resolver", "changelog", "semifold"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn command(script: &str, dry_run: bool) -> CommandConfig {
        CommandConfig {
            command: "sh".to_string(),
            args: Some(vec!["-c".to_string(), script.to_string()]),
            extra_env: BTreeMap::new(),
            stdout: StdioType::Null,
            stderr: StdioType::Null,
            dry_run: Some(dry_run),
        }
    }

    fn resolver_config(prepublish: CommandConfig, publish: CommandConfig) -> ResolverConfig {
        ResolverConfig {
            pre_check: PreCheckConfig {
                url: String::new(),
                extra_headers: BTreeMap::new(),
            },
            prepublish: vec![prepublish],
            publish: vec![publish],
            post_version: vec![],
        }
    }

    #[test]
    fn unified_publish_commands_preserve_phase_order_and_dry_run_rules() {
        let root = temporary_root();
        let package = PublishPackage {
            name: "example".to_string(),
            version: semver::Version::new(1, 0, 0),
            path: root.clone(),
            private: false,
        };
        let commands = resolver_config(
            command("printf 'prepublish\\n' >> commands.log", false),
            command("printf 'publish\\n' >> commands.log", true),
        );

        run_publish_commands(&package, &commands, false).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("commands.log")).unwrap(),
            "prepublish\npublish\n"
        );

        fs::remove_file(root.join("commands.log")).unwrap();
        run_publish_commands(&package, &commands, true).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("commands.log")).unwrap(),
            "publish\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unified_publish_commands_stop_before_publish_after_prepublish_failure() {
        let root = temporary_root();
        let package = PublishPackage {
            name: "example".to_string(),
            version: semver::Version::new(1, 0, 0),
            path: root.clone(),
            private: false,
        };
        let commands = resolver_config(
            command("exit 7", false),
            command("touch publish-marker", false),
        );

        let error = run_publish_commands(&package, &commands, false).unwrap_err();

        assert!(matches!(
            error,
            semifold_resolver::error::ResolveError::CommandError { code: Some(7), .. }
        ));
        assert!(!root.join("publish-marker").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
