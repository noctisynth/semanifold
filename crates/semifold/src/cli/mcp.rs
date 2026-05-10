use clap::Parser;
use rmcp::schemars::JsonSchema;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use rust_i18n::t;
use serde::{Deserialize, Serialize};

use semifold_resolver::{
    changeset::{BumpLevel, Changeset},
    context,
};

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

#[derive(Default, Clone)]
pub struct SemifoldMcp;

#[tool_router]
impl SemifoldMcp {
    #[tool(
        name = "get_tags",
        description = "Get all available tags from Semifold config"
    )]
    fn get_tags(&self) -> Result<String, String> {
        let ctx =
            context::Context::create().map_err(|e| format!("Failed to create context: {}", e))?;

        ctx.config
            .as_ref()
            .ok_or_else(|| t!("cli.mcp.not_initialized").into())
            .map(|c| serde_json::to_string(&c.tags).unwrap())
    }

    #[tool(
        name = "get_packages",
        description = "Get all packages from Semifold config with their paths and resolvers"
    )]
    fn get_packages(&self) -> Result<String, String> {
        let ctx =
            context::Context::create().map_err(|e| format!("Failed to create context: {}", e))?;

        ctx.config
            .as_ref()
            .ok_or_else(|| t!("cli.mcp.not_initialized").into())
            .map(|c| {
                let pkgs: serde_json::Map<_, _> = c
                    .packages
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            serde_json::json!({
                                "path": v.path,
                                "resolver": v.resolver.to_string()
                            }),
                        )
                    })
                    .collect();
                serde_json::to_string(&pkgs).unwrap()
            })
    }

    #[tool(
        name = "create_changeset",
        description = "Create a new changeset with specified packages and version bump level"
    )]
    fn create_changeset(
        &self,
        Parameters(params): Parameters<CreateChangesetParams>,
    ) -> Result<String, String> {
        let ctx =
            context::Context::create().map_err(|e| format!("Failed to create context: {}", e))?;

        let changeset_root = ctx
            .changeset_root
            .as_ref()
            .ok_or_else(|| t!("cli.mcp.changeset_dir_not_found").to_string())?;

        if let Some(config) = ctx.config.as_ref() {
            for pkg in &params.packages {
                if !config.packages.contains_key(pkg) {
                    return Err(t!("cli.mcp.package_not_found", package = pkg).into());
                }
            }
        }

        let level = match params.level.as_str() {
            "major" => BumpLevel::Major,
            "minor" => BumpLevel::Minor,
            "patch" => BumpLevel::Patch,
            _ => {
                return Err(t!("cli.mcp.invalid_level", level = params.level).into());
            }
        };

        let tag = params.tag.or_else(|| {
            ctx.config
                .as_ref()
                .and_then(|c| c.tags.keys().next())
                .cloned()
        });

        let mut cs = Changeset::new(params.name.clone(), changeset_root);
        cs.add_packages(&params.packages, level, tag);
        cs.summary(params.summary);
        cs.commit()
            .map_err(|e| format!("Failed to write changeset: {}", e))?;

        Ok(t!("cli.mcp.changeset_created", name = params.name).into())
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
    if let Some(dir) = &opts.current_dir {
        std::env::set_current_dir(dir)?;
    }
    let service = SemifoldMcp.serve(rmcp::transport::io::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
