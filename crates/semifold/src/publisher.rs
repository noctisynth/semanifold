use std::{
    collections::BTreeMap,
    future::Future,
    path::Path,
    pin::Pin,
    process::{Command, Stdio},
};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rust_i18n::t;
use semifold_core::PackageId;

use crate::publish_plan::{
    CommandPhase, CommandSpec, PlannedRegistryCheck, PublishPlan, PublishSkipReason, ReleaseAsset,
    StdioPolicy,
};

pub(crate) type ExternalFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommandOutput {
    pub exit_code: Option<i32>,
}

#[derive(Debug)]
pub(crate) struct CommandError {
    message: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

pub(crate) trait CommandRunner {
    fn run(&self, command: &CommandSpec) -> Result<CommandOutput, CommandError>;
}

pub(crate) struct SystemCommandRunner;

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
                message: t!(
                    "cli.publish.command_spawn_failed",
                    command = command.executable,
                    error = error
                )
                .to_string(),
            })?;
        if !status.success() {
            return Err(CommandError {
                message: t!(
                    "cli.publish.command_failed",
                    command = command.executable,
                    status = format!("{:?}", status.code())
                )
                .to_string(),
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
pub(crate) struct RegistryError {
    message: String,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RegistryError {}

pub(crate) trait RegistryClient {
    fn version_exists<'a>(
        &'a self,
        check: &'a PlannedRegistryCheck,
    ) -> ExternalFuture<'a, Result<bool, RegistryError>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ForgeReleaseId(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ForgeRelease {
    pub owner: String,
    pub repository: String,
    pub tag: String,
    pub title: String,
    pub body: String,
    pub prerelease: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ForgeReleaseOutcome {
    Created(ForgeReleaseId),
    AlreadyExists,
}

#[derive(Debug)]
pub(crate) struct ForgeError {
    message: String,
}

impl std::fmt::Display for ForgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ForgeError {}

pub(crate) trait ForgeClient {
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

pub(crate) struct GithubForgeClient {
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
                Err(error) => Err(ForgeError {
                    message: t!("cli.publish.forge_release_failed", error = error).to_string(),
                }),
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
                .map_err(|error| ForgeError {
                    message: t!("cli.publish.asset_upload_failed", error = error).to_string(),
                })?;
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

pub(crate) trait FileSystem {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>>;
}

pub(crate) struct SystemFileSystem;

impl FileSystem for SystemFileSystem {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PackageForgePlan {
    pub release: ForgeRelease,
    pub assets: Vec<ReleaseAsset>,
}

pub(crate) struct ForgeExecution<'a> {
    pub client: &'a dyn ForgeClient,
    pub file_system: &'a dyn FileSystem,
    pub packages: &'a BTreeMap<PackageId, PackageForgePlan>,
}

#[derive(Default)]
pub(crate) struct HttpRegistryClient {
    client: reqwest::Client,
}

impl RegistryClient for HttpRegistryClient {
    fn version_exists<'a>(
        &'a self,
        check: &'a PlannedRegistryCheck,
    ) -> ExternalFuture<'a, Result<bool, RegistryError>> {
        Box::pin(async move {
            let headers = check.extra_headers.iter().try_fold(
                HeaderMap::new(),
                |mut headers, (name, value)| {
                    let name =
                        HeaderName::from_bytes(name.as_bytes()).map_err(|error| RegistryError {
                            message: t!("cli.publish.registry_header_name_invalid", error = error)
                                .to_string(),
                        })?;
                    let value = HeaderValue::from_str(value).map_err(|error| RegistryError {
                        message: t!("cli.publish.registry_header_value_invalid", error = error)
                            .to_string(),
                    })?;
                    headers.insert(name, value);
                    Ok::<_, RegistryError>(headers)
                },
            )?;
            let response = self
                .client
                .get(&check.url)
                .headers(headers)
                .send()
                .await
                .map_err(|error| RegistryError {
                    message: t!("cli.publish.registry_preflight_failed", error = error).to_string(),
                })?;
            Ok(response.status() == reqwest::StatusCode::OK)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PublishStatus {
    Succeeded,
    Skipped(PublishSkipReason),
    Failed(PublishFailureStage),
    NotStarted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublishFailureStage {
    Preflight,
    Command(CommandPhase),
    ForgeRelease,
    AssetUpload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandDisposition {
    Executed,
    SkippedDryRun,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommandReport {
    pub phase: CommandPhase,
    pub executable: String,
    pub disposition: CommandDisposition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PackagePublishReport {
    pub package: PackageId,
    pub status: PublishStatus,
    pub commands: Vec<CommandReport>,
    pub forge: ForgeDisposition,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ForgeDisposition {
    NotRequested,
    SkippedDryRun,
    Created,
    AlreadyExists,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PublishReport {
    pub packages: Vec<PackagePublishReport>,
}

#[derive(Debug)]
pub(crate) struct PublishExecutionError {
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
            return formatter.write_str(&t!("cli.publish.execution_failed"));
        };
        let not_started = self
            .report
            .packages
            .iter()
            .filter(|package| package.status == PublishStatus::NotStarted)
            .count();
        formatter.write_str(&t!(
            "cli.publish.recovery",
            package = package,
            error = error,
            not_started = not_started
        ))
    }
}

impl std::error::Error for PublishExecutionError {}

pub(crate) async fn execute_publish_plan<C, R>(
    plan: &mut PublishPlan,
    command_runner: &C,
    registry_client: &R,
    forge: Option<ForgeExecution<'_>>,
    dry_run: bool,
) -> Result<PublishReport, PublishExecutionError>
where
    C: CommandRunner,
    R: RegistryClient,
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
            })
            .collect(),
    };

    for (index, package) in plan.packages.iter_mut().enumerate() {
        if package.skip_reason == Some(PublishSkipReason::Private) {
            report.packages[index].status = PublishStatus::Skipped(PublishSkipReason::Private);
            continue;
        }
        let Some(preflight) = &package.preflight else {
            continue;
        };
        match registry_client.version_exists(preflight).await {
            Ok(true) => {
                package.skip_reason = Some(PublishSkipReason::RegistryVersionExists);
                report.packages[index].status =
                    PublishStatus::Skipped(PublishSkipReason::RegistryVersionExists);
            }
            Ok(false) => {}
            Err(error) => {
                report.packages[index].status =
                    PublishStatus::Failed(PublishFailureStage::Preflight);
                report.packages[index].error = Some(error.to_string());
                return Err(PublishExecutionError { report });
            }
        }
    }

    for (index, package) in plan.packages.iter().enumerate() {
        if matches!(report.packages[index].status, PublishStatus::Skipped(_)) {
            continue;
        }
        for command in &package.commands {
            if dry_run && !command.run_in_dry_run {
                report.packages[index].commands.push(CommandReport {
                    phase: command.phase,
                    executable: command.executable.clone(),
                    disposition: CommandDisposition::SkippedDryRun,
                });
                continue;
            }
            if let Err(error) = command_runner.run(command) {
                report.packages[index].status =
                    PublishStatus::Failed(PublishFailureStage::Command(command.phase));
                report.packages[index].error = Some(error.to_string());
                return Err(PublishExecutionError { report });
            }
            report.packages[index].commands.push(CommandReport {
                phase: command.phase,
                executable: command.executable.clone(),
                disposition: CommandDisposition::Executed,
            });
        }
        if let Some(forge) = &forge
            && let Some(forge_plan) = forge.packages.get(&package.context.package.id)
        {
            if dry_run {
                report.packages[index].forge = ForgeDisposition::SkippedDryRun;
            } else {
                let release_id = match forge.client.create_release(&forge_plan.release).await {
                    Ok(ForgeReleaseOutcome::Created(release_id)) => {
                        report.packages[index].forge = ForgeDisposition::Created;
                        Some(release_id)
                    }
                    Ok(ForgeReleaseOutcome::AlreadyExists) => {
                        report.packages[index].forge = ForgeDisposition::AlreadyExists;
                        None
                    }
                    Err(error) => {
                        report.packages[index].status =
                            PublishStatus::Failed(PublishFailureStage::ForgeRelease);
                        report.packages[index].error = Some(error.to_string());
                        return Err(PublishExecutionError { report });
                    }
                };
                if let Some(release_id) = release_id {
                    for asset in &forge_plan.assets {
                        let content = match forge.file_system.read(&asset.path) {
                            Ok(content) => content,
                            Err(error) => {
                                report.packages[index].status =
                                    PublishStatus::Failed(PublishFailureStage::AssetUpload);
                                report.packages[index].error = Some(error.to_string());
                                return Err(PublishExecutionError { report });
                            }
                        };
                        if let Err(error) = forge
                            .client
                            .upload_asset(&forge_plan.release, &release_id, &asset.name, content)
                            .await
                        {
                            report.packages[index].status =
                                PublishStatus::Failed(PublishFailureStage::AssetUpload);
                            report.packages[index].error = Some(error.to_string());
                            return Err(PublishExecutionError { report });
                        }
                    }
                }
            }
        }
        report.packages[index].status = PublishStatus::Succeeded;
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use camino::Utf8PathBuf;
    use semifold_core::{Ecosystem, PackageId};

    use super::*;
    use crate::publish_plan::{PackagePublish, PublishContext, PublishPackageContext, PublishPlan};

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

    impl RegistryClient for StaticRegistry {
        fn version_exists<'a>(
            &'a self,
            check: &'a PlannedRegistryCheck,
        ) -> ExternalFuture<'a, Result<bool, RegistryError>> {
            Box::pin(async move {
                self.checked
                    .lock()
                    .expect("recording registry mutex is not poisoned")
                    .push(check.url.clone());
                Ok(self.existing.contains(&check.url))
            })
        }
    }

    struct RecordingForge {
        created: Mutex<Vec<String>>,
        uploaded: Mutex<Vec<String>>,
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
                Ok(ForgeReleaseOutcome::Created(ForgeReleaseId(7)))
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

    fn package(id: &str, private: bool, commands: Vec<CommandSpec>) -> PackagePublish {
        PackagePublish {
            context: PublishContext {
                package: PublishPackageContext {
                    id: PackageId::new(id),
                    name: id.to_string(),
                    ecosystem: Ecosystem::Rust,
                    version: semver::Version::new(1, 0, 0),
                    tag: format!("{id}-v1.0.0"),
                    path: Utf8PathBuf::from(id),
                    private,
                },
                repository: None,
                ci: None,
            },
            preflight: Some(PlannedRegistryCheck {
                url: id.to_string(),
                extra_headers: Default::default(),
            }),
            commands,
            assets: Vec::new(),
            skip_reason: private.then_some(PublishSkipReason::Private),
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
        let mut plan = PublishPlan {
            packages: vec![
                package("core", false, vec![command("core-publish", false)]),
                package("app", false, vec![command("app-publish", false)]),
            ],
        };
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
    async fn dry_run_only_executes_explicitly_allowed_commands() {
        let mut plan = PublishPlan {
            packages: vec![package(
                "core",
                false,
                vec![command("skip", false), command("execute", true)],
            )],
        };
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
        let mut plan = PublishPlan {
            packages: vec![
                package("core", false, vec![command("fail", false)]),
                package("app", false, vec![command("app", false)]),
            ],
        };
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
    async fn forge_release_and_assets_use_ports_and_are_skipped_in_dry_run() {
        let forge = RecordingForge {
            created: Mutex::new(Vec::new()),
            uploaded: Mutex::new(Vec::new()),
        };
        let file_system = StaticFileSystem;
        let forge_packages = BTreeMap::from([(
            PackageId::new("core"),
            PackageForgePlan {
                release: ForgeRelease {
                    owner: "owner".to_string(),
                    repository: "repo".to_string(),
                    tag: "core-v1.0.0".to_string(),
                    title: "core v1.0.0".to_string(),
                    body: "changes".to_string(),
                    prerelease: false,
                },
                assets: vec![ReleaseAsset {
                    path: "artifact.tar.gz".into(),
                    name: "artifact.tar.gz".to_string(),
                }],
            },
        )]);
        let registry = StaticRegistry {
            existing: Vec::new(),
            checked: Mutex::new(Vec::new()),
        };
        let runner = RecordingRunner {
            commands: Mutex::new(Vec::new()),
            fail: None,
        };
        let mut plan = PublishPlan {
            packages: vec![package("core", false, Vec::new())],
        };

        let report = execute_publish_plan(
            &mut plan,
            &runner,
            &registry,
            Some(ForgeExecution {
                client: &forge,
                file_system: &file_system,
                packages: &forge_packages,
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
        let mut plan = PublishPlan {
            packages: vec![package("core", false, Vec::new())],
        };
        let report = execute_publish_plan(
            &mut plan,
            &runner,
            &registry,
            Some(ForgeExecution {
                client: &forge,
                file_system: &file_system,
                packages: &forge_packages,
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
}
