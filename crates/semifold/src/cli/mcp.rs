use clap::Parser;
use rmcp::schemars::JsonSchema;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};

use semifold_resolver::{
    changeset::{BumpLevel, Changeset},
    context,
};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetTagsParams;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetPackagesParams;

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CreateChangesetParams {
    pub name: String,
    pub packages: Vec<String>,
    pub level: String,
    pub summary: String,
    #[serde(default)]
    pub tag: Option<String>,
}

#[derive(Parser, Debug)]
pub struct McpCommand {
    #[arg(long, default_value_t = false)]
    pub stdio: bool,
}

#[derive(Default, Clone)]
pub struct SemifoldMcp;

#[tool_router]
impl SemifoldMcp {
    #[tool(name = "get_tags", description = "Get all available tags")]
    fn get_tags(&self) -> Result<String, String> {
        let ctx =
            context::Context::create().map_err(|e| format!("Failed to create context: {}", e))?;

        ctx.config
            .as_ref()
            .ok_or_else(|| "Semifold not initialized. Run 'smif init' first.".to_string())
            .map(|c| serde_json::to_string(&c.tags).unwrap())
    }

    #[tool(name = "get_packages", description = "Get all available packages")]
    fn get_packages(&self) -> Result<String, String> {
        let ctx =
            context::Context::create().map_err(|e| format!("Failed to create context: {}", e))?;

        ctx.config
            .as_ref()
            .ok_or_else(|| "Semifold not initialized. Run 'smif init' first.".to_string())
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

    #[tool(name = "create_changeset", description = "create changeset")]
    fn create_changeset(
        &self,
        Parameters(params): Parameters<CreateChangesetParams>,
    ) -> Result<String, String> {
        let ctx =
            context::Context::create().map_err(|e| format!("Failed to create context: {}", e))?;

        let changeset_root = ctx
            .changeset_root
            .as_ref()
            .ok_or_else(|| "Changeset directory not found. Run 'smif init' first.".to_string())?;

        if let Some(config) = ctx.config.as_ref() {
            for pkg in &params.packages {
                if !config.packages.contains_key(pkg) {
                    return Err(format!("Package '{}' not found in config", pkg));
                }
            }
        }

        let level = match params.level.as_str() {
            "major" => BumpLevel::Major,
            "minor" => BumpLevel::Minor,
            "patch" => BumpLevel::Patch,
            _ => {
                return Err(format!(
                    "Invalid level '{}'. Use: major, minor, patch",
                    params.level
                ));
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

        Ok(format!("Changeset created: {}", params.name))
    }
}

#[tool_handler(name = "semifold_mcp", version = "0.1")]
impl ServerHandler for SemifoldMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_instructions(
            "Semifold MCP Server - Use get_tags, get_packages, create_changeset tools",
        )
    }
}

pub async fn run_mcp() -> anyhow::Result<()> {
    let service = SemifoldMcp::default()
        .serve(rmcp::transport::io::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
