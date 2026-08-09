use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use camino::Utf8PathBuf;
use semifold_core::{DependencyKind, EcosystemId, FileHash, PackageId};
use semifold_engine::{
    config_sync::{config_sync_scope, plan_config_sync},
    discovery::PackageDiscoveryService,
    release::plan_release,
    workspace::load_workspace_graph,
};
use semifold_resolver::{
    changeset::{BumpLevel, Changeset},
    config::load_config_from_str,
};

struct TemporaryRoot(PathBuf);

impl TemporaryRoot {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "semifold-plugin-ecosystem-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("plugin fixture root must be created");
        Self(path)
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(
            path.parent()
                .expect("plugin fixture file must have a parent directory"),
        )
        .expect("plugin fixture parent must be created");
        fs::write(path, content).expect("plugin fixture file must be written");
    }
}

impl Drop for TemporaryRoot {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("plugin fixture root must be removed");
    }
}

fn plugin_source(engine_hash: &str, game_hash: &str) -> String {
    format!(
        r#"
        export const metadata = {{
            "schema-version": 1,
            ecosystem: "com.example.game",
            "plugin-version": "1.0.0",
            operations: ["discover", "inspect", "plan-edits"],
            "read-patterns": ["*/manifest.json"]
        }};

        async function inspectPackage(id, path, host) {{
            const manifest = JSON.parse(await host.readText(`${{path}}/manifest.json`));
            return {{
                id,
                "manifest-name": manifest.name,
                version: manifest.version,
                "version-source": {{ kind: "package-manifest" }},
                ecosystem: "com.example.game",
                path,
                publishable: true,
                dependencies: Object.entries(manifest.dependencies ?? {{}}).map(
                    ([name, requirement]) => ({{
                        "manifest-name": name,
                        kind: "runtime",
                        requirement
                    }})
                )
            }};
        }}

        export default async function(request, host) {{
            let output;
            if (request.operation === "discover") {{
                const manifests = await host.listFiles("*/manifest.json");
                const packages = await Promise.all(manifests.map(async manifestPath => {{
                    const path = manifestPath.slice(0, -"/manifest.json".length);
                    return inspectPackage(path, path, host);
                }}));
                output = {{ packages }};
            }} else if (request.operation === "inspect") {{
                const location = request.input.package;
                output = {{ package: await inspectPackage(location.id, location.path, host) }};
            }} else {{
                const hashes = {{
                    "engine/manifest.json": "{engine_hash}",
                    "game/manifest.json": "{game_hash}"
                }};
                const edits = [];
                for (const packageId of request.input["released-packages"]) {{
                    const snapshot = request.input["workspace-packages"].find(
                        candidate => candidate.id === packageId
                    );
                    const path = `${{snapshot.path}}/manifest.json`;
                    const manifest = JSON.parse(await host.readText(path));
                    manifest.version = request.input.versions[packageId];
                    for (const dependency of snapshot.dependencies) {{
                        const version = request.input.versions[dependency.package];
                        if (version !== undefined && manifest.dependencies?.[dependency.package]) {{
                            manifest.dependencies[dependency.package] = `^${{version}}`;
                        }}
                    }}
                    edits.push({{
                        path,
                        expected: {{ kind: "existing", sha256: hashes[path] }},
                        "new-content": `${{JSON.stringify(manifest, null, 2)}}\n`,
                        source: {{ kind: "package-version", package: packageId }}
                    }});
                }}
                output = {{ edits }};
            }}
            return {{
                "schema-version": 1,
                diagnostics: [],
                status: "success",
                output: {{ operation: request.operation, output }}
            }};
        }};
        "#
    )
}

fn config() -> &'static str {
    r#"
[branches]
base = "main"
release = "release"

[tags]
feat = "Features"

[plugins."com.example.game"]
path = "plugins/game.js"

[packages.engine]
path = "engine"
resolver = "com.example.game"

[packages.game]
path = "game"
resolver = "com.example.game"

[resolver]
"#
}

#[test]
fn dynamic_plugin_drives_discovery_workspace_dependencies_and_version_edits() {
    let root = TemporaryRoot::new();
    let engine_manifest = "{\"name\":\"engine\",\"version\":\"1.0.0\",\"dependencies\":{}}\n";
    let game_manifest =
        "{\"name\":\"game\",\"version\":\"1.0.0\",\"dependencies\":{\"engine\":\"^1.0.0\"}}\n";
    root.write("engine/manifest.json", engine_manifest);
    root.write("game/manifest.json", game_manifest);
    root.write(
        "plugins/game.js",
        &plugin_source(
            FileHash::from_bytes(engine_manifest.as_bytes()).as_str(),
            FileHash::from_bytes(game_manifest.as_bytes()).as_str(),
        ),
    );
    let config = load_config_from_str(Path::new("config.toml"), config())
        .expect("dynamic plugin config must load");
    let ecosystem = EcosystemId::new("com.example.game").expect("fixture ecosystem id is valid");

    let discovery = PackageDiscoveryService::from_config(&root.0, &config)
        .expect("plugin registry must load")
        .discover(&root.0, std::slice::from_ref(&ecosystem))
        .expect("plugin discovery must succeed");
    assert_eq!(
        discovery
            .packages
            .iter()
            .map(|package| package.id.clone())
            .collect::<Vec<_>>(),
        [PackageId::new("engine"), PackageId::new("game")]
    );
    let sync_scope = config_sync_scope(&config, std::slice::from_ref(&ecosystem))
        .expect("dynamic plugin sync scope must be accepted");
    let sync_plan = plan_config_sync(
        &root.0,
        &root.0.join(".changes/config.toml"),
        &config,
        &[],
        &sync_scope,
        false,
    )
    .expect("dynamic plugin config sync must plan");
    assert!(!sync_plan.has_drift());

    let graph = load_workspace_graph(&root.0, &config).expect("plugin workspace must load");
    let game = graph
        .package(&PackageId::new("game"))
        .expect("game package must exist");
    assert_eq!(game.ecosystem, ecosystem);
    assert_eq!(game.dependencies.len(), 1);
    assert_eq!(game.dependencies[0].package, PackageId::new("engine"));
    assert_eq!(game.dependencies[0].kind, DependencyKind::Runtime);
    assert_eq!(
        graph
            .topological_order()
            .expect("workspace must be acyclic"),
        [PackageId::new("engine"), PackageId::new("game")]
    );

    let mut changeset = Changeset::new("engine-feature".to_owned(), &root.0);
    changeset.add_package(
        "engine".to_owned(),
        BumpLevel::Minor,
        Some("feat".to_owned()),
    );
    changeset.add_package("game".to_owned(), BumpLevel::Patch, Some("feat".to_owned()));
    let plan = plan_release(&root.0, &config, &[changeset]).expect("plugin release must plan");
    assert_eq!(
        plan.versions().get(&PackageId::new("engine")),
        Some(&semver::Version::new(1, 1, 0))
    );
    assert_eq!(
        plan.versions().get(&PackageId::new("game")),
        Some(&semver::Version::new(1, 0, 1))
    );
    assert_eq!(
        plan.file_edits()
            .iter()
            .map(|edit| edit.path.clone())
            .collect::<Vec<Utf8PathBuf>>(),
        [
            Utf8PathBuf::from("engine/manifest.json"),
            Utf8PathBuf::from("game/manifest.json")
        ]
    );
    assert_eq!(
        fs::read_to_string(root.0.join("engine/manifest.json"))
            .expect("engine fixture must remain readable"),
        engine_manifest
    );
    assert_eq!(
        fs::read_to_string(root.0.join("game/manifest.json"))
            .expect("game fixture must remain readable"),
        game_manifest
    );
}
