use std::{fs, io::Read};

use bytes::Bytes;
use clap::Parser;
use colored::Colorize;
use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use rust_i18n::t;

use semifold_changelog::read_latest_changelog;
use semifold_resolver::{
    config::{PackageConfig, ResolverConfig},
    context::Context,
    resolver::ResolvedPackage,
};

#[derive(Debug, Parser)]
pub(crate) struct Publish {
    #[clap(short = 'r', long, default_value_t = true, help = t!("cli.publish.flags.github_release"))]
    github_release: bool,
    #[clap(short = 'd', long, default_value_t = false, help = t!("cli.publish.flags.allow_dirty"))]
    allow_dirty: bool,
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
            &package_name.cyan()
        );
        return Ok(None);
    }

    let changelog = read_latest_changelog(&changelog_path).await?;
    let tag_name = format!("{}-{}", package_name, changelog.version);
    let release_title = format!("{} {}", package_name, changelog.version);
    let version = semver::Version::parse(&changelog.version[1..])?;

    log::debug!("Tag name: {}", &tag_name);
    log::debug!("Changelog for {}:\n\n{}", &package_name, &changelog.body);

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

pub(crate) async fn pre_check(
    resolver_config: &ResolverConfig,
    resolved_package: &ResolvedPackage,
) -> anyhow::Result<bool> {
    let url = minijinja::render!(
        &resolver_config.pre_check.url,
        package => &resolved_package,
    );
    log::debug!("Pre-check URL: {}", &url);
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
    log::debug!("Pre-check response: {:?}", &resp);
    Ok(resp.status() == StatusCode::OK)
}

pub(crate) async fn publish(ctx: &Context, github_release: bool) -> anyhow::Result<()> {
    let config = ctx.config.as_ref().unwrap();

    log::debug!(
        "Packages to publish: {:?}",
        &config.packages.keys().collect::<Vec<_>>()
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
    let mut sorted_packages = config.packages.clone().into_iter().collect::<Vec<_>>();
    for resolver in config.resolver.keys() {
        ctx.create_resolver(*resolver)
            .sort_packages(&root, &mut sorted_packages)?;
    }
    log::debug!("Sorted packages: {:?}", &sorted_packages);

    for (package_name, package) in &sorted_packages {
        let resolver_config = config
            .resolver
            .get(&package.resolver)
            .ok_or(anyhow::anyhow!(
                "Config for resolver {} not found",
                &package.resolver
            ))?;
        log::debug!("Resolver config: {:?}", &resolver_config);

        let mut resolver = ctx.create_resolver(package.resolver);
        let resolved_package = resolver.resolve(&root, package)?;
        log::debug!("Resolved package: {}", &resolved_package.name);

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
            resolver.publish(&resolved_package, resolver_config, ctx.dry_run)?;
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
                        &package_name.cyan(),
                        &format!("v{}", resolved_package.version).green()
                    );
                    continue;
                };

                let assets = ctx.get_assets(package_name)?;
                for asset in assets {
                    log::info!(
                        "Uploading asset: {} from {}",
                        &asset.name,
                        &asset.path.display()
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
                        log::warn!("Asset {} is not a file, skip upload", &asset.path.display());
                    } else {
                        log::warn!("Asset {} not found, skip upload", &asset.path.display());
                    }
                }
            } else {
                log::warn!(
                    "Skipped creating GitHub release for {} {} due to dry run",
                    &package_name.cyan(),
                    &format!("v{}", resolved_package.version).green()
                );
                log::warn!(
                    "Skipped uploading assets: {:?}",
                    &ctx.get_assets(package_name)
                );
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
