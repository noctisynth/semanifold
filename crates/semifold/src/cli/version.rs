use std::collections::{HashMap, VecDeque};

use clap::Parser;
use colored::Colorize;
use rust_i18n::t;
use semifold_changelog::{generate_changelog, utils::insert_changelog};
use semifold_resolver::{
    changeset::{BumpLevel, Changeset},
    config::{Config, ResolverConfig},
    context::Context,
    resolver::{self, Resolver, ResolverType, rust::RustResolver},
    utils,
};

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
    let config = ctx.config.as_ref().unwrap();
    let root = ctx.repo_root.as_ref().unwrap();
    let Some(repo) = ctx.git_repo.as_ref() else {
        return Err(anyhow::anyhow!(t!("cli.version.no_git_repo")));
    };
    let mut changelogs_map = HashMap::new();

    let mut sorted_packages = config.packages.clone().into_iter().collect::<Vec<_>>();
    for resolver in config.resolver.keys() {
        ctx.create_resolver(*resolver)
            .sort_packages(root, &mut sorted_packages)?;
    }
    let release_plan = release_bump_plan(root, config, changesets)?;
    let mut resolved_packages = HashMap::new();
    let mut version_map = HashMap::new();
    for (package_name, package_config) in &sorted_packages {
        let mut resolver = ctx.create_resolver(package_config.resolver);
        let resolved_package = resolver.resolve(root, package_config)?;
        let level = release_plan.levels[package_name];
        if level != BumpLevel::Unchanged {
            let mut next_version = resolved_package.version.clone();
            utils::bump_version(&mut next_version, level, &package_config.channel)?;
            version_map.insert(package_name.clone(), next_version);
        }
        resolved_packages.insert(package_name.clone(), resolved_package);
    }
    *ctx.version_bumps.borrow_mut() = version_map.clone();

    for (package_name, package_config) in &sorted_packages {
        log::debug!("Processing package: {}", package_name);
        let mut resolver = ctx.create_resolver(package_config.resolver);
        let resolved_package = resolved_packages.remove(package_name).unwrap();
        let level = release_plan.levels[package_name];

        // Skip unchanged packages
        if matches!(level, BumpLevel::Unchanged) {
            log::warn!(
                "{}",
                t!("cli.version.unchanged", package = package_name.cyan())
            );
            continue;
        }

        let bumped_version = version_map[package_name].clone();
        resolver.bump(ctx, root, &resolved_package, &bumped_version)?;

        let dependency_updates = release_plan
            .propagated_dependencies
            .get(package_name)
            .into_iter()
            .flatten()
            .filter_map(|dependency| {
                version_map
                    .get(dependency)
                    .map(|version| (dependency.clone(), version.to_string()))
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

        log::debug!("changelog for {}:\n{}", package_name, changelog);

        if !ctx.dry_run {
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

#[derive(Debug, Default, Eq, PartialEq)]
pub(crate) struct ReleaseBumpPlan {
    pub levels: HashMap<String, BumpLevel>,
    pub propagated_dependencies: HashMap<String, Vec<String>>,
}

struct Dependent {
    name: String,
    version_requirement: Option<semver::VersionReq>,
}

pub(crate) fn release_bump_plan(
    root: &std::path::Path,
    config: &Config,
    changesets: &[Changeset],
) -> anyhow::Result<ReleaseBumpPlan> {
    let mut levels = config
        .packages
        .keys()
        .map(|name| (name.clone(), utils::get_bump_level(changesets, name)))
        .collect::<HashMap<_, _>>();
    let mut dependents = HashMap::<String, Vec<Dependent>>::new();
    let mut propagated_dependencies = HashMap::<String, Vec<String>>::new();
    let mut current_versions = HashMap::new();
    for (package_name, package_config) in &config.packages {
        if package_config.resolver != ResolverType::Rust {
            continue;
        }
        current_versions.insert(
            package_name.clone(),
            RustResolver.resolve(root, package_config)?.version,
        );
        for dependency in RustResolver::runtime_dependencies(root, package_config)? {
            if config.packages.contains_key(&dependency.name) {
                dependents
                    .entry(dependency.name)
                    .or_default()
                    .push(Dependent {
                        name: package_name.clone(),
                        version_requirement: dependency.version_requirement,
                    });
            }
        }
    }

    let mut pending = levels
        .iter()
        .filter_map(|(name, level)| (*level != BumpLevel::Unchanged).then_some(name.clone()))
        .collect::<VecDeque<_>>();
    while let Some(package_name) = pending.pop_front() {
        let mut next_version = current_versions[&package_name].clone();
        utils::bump_version(
            &mut next_version,
            levels[&package_name],
            &config.packages[&package_name].channel,
        )?;

        for dependent in dependents.get(&package_name).into_iter().flatten() {
            if dependent
                .version_requirement
                .as_ref()
                .is_some_and(|requirement| requirement.matches(&next_version))
            {
                continue;
            }

            let level = levels.get_mut(&dependent.name).unwrap();
            if *level == BumpLevel::Unchanged {
                *level = BumpLevel::Patch;
                propagated_dependencies
                    .entry(dependent.name.clone())
                    .or_default()
                    .push(package_name.clone());
                pending.push_back(dependent.name.clone());
            }
        }
    }
    Ok(ReleaseBumpPlan {
        levels,
        propagated_dependencies,
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

    use semifold_resolver::{
        changeset::Changeset,
        config::{BranchesConfig, Config, PackageConfig, ReleaseChannel},
        resolver::ResolverType,
    };

    use super::{BumpLevel, release_bump_plan};

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

    fn package_config(path: &str) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Rust,
            channel: ReleaseChannel::Named("alpha".to_string()),
            assets: vec![],
        }
    }

    fn stable_package_config(path: &str) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Rust,
            channel: ReleaseChannel::Stable,
            assets: vec![],
        }
    }

    #[test]
    fn adds_transitive_rust_dependents_to_the_release_closure() {
        let root = temporary_root();
        for (path, manifest) in [
            (
                "resolver",
                "[package]\nname = \"resolver\"\nversion = \"0.3.5\"\n",
            ),
            (
                "changelog",
                "[package]\nname = \"changelog\"\nversion = \"0.2.1\"\n\n[dependencies]\nresolver = { version = \"0.3.5\", path = \"../resolver\" }\n",
            ),
            (
                "cli",
                "[package]\nname = \"cli\"\nversion = \"0.2.16\"\n\n[dependencies]\nchangelog = { version = \"0.2.1\", path = \"../changelog\" }\n",
            ),
        ] {
            let package_root = root.join(path);
            fs::create_dir_all(&package_root).unwrap();
            fs::write(package_root.join("Cargo.toml"), manifest).unwrap();
        }
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("resolver".to_string(), package_config("resolver")),
                ("changelog".to_string(), package_config("changelog")),
                ("cli".to_string(), package_config("cli")),
            ]),
            resolver: BTreeMap::new(),
        };
        let mut changeset = Changeset::new("resolver-feature".to_string(), &root);
        changeset.add_package("resolver".to_string(), BumpLevel::Minor, None);

        let release_plan = release_bump_plan(&root, &config, &[changeset]).unwrap();

        assert_eq!(release_plan.levels["resolver"], BumpLevel::Minor);
        assert_eq!(release_plan.levels["changelog"], BumpLevel::Patch);
        assert_eq!(release_plan.levels["cli"], BumpLevel::Patch);
        assert_eq!(
            release_plan.propagated_dependencies["changelog"],
            ["resolver"]
        );
        assert_eq!(release_plan.propagated_dependencies["cli"], ["changelog"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn does_not_propagate_when_the_runtime_requirement_accepts_the_next_version() {
        let root = temporary_root();
        for (path, manifest) in [
            (
                "resolver",
                "[package]\nname = \"resolver\"\nversion = \"0.3.5\"\n",
            ),
            (
                "changelog",
                "[package]\nname = \"changelog\"\nversion = \"0.2.1\"\n\n[dependencies]\nresolver = { version = \">=0.3.5, <0.4.0\", path = \"../resolver\" }\n",
            ),
        ] {
            let package_root = root.join(path);
            fs::create_dir_all(&package_root).unwrap();
            fs::write(package_root.join("Cargo.toml"), manifest).unwrap();
        }
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("resolver".to_string(), stable_package_config("resolver")),
                ("changelog".to_string(), stable_package_config("changelog")),
            ]),
            resolver: BTreeMap::new(),
        };
        let mut changeset = Changeset::new("resolver-patch".to_string(), &root);
        changeset.add_package("resolver".to_string(), BumpLevel::Patch, None);

        let release_plan = release_bump_plan(&root, &config, &[changeset]).unwrap();

        assert_eq!(release_plan.levels["resolver"], BumpLevel::Patch);
        assert_eq!(release_plan.levels["changelog"], BumpLevel::Unchanged);
        assert!(release_plan.propagated_dependencies.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_workspace_dependency_requirements_before_propagating() {
        let root = temporary_root();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.dependencies]\ncore = { version = \">=1.0.0, <2.0.0\", path = \"crates/core\" }\n",
        )
        .unwrap();
        for (path, manifest) in [
            (
                "crates/core",
                "[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
            ),
            (
                "crates/app",
                "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\ncore = { workspace = true }\n",
            ),
        ] {
            let package_root = root.join(path);
            fs::create_dir_all(&package_root).unwrap();
            fs::write(package_root.join("Cargo.toml"), manifest).unwrap();
        }
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("core".to_string(), stable_package_config("crates/core")),
                ("app".to_string(), stable_package_config("crates/app")),
            ]),
            resolver: BTreeMap::new(),
        };
        let mut changeset = Changeset::new("core-patch".to_string(), &root);
        changeset.add_package("core".to_string(), BumpLevel::Patch, None);

        let release_plan = release_bump_plan(&root, &config, &[changeset]).unwrap();

        assert_eq!(release_plan.levels["core"], BumpLevel::Patch);
        assert_eq!(release_plan.levels["app"], BumpLevel::Unchanged);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn does_not_propagate_dev_dependencies() {
        let root = temporary_root();
        for (path, manifest) in [
            (
                "resolver",
                "[package]\nname = \"resolver\"\nversion = \"0.3.5\"\n",
            ),
            (
                "consumer",
                "[package]\nname = \"consumer\"\nversion = \"0.2.1\"\n\n[dev-dependencies]\nresolver = { version = \"0.3.5\", path = \"../resolver\" }\n",
            ),
        ] {
            let package_root = root.join(path);
            fs::create_dir_all(&package_root).unwrap();
            fs::write(package_root.join("Cargo.toml"), manifest).unwrap();
        }
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("resolver".to_string(), package_config("resolver")),
                ("consumer".to_string(), package_config("consumer")),
            ]),
            resolver: BTreeMap::new(),
        };
        let mut changeset = Changeset::new("resolver-minor".to_string(), &root);
        changeset.add_package("resolver".to_string(), BumpLevel::Minor, None);

        let release_plan = release_bump_plan(&root, &config, &[changeset]).unwrap();

        assert_eq!(release_plan.levels["consumer"], BumpLevel::Unchanged);
        assert!(release_plan.propagated_dependencies.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
