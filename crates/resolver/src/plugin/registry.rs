use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use semifold_core::EcosystemId;
use sha2::{Digest, Sha256};

use super::file::{PluginFileError, ScopedPluginFileClient};
use super::http::{
    DenyPluginHttpClient, PluginHttpOrigin, PluginHttpTransport, ReqwestPluginHttpTransport,
};
use super::protocol::{PluginMetadataV1, PluginRequestV1, PluginResponseV1};
use super::runtime::{BoaPluginRuntime, MAX_SOURCE_BYTES, PluginRuntimeError};

/// Immutable configuration required to locate and authenticate one repository-local plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginDefinition {
    ecosystem: EcosystemId,
    path: Utf8PathBuf,
    sha256: String,
    allowed_origins: BTreeSet<PluginHttpOrigin>,
}

impl PluginDefinition {
    pub fn new(
        ecosystem: EcosystemId,
        path: impl Into<Utf8PathBuf>,
        sha256: impl Into<String>,
    ) -> Result<Self, PluginRegistryError> {
        if ecosystem.is_builtin() {
            return Err(PluginRegistryError::BuiltInEcosystemReserved { ecosystem });
        }
        let path = path.into();
        validate_plugin_path(&path)?;
        let mut sha256 = sha256.into();
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(PluginRegistryError::InvalidDigest { digest: sha256 });
        }
        sha256.make_ascii_lowercase();
        Ok(Self {
            ecosystem,
            path,
            sha256,
            allowed_origins: BTreeSet::new(),
        })
    }

    #[must_use]
    pub fn with_allowed_origins(
        mut self,
        allowed_origins: impl IntoIterator<Item = PluginHttpOrigin>,
    ) -> Self {
        self.allowed_origins = allowed_origins.into_iter().collect();
        self
    }

    #[must_use]
    pub const fn ecosystem(&self) -> &EcosystemId {
        &self.ecosystem
    }

    #[must_use]
    pub fn path(&self) -> &Utf8Path {
        &self.path
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub const fn allowed_origins(&self) -> &BTreeSet<PluginHttpOrigin> {
        &self.allowed_origins
    }
}

/// A digest-verified plugin bound to its project-scoped host capabilities.
#[derive(Clone)]
pub struct LoadedPlugin {
    definition: PluginDefinition,
    metadata: PluginMetadataV1,
    source: Arc<str>,
    runtime: BoaPluginRuntime,
}

impl std::fmt::Debug for LoadedPlugin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoadedPlugin")
            .field("definition", &self.definition)
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

impl LoadedPlugin {
    #[must_use]
    pub const fn definition(&self) -> &PluginDefinition {
        &self.definition
    }

    #[must_use]
    pub const fn metadata(&self) -> &PluginMetadataV1 {
        &self.metadata
    }

    pub fn execute(
        &self,
        request: &PluginRequestV1,
    ) -> Result<PluginResponseV1, PluginRuntimeError> {
        self.runtime
            .execute(&self.source, request, &self.metadata.ecosystem)
    }
}

/// Stable registry of authenticated plugins keyed by their declared ecosystem identity.
#[derive(Clone, Debug)]
pub struct PluginRegistry {
    project_root: Utf8PathBuf,
    plugins: BTreeMap<EcosystemId, LoadedPlugin>,
}

impl PluginRegistry {
    pub fn load(
        project_root: impl Into<Utf8PathBuf>,
        definitions: impl IntoIterator<Item = PluginDefinition>,
        runtime: BoaPluginRuntime,
    ) -> Result<Self, PluginRegistryError> {
        Self::load_inner(project_root.into(), definitions, runtime, None)
    }

    pub fn load_with_http_transport(
        project_root: impl Into<Utf8PathBuf>,
        definitions: impl IntoIterator<Item = PluginDefinition>,
        runtime: BoaPluginRuntime,
        transport: impl PluginHttpTransport,
    ) -> Result<Self, PluginRegistryError> {
        Self::load_inner(
            project_root.into(),
            definitions,
            runtime,
            Some(Arc::new(transport)),
        )
    }

    pub fn load_with_reqwest(
        project_root: impl Into<Utf8PathBuf>,
        definitions: impl IntoIterator<Item = PluginDefinition>,
        runtime: BoaPluginRuntime,
    ) -> Result<Self, PluginRegistryError> {
        let transport = ReqwestPluginHttpTransport::new()
            .map_err(|source| PluginRegistryError::HttpTransportInitialization { source })?;
        Self::load_with_http_transport(project_root, definitions, runtime, transport)
    }

    fn load_inner(
        project_root: Utf8PathBuf,
        definitions: impl IntoIterator<Item = PluginDefinition>,
        runtime: BoaPluginRuntime,
        http_transport: Option<Arc<dyn PluginHttpTransport>>,
    ) -> Result<Self, PluginRegistryError> {
        let project_root = canonical_project_root(project_root)?;
        let mut definitions = definitions.into_iter().collect::<Vec<_>>();
        definitions.sort_by(|left, right| {
            left.ecosystem
                .cmp(&right.ecosystem)
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.sha256.cmp(&right.sha256))
        });
        if let Some(pair) = definitions
            .windows(2)
            .find(|pair| pair[0].ecosystem == pair[1].ecosystem)
        {
            return Err(PluginRegistryError::DuplicateEcosystem {
                ecosystem: pair[0].ecosystem.clone(),
            });
        }

        let mut plugins = BTreeMap::new();
        for definition in definitions {
            let source = load_authenticated_source(&project_root, &definition)?;
            let metadata =
                runtime
                    .metadata(&source)
                    .map_err(|source| PluginRegistryError::Runtime {
                        ecosystem: definition.ecosystem.clone(),
                        source,
                    })?;
            if metadata.ecosystem != definition.ecosystem {
                return Err(PluginRegistryError::MetadataEcosystemMismatch {
                    configured: definition.ecosystem,
                    declared: metadata.ecosystem,
                });
            }
            let file_client = ScopedPluginFileClient::new(
                project_root.clone(),
                metadata.read_patterns.iter().cloned(),
            )
            .map_err(|source| PluginRegistryError::FileCapability {
                ecosystem: metadata.ecosystem.clone(),
                source,
            })?;
            let ecosystem = metadata.ecosystem.clone();
            let runtime = if let Some(transport) = &http_transport {
                runtime.clone().with_shared_http_transport(
                    definition.allowed_origins.clone(),
                    transport.clone(),
                )
            } else {
                runtime
                    .clone()
                    .with_shared_http_client(Arc::new(DenyPluginHttpClient))
            };
            let plugin = LoadedPlugin {
                definition,
                metadata,
                source: Arc::from(source),
                runtime: runtime.with_file_client(file_client),
            };
            plugins.insert(ecosystem, plugin);
        }

        Ok(Self {
            project_root,
            plugins,
        })
    }

    #[must_use]
    pub fn project_root(&self) -> &Utf8Path {
        &self.project_root
    }

    #[must_use]
    pub fn get(&self, ecosystem: &EcosystemId) -> Option<&LoadedPlugin> {
        self.plugins.get(ecosystem)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&EcosystemId, &LoadedPlugin)> {
        self.plugins.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

fn canonical_project_root(root: Utf8PathBuf) -> Result<Utf8PathBuf, PluginRegistryError> {
    let canonical =
        std::fs::canonicalize(&root).map_err(|source| PluginRegistryError::ResolveProjectRoot {
            root: root.clone(),
            source,
        })?;
    let canonical =
        Utf8PathBuf::from_path_buf(canonical).map_err(|path| PluginRegistryError::NonUtf8Path {
            path: path.display().to_string(),
        })?;
    if !canonical.is_dir() {
        return Err(PluginRegistryError::ProjectRootNotDirectory { root: canonical });
    }
    Ok(canonical)
}

fn load_authenticated_source(
    project_root: &Utf8Path,
    definition: &PluginDefinition,
) -> Result<String, PluginRegistryError> {
    let configured_path = project_root.join(&definition.path);
    let canonical = std::fs::canonicalize(&configured_path).map_err(|source| {
        PluginRegistryError::ResolvePluginPath {
            ecosystem: definition.ecosystem.clone(),
            path: definition.path.clone(),
            source,
        }
    })?;
    let canonical =
        Utf8PathBuf::from_path_buf(canonical).map_err(|path| PluginRegistryError::NonUtf8Path {
            path: path.display().to_string(),
        })?;
    if !canonical.starts_with(project_root) {
        return Err(PluginRegistryError::PluginOutsideProjectRoot {
            ecosystem: definition.ecosystem.clone(),
            path: definition.path.clone(),
        });
    }
    let metadata =
        canonical
            .metadata()
            .map_err(|source| PluginRegistryError::InspectPluginPath {
                ecosystem: definition.ecosystem.clone(),
                path: definition.path.clone(),
                source,
            })?;
    if !metadata.is_file() {
        return Err(PluginRegistryError::PluginNotFile {
            ecosystem: definition.ecosystem.clone(),
            path: definition.path.clone(),
        });
    }
    let metadata_size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    if metadata_size > MAX_SOURCE_BYTES {
        return Err(PluginRegistryError::SourceTooLarge {
            ecosystem: definition.ecosystem.clone(),
            actual: metadata_size,
            maximum: MAX_SOURCE_BYTES,
        });
    }

    let maximum =
        u64::try_from(MAX_SOURCE_BYTES).map_err(|source| PluginRegistryError::InternalLimit {
            reason: source.to_string(),
        })?;
    let file = File::open(&canonical).map_err(|source| PluginRegistryError::ReadPlugin {
        ecosystem: definition.ecosystem.clone(),
        path: definition.path.clone(),
        source,
    })?;
    let mut bytes = Vec::with_capacity(metadata_size);
    file.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| PluginRegistryError::ReadPlugin {
            ecosystem: definition.ecosystem.clone(),
            path: definition.path.clone(),
            source,
        })?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(PluginRegistryError::SourceTooLarge {
            ecosystem: definition.ecosystem.clone(),
            actual: bytes.len(),
            maximum: MAX_SOURCE_BYTES,
        });
    }

    let actual = sha256_hex(&bytes);
    if actual != definition.sha256 {
        return Err(PluginRegistryError::DigestMismatch {
            ecosystem: definition.ecosystem.clone(),
            expected: definition.sha256.clone(),
            actual,
        });
    }
    String::from_utf8(bytes).map_err(|source| PluginRegistryError::InvalidUtf8 {
        ecosystem: definition.ecosystem.clone(),
        path: definition.path.clone(),
        source,
    })
}

fn validate_plugin_path(path: &Utf8Path) -> Result<(), PluginRegistryError> {
    let value = path.as_str();
    let invalid = value.is_empty()
        || path.is_absolute()
        || value.contains('\\')
        || value.ends_with('/')
        || value.split('/').any(|segment| {
            segment.is_empty()
                || segment == "."
                || segment == ".."
                || is_windows_drive_segment(segment)
        });
    if invalid || path.extension() != Some("js") {
        return Err(PluginRegistryError::InvalidPluginPath {
            path: path.to_owned(),
        });
    }
    Ok(())
}

fn is_windows_drive_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, thiserror::Error)]
pub enum PluginRegistryError {
    #[error("plugin ecosystem id {ecosystem} is reserved for a built-in ecosystem")]
    BuiltInEcosystemReserved { ecosystem: EcosystemId },
    #[error("plugin path must be a normalized project-relative `.js` path: `{path}`")]
    InvalidPluginPath { path: Utf8PathBuf },
    #[error("plugin SHA-256 digest must contain exactly 64 hexadecimal characters: `{digest}`")]
    InvalidDigest { digest: String },
    #[error("plugin ecosystem id is registered more than once: {ecosystem}")]
    DuplicateEcosystem { ecosystem: EcosystemId },
    #[error("failed to resolve plugin project root `{root}`: {source}")]
    ResolveProjectRoot {
        root: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("plugin project root is not a directory: `{root}`")]
    ProjectRootNotDirectory { root: Utf8PathBuf },
    #[error("plugin path is not UTF-8: `{path}`")]
    NonUtf8Path { path: String },
    #[error("failed to resolve plugin path `{path}` for {ecosystem}: {source}")]
    ResolvePluginPath {
        ecosystem: EcosystemId,
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("plugin path `{path}` for {ecosystem} resolves outside the project root")]
    PluginOutsideProjectRoot {
        ecosystem: EcosystemId,
        path: Utf8PathBuf,
    },
    #[error("failed to inspect plugin path `{path}` for {ecosystem}: {source}")]
    InspectPluginPath {
        ecosystem: EcosystemId,
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("plugin path `{path}` for {ecosystem} is not a regular file")]
    PluginNotFile {
        ecosystem: EcosystemId,
        path: Utf8PathBuf,
    },
    #[error("plugin source for {ecosystem} contains {actual} bytes; maximum is {maximum}")]
    SourceTooLarge {
        ecosystem: EcosystemId,
        actual: usize,
        maximum: usize,
    },
    #[error("failed to read plugin `{path}` for {ecosystem}: {source}")]
    ReadPlugin {
        ecosystem: EcosystemId,
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("plugin digest mismatch for {ecosystem}: expected {expected}, got {actual}")]
    DigestMismatch {
        ecosystem: EcosystemId,
        expected: String,
        actual: String,
    },
    #[error("plugin `{path}` for {ecosystem} is not valid UTF-8: {source}")]
    InvalidUtf8 {
        ecosystem: EcosystemId,
        path: Utf8PathBuf,
        #[source]
        source: std::string::FromUtf8Error,
    },
    #[error(
        "plugin metadata declares ecosystem {declared}, but configuration registers {configured}"
    )]
    MetadataEcosystemMismatch {
        configured: EcosystemId,
        declared: EcosystemId,
    },
    #[error("failed to load plugin runtime for {ecosystem}: {source}")]
    Runtime {
        ecosystem: EcosystemId,
        #[source]
        source: PluginRuntimeError,
    },
    #[error("failed to configure file capabilities for {ecosystem}: {source}")]
    FileCapability {
        ecosystem: EcosystemId,
        #[source]
        source: PluginFileError,
    },
    #[error("failed to initialize plugin HTTP transport: {source}")]
    HttpTransportInitialization {
        #[source]
        source: super::http::PluginHttpError,
    },
    #[error("invalid internal plugin source limit: {reason}")]
    InternalLimit { reason: String },
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::plugin::http::{
        PluginHttpFuture, PluginHttpRequest, PluginHttpResponse, PluginHttpTransport,
    };
    use crate::plugin::protocol::{PluginCallV1, PluginDiscoverInputV1};

    fn fixture_root(test: &str) -> Utf8PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "semifold-plugin-registry-{}-{test}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        Utf8PathBuf::from_path_buf(root).unwrap()
    }

    fn plugin_source(ecosystem: &str, pattern: &str) -> String {
        format!(
            r#"
            export const metadata = {{
                "schema-version": 1,
                ecosystem: "{ecosystem}",
                "plugin-version": "1.0.0",
                operations: ["discover", "inspect", "plan-edits"],
                "read-patterns": ["{pattern}"]
            }};
            export default async function(request, host) {{
                const files = await host.listFiles("{pattern}");
                await Promise.all(files.map(path => host.readText(path)));
                return {{
                    "schema-version": request["schema-version"],
                    diagnostics: [],
                    status: "success",
                    output: {{
                        operation: request.operation,
                        output: {{ packages: [] }}
                    }}
                }};
            }};
            "#
        )
    }

    fn write_plugin(root: &Utf8Path, path: &str, source: &str) -> PluginDefinition {
        let path = Utf8PathBuf::from(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(root.join(parent)).unwrap();
        }
        fs::write(root.join(&path), source).unwrap();
        let ecosystem = source
            .split("ecosystem: \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap();
        PluginDefinition::new(
            EcosystemId::new(ecosystem).unwrap(),
            path,
            sha256_hex(source.as_bytes()),
        )
        .unwrap()
    }

    fn discover_request() -> PluginRequestV1 {
        PluginRequestV1::new(PluginCallV1::Discover(PluginDiscoverInputV1 {
            project_root: ".".to_owned(),
        }))
    }

    #[derive(Clone, Debug)]
    struct RecordingTransport {
        requests: Arc<Mutex<Vec<PluginHttpRequest>>>,
    }

    impl PluginHttpTransport for RecordingTransport {
        fn send_once(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_> {
            Box::pin(async move {
                self.requests.lock().unwrap().push(request.clone());
                Ok(PluginHttpResponse {
                    url: request.url,
                    status: 200,
                    headers: vec![("content-type".to_owned(), b"text/plain".to_vec())],
                    body: b"ok".to_vec(),
                })
            })
        }
    }

    #[test]
    fn loads_in_stable_identity_order_and_binds_project_file_capabilities() {
        let root = fixture_root("stable-order");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data/package.json"), "{}").unwrap();
        let alpha_source = plugin_source("com.example.alpha", "data/*.json");
        let zeta_source = plugin_source("com.example.zeta", "data/*.json");
        let alpha = write_plugin(&root, "plugins/alpha.js", &alpha_source);
        let zeta = write_plugin(&root, "plugins/zeta.js", &zeta_source);

        let registry =
            PluginRegistry::load(root.clone(), [zeta, alpha], BoaPluginRuntime::default()).unwrap();
        let identities = registry
            .iter()
            .map(|(ecosystem, _)| ecosystem.as_str())
            .collect::<Vec<_>>();
        assert_eq!(identities, vec!["com.example.alpha", "com.example.zeta"]);
        let alpha = EcosystemId::new("com.example.alpha").unwrap();
        registry
            .get(&alpha)
            .unwrap()
            .execute(&discover_request())
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn binds_definition_origins_to_an_injected_transport_and_defaults_to_deny() {
        let root = fixture_root("network-origins");
        let source = r#"
            export const metadata = {
                "schema-version": 1,
                ecosystem: "com.example.network",
                "plugin-version": "1.0.0",
                operations: ["discover", "inspect", "plan-edits"]
            };
            export default async function(request) {
                const response = await fetch("https://api.example.test/data");
                if (await response.text() !== "ok") {
                    throw new Error("unexpected transport response");
                }
                return {
                    "schema-version": request["schema-version"],
                    diagnostics: [],
                    status: "success",
                    output: {
                        operation: request.operation,
                        output: { packages: [] }
                    }
                };
            };
        "#;
        let definition = write_plugin(&root, "network.js", source)
            .with_allowed_origins([PluginHttpOrigin::parse("https://api.example.test").unwrap()]);

        let denied = PluginRegistry::load(
            root.clone(),
            [definition.clone()],
            BoaPluginRuntime::default(),
        )
        .unwrap();
        assert!(matches!(
            denied
                .get(definition.ecosystem())
                .unwrap()
                .execute(&discover_request()),
            Err(PluginRuntimeError::EntrypointInvocation(message))
                if message.contains("network access is not configured")
        ));

        let requests = Arc::new(Mutex::new(Vec::new()));
        let registry = PluginRegistry::load_with_http_transport(
            root.clone(),
            [definition.clone()],
            BoaPluginRuntime::default(),
            RecordingTransport {
                requests: requests.clone(),
            },
        )
        .unwrap();
        registry
            .get(definition.ecosystem())
            .unwrap()
            .execute(&discover_request())
            .unwrap();
        assert_eq!(requests.lock().unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verifies_digest_before_parsing_or_executing_the_module() {
        let root = fixture_root("digest-first");
        fs::write(root.join("invalid.js"), "this is not JavaScript").unwrap();
        let definition = PluginDefinition::new(
            EcosystemId::new("com.example.invalid").unwrap(),
            "invalid.js",
            "0".repeat(64),
        )
        .unwrap();

        assert!(matches!(
            PluginRegistry::load(root.clone(), [definition], BoaPluginRuntime::default()),
            Err(PluginRegistryError::DigestMismatch { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn applies_the_source_size_and_utf8_limits_before_metadata_loading() {
        let root = fixture_root("source-limits");
        let large = vec![b'x'; MAX_SOURCE_BYTES + 1];
        fs::write(root.join("large.js"), &large).unwrap();
        let large_definition = PluginDefinition::new(
            EcosystemId::new("com.example.large").unwrap(),
            "large.js",
            sha256_hex(&large),
        )
        .unwrap();
        assert!(matches!(
            PluginRegistry::load(
                root.clone(),
                [large_definition],
                BoaPluginRuntime::default()
            ),
            Err(PluginRegistryError::SourceTooLarge { .. })
        ));

        let invalid_utf8 = [0xff, 0xfe];
        fs::write(root.join("invalid-utf8.js"), invalid_utf8).unwrap();
        let utf8_definition = PluginDefinition::new(
            EcosystemId::new("com.example.invalid-utf8").unwrap(),
            "invalid-utf8.js",
            sha256_hex(&invalid_utf8),
        )
        .unwrap();
        assert!(matches!(
            PluginRegistry::load(root.clone(), [utf8_definition], BoaPluginRuntime::default()),
            Err(PluginRegistryError::InvalidUtf8 { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_load_failures_in_stable_ecosystem_order() {
        let root = fixture_root("stable-errors");
        fs::write(root.join("alpha.js"), "invalid alpha").unwrap();
        fs::write(root.join("zeta.js"), "invalid zeta").unwrap();
        let alpha = PluginDefinition::new(
            EcosystemId::new("com.example.alpha").unwrap(),
            "alpha.js",
            "0".repeat(64),
        )
        .unwrap();
        let zeta = PluginDefinition::new(
            EcosystemId::new("com.example.zeta").unwrap(),
            "zeta.js",
            "0".repeat(64),
        )
        .unwrap();

        assert!(matches!(
            PluginRegistry::load(root.clone(), [zeta, alpha], BoaPluginRuntime::default()),
            Err(PluginRegistryError::DigestMismatch { ecosystem, .. })
                if ecosystem.as_str() == "com.example.alpha"
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_duplicate_and_mismatched_ecosystem_identities() {
        let root = fixture_root("identity");
        let source = plugin_source("com.example.declared", "data/*.json");
        let declared = write_plugin(&root, "declared.js", &source);
        let mismatched = PluginDefinition::new(
            EcosystemId::new("com.example.configured").unwrap(),
            declared.path().to_owned(),
            declared.sha256(),
        )
        .unwrap();
        assert!(matches!(
            PluginRegistry::load(root.clone(), [mismatched], BoaPluginRuntime::default()),
            Err(PluginRegistryError::MetadataEcosystemMismatch { .. })
        ));

        let duplicate = declared.clone();
        assert!(matches!(
            PluginRegistry::load(
                root.clone(),
                [declared, duplicate],
                BoaPluginRuntime::default()
            ),
            Err(PluginRegistryError::DuplicateEcosystem { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validates_definition_identity_path_and_digest() {
        assert!(matches!(
            PluginDefinition::new(
                EcosystemId::new("rust").unwrap(),
                "plugin.js",
                "0".repeat(64)
            ),
            Err(PluginRegistryError::BuiltInEcosystemReserved { .. })
        ));
        assert!(matches!(
            PluginDefinition::new(
                EcosystemId::new("com.example.plugin").unwrap(),
                "../plugin.js",
                "0".repeat(64)
            ),
            Err(PluginRegistryError::InvalidPluginPath { .. })
        ));
        assert!(matches!(
            PluginDefinition::new(
                EcosystemId::new("com.example.plugin").unwrap(),
                "plugin.js",
                "not-a-digest"
            ),
            Err(PluginRegistryError::InvalidDigest { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_plugin_symlinks_outside_the_project_root() {
        use std::os::unix::fs::symlink;

        let root = fixture_root("outside-root");
        let outside = fixture_root("outside-source");
        let source = plugin_source("com.example.outside", "data/*.json");
        fs::write(outside.join("outside.js"), &source).unwrap();
        symlink(outside.join("outside.js"), root.join("plugin.js")).unwrap();
        let definition = PluginDefinition::new(
            EcosystemId::new("com.example.outside").unwrap(),
            "plugin.js",
            sha256_hex(source.as_bytes()),
        )
        .unwrap();

        assert!(matches!(
            PluginRegistry::load(root.clone(), [definition], BoaPluginRuntime::default()),
            Err(PluginRegistryError::PluginOutsideProjectRoot { .. })
        ));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
