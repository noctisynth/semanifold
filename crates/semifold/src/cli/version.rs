use std::{collections::HashMap, path::Path};

use anyhow::Context as _;
use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;
use colored::Colorize;
use rust_i18n::t;
use semifold_changelog::{
    generate_changelog,
    utils::{insert_changelog, render_changelog},
};
use semifold_core::{
    EditSource, FileEdit, FileEditExpectation, FileHash, PackageId, ReleaseReason,
};
use semifold_resolver::{
    changeset::Changeset,
    config::{PackageConfig, ResolverConfig},
    context::Context,
    resolver, utils,
};

use crate::{file_edit_executor::FileEditExecutor, release::plan_release};

#[derive(Parser, Debug)]
pub(crate) struct Version {
    #[clap(long, help = t!("cli.version.flags.allow_dirty"))]
    allow_dirty: bool,
}

pub(crate) fn post_version(ctx: &Context) -> anyhow::Result<()> {
    let packages = ctx.get_packages();
    for (package_name, package_config) in packages {
        let resolver_config = ctx.get_resolver_config(package_config.resolver);
        if let Some(ResolverConfig { post_version, .. }) = &resolver_config {
            for command in post_version {
                let args = command.args.as_deref().unwrap_or_default();
                if ctx.dry_run && !command.dry_run.unwrap_or(false) {
                    log::warn!(
                        "{}",
                        t!(
                            "cli.version.skip_post_version",
                            command = format!("{} {}", command.command, args.join(" ")).magenta(),
                            package = package_name.cyan()
                        )
                    );
                    continue;
                }

                log::info!(
                    "{}",
                    t!(
                        "cli.version.run_post_version",
                        command = format!("{} {}", command.command, args.join(" ")).magenta(),
                        package = package_name.cyan()
                    )
                );
                utils::run_command(command, &package_config.path)?;
            }
        } else {
            log::warn!(
                "{}",
                t!(
                    "cli.version.no_resolver_config",
                    resolver = package_config.resolver.to_string().cyan(),
                    package = package_name.cyan()
                )
            );
        }
    }
    Ok(())
}

pub(crate) async fn version(
    ctx: &Context,
    changesets: &[Changeset],
) -> anyhow::Result<HashMap<String, String>> {
    let config = ctx
        .config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(t!("cli.not_initialized")))?;
    let root = ctx
        .repo_root
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(t!("cli.version.no_git_repo")))?;
    let Some(repo) = ctx.git_repo.as_ref() else {
        return Err(anyhow::anyhow!(t!("cli.version.no_git_repo")));
    };
    let mut changelogs_map = HashMap::new();

    let release_plan = plan_release(root, config, changesets)?;
    let edit_root = Utf8Path::from_path(root).context(t!("cli.version.edit_non_utf8_root"))?;
    if ctx.dry_run {
        FileEditExecutor::new(edit_root).validate(release_plan.file_edits())?;
        return Ok(HashMap::new());
    }

    let version_map = release_plan
        .packages()
        .iter()
        .map(|package| {
            (
                package.id.as_str().to_string(),
                package.next_version.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
    let mut file_edits = release_plan.file_edits().to_vec();
    for package_id in release_plan.order() {
        let has_planned_manifest_edit = release_plan.file_edits().iter().any(|edit| {
            matches!(
                &edit.source,
                EditSource::PackageVersion { package } if package == package_id
            )
        });
        if !has_planned_manifest_edit {
            continue;
        }
        let package_name = package_id.as_str();
        let package_config = config.packages.get(package_name).ok_or_else(|| {
            anyhow::anyhow!(t!(
                "cli.version.plan_package_missing",
                package = package_name
            ))
        })?;
        let package_release = release_plan.package(package_id).ok_or_else(|| {
            anyhow::anyhow!(t!(
                "cli.version.plan_package_missing",
                package = package_name
            ))
        })?;
        let dependency_updates = package_release
            .reasons
            .iter()
            .filter_map(|reason| match reason {
                ReleaseReason::DependencyPropagation { dependency, .. } => version_map
                    .get(dependency.as_str())
                    .map(|version| (dependency.as_str().to_string(), version.to_string())),
                ReleaseReason::Changeset { .. } => None,
            })
            .collect::<Vec<_>>();
        let changelog = generate_changelog(
            ctx,
            repo,
            changesets,
            package_name,
            &package_release.next_version.to_string(),
            &dependency_updates,
        )
        .await?;
        file_edits.push(plan_changelog_edit(
            root,
            package_config,
            package_id,
            &changelog,
        )?);
        changelogs_map.insert(package_name.to_string(), changelog);
    }
    FileEditExecutor::new(edit_root).apply(&file_edits)?;

    for package_id in release_plan.order() {
        let package_name = package_id.as_str();
        let package_config = config.packages.get(package_name).ok_or_else(|| {
            anyhow::anyhow!(t!(
                "cli.version.plan_package_missing",
                package = package_name
            ))
        })?;
        let package_release = release_plan.package(package_id).ok_or_else(|| {
            anyhow::anyhow!(t!(
                "cli.version.plan_package_missing",
                package = package_name
            ))
        })?;
        let bumped_version = package_release.next_version.clone();
        let has_planned_edit = release_plan.file_edits().iter().any(|edit| {
            matches!(
                &edit.source,
                EditSource::PackageVersion { package } if package == package_id
            )
        });
        if !has_planned_edit {
            log::debug!("Processing package: {}", package_name);
            let mut resolver = ctx.create_resolver(package_config.resolver);
            let resolved_package = resolver.resolve(root, package_config)?;
            resolver.bump(ctx, root, &resolved_package, &bumped_version)?;

            let dependency_updates = package_release
                .reasons
                .iter()
                .filter_map(|reason| match reason {
                    ReleaseReason::DependencyPropagation { dependency, .. } => version_map
                        .get(dependency.as_str())
                        .map(|version| (dependency.as_str().to_string(), version.to_string())),
                    ReleaseReason::Changeset { .. } => None,
                })
                .collect::<Vec<_>>();
            let changelog = generate_changelog(
                ctx,
                repo,
                changesets,
                package_name,
                &bumped_version.to_string(),
                &dependency_updates,
            )
            .await?;
            changelogs_map.insert(package_name.to_string(), changelog.clone());
            insert_changelog(
                root.join(&package_config.path).join("CHANGELOG.md"),
                &changelog,
            )
            .await?;
        }
    }

    post_version(ctx)?;
    if !ctx.dry_run {
        changesets.iter().try_for_each(|c| c.clean())?;
    }

    Ok(changelogs_map)
}

fn plan_changelog_edit(
    root: &Path,
    package_config: &PackageConfig,
    package: &PackageId,
    entry: &str,
) -> anyhow::Result<FileEdit> {
    let relative_path = package_config.path.join("CHANGELOG.md");
    let path = Utf8PathBuf::from_path_buf(relative_path.clone())
        .map_err(|_| anyhow::anyhow!(t!("cli.version.edit_non_utf8_path")))?;
    let absolute_path = root.join(&relative_path);
    let content = match std::fs::read_to_string(&absolute_path) {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let expected = content
        .as_ref()
        .map_or(FileEditExpectation::Missing, |content| {
            FileEditExpectation::Existing {
                hash: FileHash::from_bytes(content.as_bytes()),
            }
        });
    let new_content = render_changelog(&absolute_path, content.as_deref(), entry)?;

    Ok(FileEdit {
        path,
        expected,
        new_content,
        source: EditSource::Changelog {
            package: package.clone(),
        },
    })
}

pub(crate) async fn run(opts: &Version, ctx: &Context) -> anyhow::Result<()> {
    if !ctx.is_initialized() {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    if !opts.allow_dirty && !ctx.is_git_repo_clean() {
        return Err(anyhow::anyhow!(t!("cli.dirty_repo")));
    }

    let changesets = resolver::get_changesets(ctx)?;
    if changesets.is_empty() {
        log::warn!("{}", t!("cli.version.empty_changesets"));
        return Ok(());
    }

    version(ctx, &changesets).await?;

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

    use semifold_core::{FileEditExpectation, PackageId};
    use semifold_resolver::{
        changeset::Changeset,
        config::{BranchesConfig, Config, PackageConfig, ReleaseChannel},
        resolver::ResolverType,
    };

    use super::plan_changelog_edit;
    use crate::release::plan_release;

    fn temporary_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "semifold-version-closure-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn package_config(path: &str, resolver: ResolverType) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver,
            channel: ReleaseChannel::Stable,
            assets: vec![],
        }
    }

    #[test]
    fn plans_a_node_changeset_in_a_mixed_workspace_without_a_rust_only_lookup() {
        let root = temporary_root();
        fs::create_dir_all(root.join("rust")).unwrap();
        fs::write(
            root.join("rust/Cargo.toml"),
            "[package]\nname = \"rust-lib\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("node")).unwrap();
        fs::write(
            root.join("node/package.json"),
            r#"{"name":"node-lib","version":"1.0.0"}"#,
        )
        .unwrap();
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                (
                    "node-lib".to_string(),
                    package_config("node", ResolverType::Nodejs),
                ),
                (
                    "rust-lib".to_string(),
                    package_config("rust", ResolverType::Rust),
                ),
            ]),
            resolver: BTreeMap::new(),
        };
        let mut changeset = Changeset::new("node-patch".to_string(), &root);
        changeset.add_package(
            "node-lib".to_string(),
            semifold_resolver::changeset::BumpLevel::Patch,
            None,
        );

        let release_plan = plan_release(&root, &config, &[changeset]).unwrap();

        assert_eq!(release_plan.order(), [PackageId::new("node-lib")]);
        assert_eq!(
            release_plan
                .package(&PackageId::new("node-lib"))
                .unwrap()
                .next_version,
            semver::Version::new(1, 0, 1)
        );
        assert_eq!(release_plan.file_edits().len(), 1);
        assert_eq!(
            release_plan.file_edits()[0].path.as_str(),
            "node/package.json"
        );
        assert!(
            release_plan.file_edits()[0]
                .new_content
                .contains("\"version\": \"1.0.1\"")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_new_and_existing_changelog_edits() {
        let root = temporary_root();
        let package = package_config("package", ResolverType::Nodejs);
        fs::create_dir_all(root.join("package")).unwrap();

        let new_edit =
            plan_changelog_edit(&root, &package, &PackageId::new("app"), "## v1.0.0").unwrap();
        assert_eq!(new_edit.path.as_str(), "package/CHANGELOG.md");
        assert_eq!(new_edit.expected, FileEditExpectation::Missing);
        assert_eq!(new_edit.new_content, "# Changelog\n\n## v1.0.0\n");

        fs::write(root.join("package/CHANGELOG.md"), "# Changelog\n").unwrap();
        let existing_edit =
            plan_changelog_edit(&root, &package, &PackageId::new("app"), "## v1.0.0").unwrap();
        assert!(matches!(
            existing_edit.expected,
            FileEditExpectation::Existing { .. }
        ));
        assert_eq!(existing_edit.new_content, "# Changelog\n\n## v1.0.0\n");
        fs::remove_dir_all(root).unwrap();
    }
}
