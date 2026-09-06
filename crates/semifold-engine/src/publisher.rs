use semifold_changelog::github::{GitHubFailure, GitHubOperation};
use std::{
    collections::BTreeMap,
    future::Future,
    io::Write,
    path::Path,
    pin::Pin,
    process::{Command, Stdio},
    time::{Duration, SystemTime},
};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER, USER_AGENT};
use semifold_core::PackageId;

use crate::publish_plan::{
    AssetDeclaration, CommandPhase, CommandSpec, ForgeRelease, PlannedPreCheck,
    PublishPackageContext, PublishPlan, PublishSkipReason, StdioPolicy,
};

pub type ExternalFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
}

#[derive(Debug)]
pub struct CommandError {
    message: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

pub trait CommandRunner {
    fn run(&self, command: &CommandSpec) -> Result<CommandOutput, CommandError>;
}

pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, command: &CommandSpec) -> Result<CommandOutput, CommandError> {
        let status = Command::new(&command.executable)
            .args(&command.args)
            .envs(&command.environment)
            .current_dir(&command.working_directory)
            .stdout(stdio(command.stdout))
            .stderr(stdio(command.stderr))
            .status()
            .map_err(|error| CommandError {
                message: format!("failed to run command {}: {error}", command.executable),
            })?;
        if !status.success() {
            return Err(CommandError {
                message: format!(
                    "command {} exited with status {:?}",
                    command.executable,
                    status.code()
                ),
            });
        }
        Ok(CommandOutput {
            exit_code: status.code(),
        })
    }
}

fn stdio(policy: StdioPolicy) -> Stdio {
    match policy {
        StdioPolicy::Inherit => Stdio::inherit(),
        StdioPolicy::Pipe => Stdio::piped(),
        StdioPolicy::Null => Stdio::null(),
    }
}

#[derive(Debug)]
pub struct PreCheckError {
    message: String,
}

impl std::fmt::Display for PreCheckError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PreCheckError {}

pub trait PreCheckRunner {
    fn version_exists<'a>(
        &'a self,
        check: &'a PlannedPreCheck,
        package: &'a PublishPackageContext,
    ) -> ExternalFuture<'a, Result<bool, PreCheckError>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForgeReleaseId(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForgeReleaseOutcome {
    Created(ForgeReleaseId),
    AlreadyExists,
}

#[derive(Debug)]
pub struct ForgeError {
    message: String,
    github: Option<GitHubFailure>,
}

impl std::fmt::Display for ForgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ForgeError {}

impl ForgeError {
    fn github(operation: GitHubOperation, error: octocrab::Error) -> Self {
        let failure = GitHubFailure::new(operation, error);
        Self {
            message: failure.to_string(),
            github: Some(failure),
        }
    }
}

pub trait ForgeClient {
    fn create_release<'a>(
        &'a self,
        release: &'a ForgeRelease,
    ) -> ExternalFuture<'a, Result<ForgeReleaseOutcome, ForgeError>>;

    fn upload_asset<'a>(
        &'a self,
        release: &'a ForgeRelease,
        release_id: &'a ForgeReleaseId,
        name: &'a str,
        content: Vec<u8>,
    ) -> ExternalFuture<'a, Result<(), ForgeError>>;
}

pub struct GithubForgeClient {
    client: octocrab::Octocrab,
}

impl GithubForgeClient {
    pub fn new(client: octocrab::Octocrab) -> Self {
        Self { client }
    }
}

impl ForgeClient for GithubForgeClient {
    fn create_release<'a>(
        &'a self,
        release: &'a ForgeRelease,
    ) -> ExternalFuture<'a, Result<ForgeReleaseOutcome, ForgeError>> {
        Box::pin(async move {
            match self
                .client
                .repos(&release.owner, &release.repository)
                .releases()
                .create(&release.tag)
                .name(&release.title)
                .body(&release.body)
                .prerelease(release.prerelease)
                .send()
                .await
            {
                Ok(created) => Ok(ForgeReleaseOutcome::Created(ForgeReleaseId(created.id.0))),
                Err(octocrab::Error::GitHub { source, .. })
                    if source.status_code == reqwest::StatusCode::UNPROCESSABLE_ENTITY
                        && release_already_exists(source.errors.as_deref().unwrap_or_default()) =>
                {
                    Ok(ForgeReleaseOutcome::AlreadyExists)
                }
                Err(error) => Err(ForgeError::github(GitHubOperation::CreateRelease, error)),
            }
        })
    }

    fn upload_asset<'a>(
        &'a self,
        release: &'a ForgeRelease,
        release_id: &'a ForgeReleaseId,
        name: &'a str,
        content: Vec<u8>,
    ) -> ExternalFuture<'a, Result<(), ForgeError>> {
        Box::pin(async move {
            self.client
                .repos(&release.owner, &release.repository)
                .releases()
                .upload_asset(release_id.0, name, bytes::Bytes::from(content))
                .send()
                .await
                .map_err(|error| ForgeError::github(GitHubOperation::UploadAsset, error))?;
            Ok(())
        })
    }
}

fn release_already_exists(errors: &[serde_json::Value]) -> bool {
    errors.first().is_some_and(|error| {
        error
            .as_object()
            .and_then(|error| error.get("code"))
            .and_then(serde_json::Value::as_str)
            == Some("already_exists")
    })
}

pub trait FileSystem {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseAsset {
    pub path: std::path::PathBuf,
    pub name: String,
}

#[derive(Debug)]
pub struct AssetResolveError {
    message: String,
}

impl std::fmt::Display for AssetResolveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AssetResolveError {}

pub trait AssetResolver {
    fn resolve(
        &self,
        root: &Path,
        declarations: &[AssetDeclaration],
    ) -> Result<Vec<ReleaseAsset>, AssetResolveError>;
}

pub struct SystemAssetResolver;

impl AssetResolver for SystemAssetResolver {
    fn resolve(
        &self,
        root: &Path,
        declarations: &[AssetDeclaration],
    ) -> Result<Vec<ReleaseAsset>, AssetResolveError> {
        let mut assets = Vec::new();
        for declaration in declarations {
            match declaration {
                AssetDeclaration::Path { path, name } => {
                    let path = root.join(path);
                    if !path.is_file() {
                        return Err(AssetResolveError {
                            message: format!(
                                "configured release asset does not exist: {}",
                                path.display()
                            ),
                        });
                    }
                    assets.push(ReleaseAsset {
                        path,
                        name: name.clone(),
                    });
                }
                AssetDeclaration::Glob { pattern } => {
                    let absolute_pattern = root.join(pattern).to_string_lossy().to_string();
                    let mut matched = Vec::new();
                    for entry in
                        glob::glob(&absolute_pattern).map_err(|error| AssetResolveError {
                            message: format!("invalid release asset glob {pattern}: {error}"),
                        })?
                    {
                        let path = entry.map_err(|error| AssetResolveError {
                            message: format!(
                                "failed to resolve release asset glob {pattern}: {error}"
                            ),
                        })?;
                        if path.is_file() {
                            let name = path.file_name().map_or_else(
                                || path.to_string_lossy().to_string(),
                                |name| name.to_string_lossy().to_string(),
                            );
                            matched.push(ReleaseAsset { path, name });
                        }
                    }
                    if matched.is_empty() {
                        return Err(AssetResolveError {
                            message: format!("release asset glob matched no files: {pattern}"),
                        });
                    }
                    assets.extend(matched);
                }
            }
        }
        assets.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.name.cmp(&right.name))
        });
        Ok(assets)
    }
}

pub struct SystemFileSystem;

impl FileSystem for SystemFileSystem {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }
}

pub struct ForgeExecution<'a> {
    pub client: &'a dyn ForgeClient,
    pub file_system: &'a dyn FileSystem,
    pub asset_resolver: &'a dyn AssetResolver,
    pub root: &'a Path,
}

#[derive(Default)]
pub struct SystemPreCheckRunner {
    client: reqwest::Client,
}

const DEFAULT_USER_AGENT: &str = concat!(
    "semifold-engine/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/noctisynth/semifold)"
);
const MAX_ERROR_BODY_BYTES: usize = 4 * 1024;

impl PreCheckRunner for SystemPreCheckRunner {
    fn version_exists<'a>(
        &'a self,
        check: &'a PlannedPreCheck,
        package: &'a PublishPackageContext,
    ) -> ExternalFuture<'a, Result<bool, PreCheckError>> {
        Box::pin(async move {
            match check {
                PlannedPreCheck::Http {
                    url,
                    extra_headers,
                    retry,
                } => self.http_version_exists(url, extra_headers, retry).await,
                PlannedPreCheck::Command {
                    executable,
                    args,
                    environment,
                    working_directory,
                } => command_version_exists(
                    executable,
                    args,
                    environment,
                    working_directory.as_std_path(),
                    package,
                ),
            }
        })
    }
}

impl SystemPreCheckRunner {
    async fn http_version_exists(
        &self,
        url: &str,
        extra_headers: &BTreeMap<String, String>,
        retry: &[u64],
    ) -> Result<bool, PreCheckError> {
        let headers = pre_check_headers(extra_headers)?;
        let mut retry_delays = retry.iter();
        loop {
            let response = self.client.get(url).headers(headers.clone()).send().await;
            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    let Some(delay) = retry_delays.next() else {
                        return Err(PreCheckError {
                            message: format!("registry preflight failed: {error}"),
                        });
                    };
                    tokio::time::sleep(Duration::from_secs(*delay)).await;
                    continue;
                }
            };
            match response.status() {
                reqwest::StatusCode::OK => return Ok(true),
                reqwest::StatusCode::NOT_FOUND => return Ok(false),
                status if retryable_http_status(status) => {
                    let Some(configured_delay) = retry_delays.next() else {
                        return Err(unexpected_http_response(response).await);
                    };
                    let delay = retry_after_delay(response.headers(), SystemTime::now())
                        .unwrap_or_else(|| Duration::from_secs(*configured_delay));
                    tokio::time::sleep(delay).await;
                }
                _ => return Err(unexpected_http_response(response).await),
            }
        }
    }
}

fn pre_check_headers(extra_headers: &BTreeMap<String, String>) -> Result<HeaderMap, PreCheckError> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_USER_AGENT));
    for (name, value) in extra_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| PreCheckError {
            message: format!("invalid registry header name: {error}"),
        })?;
        let value = HeaderValue::from_str(value).map_err(|error| PreCheckError {
            message: format!("invalid registry header value: {error}"),
        })?;
        headers.insert(name, value);
    }
    Ok(headers)
}

fn retryable_http_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::REQUEST_TIMEOUT
            | reqwest::StatusCode::TOO_EARLY
            | reqwest::StatusCode::TOO_MANY_REQUESTS
            | reqwest::StatusCode::INTERNAL_SERVER_ERROR
            | reqwest::StatusCode::BAD_GATEWAY
            | reqwest::StatusCode::SERVICE_UNAVAILABLE
            | reqwest::StatusCode::GATEWAY_TIMEOUT
    )
}

fn retry_after_delay(headers: &HeaderMap, now: SystemTime) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let retry_at = httpdate::parse_http_date(value).ok()?;
    Some(retry_at.duration_since(now).unwrap_or(Duration::ZERO))
}

async fn unexpected_http_response(mut response: reqwest::Response) -> PreCheckError {
    let status = response.status();
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let request_ids = ["x-request-id", "request-id", "x-amzn-requestid", "cf-ray"]
        .into_iter()
        .filter_map(|name| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(|value| format!("{name}={value}"))
        })
        .collect::<Vec<_>>();
    let body = read_limited_response_body(&mut response).await;
    let mut message = format!("registry preflight returned unexpected HTTP status {status}");
    if let Some(retry_after) = retry_after {
        message.push_str(&format!("; Retry-After: {retry_after}"));
    }
    if !request_ids.is_empty() {
        message.push_str(&format!("; request ID: {}", request_ids.join(", ")));
    }
    match body {
        Ok((body, truncated)) if !body.trim().is_empty() => {
            message.push_str(&format!("; response body: {}", body.trim()));
            if truncated {
                message.push_str(" [truncated]");
            }
        }
        Ok(_) => {}
        Err(error) => message.push_str(&format!("; failed to read response body: {error}")),
    }
    PreCheckError { message }
}

async fn read_limited_response_body(
    response: &mut reqwest::Response,
) -> Result<(String, bool), reqwest::Error> {
    let limit_with_probe = MAX_ERROR_BODY_BYTES.saturating_add(1);
    let mut body = Vec::with_capacity(limit_with_probe.min(1024));
    while let Some(chunk) = response.chunk().await? {
        let remaining = limit_with_probe.saturating_sub(body.len());
        if chunk.len() > remaining {
            body.extend_from_slice(&chunk[..remaining]);
            break;
        }
        body.extend_from_slice(&chunk);
        if body.len() == limit_with_probe {
            break;
        }
    }
    let truncated = body.len() > MAX_ERROR_BODY_BYTES;
    body.truncate(MAX_ERROR_BODY_BYTES);
    Ok((String::from_utf8_lossy(&body).into_owned(), truncated))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandPreCheckOutput {
    exists: bool,
}

fn command_version_exists(
    executable: &str,
    args: &[String],
    environment: &std::collections::BTreeMap<String, String>,
    working_directory: &Path,
    package: &PublishPackageContext,
) -> Result<bool, PreCheckError> {
    let mut input = serde_json::to_vec(package).map_err(|error| PreCheckError {
        message: format!("failed to serialize command pre-check input: {error}"),
    })?;
    input.push(b'\n');
    let mut child = Command::new(executable)
        .args(args)
        .envs(environment)
        .current_dir(working_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| PreCheckError {
            message: format!("failed to run pre-check command {executable}: {error}"),
        })?;
    let mut stdin = child.stdin.take().ok_or_else(|| PreCheckError {
        message: format!("pre-check command {executable} did not provide piped stdin"),
    })?;
    if let Err(error) = stdin.write_all(&input) {
        drop(stdin);
        let _ = child.wait();
        return Err(PreCheckError {
            message: format!("failed to write pre-check input for {executable}: {error}"),
        });
    }
    drop(stdin);
    let output = child.wait_with_output().map_err(|error| PreCheckError {
        message: format!("failed to wait for pre-check command {executable}: {error}"),
    })?;
    if !output.status.success() {
        return Err(PreCheckError {
            message: format!(
                "pre-check command {executable} exited with status {:?}",
                output.status.code()
            ),
        });
    }
    let stdout = std::str::from_utf8(&output.stdout).map_err(|error| PreCheckError {
        message: format!("pre-check command {executable} returned non-UTF-8 stdout: {error}"),
    })?;
    parse_command_pre_check_output(executable, stdout)
}

fn parse_command_pre_check_output(executable: &str, stdout: &str) -> Result<bool, PreCheckError> {
    let trimmed = stdout.trim();
    if trimmed.contains(['\n', '\r']) {
        return Err(PreCheckError {
            message: format!("pre-check command {executable} returned more than one line"),
        });
    }
    let result: CommandPreCheckOutput =
        serde_json::from_str(trimmed).map_err(|error| PreCheckError {
            message: format!("pre-check command {executable} returned invalid JSON: {error}"),
        })?;
    Ok(result.exists)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublishStatus {
    Succeeded,
    Skipped(PublishSkipReason),
    Failed(PublishFailureStage),
    NotStarted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishFailureStage {
    Preflight,
    Command(CommandPhase),
    ForgeRelease,
    AssetUpload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandDisposition {
    Executed,
    SkippedDryRun,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandReport {
    pub phase: CommandPhase,
    pub executable: String,
    pub disposition: CommandDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackagePublishReport {
    pub package: PackageId,
    pub status: PublishStatus,
    pub commands: Vec<CommandReport>,
    pub forge: ForgeDisposition,
    pub error: Option<String>,
    pub github_failure: Option<GitHubFailure>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForgeDisposition {
    NotRequested,
    SkippedDryRun,
    Created,
    AlreadyExists,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishReport {
    pub packages: Vec<PackagePublishReport>,
}

#[derive(Debug)]
pub struct PublishExecutionError {
    pub report: PublishReport,
}

impl std::fmt::Display for PublishExecutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let failed = self
            .report
            .packages
            .iter()
            .find(|package| matches!(package.status, PublishStatus::Failed(_)))
            .map(|package| {
                (
                    package.package.as_str(),
                    package.error.as_deref().unwrap_or_default(),
                )
            });
        let Some((package, error)) = failed else {
            return formatter.write_str("publish execution failed");
        };
        let not_started = self
            .report
            .packages
            .iter()
            .filter(|package| package.status == PublishStatus::NotStarted)
            .count();
        write!(
            formatter,
            "publishing failed for {package}: {error}; {not_started} package(s) were not started"
        )
    }
}

impl std::error::Error for PublishExecutionError {}

pub async fn execute_publish_plan<C, R>(
    plan: &mut PublishPlan,
    command_runner: &C,
    registry_client: &R,
    forge: Option<ForgeExecution<'_>>,
    dry_run: bool,
) -> Result<PublishReport, PublishExecutionError>
where
    C: CommandRunner,
    R: PreCheckRunner,
{
    let mut report = PublishReport {
        packages: plan
            .packages
            .iter()
            .map(|package| PackagePublishReport {
                package: package.context.package.id.clone(),
                status: PublishStatus::NotStarted,
                commands: Vec::new(),
                forge: ForgeDisposition::NotRequested,
                error: None,
                github_failure: None,
            })
            .collect(),
    };

    for (package, package_report) in plan.packages.iter_mut().zip(&mut report.packages) {
        if let Some(skip_reason @ PublishSkipReason::MissingChangelog) = package.skip_reason {
            package_report.status = PublishStatus::Skipped(skip_reason);
            continue;
        }
        if package.context.package.private {
            continue;
        }
        let Some(preflight) = &package.preflight else {
            continue;
        };
        match registry_client
            .version_exists(preflight, &package.context.package)
            .await
        {
            Ok(true) => {
                package.skip_reason = Some(PublishSkipReason::RegistryVersionExists);
                package_report.status =
                    PublishStatus::Skipped(PublishSkipReason::RegistryVersionExists);
            }
            Ok(false) => {}
            Err(error) => {
                package_report.status = PublishStatus::Failed(PublishFailureStage::Preflight);
                package_report.error = Some(error.to_string());
                return Err(PublishExecutionError { report });
            }
        }
    }

    for (package, package_report) in plan.packages.iter().zip(&mut report.packages) {
        if matches!(
            package_report.status,
            PublishStatus::Skipped(PublishSkipReason::MissingChangelog)
        ) {
            continue;
        }
        let registry_version_exists = matches!(
            package_report.status,
            PublishStatus::Skipped(PublishSkipReason::RegistryVersionExists)
        );
        if !package.context.package.private && !registry_version_exists {
            for command in &package.commands {
                if dry_run && !command.run_in_dry_run {
                    package_report.commands.push(CommandReport {
                        phase: command.phase,
                        executable: command.executable.clone(),
                        disposition: CommandDisposition::SkippedDryRun,
                    });
                    continue;
                }
                if let Err(error) = command_runner.run(command) {
                    package_report.status =
                        PublishStatus::Failed(PublishFailureStage::Command(command.phase));
                    package_report.error = Some(error.to_string());
                    return Err(PublishExecutionError { report });
                }
                package_report.commands.push(CommandReport {
                    phase: command.phase,
                    executable: command.executable.clone(),
                    disposition: CommandDisposition::Executed,
                });
            }
        }
        if let Some(forge) = &forge
            && let Some(forge_plan) = &package.forge
        {
            if dry_run {
                package_report.forge = ForgeDisposition::SkippedDryRun;
            } else {
                let release_id = match forge.client.create_release(&forge_plan.release).await {
                    Ok(ForgeReleaseOutcome::Created(release_id)) => {
                        package_report.forge = ForgeDisposition::Created;
                        Some(release_id)
                    }
                    Ok(ForgeReleaseOutcome::AlreadyExists) => {
                        package_report.forge = ForgeDisposition::AlreadyExists;
                        None
                    }
                    Err(error) => {
                        package_report.status =
                            PublishStatus::Failed(PublishFailureStage::ForgeRelease);
                        package_report.error = Some(error.to_string());
                        package_report.github_failure = error.github;
                        return Err(PublishExecutionError { report });
                    }
                };
                if let Some(release_id) = release_id {
                    let assets = match forge.asset_resolver.resolve(forge.root, &package.assets) {
                        Ok(assets) => assets,
                        Err(error) => {
                            package_report.status =
                                PublishStatus::Failed(PublishFailureStage::AssetUpload);
                            package_report.error = Some(error.to_string());
                            return Err(PublishExecutionError { report });
                        }
                    };
                    for asset in &assets {
                        let content = match forge.file_system.read(&asset.path) {
                            Ok(content) => content,
                            Err(error) => {
                                package_report.status =
                                    PublishStatus::Failed(PublishFailureStage::AssetUpload);
                                package_report.error = Some(error.to_string());
                                return Err(PublishExecutionError { report });
                            }
                        };
                        if let Err(error) = forge
                            .client
                            .upload_asset(&forge_plan.release, &release_id, &asset.name, content)
                            .await
                        {
                            package_report.status =
                                PublishStatus::Failed(PublishFailureStage::AssetUpload);
                            package_report.error = Some(error.to_string());
                            package_report.github_failure = error.github;
                            return Err(PublishExecutionError { report });
                        }
                    }
                }
            }
        }
        if !registry_version_exists {
            package_report.status = if package.context.package.private && package.forge.is_none() {
                PublishStatus::Skipped(PublishSkipReason::Private)
            } else {
                PublishStatus::Succeeded
            };
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{Mutex, mpsc},
        thread,
    };

    use camino::Utf8PathBuf;
    use semifold_core::{EcosystemId, PackageId};

    use super::*;
    use crate::publish_plan::{PackagePublish, PublishContext, PublishPackageContext, PublishPlan};

    fn serve_http_responses(responses: Vec<String>) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                loop {
                    let read = stream.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let _ = sender.send(String::from_utf8_lossy(&request).into_owned());
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        (format!("http://{address}"), receiver)
    }

    #[tokio::test]
    async fn github_adapter_preserves_api_errors_for_release_and_asset_operations() {
        let body = r#"{"message":"Validation Failed","errors":[{"resource":"Release","field":"tag_name","code":"invalid"}],"documentation_url":"https://docs.github.com/rest/releases"}"#;
        let response = format!(
            "HTTP/1.1 422 Unprocessable Entity\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let existing_body =
            r#"{"message":"Validation Failed","errors":[{"code":"already_exists"}]}"#;
        let existing_response = format!(
            "HTTP/1.1 422 Unprocessable Entity\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{existing_body}",
            existing_body.len()
        );
        let (url, _requests) =
            serve_http_responses(vec![response.clone(), response, existing_response]);
        let client = octocrab::Octocrab::builder()
            .base_uri(url.as_str())
            .unwrap()
            .upload_uri(url.as_str())
            .unwrap()
            .build()
            .unwrap();
        let forge = GithubForgeClient::new(client);
        let release = ForgeRelease {
            owner: "owner".into(),
            repository: "repo".into(),
            tag: "pkg-v1.0.0".into(),
            title: "pkg".into(),
            body: "changes".into(),
            prerelease: false,
        };
        let errors = [
            (
                forge.create_release(&release).await.unwrap_err(),
                GitHubOperation::CreateRelease,
            ),
            (
                forge
                    .upload_asset(&release, &ForgeReleaseId(1), "asset.txt", vec![])
                    .await
                    .unwrap_err(),
                GitHubOperation::UploadAsset,
            ),
        ];
        assert_eq!(
            forge.create_release(&release).await.unwrap(),
            ForgeReleaseOutcome::AlreadyExists
        );
        for (error, operation) in errors {
            let diagnostic = error.github.unwrap();
            assert_eq!(diagnostic.operation, operation);
            assert_eq!(diagnostic.diagnostic.status_code, Some(422));
            assert_eq!(diagnostic.diagnostic.message, "Validation Failed");
            assert!(
                diagnostic
                    .diagnostic
                    .details
                    .iter()
                    .any(|detail| detail.contains("tag_name"))
            );
            assert_eq!(
                diagnostic.diagnostic.documentation_url.as_deref(),
                Some("https://docs.github.com/rest/releases")
            );
        }
    }

    #[tokio::test]
    async fn github_release_failure_reaches_the_publish_report() {
        let body = r#"{"message":"Resource not accessible by integration"}"#;
        let response = format!(
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (url, _requests) = serve_http_responses(vec![response]);
        let forge = GithubForgeClient::new(
            octocrab::Octocrab::builder()
                .base_uri(url)
                .unwrap()
                .build()
                .unwrap(),
        );
        let mut plan = publish_plan(vec![package("pkg", false, vec![])]);
        plan.packages[0].forge = Some(crate::publish_plan::PackageForgePlan {
            release: ForgeRelease {
                owner: "owner".into(),
                repository: "repo".into(),
                tag: "pkg-v1.0.0".into(),
                title: "pkg".into(),
                body: "changes".into(),
                prerelease: false,
            },
        });
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let registry = StaticRegistry {
            existing: Vec::new(),
            checked: Mutex::new(Vec::new()),
        };
        let error = execute_publish_plan(
            &mut plan,
            &runner,
            &registry,
            Some(ForgeExecution {
                client: &forge,
                file_system: &StaticFileSystem,
                asset_resolver: &StaticAssetResolver,
                root: Path::new("."),
            }),
            false,
        )
        .await
        .unwrap_err();
        let package = &error.report.packages[0];
        assert_eq!(
            package.status,
            PublishStatus::Failed(PublishFailureStage::ForgeRelease)
        );
        assert_eq!(
            package
                .github_failure
                .as_ref()
                .unwrap()
                .diagnostic
                .status_code,
            Some(403)
        );
        assert!(
            package
                .error
                .as_ref()
                .unwrap()
                .contains("Resource not accessible by integration")
        );
    }

    struct RecordingRunner {
        commands: Mutex<Vec<String>>,
        fail: Option<String>,
    }

    impl CommandRunner for RecordingRunner {
        fn run(&self, command: &CommandSpec) -> Result<CommandOutput, CommandError> {
            self.commands
                .lock()
                .expect("recording command mutex is not poisoned")
                .push(command.executable.clone());
            if self.fail.as_deref() == Some(&command.executable) {
                return Err(CommandError {
                    message: "planned failure".to_string(),
                });
            }
            Ok(CommandOutput { exit_code: Some(0) })
        }
    }

    struct StaticRegistry {
        existing: Vec<String>,
        checked: Mutex<Vec<String>>,
    }

    impl PreCheckRunner for StaticRegistry {
        fn version_exists<'a>(
            &'a self,
            check: &'a PlannedPreCheck,
            _package: &'a PublishPackageContext,
        ) -> ExternalFuture<'a, Result<bool, PreCheckError>> {
            Box::pin(async move {
                let PlannedPreCheck::Http { url, .. } = check else {
                    return Err(PreCheckError {
                        message: "unexpected command pre-check in test".to_string(),
                    });
                };
                self.checked
                    .lock()
                    .expect("recording registry mutex is not poisoned")
                    .push(url.clone());
                Ok(self.existing.contains(url))
            })
        }
    }

    struct RecordingForge {
        created: Mutex<Vec<String>>,
        uploaded: Mutex<Vec<String>>,
        outcome: ForgeReleaseOutcome,
    }

    impl ForgeClient for RecordingForge {
        fn create_release<'a>(
            &'a self,
            release: &'a ForgeRelease,
        ) -> ExternalFuture<'a, Result<ForgeReleaseOutcome, ForgeError>> {
            Box::pin(async move {
                self.created
                    .lock()
                    .expect("recording forge mutex is not poisoned")
                    .push(release.tag.clone());
                Ok(self.outcome.clone())
            })
        }

        fn upload_asset<'a>(
            &'a self,
            _release: &'a ForgeRelease,
            _release_id: &'a ForgeReleaseId,
            name: &'a str,
            _content: Vec<u8>,
        ) -> ExternalFuture<'a, Result<(), ForgeError>> {
            Box::pin(async move {
                self.uploaded
                    .lock()
                    .expect("recording forge mutex is not poisoned")
                    .push(name.to_string());
                Ok(())
            })
        }
    }

    struct StaticFileSystem;

    impl FileSystem for StaticFileSystem {
        fn read(&self, _path: &Path) -> std::io::Result<Vec<u8>> {
            Ok(vec![1, 2, 3])
        }
    }

    struct StaticAssetResolver;

    impl AssetResolver for StaticAssetResolver {
        fn resolve(
            &self,
            _root: &Path,
            _declarations: &[AssetDeclaration],
        ) -> Result<Vec<ReleaseAsset>, AssetResolveError> {
            Ok(vec![ReleaseAsset {
                path: "artifact.tar.gz".into(),
                name: "artifact.tar.gz".to_string(),
            }])
        }
    }

    struct PostCommandAssetResolver<'a> {
        commands: &'a Mutex<Vec<String>>,
    }

    impl AssetResolver for PostCommandAssetResolver<'_> {
        fn resolve(
            &self,
            _root: &Path,
            _declarations: &[AssetDeclaration],
        ) -> Result<Vec<ReleaseAsset>, AssetResolveError> {
            let commands = self
                .commands
                .lock()
                .expect("recording command mutex is not poisoned");
            if commands.as_slice() != ["build-assets"] {
                return Err(AssetResolveError {
                    message: "assets resolved before package commands".to_string(),
                });
            }
            Ok(vec![ReleaseAsset {
                path: "generated.tar.gz".into(),
                name: "generated.tar.gz".to_string(),
            }])
        }
    }

    fn package(id: &str, private: bool, commands: Vec<CommandSpec>) -> PackagePublish {
        PackagePublish {
            context: PublishContext {
                package: PublishPackageContext {
                    id: PackageId::new(id),
                    name: id.to_string(),
                    ecosystem: EcosystemId::RUST,
                    version: semver::Version::new(1, 0, 0),
                    tag: format!("{id}-v1.0.0"),
                    path: Utf8PathBuf::from(id),
                    private,
                },
                repository: None,
                ci: None,
            },
            preflight: Some(PlannedPreCheck::Http {
                url: id.to_string(),
                extra_headers: Default::default(),
                retry: Vec::new(),
            }),
            commands,
            assets: Vec::new(),
            forge: None,
            skip_reason: None,
        }
    }

    fn publish_plan(packages: Vec<PackagePublish>) -> PublishPlan {
        PublishPlan {
            project_root: Utf8PathBuf::from("."),
            packages,
        }
    }

    fn command(executable: &str, run_in_dry_run: bool) -> CommandSpec {
        CommandSpec {
            executable: executable.to_string(),
            args: Vec::new(),
            environment: Default::default(),
            working_directory: Utf8PathBuf::from("."),
            phase: CommandPhase::Publish,
            stdout: StdioPolicy::Null,
            stderr: StdioPolicy::Null,
            run_in_dry_run,
        }
    }

    #[tokio::test]
    async fn completes_all_preflights_before_running_commands() {
        let mut plan = publish_plan(vec![
            package("core", false, vec![command("core-publish", false)]),
            package("app", false, vec![command("app-publish", false)]),
        ]);
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let registry = StaticRegistry {
            existing: vec!["app".to_string()],
            checked: Mutex::new(Vec::new()),
        };

        let report = execute_publish_plan(&mut plan, &runner, &registry, None, false)
            .await
            .unwrap();

        assert_eq!(
            *registry
                .checked
                .lock()
                .expect("recording registry mutex is not poisoned"),
            ["core", "app"]
        );
        assert_eq!(
            *runner
                .commands
                .lock()
                .expect("recording command mutex is not poisoned"),
            ["core-publish"]
        );
        assert_eq!(report.packages[0].status, PublishStatus::Succeeded);
        assert_eq!(
            report.packages[1].status,
            PublishStatus::Skipped(PublishSkipReason::RegistryVersionExists)
        );
    }

    #[tokio::test]
    async fn registry_version_skip_still_creates_forge_release_with_current_assets() {
        let forge = RecordingForge {
            created: Mutex::new(Vec::new()),
            uploaded: Mutex::new(Vec::new()),
            outcome: ForgeReleaseOutcome::Created(ForgeReleaseId(7)),
        };
        let file_system = StaticFileSystem;
        let asset_resolver = StaticAssetResolver;
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let registry = StaticRegistry {
            existing: vec!["core".to_string()],
            checked: Mutex::new(Vec::new()),
        };
        let mut package = package("core", false, vec![command("must-not-run", false)]);
        package.forge = Some(crate::publish_plan::PackageForgePlan {
            release: ForgeRelease {
                owner: "owner".to_string(),
                repository: "repo".to_string(),
                tag: "core-v1.0.0".to_string(),
                title: "core v1.0.0".to_string(),
                body: "changes".to_string(),
                prerelease: false,
            },
        });
        package.assets = vec![AssetDeclaration::Glob {
            pattern: "artifact*.tar.gz".to_string(),
        }];
        let mut plan = publish_plan(vec![package]);

        let report = execute_publish_plan(
            &mut plan,
            &runner,
            &registry,
            Some(ForgeExecution {
                client: &forge,
                file_system: &file_system,
                asset_resolver: &asset_resolver,
                root: Path::new("."),
            }),
            false,
        )
        .await
        .unwrap();

        assert_eq!(
            report.packages[0].status,
            PublishStatus::Skipped(PublishSkipReason::RegistryVersionExists)
        );
        assert_eq!(report.packages[0].forge, ForgeDisposition::Created);
        assert!(
            runner
                .commands
                .lock()
                .expect("recording command mutex is not poisoned")
                .is_empty()
        );
        assert_eq!(
            *forge
                .created
                .lock()
                .expect("recording forge mutex is not poisoned"),
            ["core-v1.0.0"]
        );
        assert_eq!(
            *forge
                .uploaded
                .lock()
                .expect("recording forge mutex is not poisoned"),
            ["artifact.tar.gz"]
        );
    }

    #[tokio::test]
    async fn existing_forge_release_does_not_resolve_or_recover_assets() {
        struct FailingAssetResolver;

        impl AssetResolver for FailingAssetResolver {
            fn resolve(
                &self,
                _root: &Path,
                _declarations: &[AssetDeclaration],
            ) -> Result<Vec<ReleaseAsset>, AssetResolveError> {
                Err(AssetResolveError {
                    message: "assets must not be resolved".to_string(),
                })
            }
        }

        let forge = RecordingForge {
            created: Mutex::new(Vec::new()),
            uploaded: Mutex::new(Vec::new()),
            outcome: ForgeReleaseOutcome::AlreadyExists,
        };
        let file_system = StaticFileSystem;
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let registry = StaticRegistry {
            existing: vec!["core".to_string()],
            checked: Mutex::new(Vec::new()),
        };
        let mut package = package("core", false, vec![command("must-not-run", false)]);
        package.forge = Some(crate::publish_plan::PackageForgePlan {
            release: ForgeRelease {
                owner: "owner".to_string(),
                repository: "repo".to_string(),
                tag: "core-v1.0.0".to_string(),
                title: "core v1.0.0".to_string(),
                body: "changes".to_string(),
                prerelease: false,
            },
        });
        package.assets = vec![AssetDeclaration::Glob {
            pattern: "artifact*.tar.gz".to_string(),
        }];
        let mut plan = publish_plan(vec![package]);

        let report = execute_publish_plan(
            &mut plan,
            &runner,
            &registry,
            Some(ForgeExecution {
                client: &forge,
                file_system: &file_system,
                asset_resolver: &FailingAssetResolver,
                root: Path::new("."),
            }),
            false,
        )
        .await
        .unwrap();

        assert_eq!(
            report.packages[0].status,
            PublishStatus::Skipped(PublishSkipReason::RegistryVersionExists)
        );
        assert_eq!(report.packages[0].forge, ForgeDisposition::AlreadyExists);
        assert!(
            runner
                .commands
                .lock()
                .expect("recording command mutex is not poisoned")
                .is_empty()
        );
        assert!(
            forge
                .uploaded
                .lock()
                .expect("recording forge mutex is not poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn dry_run_only_executes_explicitly_allowed_commands() {
        let mut plan = publish_plan(vec![package(
            "core",
            false,
            vec![command("skip", false), command("execute", true)],
        )]);
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let registry = StaticRegistry {
            existing: Vec::new(),
            checked: Mutex::new(Vec::new()),
        };

        let report = execute_publish_plan(&mut plan, &runner, &registry, None, true)
            .await
            .unwrap();

        assert_eq!(
            *runner
                .commands
                .lock()
                .expect("recording command mutex is not poisoned"),
            ["execute"]
        );
        assert_eq!(
            report.packages[0]
                .commands
                .iter()
                .map(|command| command.disposition)
                .collect::<Vec<_>>(),
            [
                CommandDisposition::SkippedDryRun,
                CommandDisposition::Executed
            ]
        );
    }

    #[tokio::test]
    async fn command_failure_marks_following_packages_not_started() {
        let mut plan = publish_plan(vec![
            package("core", false, vec![command("fail", false)]),
            package("app", false, vec![command("app", false)]),
        ]);
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: Some("fail".to_string()),
        };
        let registry = StaticRegistry {
            existing: Vec::new(),
            checked: Mutex::new(Vec::new()),
        };

        let error = execute_publish_plan(&mut plan, &runner, &registry, None, false)
            .await
            .unwrap_err();

        assert_eq!(
            error.report.packages[0].status,
            PublishStatus::Failed(PublishFailureStage::Command(CommandPhase::Publish))
        );
        assert_eq!(error.report.packages[1].status, PublishStatus::NotStarted);
    }

    #[tokio::test]
    async fn missing_changelog_skips_registry_and_commands() {
        let mut missing = package("core", false, vec![command("publish", false)]);
        missing.skip_reason = Some(PublishSkipReason::MissingChangelog);
        let mut plan = publish_plan(vec![missing]);
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let registry = StaticRegistry {
            existing: Vec::new(),
            checked: Mutex::new(Vec::new()),
        };

        let report = execute_publish_plan(&mut plan, &runner, &registry, None, false)
            .await
            .unwrap();

        assert_eq!(
            report.packages[0].status,
            PublishStatus::Skipped(PublishSkipReason::MissingChangelog)
        );
        assert!(
            registry
                .checked
                .lock()
                .expect("recording registry mutex is not poisoned")
                .is_empty()
        );
        assert!(
            runner
                .commands
                .lock()
                .expect("recording command mutex is not poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn private_package_skips_registry_commands_but_creates_forge_release_with_assets() {
        let forge = RecordingForge {
            created: Mutex::new(Vec::new()),
            uploaded: Mutex::new(Vec::new()),
            outcome: ForgeReleaseOutcome::Created(ForgeReleaseId(7)),
        };
        let file_system = StaticFileSystem;
        let asset_resolver = StaticAssetResolver;
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let registry = StaticRegistry {
            existing: Vec::new(),
            checked: Mutex::new(Vec::new()),
        };
        let mut plan = publish_plan(vec![package(
            "private",
            true,
            vec![command("must-not-run", false)],
        )]);
        plan.packages[0].forge = Some(crate::publish_plan::PackageForgePlan {
            release: ForgeRelease {
                owner: "owner".to_string(),
                repository: "repo".to_string(),
                tag: "private-v1.0.0".to_string(),
                title: "private v1.0.0".to_string(),
                body: "changes".to_string(),
                prerelease: false,
            },
        });
        plan.packages[0].assets = vec![AssetDeclaration::Glob {
            pattern: "artifact*.tar.gz".to_string(),
        }];

        let report = execute_publish_plan(
            &mut plan,
            &runner,
            &registry,
            Some(ForgeExecution {
                client: &forge,
                file_system: &file_system,
                asset_resolver: &asset_resolver,
                root: Path::new("."),
            }),
            false,
        )
        .await
        .unwrap();

        assert_eq!(report.packages[0].status, PublishStatus::Succeeded);
        assert_eq!(report.packages[0].forge, ForgeDisposition::Created);
        assert!(
            registry
                .checked
                .lock()
                .expect("recording registry mutex is not poisoned")
                .is_empty()
        );
        assert!(
            runner
                .commands
                .lock()
                .expect("recording command mutex is not poisoned")
                .is_empty()
        );
        assert_eq!(
            *forge
                .uploaded
                .lock()
                .expect("recording forge mutex is not poisoned"),
            ["artifact.tar.gz"]
        );
    }

    #[tokio::test]
    async fn resolves_assets_after_package_commands() {
        let forge = RecordingForge {
            created: Mutex::new(Vec::new()),
            uploaded: Mutex::new(Vec::new()),
            outcome: ForgeReleaseOutcome::Created(ForgeReleaseId(7)),
        };
        let file_system = StaticFileSystem;
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let asset_resolver = PostCommandAssetResolver {
            commands: &runner.commands,
        };
        let registry = StaticRegistry {
            existing: Vec::new(),
            checked: Mutex::new(Vec::new()),
        };
        let mut plan = publish_plan(vec![package(
            "core",
            false,
            vec![command("build-assets", false)],
        )]);
        plan.packages[0].forge = Some(crate::publish_plan::PackageForgePlan {
            release: ForgeRelease {
                owner: "owner".to_string(),
                repository: "repo".to_string(),
                tag: "core-v1.0.0".to_string(),
                title: "core v1.0.0".to_string(),
                body: "changes".to_string(),
                prerelease: false,
            },
        });
        plan.packages[0].assets = vec![AssetDeclaration::Glob {
            pattern: "generated*.tar.gz".to_string(),
        }];

        let report = execute_publish_plan(
            &mut plan,
            &runner,
            &registry,
            Some(ForgeExecution {
                client: &forge,
                file_system: &file_system,
                asset_resolver: &asset_resolver,
                root: Path::new("."),
            }),
            false,
        )
        .await
        .unwrap();

        assert_eq!(report.packages[0].status, PublishStatus::Succeeded);
        assert_eq!(
            *forge
                .uploaded
                .lock()
                .expect("recording forge mutex is not poisoned"),
            ["generated.tar.gz"]
        );
    }

    #[tokio::test]
    async fn forge_release_and_assets_use_ports_and_are_skipped_in_dry_run() {
        let forge = RecordingForge {
            created: Mutex::new(Vec::new()),
            uploaded: Mutex::new(Vec::new()),
            outcome: ForgeReleaseOutcome::Created(ForgeReleaseId(7)),
        };
        let file_system = StaticFileSystem;
        let asset_resolver = StaticAssetResolver;
        let root = Path::new(".");
        let registry = StaticRegistry {
            existing: Vec::new(),
            checked: Mutex::new(Vec::new()),
        };
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let mut plan = publish_plan(vec![package("core", false, Vec::new())]);
        plan.packages[0].forge = Some(crate::publish_plan::PackageForgePlan {
            release: ForgeRelease {
                owner: "owner".to_string(),
                repository: "repo".to_string(),
                tag: "core-v1.0.0".to_string(),
                title: "core v1.0.0".to_string(),
                body: "changes".to_string(),
                prerelease: false,
            },
        });
        plan.packages[0].assets = vec![AssetDeclaration::Glob {
            pattern: "artifact*.tar.gz".to_string(),
        }];

        let report = execute_publish_plan(
            &mut plan,
            &runner,
            &registry,
            Some(ForgeExecution {
                client: &forge,
                file_system: &file_system,
                asset_resolver: &asset_resolver,
                root,
            }),
            false,
        )
        .await
        .unwrap();
        assert_eq!(report.packages[0].forge, ForgeDisposition::Created);
        assert_eq!(
            *forge
                .created
                .lock()
                .expect("recording forge mutex is not poisoned"),
            ["core-v1.0.0"]
        );
        assert_eq!(
            *forge
                .uploaded
                .lock()
                .expect("recording forge mutex is not poisoned"),
            ["artifact.tar.gz"]
        );

        forge
            .created
            .lock()
            .expect("recording forge mutex is not poisoned")
            .clear();
        forge
            .uploaded
            .lock()
            .expect("recording forge mutex is not poisoned")
            .clear();
        let mut plan = publish_plan(vec![package("core", false, Vec::new())]);
        plan.packages[0].forge = Some(crate::publish_plan::PackageForgePlan {
            release: ForgeRelease {
                owner: "owner".to_string(),
                repository: "repo".to_string(),
                tag: "core-v1.0.0".to_string(),
                title: "core v1.0.0".to_string(),
                body: "changes".to_string(),
                prerelease: false,
            },
        });
        plan.packages[0].assets = vec![AssetDeclaration::Glob {
            pattern: "artifact*.tar.gz".to_string(),
        }];
        let report = execute_publish_plan(
            &mut plan,
            &runner,
            &registry,
            Some(ForgeExecution {
                client: &forge,
                file_system: &file_system,
                asset_resolver: &asset_resolver,
                root,
            }),
            true,
        )
        .await
        .unwrap();
        assert_eq!(report.packages[0].forge, ForgeDisposition::SkippedDryRun);
        assert!(
            forge
                .created
                .lock()
                .expect("recording forge mutex is not poisoned")
                .is_empty()
        );
        assert!(
            forge
                .uploaded
                .lock()
                .expect("recording forge mutex is not poisoned")
                .is_empty()
        );
    }

    #[test]
    fn command_pre_check_output_requires_one_exact_json_object() {
        assert!(parse_command_pre_check_output("check", "{\"exists\":true}\n").unwrap());
        assert!(!parse_command_pre_check_output("check", "{\"exists\":false}").unwrap());
        assert!(parse_command_pre_check_output("check", "true").is_err());
        assert!(parse_command_pre_check_output("check", "{\"exists\":true,\"extra\":1}").is_err());
        assert!(parse_command_pre_check_output("check", "{\"exists\":true}\nnoise").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn command_pre_check_receives_package_json_on_stdin() {
        let package = package("core", false, Vec::new());
        let result = command_version_exists(
            "sh",
            &[
                "-c".to_string(),
                "read input; case \"$input\" in *'\"name\":\"core\"'*) printf '{\"exists\":true}' ;; *) exit 9 ;; esac"
                    .to_string(),
            ],
            &Default::default(),
            Path::new("."),
            &package.context.package,
        )
        .unwrap();

        assert!(result);
    }

    #[test]
    fn http_pre_check_classifies_only_transient_statuses_as_retryable() {
        for status in [
            reqwest::StatusCode::REQUEST_TIMEOUT,
            reqwest::StatusCode::TOO_EARLY,
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            reqwest::StatusCode::BAD_GATEWAY,
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            reqwest::StatusCode::GATEWAY_TIMEOUT,
        ] {
            assert!(retryable_http_status(status));
        }
        for status in [
            reqwest::StatusCode::BAD_REQUEST,
            reqwest::StatusCode::UNAUTHORIZED,
            reqwest::StatusCode::FORBIDDEN,
            reqwest::StatusCode::NOT_FOUND,
        ] {
            assert!(!retryable_http_status(status));
        }
    }

    #[test]
    fn http_pre_check_parses_both_retry_after_formats() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("12"));
        assert_eq!(
            retry_after_delay(&headers, now),
            Some(Duration::from_secs(12))
        );

        let retry_at = now + Duration::from_secs(30);
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_str(&httpdate::fmt_http_date(retry_at)).unwrap(),
        );
        assert_eq!(
            retry_after_delay(&headers, now),
            Some(Duration::from_secs(30))
        );
    }

    #[tokio::test]
    async fn http_pre_check_injects_default_user_agent_and_allows_override() {
        for (configured, expected) in [
            (BTreeMap::new(), DEFAULT_USER_AGENT),
            (
                BTreeMap::from([("user-agent".to_string(), "custom-agent".to_string())]),
                "custom-agent",
            ),
        ] {
            let (url, requests) = serve_http_responses(vec![
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_string(),
            ]);
            let exists = SystemPreCheckRunner::default()
                .http_version_exists(&url, &configured, &[])
                .await
                .unwrap();
            assert!(!exists);
            let request = requests.recv().unwrap().to_ascii_lowercase();
            assert!(request.contains(&format!("user-agent: {}", expected.to_ascii_lowercase())));
        }
    }

    #[tokio::test]
    async fn http_pre_check_retries_transient_failures_and_reports_final_response() {
        let (url, requests) = serve_http_responses(vec![
            "HTTP/1.1 503 Service Unavailable\r\nRetry-After: 0\r\nContent-Length: 9\r\nConnection: close\r\n\r\ntry later"
                .to_string(),
            "HTTP/1.1 403 Forbidden\r\nX-Request-Id: request-123\r\nContent-Length: 6\r\nConnection: close\r\n\r\ndenied"
                .to_string(),
        ]);

        let error = SystemPreCheckRunner::default()
            .http_version_exists(&url, &BTreeMap::new(), &[0])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("403 Forbidden"));
        assert!(error.to_string().contains("x-request-id=request-123"));
        assert!(error.to_string().contains("response body: denied"));
        assert_eq!(requests.iter().count(), 2);
    }

    #[tokio::test]
    async fn http_pre_check_truncates_large_error_responses() {
        let body = "x".repeat(MAX_ERROR_BODY_BYTES + 100);
        let response = format!(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (url, _) = serve_http_responses(vec![response]);

        let error = SystemPreCheckRunner::default()
            .http_version_exists(&url, &BTreeMap::new(), &[])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("[truncated]"));
    }
}
