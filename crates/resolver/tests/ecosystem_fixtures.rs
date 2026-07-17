use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use insta::assert_snapshot;
use semifold_resolver::{
    config::{PackageConfig, ReleaseChannel},
    context::Context,
    resolver::{
        ResolvedPackage, Resolver, ResolverType, cpp::CppResolver, nodejs::NodejsResolver,
        python::PythonResolver, rust::RustResolver,
    },
};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn temp_dir(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "semifold-resolver-fixture-{test_name}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

fn fixture(path: &str) -> PathBuf {
    Path::new(FIXTURES).join(path)
}

fn copy_fixture(source: &str, destination: &Path) {
    let source = fixture(source);
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::copy(source, destination).unwrap();
}

fn package(path: &str, name: &str) -> ResolvedPackage {
    ResolvedPackage {
        name: name.to_string(),
        version: semver::Version::parse("1.0.0").unwrap(),
        path: PathBuf::from(path),
        private: false,
    }
}

fn config(path: &str, resolver: ResolverType) -> PackageConfig {
    PackageConfig {
        path: path.into(),
        resolver,
        channel: ReleaseChannel::Stable,
        assets: vec![],
    }
}

fn render_packages(mut packages: Vec<ResolvedPackage>) -> String {
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    packages
        .into_iter()
        .map(|package| {
            format!(
                "name = {}\nversion = {}\npath = {}\nprivate = {}",
                package.name,
                package.version,
                package.path.display(),
                package.private
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[test]
fn rust_manifest_parsing_matches_snapshot() {
    let root = temp_dir("rust-parse");
    copy_fixture("rust/workspace.Cargo.toml", &root.join("Cargo.toml"));
    copy_fixture("rust/core.Cargo.toml", &root.join("crates/core/Cargo.toml"));
    copy_fixture("rust/app.Cargo.toml", &root.join("crates/app/Cargo.toml"));

    assert_snapshot!(
        "rust_manifest_parsing",
        render_packages(RustResolver.resolve_all(&root).unwrap())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn node_manifest_parsing_matches_snapshot() {
    let root = temp_dir("node-parse");
    let manifest = root.join("package.json");
    copy_fixture("node/app.before.json", &manifest);

    assert_snapshot!(
        "node_manifest_parsing",
        render_packages(vec![
            NodejsResolver
                .resolve(&root, &config(".", ResolverType::Nodejs))
                .unwrap()
        ])
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn python_manifest_parsing_matches_snapshot() {
    let root = temp_dir("python-parse");
    let manifest = root.join("pyproject.toml");
    copy_fixture("python/app.before.toml", &manifest);

    assert_snapshot!(
        "python_manifest_parsing",
        render_packages(vec![
            PythonResolver
                .resolve(&root, &config(".", ResolverType::Python))
                .unwrap()
        ])
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cpp_manifest_parsing_matches_snapshot() {
    let root = temp_dir("cpp-parse");
    let manifest = root.join("CMakeLists.txt");
    copy_fixture("cpp/CMakeLists.before.txt", &manifest);

    assert_snapshot!(
        "cpp_manifest_parsing",
        render_packages(vec![
            CppResolver
                .resolve(&root, &config(".", ResolverType::Cpp))
                .unwrap()
        ])
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cpp_workspace_fixture_discovers_members_and_orders_dependencies() {
    let root = temp_dir("cpp-workspace");
    copy_fixture("cpp/workspace.CMakeLists.txt", &root.join("CMakeLists.txt"));
    copy_fixture(
        "cpp/core.CMakeLists.txt",
        &root.join("libraries/core/CMakeLists.txt"),
    );
    copy_fixture(
        "cpp/app.CMakeLists.txt",
        &root.join("applications/app/CMakeLists.txt"),
    );

    assert_snapshot!(
        "cpp_workspace_manifest_parsing",
        render_packages(CppResolver.resolve_all(&root).unwrap())
    );

    let mut packages = vec![
        (
            "app".to_string(),
            config("applications/app", ResolverType::Cpp),
        ),
        (
            "core".to_string(),
            config("libraries/core", ResolverType::Cpp),
        ),
    ];
    CppResolver.sort_packages(&root, &mut packages).unwrap();
    assert_eq!(
        packages
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        vec!["core", "app"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rust_manifest_rewrite_matches_snapshot() {
    let root = temp_dir("rust-golden");
    let manifest = root.join("crates/app/Cargo.toml");
    copy_fixture("rust/app.before.toml", &manifest);

    let context = Context::default();
    context
        .version_bumps
        .borrow_mut()
        .insert("core".to_string(), semver::Version::parse("1.1.0").unwrap());
    RustResolver
        .bump(
            &context,
            &root,
            &package("crates/app", "app"),
            &semver::Version::parse("1.0.1").unwrap(),
        )
        .unwrap();

    assert_snapshot!(
        "rust_manifest_rewrite",
        fs::read_to_string(manifest).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn node_manifest_rewrite_matches_snapshot() {
    let root = temp_dir("node-golden");
    let manifest = root.join("packages/app/package.json");
    copy_fixture("node/app.before.json", &manifest);

    NodejsResolver
        .bump(
            &Context::default(),
            &root,
            &package("packages/app", "app"),
            &semver::Version::parse("1.0.1").unwrap(),
        )
        .unwrap();

    assert_snapshot!(
        "node_manifest_rewrite",
        fs::read_to_string(manifest).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn python_manifest_rewrite_matches_snapshots() {
    let root = temp_dir("python-golden");
    let manifest = root.join("packages/app/pyproject.toml");
    let init = root.join("packages/app/src/app/__init__.py");
    copy_fixture("python/app.before.toml", &manifest);
    copy_fixture("python/init.before.py", &init);

    PythonResolver
        .bump(
            &Context::default(),
            &root,
            &package("packages/app", "app"),
            &semver::Version::parse("1.0.1").unwrap(),
        )
        .unwrap();

    assert_snapshot!(
        "python_manifest_rewrite",
        fs::read_to_string(manifest).unwrap()
    );
    assert_snapshot!(
        "python_static_version_rewrite",
        fs::read_to_string(init).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cpp_manifest_rewrite_matches_snapshots() {
    let root = temp_dir("cpp-golden");
    let cmake = root.join("CMakeLists.txt");
    let vcpkg = root.join("vcpkg.json");
    copy_fixture("cpp/CMakeLists.before.txt", &cmake);
    copy_fixture("cpp/vcpkg.before.json", &vcpkg);

    CppResolver
        .bump(
            &Context::default(),
            &root,
            &package(".", "example"),
            &semver::Version::parse("1.0.1").unwrap(),
        )
        .unwrap();

    assert_snapshot!(
        "cpp_cmake_manifest_rewrite",
        fs::read_to_string(cmake).unwrap()
    );
    assert_snapshot!(
        "cpp_vcpkg_manifest_rewrite",
        fs::read_to_string(vcpkg).unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rust_workspace_dependency_fixture_is_discovered_and_retained() {
    let root = temp_dir("rust-workspace-dependency");
    copy_fixture("rust/workspace.Cargo.toml", &root.join("Cargo.toml"));
    copy_fixture("rust/core.Cargo.toml", &root.join("crates/core/Cargo.toml"));
    copy_fixture("rust/app.Cargo.toml", &root.join("crates/app/Cargo.toml"));

    let mut packages = RustResolver.resolve_all(&root).unwrap();
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    assert_eq!(
        packages
            .iter()
            .map(|package| &package.name)
            .collect::<Vec<_>>(),
        vec!["app", "core"]
    );

    let dependencies =
        RustResolver::internal_dependencies(&root, &config("crates/app", ResolverType::Rust))
            .unwrap();
    assert_eq!(dependencies, vec!["core"]);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn rust_single_and_private_package_fixtures_match_snapshot() {
    let root = temp_dir("rust-single-private");
    copy_fixture("rust/single.Cargo.toml", &root.join("single/Cargo.toml"));
    copy_fixture("rust/private.Cargo.toml", &root.join("private/Cargo.toml"));

    assert_snapshot!(
        "rust_single_and_private_packages",
        render_packages(vec![
            RustResolver
                .resolve(&root, &config("single", ResolverType::Rust))
                .unwrap(),
            RustResolver
                .resolve(&root, &config("private", ResolverType::Rust))
                .unwrap(),
        ])
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn node_npm_and_pnpm_workspace_fixtures_match_snapshot() {
    let root = temp_dir("node-workspaces");
    copy_fixture("node/npm.root.package.json", &root.join("npm/package.json"));
    copy_fixture(
        "node/core.package.json",
        &root.join("npm/packages/core/package.json"),
    );
    copy_fixture(
        "node/private.package.json",
        &root.join("npm/packages/private/package.json"),
    );
    copy_fixture(
        "node/pnpm.root.package.json",
        &root.join("pnpm/package.json"),
    );
    copy_fixture(
        "node/pnpm-workspace.yaml",
        &root.join("pnpm/pnpm-workspace.yaml"),
    );
    copy_fixture(
        "node/core.package.json",
        &root.join("pnpm/packages/core/package.json"),
    );
    copy_fixture(
        "node/private.package.json",
        &root.join("pnpm/packages/private/package.json"),
    );

    assert_snapshot!(
        "node_npm_workspace_parsing",
        render_packages(NodejsResolver.resolve_all(&root.join("npm")).unwrap())
    );
    assert_snapshot!(
        "node_pnpm_workspace_parsing",
        render_packages(NodejsResolver.resolve_all(&root.join("pnpm")).unwrap())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn node_peer_dependency_fixture_orders_dependency_before_dependent() {
    let root = temp_dir("node-peer-dependency");
    copy_fixture(
        "node/core.package.json",
        &root.join("packages/core/package.json"),
    );
    copy_fixture(
        "node/peer-app.package.json",
        &root.join("packages/app/package.json"),
    );

    let mut packages = vec![
        (
            "app".to_string(),
            config("packages/app", ResolverType::Nodejs),
        ),
        (
            "core".to_string(),
            config("packages/core", ResolverType::Nodejs),
        ),
    ];
    NodejsResolver.sort_packages(&root, &mut packages).unwrap();

    assert_eq!(
        packages
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        vec!["core", "app"]
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn python_metadata_and_monorepo_fixtures_match_snapshot() {
    let root = temp_dir("python-monorepo");
    copy_fixture("python/root.pyproject.toml", &root.join("pyproject.toml"));
    copy_fixture(
        "python/pep.pyproject.toml",
        &root.join("packages/pep/pyproject.toml"),
    );
    copy_fixture(
        "python/poetry.pyproject.toml",
        &root.join("libs/poetry/pyproject.toml"),
    );
    copy_fixture(
        "python/hatch.pyproject.toml",
        &root.join("apps/hatch/pyproject.toml"),
    );
    copy_fixture(
        "python/hatch.init.py",
        &root.join("apps/hatch/src/hatch/__init__.py"),
    );
    copy_fixture("python/setup.cfg", &root.join("packages/cfg/setup.cfg"));

    assert_snapshot!(
        "python_monorepo_manifest_parsing",
        render_packages(PythonResolver.resolve_all(&root).unwrap())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn node_dependency_fixture_orders_dependency_before_dependent() {
    let root = temp_dir("node-dependencies");
    copy_fixture(
        "node/core.package.json",
        &root.join("packages/core/package.json"),
    );
    copy_fixture(
        "node/app.package.json",
        &root.join("packages/app/package.json"),
    );

    let mut packages = vec![
        (
            "app".to_string(),
            config("packages/app", ResolverType::Nodejs),
        ),
        (
            "core".to_string(),
            config("packages/core", ResolverType::Nodejs),
        ),
    ];
    NodejsResolver.sort_packages(&root, &mut packages).unwrap();

    assert_eq!(
        packages
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        vec!["core", "app"]
    );
    fs::remove_dir_all(root).unwrap();
}
