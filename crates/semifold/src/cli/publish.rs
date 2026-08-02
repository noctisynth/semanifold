use std::collections::BTreeMap;

use clap::Parser;
use rust_i18n::t;
use semifold_changelog::read_latest_changelog;
use semifold_engine::{
    Project,
    publish_plan::plan_publish,
    publisher::{
        ForgeExecution, ForgeRelease, GithubForgeClient, HttpRegistryClient, PackageForgePlan,
        PublishReport, SystemAssetResolver, SystemCommandRunner, SystemFileSystem,
        execute_publish_plan,
    },
};

use crate::cli::version::{is_git_repo_clean, repository_context};

#[derive(Debug, Parser)]
pub(crate) struct Publish {
    #[clap(short = 'r', long, default_value_t = true, help = t!("cli.publish.flags.github_release"))]
    github_release: bool,
    #[clap(short = 'd', long, default_value_t = false, help = t!("cli.publish.flags.allow_dirty"))]
    allow_dirty: bool,
}

pub(crate) async fn publish(
    project: &Project,
    dry_run: bool,
    github_release: bool,
) -> anyhow::Result<PublishReport> {
    let config = &project.config;

    log::debug!(
        "Packages to publish: {:?}",
        config.packages.keys().collect::<Vec<_>>()
    );

    let should_create_github_release = std::env::var("GITHUB_ACTIONS").is_ok() && github_release;
    let root = project.root.as_std_path();
    let mut plan = plan_publish(root, config)
        .map_err(|error| anyhow::anyhow!(t!("cli.publish.plan_failed", error = error)))?;
    log::debug!("Packages to publish: {:?}", plan.packages);
    let mut changelogs = BTreeMap::new();
    for package in &plan.packages {
        if package.skip_reason.is_some() {
            continue;
        }
        let changelog_path = root
            .join(package.context.package.path.as_std_path())
            .join("CHANGELOG.md");
        changelogs.insert(
            package.context.package.id.clone(),
            read_latest_changelog(&changelog_path).await?,
        );
    }
    let forge_client = if should_create_github_release {
        let client = if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            octocrab::Octocrab::builder()
                .personal_token(token)
                .build()?
        } else {
            octocrab::Octocrab::default()
        };
        Some(GithubForgeClient::new(client))
    } else {
        None
    };
    let mut forge_packages = BTreeMap::new();
    if should_create_github_release {
        let repository = repository_context()
            .ok_or_else(|| anyhow::anyhow!(t!("cli.publish.repo_info_missing")))?;
        for package in &plan.packages {
            if package.skip_reason.is_some() {
                continue;
            }
            let changelog = changelogs.get(&package.context.package.id).ok_or_else(|| {
                anyhow::anyhow!(t!(
                    "cli.publish.changelog_missing_after_validation",
                    package = package.context.package.id.as_str()
                ))
            })?;
            forge_packages.insert(
                package.context.package.id.clone(),
                PackageForgePlan {
                    release: ForgeRelease {
                        owner: repository.owner.clone(),
                        repository: repository.name.clone(),
                        tag: package.context.package.tag.clone(),
                        title: format!("{} {}", package.context.package.name, changelog.version),
                        body: changelog.body.clone(),
                        prerelease: !package.context.package.version.pre.is_empty(),
                    },
                },
            );
        }
    }
    let file_system = SystemFileSystem;
    let asset_resolver = SystemAssetResolver;
    let forge = forge_client.as_ref().map(|client| ForgeExecution {
        client,
        file_system: &file_system,
        asset_resolver: &asset_resolver,
        root,
        packages: &forge_packages,
    });
    let registry_client = HttpRegistryClient::default();
    let report = execute_publish_plan(
        &mut plan,
        &SystemCommandRunner,
        &registry_client,
        forge,
        dry_run,
    )
    .await?;

    Ok(report)
}

pub(crate) async fn run(opts: &Publish, project: &Project, dry_run: bool) -> anyhow::Result<()> {
    if !opts.allow_dirty && !is_git_repo_clean(project)? {
        return Err(anyhow::anyhow!(t!("cli.dirty_repo")));
    }

    let _report = publish(project, dry_run, opts.github_release).await?;

    Ok(())
}
