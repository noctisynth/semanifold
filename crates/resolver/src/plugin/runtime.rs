use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

use boa_engine::module::IdleModuleLoader;
use boa_engine::object::builtins::JsPromise;
use boa_engine::{
    Context, Finalize, JsData, JsError, JsResult, JsString, JsValue, Module, Source, Trace,
    js_string,
};
use boa_runtime::fetch::Fetcher;
use boa_runtime::fetch::request::JsRequest;
use boa_runtime::fetch::response::JsResponse;
use semifold_core::EcosystemId;

use super::protocol::{PluginMetadataV1, PluginProtocolError, PluginRequestV1, PluginResponseV1};

const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_LOOP_ITERATIONS: u64 = 10_000_000;
const MAX_RECURSION_DEPTH: usize = 256;
const MAX_VM_STACK_VALUES: usize = 10_240;

/// Runtime-neutral request passed to the Semifold-controlled HTTP backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

/// Runtime-neutral response returned by the Semifold-controlled HTTP backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginHttpResponse {
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

/// Error returned by a plugin HTTP backend without exposing its implementation type to Boa.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct PluginHttpError {
    message: String,
}

pub type PluginHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PluginHttpResponse, PluginHttpError>> + Send + 'a>>;

impl PluginHttpError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Host-controlled network capability used by JavaScript `fetch`.
///
/// Implementations are responsible for enforcing allowed origins, redirects, address ranges,
/// timeouts and byte/request budgets before returning a response.
pub trait PluginHttpClient: fmt::Debug + Send + Sync + 'static {
    fn send(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_>;
}

/// Default network capability. Plugins cannot access the network unless the host injects a client.
#[derive(Clone, Copy, Debug, Default)]
pub struct DenyPluginHttpClient;

impl PluginHttpClient for DenyPluginHttpClient {
    fn send(&self, _request: PluginHttpRequest) -> PluginHttpFuture<'_> {
        Box::pin(async {
            Err(PluginHttpError::new(
                "plugin network access is not configured",
            ))
        })
    }
}

#[derive(Clone, Trace, Finalize, JsData)]
struct BoaFetcher {
    #[unsafe_ignore_trace]
    client: Arc<dyn PluginHttpClient>,
}

impl fmt::Debug for BoaFetcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("BoaFetcher").finish_non_exhaustive()
    }
}

impl Fetcher for BoaFetcher {
    async fn fetch(
        self: Rc<Self>,
        request: JsRequest,
        _context: &std::cell::RefCell<&mut Context>,
    ) -> JsResult<JsResponse> {
        let request = request.into_inner();
        let plugin_request = PluginHttpRequest {
            method: request.method().to_string(),
            url: request.uri().to_string(),
            headers: request
                .headers()
                .iter()
                .map(|(name, value)| (name.as_str().to_owned(), value.as_bytes().to_vec()))
                .collect(),
            body: request.body().clone(),
        };
        let response = self
            .client
            .send(plugin_request)
            .await
            .map_err(JsError::from_rust)?;
        let PluginHttpResponse {
            url,
            status,
            headers,
            body,
        } = response;
        let mut builder = http::Response::builder().status(status);
        for (name, value) in headers {
            builder = builder.header(name, value);
        }
        let response = builder.body(body).map_err(JsError::from_rust)?;
        Ok(JsResponse::basic(JsString::from(url), response))
    }
}

#[derive(Clone, Copy, Debug)]
struct BoaLimits {
    max_source_bytes: usize,
    max_request_bytes: usize,
    max_response_bytes: usize,
    max_loop_iterations: u64,
    max_recursion_depth: usize,
    max_vm_stack_values: usize,
}

impl Default for BoaLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: MAX_SOURCE_BYTES,
            max_request_bytes: MAX_REQUEST_BYTES,
            max_response_bytes: MAX_RESPONSE_BYTES,
            max_loop_iterations: MAX_LOOP_ITERATIONS,
            max_recursion_depth: MAX_RECURSION_DEPTH,
            max_vm_stack_values: MAX_VM_STACK_VALUES,
        }
    }
}

/// Embedded Boa host for the schema-versioned Semifold plugin protocol.
#[derive(Clone)]
pub struct BoaPluginRuntime {
    http_client: Arc<dyn PluginHttpClient>,
    limits: BoaLimits,
}

impl fmt::Debug for BoaPluginRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoaPluginRuntime")
            .finish_non_exhaustive()
    }
}

impl Default for BoaPluginRuntime {
    fn default() -> Self {
        Self::new(DenyPluginHttpClient)
    }
}

impl BoaPluginRuntime {
    #[must_use]
    pub fn new(http_client: impl PluginHttpClient) -> Self {
        Self {
            http_client: Arc::new(http_client),
            limits: BoaLimits::default(),
        }
    }

    /// Loads and validates the named `metadata` export in a fresh Boa context.
    pub fn metadata(&self, source: &str) -> Result<PluginMetadataV1, PluginRuntimeError> {
        let (mut context, module) = self.load_module(source, Arc::new(DenyPluginHttpClient))?;
        let value = module
            .get_value(js_string!("metadata"), &mut context)
            .map_err(|error| PluginRuntimeError::MissingMetadata(error.to_string()))?;
        let value = value
            .to_json(&mut context)
            .map_err(|error| PluginRuntimeError::InvalidMetadata(error.to_string()))?
            .ok_or(PluginRuntimeError::MetadataNotJson)?;
        let metadata = serde_json::from_value::<PluginMetadataV1>(value)
            .map_err(PluginRuntimeError::MetadataDeserialization)?;
        metadata.validate()?;
        Ok(metadata)
    }

    /// Executes the module's default export with a validated protocol request.
    pub fn execute(
        &self,
        source: &str,
        request: &PluginRequestV1,
        plugin: &EcosystemId,
    ) -> Result<PluginResponseV1, PluginRuntimeError> {
        request.validate()?;
        let request_json =
            serde_json::to_value(request).map_err(PluginRuntimeError::RequestSerialization)?;
        let request_bytes =
            serde_json::to_vec(&request_json).map_err(PluginRuntimeError::RequestSerialization)?;
        if request_bytes.len() > self.limits.max_request_bytes {
            return Err(PluginRuntimeError::RequestTooLarge {
                actual: request_bytes.len(),
                maximum: self.limits.max_request_bytes,
            });
        }

        let (mut context, module) = self.load_module(source, self.http_client.clone())?;
        let entrypoint = module
            .get_value(js_string!("default"), &mut context)
            .map_err(|error| PluginRuntimeError::MissingEntrypoint(error.to_string()))?;
        let Some(entrypoint) = entrypoint.as_callable() else {
            return Err(PluginRuntimeError::EntrypointNotCallable);
        };
        let request_value = JsValue::from_json(&request_json, &mut context)
            .map_err(|error| PluginRuntimeError::RequestConversion(error.to_string()))?;
        let result = entrypoint
            .call(&JsValue::undefined(), &[request_value], &mut context)
            .map_err(|error| PluginRuntimeError::EntrypointInvocation(error.to_string()))?;
        let result = JsPromise::resolve(result, &mut context)
            .await_blocking(&mut context)
            .map_err(|error| PluginRuntimeError::EntrypointInvocation(error.to_string()))?;
        let result = result
            .to_json(&mut context)
            .map_err(|error| PluginRuntimeError::ResponseConversion(error.to_string()))?
            .ok_or(PluginRuntimeError::ResponseNotJson)?;
        let response_bytes =
            serde_json::to_vec(&result).map_err(PluginRuntimeError::ResponseSerialization)?;
        if response_bytes.len() > self.limits.max_response_bytes {
            return Err(PluginRuntimeError::ResponseTooLarge {
                actual: response_bytes.len(),
                maximum: self.limits.max_response_bytes,
            });
        }
        let response = serde_json::from_value::<PluginResponseV1>(result)
            .map_err(PluginRuntimeError::ResponseDeserialization)?;
        response.validate_for(request, plugin)?;
        Ok(response)
    }

    fn load_module(
        &self,
        source: &str,
        http_client: Arc<dyn PluginHttpClient>,
    ) -> Result<(Context, Module), PluginRuntimeError> {
        if source.len() > self.limits.max_source_bytes {
            return Err(PluginRuntimeError::SourceTooLarge {
                actual: source.len(),
                maximum: self.limits.max_source_bytes,
            });
        }
        let mut context = Context::builder()
            .module_loader(Rc::new(IdleModuleLoader))
            .build()
            .map_err(|error| PluginRuntimeError::RuntimeInitialization(error.to_string()))?;
        {
            let limits = context.runtime_limits_mut();
            limits.set_loop_iteration_limit(self.limits.max_loop_iterations);
            limits.set_recursion_limit(self.limits.max_recursion_depth);
            limits.set_stack_size_limit(self.limits.max_vm_stack_values);
        }
        boa_runtime::fetch::register(
            BoaFetcher {
                client: http_client,
            },
            None,
            &mut context,
        )
        .map_err(|error| PluginRuntimeError::RuntimeInitialization(error.to_string()))?;
        boa_runtime::url::Url::register(None, &mut context)
            .map_err(|error| PluginRuntimeError::RuntimeInitialization(error.to_string()))?;

        let module = Module::parse(Source::from_bytes(source.as_bytes()), None, &mut context)
            .map_err(|error| PluginRuntimeError::ModuleParsing(error.to_string()))?;
        module
            .load_link_evaluate(&mut context)
            .await_blocking(&mut context)
            .map_err(|error| PluginRuntimeError::ModuleEvaluation(error.to_string()))?;
        Ok((context, module))
    }

    #[cfg(test)]
    fn with_loop_limit(mut self, maximum: u64) -> Self {
        self.limits.max_loop_iterations = maximum;
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PluginRuntimeError {
    #[error("plugin source contains {actual} bytes; maximum is {maximum}")]
    SourceTooLarge { actual: usize, maximum: usize },
    #[error("plugin request contains {actual} bytes; maximum is {maximum}")]
    RequestTooLarge { actual: usize, maximum: usize },
    #[error("plugin response contains {actual} bytes; maximum is {maximum}")]
    ResponseTooLarge { actual: usize, maximum: usize },
    #[error("failed to initialize Boa plugin runtime: {0}")]
    RuntimeInitialization(String),
    #[error("failed to parse plugin module: {0}")]
    ModuleParsing(String),
    #[error("failed to evaluate plugin module: {0}")]
    ModuleEvaluation(String),
    #[error("plugin module does not export metadata: {0}")]
    MissingMetadata(String),
    #[error("plugin metadata is not JSON-compatible: {0}")]
    InvalidMetadata(String),
    #[error("plugin metadata resolved to undefined")]
    MetadataNotJson,
    #[error("failed to deserialize plugin metadata: {0}")]
    MetadataDeserialization(#[source] serde_json::Error),
    #[error("plugin module does not have a default export: {0}")]
    MissingEntrypoint(String),
    #[error("plugin module default export must be a function")]
    EntrypointNotCallable,
    #[error("failed to serialize plugin request: {0}")]
    RequestSerialization(#[source] serde_json::Error),
    #[error("failed to convert plugin request to JavaScript: {0}")]
    RequestConversion(String),
    #[error("plugin entrypoint failed: {0}")]
    EntrypointInvocation(String),
    #[error("plugin response is not JSON-compatible: {0}")]
    ResponseConversion(String),
    #[error("plugin response resolved to undefined")]
    ResponseNotJson,
    #[error("failed to serialize plugin response: {0}")]
    ResponseSerialization(#[source] serde_json::Error),
    #[error("failed to deserialize plugin response: {0}")]
    ResponseDeserialization(#[source] serde_json::Error),
    #[error(transparent)]
    Protocol(#[from] PluginProtocolError),
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;

    use super::*;
    use crate::plugin::protocol::{PluginCallV1, PluginDiscoverInputV1};

    const PLUGIN_ID: &str = "com.example.engine";

    fn plugin_id() -> EcosystemId {
        EcosystemId::new(PLUGIN_ID).unwrap()
    }

    fn request() -> PluginRequestV1 {
        PluginRequestV1::new(PluginCallV1::Discover(PluginDiscoverInputV1 {
            project_root: ".".to_owned(),
        }))
    }

    fn module_source(body: &str) -> String {
        format!(
            r#"
            export const metadata = {{
                "schema-version": 1,
                ecosystem: "{PLUGIN_ID}",
                "plugin-version": "1.0.0",
                operations: ["discover", "inspect", "plan-edits"]
            }};
            export default {body};
            "#
        )
    }

    #[test]
    fn loads_metadata_and_executes_async_default_export() {
        let source = module_source(
            r#"async function(request) {
                await Promise.resolve();
                return {
                    "schema-version": request["schema-version"],
                    diagnostics: [],
                    status: "success",
                    output: {
                        operation: request.operation,
                        output: { packages: [] }
                    }
                };
            }"#,
        );
        let runtime = BoaPluginRuntime::default();

        let metadata = runtime.metadata(&source).unwrap();
        assert_eq!(metadata.ecosystem, plugin_id());
        let response = runtime.execute(&source, &request(), &plugin_id()).unwrap();
        assert_eq!(response.diagnostics, Vec::new());
    }

    #[test]
    fn rejects_imports_in_single_file_modules() {
        let source = format!(
            "import value from './other.js'; {}",
            module_source("function() { return value; }")
        );
        assert!(matches!(
            BoaPluginRuntime::default().metadata(&source),
            Err(PluginRuntimeError::ModuleEvaluation(message))
                if message.contains("module resolution is disabled")
        ));
    }

    #[test]
    fn stops_plugins_that_exceed_the_loop_budget() {
        let source =
            module_source("function() { for (let i = 0; i < 20; i += 1) {} return undefined; }");
        let runtime = BoaPluginRuntime::default().with_loop_limit(10);
        let error = runtime
            .execute(&source, &request(), &plugin_id())
            .unwrap_err();
        assert!(
            matches!(
                &error,
                PluginRuntimeError::EntrypointInvocation(message)
                    if message.to_ascii_lowercase().contains("loop iteration limit")
            ),
            "{error:?}"
        );
    }

    #[derive(Clone, Debug)]
    struct RecordingHttpClient {
        requests: Arc<Mutex<Vec<PluginHttpRequest>>>,
    }

    impl PluginHttpClient for RecordingHttpClient {
        fn send(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_> {
            Box::pin(async move {
                self.requests.lock().unwrap().push(request.clone());
                let response = json!({
                    "schema-version": 1,
                    "diagnostics": [],
                    "status": "success",
                    "output": {
                        "operation": "discover",
                        "output": { "packages": [] }
                    }
                });
                Ok(PluginHttpResponse {
                    url: request.url,
                    status: 200,
                    headers: vec![("content-type".to_owned(), b"application/json".to_vec())],
                    body: serde_json::to_vec(&response).unwrap(),
                })
            })
        }
    }

    #[test]
    fn metadata_loading_never_uses_the_configured_network_backend() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let runtime = BoaPluginRuntime::new(RecordingHttpClient {
            requests: requests.clone(),
        });
        let source = r#"
            const response = await fetch("https://api.example.test/metadata");
            export const metadata = await response.json();
            export default function() {};
        "#;

        assert!(matches!(
            runtime.metadata(source),
            Err(PluginRuntimeError::ModuleEvaluation(message))
                if message.contains("network access is not configured")
        ));
        assert!(requests.lock().unwrap().is_empty());
    }

    #[test]
    fn routes_fetch_through_the_injected_host_backend() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let runtime = BoaPluginRuntime::new(RecordingHttpClient {
            requests: requests.clone(),
        });
        let source = module_source(
            r#"async function() {
                const response = await fetch("https://api.example.test/plugin");
                return await response.json();
            }"#,
        );

        runtime.execute(&source, &request(), &plugin_id()).unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "https://api.example.test/plugin");
    }

    #[test]
    fn network_is_denied_without_an_injected_backend() {
        let source = module_source(
            r#"async function() {
                await fetch("https://api.example.test/plugin");
            }"#,
        );
        assert!(matches!(
            BoaPluginRuntime::default().execute(&source, &request(), &plugin_id()),
            Err(PluginRuntimeError::EntrypointInvocation(message))
                if message.contains("network access is not configured")
        ));
    }

    #[test]
    fn runtime_handle_is_send_and_sync() {
        fn assert_send_and_sync<T: Send + Sync>() {}
        assert_send_and_sync::<BoaPluginRuntime>();
    }
}
