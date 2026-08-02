use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use rmcp::schemars::JsonSchema;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use rust_i18n::t;
use semifold_core::{BumpLevel, PackageId};
use semifold_engine::{
    ChangesetDraft, ChangesetPackageInput, Project, ProjectLocation, SemifoldService,
    SystemDependencies,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[schemars(title = t!("cli.mcp.params.name"), description = t!("cli.mcp.tools.create_changeset"))]
pub struct CreateChangesetParams {
    #[schemars(title = t!("cli.mcp.params.name"), description = t!("cli.mcp.params.name_desc"))]
    pub name: String,
    #[schemars(title = t!("cli.mcp.params.packages"), description = t!("cli.mcp.params.packages_desc"))]
    pub packages: Vec<String>,
    #[schemars(title = t!("cli.mcp.params.level"), description = t!("cli.mcp.params.level_desc"))]
    pub level: String,
    #[schemars(title = t!("cli.mcp.params.summary"), description = t!("cli.mcp.params.summary_desc"))]
    pub summary: String,
    #[serde(default)]
    #[schemars(title = t!("cli.mcp.params.tag"), description = t!("cli.mcp.params.tag_desc"))]
    pub tag: Option<String>,
}

#[derive(Parser, Debug)]
pub struct McpCommand {
    #[arg(
        short = 'C',
        long = "cd",
        help = t!("cli.mcp.flags.current_dir")
    )]
    pub current_dir: Option<String>,
}

#[derive(Clone)]
pub struct SemifoldMcp {
    project: Arc<Project>,
    service: Arc<SemifoldService<SystemDependencies>>,
}

impl SemifoldMcp {
    fn new(project: Project) -> Self {
        Self {
            project: Arc::new(project),
            service: Arc::new(SemifoldService::new(SystemDependencies)),
        }
    }
}

#[tool_router]
impl SemifoldMcp {
    #[tool(
        name = "get_tags",
        description = "Get all available tags from Semifold config"
    )]
    fn get_tags(&self) -> Result<String, String> {
        serde_json::to_string(&self.project.config.tags).map_err(|error| error.to_string())
    }

    #[tool(
        name = "get_packages",
        description = "Get all packages from Semifold config with their paths and resolvers"
    )]
    fn get_packages(&self) -> Result<String, String> {
        let packages: serde_json::Map<_, _> = self
            .project
            .config
            .packages
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    serde_json::json!({
                        "path": value.path,
                        "resolver": value.resolver.to_string()
                    }),
                )
            })
            .collect();
        serde_json::to_string(&packages).map_err(|error| error.to_string())
    }

    #[tool(
        name = "create_changeset",
        description = "Create a new changeset with specified packages and version bump level"
    )]
    fn create_changeset(
        &self,
        Parameters(params): Parameters<CreateChangesetParams>,
    ) -> Result<String, String> {
        let level = match params.level.as_str() {
            "major" => BumpLevel::Major,
            "minor" => BumpLevel::Minor,
            "patch" => BumpLevel::Patch,
            _ => {
                return Err(t!("cli.mcp.invalid_level", level = params.level).into());
            }
        };

        let tag = params
            .tag
            .or_else(|| self.project.config.tags.keys().next().cloned());

        let packages = params
            .packages
            .into_iter()
            .map(|package| ChangesetPackageInput {
                package: PackageId::new(package),
                bump: level,
                tag: tag.clone(),
            })
            .collect();
        let id = self
            .service
            .create_changeset(
                &self.project,
                ChangesetDraft {
                    name: params.name,
                    packages,
                    summary: params.summary,
                },
            )
            .map_err(|error| t!("cli.mcp.create_failed", error = error).to_string())?;

        Ok(t!("cli.mcp.changeset_created", name = id.as_str()).into())
    }
}

#[tool_handler(name = "semifold_mcp", version = "0.1.0")]
impl ServerHandler for SemifoldMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions(t!("cli.mcp.instructions"))
    }
}

pub async fn run_mcp(opts: &McpCommand) -> anyhow::Result<()> {
    let start = opts
        .current_dir
        .as_ref()
        .map_or_else(std::env::current_dir, |directory| {
            Ok(PathBuf::from(directory))
        })?;
    let changeset_dir = std::env::var_os("CHANGESET_PATH").map(PathBuf::from);
    let project = ProjectLocation::discover_with_changeset_dir(&start, changeset_dir.as_deref())
        .and_then(ProjectLocation::load)
        .map_err(|error| anyhow::anyhow!(t!("cli.project_load_failed", error = error)))?;
    let service = SemifoldMcp::new(project)
        .serve(rmcp::transport::io::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
