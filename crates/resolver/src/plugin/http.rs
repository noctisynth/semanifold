use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::Url;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::runtime::{Builder as RuntimeBuilder, Handle, Runtime};
use tokio::sync::oneshot;

pub(crate) const MAX_HTTP_REQUEST_BODY_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_HTTP_RESPONSE_BODY_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const MAX_OPERATION_HTTP_REQUEST_BODY_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_OPERATION_HTTP_RESPONSE_BODY_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const MAX_OPERATION_HTTP_REQUESTS: usize = 8;
pub(crate) const MAX_CONCURRENT_HTTP_REQUESTS: usize = 4;
const MAX_REDIRECTS: usize = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Runtime-neutral request passed to a Semifold-controlled HTTP client or transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginHttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

/// Runtime-neutral response returned by a Semifold-controlled HTTP client or transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginHttpResponse {
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

pub type PluginHttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PluginHttpResponse, PluginHttpError>> + Send + 'a>>;

/// Complete network capability consumed by the Boa `fetch` implementation.
pub trait PluginHttpClient: fmt::Debug + Send + Sync + 'static {
    fn send(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_>;
}

/// One HTTP exchange with redirects disabled.
///
/// Implementations must not use ambient proxies or credentials. A concrete network transport must
/// also prevent unsafe resolved addresses from reaching its connector.
pub trait PluginHttpTransport: fmt::Debug + Send + Sync + 'static {
    fn send_once(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_>;
}

/// Default network capability. Plugins cannot access the network unless the host injects a client.
#[derive(Clone, Copy, Debug, Default)]
pub struct DenyPluginHttpClient;

impl PluginHttpClient for DenyPluginHttpClient {
    fn send(&self, _request: PluginHttpRequest) -> PluginHttpFuture<'_> {
        Box::pin(async { Err(PluginHttpError::NotConfigured) })
    }
}

/// Canonical exact HTTPS origin accepted by a plugin definition.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PluginHttpOrigin(String);

impl PluginHttpOrigin {
    pub fn parse(value: &str) -> Result<Self, PluginHttpError> {
        let url = Url::parse(value).map_err(|source| PluginHttpError::InvalidOrigin {
            origin: value.to_owned(),
            reason: source.to_string(),
        })?;
        let valid = url.scheme() == "https"
            && url.host().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none();
        if !valid {
            return Err(PluginHttpError::InvalidOrigin {
                origin: value.to_owned(),
                reason: "origins must contain only an HTTPS scheme, host, and optional port"
                    .to_owned(),
            });
        }
        validate_literal_host(&url)?;
        Ok(Self(url.origin().ascii_serialization()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for PluginHttpOrigin {
    type Err = PluginHttpError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for PluginHttpOrigin {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PluginHttpOrigin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Client that enforces exact HTTPS origins and validates each redirect before another exchange.
#[derive(Clone)]
pub struct ScopedPluginHttpClient {
    allowed_origins: BTreeSet<PluginHttpOrigin>,
    transport: Arc<dyn PluginHttpTransport>,
}

impl fmt::Debug for ScopedPluginHttpClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScopedPluginHttpClient")
            .field("allowed_origins", &self.allowed_origins)
            .finish_non_exhaustive()
    }
}

impl ScopedPluginHttpClient {
    #[must_use]
    pub fn new(
        allowed_origins: impl IntoIterator<Item = PluginHttpOrigin>,
        transport: impl PluginHttpTransport,
    ) -> Self {
        Self {
            allowed_origins: allowed_origins.into_iter().collect(),
            transport: Arc::new(transport),
        }
    }

    pub(crate) fn from_shared(
        allowed_origins: BTreeSet<PluginHttpOrigin>,
        transport: Arc<dyn PluginHttpTransport>,
    ) -> Self {
        Self {
            allowed_origins,
            transport,
        }
    }

    async fn send_inner(
        &self,
        mut request: PluginHttpRequest,
    ) -> Result<PluginHttpResponse, PluginHttpError> {
        let mut redirects = 0_usize;
        loop {
            let current_url = self.validate_url(&request.url)?;
            request.url = current_url.to_string();
            let mut response = self.transport.send_once(request.clone()).await?;
            response.url = current_url.to_string();
            let Some(location) = redirect_location(&response)? else {
                return Ok(response);
            };
            if redirects >= MAX_REDIRECTS {
                return Err(PluginHttpError::TooManyRedirects {
                    maximum: MAX_REDIRECTS,
                });
            }
            let next_url =
                current_url
                    .join(&location)
                    .map_err(|source| PluginHttpError::InvalidRedirect {
                        location,
                        reason: source.to_string(),
                    })?;
            let next_url = self.validate_url(next_url.as_str())?;
            rewrite_redirect_request(&mut request, response.status);
            if current_url.origin() != next_url.origin() {
                remove_sensitive_headers(&mut request.headers);
            }
            request.url = next_url.to_string();
            redirects += 1;
        }
    }

    fn validate_url(&self, value: &str) -> Result<Url, PluginHttpError> {
        let mut url = Url::parse(value).map_err(|source| PluginHttpError::InvalidUrl {
            url: value.to_owned(),
            reason: source.to_string(),
        })?;
        if url.scheme() != "https" || url.host().is_none() {
            return Err(PluginHttpError::HttpsRequired {
                url: value.to_owned(),
            });
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(PluginHttpError::CredentialsInUrl {
                url: value.to_owned(),
            });
        }
        validate_literal_host(&url)?;
        let origin = PluginHttpOrigin(url.origin().ascii_serialization());
        if !self.allowed_origins.contains(&origin) {
            return Err(PluginHttpError::OriginNotAllowed { origin: origin.0 });
        }
        url.set_fragment(None);
        Ok(url)
    }
}

impl PluginHttpClient for ScopedPluginHttpClient {
    fn send(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_> {
        Box::pin(async move { self.send_inner(request).await })
    }
}

/// Async reqwest transport backed by its own reusable Tokio runtime.
#[derive(Clone)]
pub struct ReqwestPluginHttpTransport {
    inner: Arc<ReqwestTransportInner>,
}

impl fmt::Debug for ReqwestPluginHttpTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReqwestPluginHttpTransport")
            .finish_non_exhaustive()
    }
}

impl ReqwestPluginHttpTransport {
    pub fn new() -> Result<Self, PluginHttpError> {
        let runtime = RuntimeBuilder::new_multi_thread()
            .worker_threads(2)
            .thread_name("semifold-plugin-http")
            .enable_all()
            .build()
            .map_err(|source| PluginHttpError::TransportInitialization {
                reason: source.to_string(),
            })?;
        let handle = runtime.handle().clone();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .https_only(true)
            .timeout(REQUEST_TIMEOUT)
            .dns_resolver(Arc::new(GlobalDnsResolver))
            .build()
            .map_err(|source| PluginHttpError::TransportInitialization {
                reason: source.to_string(),
            })?;
        Ok(Self {
            inner: Arc::new(ReqwestTransportInner {
                client,
                handle,
                runtime: Some(runtime),
            }),
        })
    }
}

impl PluginHttpTransport for ReqwestPluginHttpTransport {
    fn send_once(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_> {
        let client = self.inner.client.clone();
        let (sender, receiver) = oneshot::channel();
        self.inner.handle.spawn(async move {
            let result = send_reqwest(client, request).await;
            let _ignored = sender.send(result);
        });
        Box::pin(async move {
            receiver
                .await
                .map_err(|source| PluginHttpError::TransportTask {
                    reason: source.to_string(),
                })?
        })
    }
}

struct ReqwestTransportInner {
    client: reqwest::Client,
    handle: Handle,
    runtime: Option<Runtime>,
}

impl Drop for ReqwestTransportInner {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

async fn send_reqwest(
    client: reqwest::Client,
    request: PluginHttpRequest,
) -> Result<PluginHttpResponse, PluginHttpError> {
    let target = Url::parse(&request.url).map_err(|source| PluginHttpError::InvalidUrl {
        url: request.url.clone(),
        reason: source.to_string(),
    })?;
    if target.scheme() != "https" {
        return Err(PluginHttpError::HttpsRequired { url: request.url });
    }
    validate_literal_host(&target)?;
    let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|source| {
        PluginHttpError::InvalidMethod {
            method: request.method.clone(),
            reason: source.to_string(),
        }
    })?;
    let mut builder = client.request(method, &request.url);
    for (name, value) in request.headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|source| {
            PluginHttpError::InvalidHeader {
                name: name.clone(),
                reason: source.to_string(),
            }
        })?;
        let value = reqwest::header::HeaderValue::from_bytes(&value).map_err(|source| {
            PluginHttpError::InvalidHeader {
                name: name.as_str().to_owned(),
                reason: source.to_string(),
            }
        })?;
        builder = builder.header(name, value);
    }
    let mut response =
        builder
            .body(request.body)
            .send()
            .await
            .map_err(|source| PluginHttpError::Transport {
                reason: source.to_string(),
            })?;
    let url = response.url().to_string();
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| (name.as_str().to_owned(), value.as_bytes().to_vec()))
        .collect();
    let mut body = Vec::new();
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(source) => {
                return Err(PluginHttpError::TransportResponse {
                    received: body.len(),
                    reason: source.to_string(),
                });
            }
        };
        let actual = body.len().saturating_add(chunk.len());
        if actual > MAX_HTTP_RESPONSE_BODY_BYTES {
            return Err(PluginHttpError::ResponseBodyTooLarge {
                actual,
                maximum: MAX_HTTP_RESPONSE_BODY_BYTES,
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(PluginHttpResponse {
        url,
        status,
        headers,
        body,
    })
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PluginHttpLimits {
    pub max_request_body_bytes: usize,
    pub max_response_body_bytes: usize,
    pub max_operation_request_body_bytes: usize,
    pub max_operation_response_body_bytes: usize,
    pub max_operation_requests: usize,
    pub max_concurrent_requests: usize,
}

impl Default for PluginHttpLimits {
    fn default() -> Self {
        Self {
            max_request_body_bytes: MAX_HTTP_REQUEST_BODY_BYTES,
            max_response_body_bytes: MAX_HTTP_RESPONSE_BODY_BYTES,
            max_operation_request_body_bytes: MAX_OPERATION_HTTP_REQUEST_BODY_BYTES,
            max_operation_response_body_bytes: MAX_OPERATION_HTTP_RESPONSE_BODY_BYTES,
            max_operation_requests: MAX_OPERATION_HTTP_REQUESTS,
            max_concurrent_requests: MAX_CONCURRENT_HTTP_REQUESTS,
        }
    }
}

#[derive(Debug, Default)]
struct PluginHttpBudget {
    requests: usize,
    concurrent_requests: usize,
    request_body_bytes: usize,
    response_body_bytes: usize,
}

pub(crate) struct BudgetedPluginHttpClient {
    client: Arc<dyn PluginHttpClient>,
    budget: Arc<Mutex<PluginHttpBudget>>,
    limits: PluginHttpLimits,
}

impl fmt::Debug for BudgetedPluginHttpClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BudgetedPluginHttpClient")
            .finish_non_exhaustive()
    }
}

impl BudgetedPluginHttpClient {
    pub(crate) fn new(client: Arc<dyn PluginHttpClient>, limits: PluginHttpLimits) -> Self {
        Self {
            client,
            budget: Arc::new(Mutex::new(PluginHttpBudget::default())),
            limits,
        }
    }
}

impl PluginHttpClient for BudgetedPluginHttpClient {
    fn send(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_> {
        Box::pin(async move {
            let permit =
                PluginHttpPermit::acquire(self.budget.clone(), self.limits, request.body.len())?;
            match self.client.send(request).await {
                Ok(response) => {
                    permit.finish(response.body.len())?;
                    Ok(response)
                }
                Err(error) => finish_failed_response(permit, error),
            }
        })
    }
}

pub(crate) struct BudgetedPluginHttpTransport {
    transport: Arc<dyn PluginHttpTransport>,
    budget: Arc<Mutex<PluginHttpBudget>>,
    limits: PluginHttpLimits,
}

impl fmt::Debug for BudgetedPluginHttpTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BudgetedPluginHttpTransport")
            .finish_non_exhaustive()
    }
}

impl BudgetedPluginHttpTransport {
    pub(crate) fn new(transport: Arc<dyn PluginHttpTransport>, limits: PluginHttpLimits) -> Self {
        Self {
            transport,
            budget: Arc::new(Mutex::new(PluginHttpBudget::default())),
            limits,
        }
    }
}

impl PluginHttpTransport for BudgetedPluginHttpTransport {
    fn send_once(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_> {
        Box::pin(async move {
            let permit =
                PluginHttpPermit::acquire(self.budget.clone(), self.limits, request.body.len())?;
            match self.transport.send_once(request).await {
                Ok(response) => {
                    permit.finish(response.body.len())?;
                    Ok(response)
                }
                Err(error) => finish_failed_response(permit, error),
            }
        })
    }
}

struct PluginHttpPermit {
    budget: Arc<Mutex<PluginHttpBudget>>,
    limits: PluginHttpLimits,
    active: bool,
}

impl PluginHttpPermit {
    fn acquire(
        budget: Arc<Mutex<PluginHttpBudget>>,
        limits: PluginHttpLimits,
        request_body_bytes: usize,
    ) -> Result<Self, PluginHttpError> {
        if request_body_bytes > limits.max_request_body_bytes {
            return Err(PluginHttpError::RequestBodyTooLarge {
                actual: request_body_bytes,
                maximum: limits.max_request_body_bytes,
            });
        }
        {
            let mut state = budget
                .lock()
                .map_err(|_| PluginHttpError::BudgetStateUnavailable)?;
            let requests = state.requests.saturating_add(1);
            if requests > limits.max_operation_requests {
                return Err(PluginHttpError::TooManyRequests {
                    actual: requests,
                    maximum: limits.max_operation_requests,
                });
            }
            let concurrent = state.concurrent_requests.saturating_add(1);
            if concurrent > limits.max_concurrent_requests {
                return Err(PluginHttpError::TooManyConcurrentRequests {
                    actual: concurrent,
                    maximum: limits.max_concurrent_requests,
                });
            }
            let total_body = state.request_body_bytes.saturating_add(request_body_bytes);
            if total_body > limits.max_operation_request_body_bytes {
                return Err(PluginHttpError::OperationRequestBodyTooLarge {
                    actual: total_body,
                    maximum: limits.max_operation_request_body_bytes,
                });
            }
            state.requests = requests;
            state.concurrent_requests = concurrent;
            state.request_body_bytes = total_body;
        }
        Ok(Self {
            budget,
            limits,
            active: true,
        })
    }

    fn finish(mut self, response_body_bytes: usize) -> Result<(), PluginHttpError> {
        {
            let mut state = self
                .budget
                .lock()
                .map_err(|_| PluginHttpError::BudgetStateUnavailable)?;
            state.concurrent_requests = state.concurrent_requests.saturating_sub(1);
            self.active = false;
            let total_body = state
                .response_body_bytes
                .saturating_add(response_body_bytes);
            state.response_body_bytes = total_body;
            if response_body_bytes > self.limits.max_response_body_bytes {
                Err(PluginHttpError::ResponseBodyTooLarge {
                    actual: response_body_bytes,
                    maximum: self.limits.max_response_body_bytes,
                })
            } else if total_body > self.limits.max_operation_response_body_bytes {
                Err(PluginHttpError::OperationResponseBodyTooLarge {
                    actual: total_body,
                    maximum: self.limits.max_operation_response_body_bytes,
                })
            } else {
                Ok(())
            }
        }
    }
}

fn finish_failed_response(
    permit: PluginHttpPermit,
    error: PluginHttpError,
) -> Result<PluginHttpResponse, PluginHttpError> {
    if let Some(received) = error.response_body_bytes() {
        permit.finish(received)?;
    }
    Err(error)
}

impl Drop for PluginHttpPermit {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Ok(mut state) = self.budget.lock() {
            state.concurrent_requests = state.concurrent_requests.saturating_sub(1);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct GlobalDnsResolver;

impl Resolve for GlobalDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|source| -> Box<dyn std::error::Error + Send + Sync> {
                    Box::new(PluginHttpError::DnsResolution {
                        host: host.clone(),
                        reason: source.to_string(),
                    })
                })?
                .collect::<Vec<_>>();
            if addresses.is_empty() {
                return Err(Box::new(PluginHttpError::DnsResolution {
                    host,
                    reason: "the resolver returned no addresses".to_owned(),
                })
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            if let Some(address) = addresses
                .iter()
                .find(|address| !is_global_address(address.ip()))
            {
                return Err(Box::new(PluginHttpError::UnsafeAddress {
                    address: address.ip(),
                })
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

fn validate_literal_host(url: &Url) -> Result<(), PluginHttpError> {
    let address = url
        .host_str()
        .map(|host| host.trim_start_matches('[').trim_end_matches(']'))
        .and_then(|host| host.parse::<IpAddr>().ok());
    if let Some(address) = address
        && !is_global_address(address)
    {
        return Err(PluginHttpError::UnsafeAddress { address });
    }
    Ok(())
}

fn is_global_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_global_ipv4(address),
        IpAddr::V6(address) => is_global_ipv6(address),
    }
}

fn is_global_ipv4(address: Ipv4Addr) -> bool {
    let [first, second, third, _fourth] = address.octets();
    !(first == 0
        || first == 10
        || first == 127
        || first >= 224
        || first == 100 && (64..=127).contains(&second)
        || first == 169 && second == 254
        || first == 172 && (16..=31).contains(&second)
        || first == 192 && second == 0 && third == 0
        || first == 192 && second == 0 && third == 2
        || first == 192 && second == 88 && third == 99
        || first == 192 && second == 168
        || first == 198 && matches!(second, 18 | 19)
        || first == 198 && second == 51 && third == 100
        || first == 203 && second == 0 && third == 113)
}

fn is_global_ipv6(address: Ipv6Addr) -> bool {
    if let Some(address) = address.to_ipv4() {
        return is_global_ipv4(address);
    }
    let segments = address.segments();
    let in_global_unicast = segments[0] & 0xe000 == 0x2000;
    let ietf_special = segments[0] == 0x2001 && segments[1] < 0x0200;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    let six_to_four = segments[0] == 0x2002;
    let documentation_v2 = segments[0] == 0x3fff && segments[1] & 0xf000 == 0;
    in_global_unicast && !ietf_special && !documentation && !six_to_four && !documentation_v2
}

fn redirect_location(response: &PluginHttpResponse) -> Result<Option<String>, PluginHttpError> {
    if !matches!(response.status, 301 | 302 | 303 | 307 | 308) {
        return Ok(None);
    }
    let Some((_, value)) = response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("location"))
    else {
        return Ok(None);
    };
    std::str::from_utf8(value)
        .map(str::to_owned)
        .map(Some)
        .map_err(|source| PluginHttpError::InvalidRedirect {
            location: String::from_utf8_lossy(value).into_owned(),
            reason: source.to_string(),
        })
}

fn rewrite_redirect_request(request: &mut PluginHttpRequest, status: u16) {
    let rewrite_to_get = status == 303 && !request.method.eq_ignore_ascii_case("HEAD")
        || matches!(status, 301 | 302) && request.method.eq_ignore_ascii_case("POST");
    if rewrite_to_get {
        request.method = "GET".to_owned();
        request.body.clear();
        request.headers.retain(|(name, _)| {
            !matches_header(
                name,
                &["content-length", "content-type", "transfer-encoding"],
            )
        });
    }
}

fn remove_sensitive_headers(headers: &mut Vec<(String, Vec<u8>)>) {
    headers.retain(|(name, _)| {
        !matches_header(name, &["authorization", "cookie", "proxy-authorization"])
    });
}

fn matches_header(name: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

#[derive(Debug, thiserror::Error)]
pub enum PluginHttpError {
    #[error("plugin network access is not configured")]
    NotConfigured,
    #[error("invalid plugin HTTP origin `{origin}`: {reason}")]
    InvalidOrigin { origin: String, reason: String },
    #[error("invalid plugin HTTP URL `{url}`: {reason}")]
    InvalidUrl { url: String, reason: String },
    #[error("plugin HTTP URL must use HTTPS: `{url}`")]
    HttpsRequired { url: String },
    #[error("plugin HTTP URL must not contain credentials: `{url}`")]
    CredentialsInUrl { url: String },
    #[error("plugin HTTP origin is not allowed: `{origin}`")]
    OriginNotAllowed { origin: String },
    #[error("plugin HTTP target resolves to a non-global address: {address}")]
    UnsafeAddress { address: IpAddr },
    #[error("failed to resolve plugin HTTP host `{host}`: {reason}")]
    DnsResolution { host: String, reason: String },
    #[error("invalid plugin redirect location `{location}`: {reason}")]
    InvalidRedirect { location: String, reason: String },
    #[error("plugin HTTP request exceeded the redirect limit of {maximum}")]
    TooManyRedirects { maximum: usize },
    #[error("failed to initialize plugin HTTP transport: {reason}")]
    TransportInitialization { reason: String },
    #[error("plugin HTTP transport task failed: {reason}")]
    TransportTask { reason: String },
    #[error("plugin HTTP transport failed: {reason}")]
    Transport { reason: String },
    #[error("plugin HTTP transport failed after receiving {received} response bytes: {reason}")]
    TransportResponse { received: usize, reason: String },
    #[error("invalid plugin HTTP method `{method}`: {reason}")]
    InvalidMethod { method: String, reason: String },
    #[error("invalid plugin HTTP header `{name}`: {reason}")]
    InvalidHeader { name: String, reason: String },
    #[error("plugin HTTP request body contains {actual} bytes; maximum is {maximum}")]
    RequestBodyTooLarge { actual: usize, maximum: usize },
    #[error("plugin HTTP response body contains {actual} bytes; maximum is {maximum}")]
    ResponseBodyTooLarge { actual: usize, maximum: usize },
    #[error("plugin HTTP operation attempted {actual} requests; maximum is {maximum}")]
    TooManyRequests { actual: usize, maximum: usize },
    #[error("plugin HTTP operation attempted {actual} concurrent requests; maximum is {maximum}")]
    TooManyConcurrentRequests { actual: usize, maximum: usize },
    #[error("plugin HTTP operation sent {actual} request-body bytes; maximum is {maximum}")]
    OperationRequestBodyTooLarge { actual: usize, maximum: usize },
    #[error("plugin HTTP operation received {actual} response-body bytes; maximum is {maximum}")]
    OperationResponseBodyTooLarge { actual: usize, maximum: usize },
    #[error("plugin HTTP budget state is unavailable")]
    BudgetStateUnavailable,
    #[error("plugin HTTP backend failed: {message}")]
    Backend { message: String },
}

impl PluginHttpError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self::Backend {
            message: message.into(),
        }
    }

    fn response_body_bytes(&self) -> Option<usize> {
        match self {
            Self::ResponseBodyTooLarge { actual, .. } => Some(*actual),
            Self::TransportResponse { received, .. } => Some(*received),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    fn request(url: &str) -> PluginHttpRequest {
        PluginHttpRequest {
            method: "GET".to_owned(),
            url: url.to_owned(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    fn response(status: u16, location: Option<&str>, body: &[u8]) -> PluginHttpResponse {
        PluginHttpResponse {
            url: "https://transport.invalid/ignored".to_owned(),
            status,
            headers: location
                .map(|location| vec![("location".to_owned(), location.as_bytes().to_vec())])
                .unwrap_or_default(),
            body: body.to_vec(),
        }
    }

    fn block_on<T>(future: impl Future<Output = T>) -> T {
        RuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[derive(Clone, Debug)]
    struct ScriptedTransport {
        responses: Arc<Mutex<VecDeque<PluginHttpResponse>>>,
        requests: Arc<Mutex<Vec<PluginHttpRequest>>>,
    }

    impl ScriptedTransport {
        fn new(responses: impl IntoIterator<Item = PluginHttpResponse>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses.into_iter().collect())),
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl PluginHttpTransport for ScriptedTransport {
        fn send_once(&self, request: PluginHttpRequest) -> PluginHttpFuture<'_> {
            Box::pin(async move {
                self.requests.lock().unwrap().push(request);
                self.responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .ok_or_else(|| PluginHttpError::new("scripted response queue is empty"))
            })
        }
    }

    #[test]
    fn canonicalizes_exact_https_origins_and_rejects_unsafe_shapes() {
        let origin = PluginHttpOrigin::parse("https://EXAMPLE.com:443/").unwrap();
        assert_eq!(origin.as_str(), "https://example.com");
        assert_eq!(
            serde_json::to_string(&origin).unwrap(),
            r#""https://example.com""#
        );
        assert_eq!(
            serde_json::from_str::<PluginHttpOrigin>(r#""https://EXAMPLE.com:443/""#).unwrap(),
            origin
        );
        assert!(matches!(
            PluginHttpOrigin::parse("http://example.com"),
            Err(PluginHttpError::InvalidOrigin { .. })
        ));
        assert!(matches!(
            PluginHttpOrigin::parse("https://example.com/path"),
            Err(PluginHttpError::InvalidOrigin { .. })
        ));
        assert!(matches!(
            PluginHttpOrigin::parse("https://user@example.com"),
            Err(PluginHttpError::InvalidOrigin { .. })
        ));
        assert!(matches!(
            PluginHttpOrigin::parse("https://127.0.0.1"),
            Err(PluginHttpError::UnsafeAddress { .. })
        ));
        assert!(matches!(
            PluginHttpOrigin::parse("https://[::1]"),
            Err(PluginHttpError::UnsafeAddress { .. })
        ));
    }

    #[test]
    fn address_policy_only_accepts_publicly_routable_targets() {
        assert!(is_global_address("8.8.8.8".parse().unwrap()));
        assert!(is_global_address("2606:4700:4700::1111".parse().unwrap()));
        for address in [
            "0.0.0.0",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.0.2.1",
            "192.168.0.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "2002:0808:0808::1",
        ] {
            assert!(!is_global_address(address.parse().unwrap()), "{address}");
        }
    }

    #[test]
    fn validates_each_redirect_and_strips_cross_origin_sensitive_headers() {
        let transport = ScriptedTransport::new([
            response(302, Some("https://second.example.test/final"), b""),
            response(200, None, b"done"),
        ]);
        let requests = transport.requests.clone();
        let client = ScopedPluginHttpClient::new(
            [
                PluginHttpOrigin::parse("https://first.example.test").unwrap(),
                PluginHttpOrigin::parse("https://second.example.test").unwrap(),
            ],
            transport,
        );
        let mut initial = request("https://first.example.test/start#fragment");
        initial.method = "POST".to_owned();
        initial.body = b"payload".to_vec();
        initial.headers = vec![
            ("authorization".to_owned(), b"Bearer secret".to_vec()),
            ("cookie".to_owned(), b"session=secret".to_vec()),
            ("content-type".to_owned(), b"text/plain".to_vec()),
            ("x-plugin".to_owned(), b"preserved".to_vec()),
        ];

        let result = block_on(client.send(initial)).unwrap();
        assert_eq!(result.url, "https://second.example.test/final");
        assert_eq!(result.body, b"done");
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].url, "https://first.example.test/start");
        assert_eq!(requests[1].method, "GET");
        assert!(requests[1].body.is_empty());
        assert_eq!(
            requests[1].headers,
            vec![("x-plugin".to_owned(), b"preserved".to_vec())]
        );
    }

    #[test]
    fn denies_unlisted_initial_and_redirect_origins_before_transport() {
        let transport = ScriptedTransport::new([response(
            302,
            Some("https://blocked.example.test/final"),
            b"",
        )]);
        let requests = transport.requests.clone();
        let client = ScopedPluginHttpClient::new(
            [PluginHttpOrigin::parse("https://allowed.example.test").unwrap()],
            transport,
        );

        assert!(matches!(
            block_on(client.send(request("https://unlisted.example.test/start"))),
            Err(PluginHttpError::OriginNotAllowed { .. })
        ));
        assert!(requests.lock().unwrap().is_empty());
        assert!(matches!(
            block_on(client.send(request("https://allowed.example.test/start"))),
            Err(PluginHttpError::OriginNotAllowed { .. })
        ));
        assert_eq!(requests.lock().unwrap().len(), 1);
    }

    #[test]
    fn counts_redirect_exchanges_against_operation_budgets() {
        let transport = ScriptedTransport::new([
            response(307, Some("/next"), b""),
            response(200, None, b"done"),
        ]);
        let limits = PluginHttpLimits {
            max_operation_requests: 1,
            ..PluginHttpLimits::default()
        };
        let transport: Arc<dyn PluginHttpTransport> = Arc::new(BudgetedPluginHttpTransport::new(
            Arc::new(transport),
            limits,
        ));
        let client = ScopedPluginHttpClient::from_shared(
            BTreeSet::from([PluginHttpOrigin::parse("https://allowed.example.test").unwrap()]),
            transport,
        );

        assert!(matches!(
            block_on(client.send(request("https://allowed.example.test/start"))),
            Err(PluginHttpError::TooManyRequests {
                actual: 2,
                maximum: 1
            })
        ));
    }

    #[test]
    fn stops_after_five_followed_redirects() {
        let transport =
            ScriptedTransport::new((0..=MAX_REDIRECTS).map(|_| response(307, Some("/next"), b"")));
        let requests = transport.requests.clone();
        let client = ScopedPluginHttpClient::new(
            [PluginHttpOrigin::parse("https://allowed.example.test").unwrap()],
            transport,
        );

        assert!(matches!(
            block_on(client.send(request("https://allowed.example.test/start"))),
            Err(PluginHttpError::TooManyRedirects {
                maximum: MAX_REDIRECTS
            })
        ));
        assert_eq!(requests.lock().unwrap().len(), MAX_REDIRECTS + 1);
    }

    #[test]
    fn enforces_request_response_and_concurrency_budgets() {
        let limits = PluginHttpLimits {
            max_request_body_bytes: 8,
            max_response_body_bytes: 8,
            max_operation_request_body_bytes: 10,
            max_operation_response_body_bytes: 10,
            max_operation_requests: 8,
            max_concurrent_requests: 4,
        };
        let budget = Arc::new(Mutex::new(PluginHttpBudget::default()));
        let permits = (0..4)
            .map(|_| PluginHttpPermit::acquire(budget.clone(), limits, 2).unwrap())
            .collect::<Vec<_>>();
        assert!(matches!(
            PluginHttpPermit::acquire(budget.clone(), limits, 1),
            Err(PluginHttpError::TooManyConcurrentRequests {
                actual: 5,
                maximum: 4
            })
        ));
        drop(permits);

        let permit = PluginHttpPermit::acquire(budget.clone(), limits, 2).unwrap();
        assert!(matches!(
            permit.finish(9),
            Err(PluginHttpError::ResponseBodyTooLarge {
                actual: 9,
                maximum: 8
            })
        ));
        let permit = PluginHttpPermit::acquire(budget.clone(), limits, 0).unwrap();
        assert!(matches!(
            permit.finish(2),
            Err(PluginHttpError::OperationResponseBodyTooLarge {
                actual: 11,
                maximum: 10
            })
        ));
        assert!(matches!(
            PluginHttpPermit::acquire(budget, limits, 9),
            Err(PluginHttpError::RequestBodyTooLarge {
                actual: 9,
                maximum: 8
            })
        ));
    }

    #[test]
    fn reqwest_transport_is_send_sync_and_safe_to_drop_inside_async_context() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ReqwestPluginHttpTransport>();

        block_on(async {
            let transport = ReqwestPluginHttpTransport::new().unwrap();
            drop(transport);
        });
    }
}
