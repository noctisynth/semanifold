use std::{collections::BTreeMap, path::Path};

use semifold_core::{
    BumpLevel, ChangesetId, ChangesetInput, DependencyKind, Ecosystem, PackageId,
    PackageReleasePolicy, ReleaseChannel, ReleasePlan, ReleasePlanner, ReleasePolicies,
    WorkspaceGraph,
};
use semifold_resolver::{
    adapter::{EcosystemAdapter, EcosystemPlanInput},
    changeset::{BumpLevel as ResolverBumpLevel, Changeset},
    config::{Config, ReleaseChannel as ResolverReleaseChannel},
    resolver::{
        cpp::CppResolver, nodejs::NodejsResolver, python::PythonResolver, rust::RustResolver,
    },
};
use semver::VersionReq;

use crate::workspace::load_workspace_graph;

/// Builds the immutable release plan from the current migration-layer inputs.
pub(crate) fn plan_release(
    root: &Path,
    config: &Config,
    changesets: &[Changeset],
) -> anyhow::Result<ReleasePlan> {
    let graph = load_workspace_graph(root, config)?;
    let changesets = changeset_inputs(changesets);
    let policies = release_policies(&graph, config)?;
    let plan = ReleasePlanner::plan(&graph, &changesets, &policies)?;
    let rust_workspace_packages = graph
        .packages()
        .filter(|package| package.ecosystem == Ecosystem::Rust)
        .cloned()
        .collect::<Vec<_>>();
    let rust_released_packages = plan
        .packages()
        .iter()
        .filter(|release| release.ecosystem == Ecosystem::Rust)
        .map(|release| release.id.clone())
        .collect::<Vec<_>>();
    let project_root = camino::Utf8Path::from_path(root)
        .ok_or_else(|| anyhow::anyhow!("project root is not valid UTF-8"))?;
    let mut file_edits = RustResolver.plan_edits(EcosystemPlanInput {
        project_root,
        workspace_packages: &rust_workspace_packages,
        released_packages: &rust_released_packages,
        versions: plan.versions(),
    })?;
    let node_workspace_packages = graph
        .packages()
        .filter(|package| package.ecosystem == Ecosystem::Node)
        .cloned()
        .collect::<Vec<_>>();
    let node_released_packages = plan
        .packages()
        .iter()
        .filter(|release| release.ecosystem == Ecosystem::Node)
        .map(|release| release.id.clone())
        .collect::<Vec<_>>();
    file_edits.extend(NodejsResolver.plan_edits(EcosystemPlanInput {
        project_root,
        workspace_packages: &node_workspace_packages,
        released_packages: &node_released_packages,
        versions: plan.versions(),
    })?);
    let python_workspace_packages = graph
        .packages()
        .filter(|package| package.ecosystem == Ecosystem::Python)
        .cloned()
        .collect::<Vec<_>>();
    let python_released_packages = plan
        .packages()
        .iter()
        .filter(|release| release.ecosystem == Ecosystem::Python)
        .map(|release| release.id.clone())
        .collect::<Vec<_>>();
    file_edits.extend(PythonResolver.plan_edits(EcosystemPlanInput {
        project_root,
        workspace_packages: &python_workspace_packages,
        released_packages: &python_released_packages,
        versions: plan.versions(),
    })?);
    let cpp_workspace_packages = graph
        .packages()
        .filter(|package| package.ecosystem == Ecosystem::Cpp)
        .cloned()
        .collect::<Vec<_>>();
    let cpp_released_packages = plan
        .packages()
        .iter()
        .filter(|release| release.ecosystem == Ecosystem::Cpp)
        .map(|release| release.id.clone())
        .collect::<Vec<_>>();
    file_edits.extend(CppResolver.plan_edits(EcosystemPlanInput {
        project_root,
        workspace_packages: &cpp_workspace_packages,
        released_packages: &cpp_released_packages,
        versions: plan.versions(),
    })?);
    Ok(plan.with_file_edits(file_edits)?)
}

fn changeset_inputs(changesets: &[Changeset]) -> Vec<ChangesetInput> {
    changesets
        .iter()
        .map(|changeset| {
            let mut releases = BTreeMap::<PackageId, BumpLevel>::new();
            for package in &changeset.packages {
                let level = bump_level(package.level);
                releases
                    .entry(PackageId::new(&package.name))
                    .and_modify(|current| *current = (*current).max(level))
                    .or_insert(level);
            }
            ChangesetInput {
                id: ChangesetId::new(&changeset.name),
                releases,
            }
        })
        .collect()
}

fn release_policies(graph: &WorkspaceGraph, config: &Config) -> anyhow::Result<ReleasePolicies> {
    graph
        .packages()
        .map(|package| {
            let package_config = &config.packages[package.id.as_str()];
            let propagating_dependencies = if package.ecosystem == Ecosystem::Rust {
                package
                    .dependencies
                    .iter()
                    .filter(|dependency| dependency.kind == DependencyKind::Runtime)
                    .map(|dependency| {
                        let requirement = dependency
                            .requirement
                            .as_deref()
                            .map(VersionReq::parse)
                            .transpose()?;
                        Ok((dependency.package.clone(), requirement))
                    })
                    .collect::<anyhow::Result<BTreeMap<_, _>>>()?
            } else {
                BTreeMap::new()
            };
            Ok((
                package.id.clone(),
                PackageReleasePolicy {
                    channel: release_channel(&package_config.channel),
                    propagating_dependencies,
                },
            ))
        })
        .collect()
}

const fn bump_level(level: ResolverBumpLevel) -> BumpLevel {
    match level {
        ResolverBumpLevel::Major => BumpLevel::Major,
        ResolverBumpLevel::Minor => BumpLevel::Minor,
        ResolverBumpLevel::Patch => BumpLevel::Patch,
        ResolverBumpLevel::Unchanged => BumpLevel::Unchanged,
    }
}

fn release_channel(channel: &ResolverReleaseChannel) -> ReleaseChannel {
    match channel {
        ResolverReleaseChannel::Stable => ReleaseChannel::Stable,
        ResolverReleaseChannel::Named(name) => ReleaseChannel::Named(name.clone()),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use semifold_core::{PackageId, ReleaseReason};
    use semifold_resolver::{
        changeset::Changeset,
        config::{BranchesConfig, PackageConfig},
        resolver::ResolverType,
    };

    use super::*;

    static NEXT_TEMPORARY_ROOT: AtomicU64 = AtomicU64::new(0);

    fn temporary_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "semifold-release-plan-{}-{nonce}-{}",
            std::process::id(),
            NEXT_TEMPORARY_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn package(path: &str) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Rust,
            channel: ResolverReleaseChannel::Stable,
            assets: vec![],
        }
    }

    fn python_package(path: &str) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Python,
            channel: ResolverReleaseChannel::Stable,
            assets: vec![],
        }
    }

    #[test]
    fn bridges_resolver_inputs_into_the_core_release_plan() {
        let root = temporary_root();
        for (path, manifest) in [
            ("core", "[package]\nname = \"core\"\nversion = \"1.0.0\"\n"),
            (
                "app",
                "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\ncore = { version = \"^1.0.0\", path = \"../core\" }\n",
            ),
        ] {
            fs::create_dir_all(root.join(path)).unwrap();
            fs::write(root.join(path).join("Cargo.toml"), manifest).unwrap();
        }
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("app".to_string(), package("app")),
                ("core".to_string(), package("core")),
            ]),
            resolver: BTreeMap::new(),
        };
        let mut changeset = Changeset::new("core-major".to_string(), &root);
        changeset.add_package("core".to_string(), ResolverBumpLevel::Major, None);

        let plan = plan_release(&root, &config, &[changeset]).unwrap();

        assert_eq!(
            plan.order(),
            [PackageId::new("core"), PackageId::new("app")]
        );
        let app = plan.package(&PackageId::new("app")).unwrap();
        assert_eq!(app.bump, BumpLevel::Patch);
        assert!(matches!(
            app.reasons.as_slice(),
            [ReleaseReason::DependencyPropagation { dependency, .. }]
                if dependency == &PackageId::new("core")
        ));
        assert_eq!(
            plan.file_edits()
                .iter()
                .map(|edit| edit.path.as_str())
                .collect::<Vec<_>>(),
            ["app/Cargo.toml", "core/Cargo.toml"]
        );
        assert!(
            plan.file_edits()[0]
                .new_content
                .contains("version = \"2.0.0\"")
        );
        assert!(
            plan.file_edits()[1]
                .new_content
                .contains("version = \"2.0.0\"")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_one_shared_workspace_dependency_edit_for_aliased_package_ids() {
        let root = temporary_root();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"core\"]\n\n[workspace.dependencies]\ncore-alias = { package = \"core\", version = \"^1.0.0\", path = \"core\" }\n",
        )
        .unwrap();
        for (path, manifest) in [
            ("core", "[package]\nname = \"core\"\nversion = \"1.0.0\"\n"),
            (
                "app",
                "[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\ncore-alias = { workspace = true }\n",
            ),
        ] {
            fs::create_dir_all(root.join(path)).unwrap();
            fs::write(root.join(path).join("Cargo.toml"), manifest).unwrap();
        }
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([
                ("app-id".to_string(), package("app")),
                ("core-id".to_string(), package("core")),
            ]),
            resolver: BTreeMap::new(),
        };
        let mut changeset = Changeset::new("core-major".to_string(), &root);
        changeset.add_package("core-id".to_string(), ResolverBumpLevel::Major, None);

        let plan = plan_release(&root, &config, &[changeset]).unwrap();

        assert_eq!(
            plan.order(),
            [PackageId::new("core-id"), PackageId::new("app-id")]
        );
        assert_eq!(
            plan.file_edits()
                .iter()
                .map(|edit| edit.path.as_str())
                .collect::<Vec<_>>(),
            ["Cargo.toml", "app/Cargo.toml", "core/Cargo.toml"]
        );
        let workspace_edit = plan
            .file_edits()
            .iter()
            .find(|edit| edit.path == "Cargo.toml")
            .unwrap();
        assert!(
            workspace_edit.new_content.contains(
                "core-alias = { package = \"core\", version = \"2.0.0\", path = \"core\" }"
            )
        );
        assert_eq!(
            plan.file_edits()
                .iter()
                .filter(|edit| edit.path == "Cargo.toml")
                .count(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dynamic_python_version_can_plan_a_release_without_writing_cargo() {
        let root = temporary_root();
        fs::write(
            root.join("pyproject.toml"),
            "[project]\nname = \"native-example\"\ndynamic = [\"version\"]\n",
        )
        .unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"native-example\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        let config = Config {
            branches: BranchesConfig {
                base: "main".to_string(),
                release: "release".to_string(),
            },
            tags: BTreeMap::new(),
            packages: BTreeMap::from([("native-example".to_string(), python_package("."))]),
            resolver: BTreeMap::new(),
        };
        let mut changeset = Changeset::new("python-patch".to_string(), &root);
        changeset.add_package("native-example".to_string(), ResolverBumpLevel::Patch, None);

        let plan = plan_release(&root, &config, &[changeset]).unwrap();

        assert_eq!(
            plan.package(&PackageId::new("native-example"))
                .unwrap()
                .next_version,
            semver::Version::new(1, 0, 1)
        );
        assert!(plan.file_edits().is_empty());
        assert!(
            fs::read_to_string(root.join("Cargo.toml"))
                .unwrap()
                .contains("version = \"1.0.0\"")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
