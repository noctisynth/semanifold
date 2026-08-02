use clap::{Parser, Subcommand, builder::styling};
use rust_i18n::t;
use semifold_core::RepositoryContext;
use semifold_engine::Project;

pub mod ci;
pub mod commit;
pub mod config;
pub mod init;
pub mod mcp;
pub mod publish;
pub mod status;
pub mod version;

pub(crate) fn repository_context() -> Option<RepositoryContext> {
    let repository = std::env::var("GITHUB_REPOSITORY").ok()?;
    let (owner, name) = repository.split_once('/')?;
    let host =
        std::env::var("GITHUB_SERVER_URL").unwrap_or_else(|_| "https://github.com".to_string());
    Some(RepositoryContext {
        host: host.clone(),
        owner: owner.to_string(),
        name: name.to_string(),
        web_url: format!("{}/{owner}/{name}", host.trim_end_matches('/')),
        commit: None,
    })
}

pub(crate) fn is_git_repo_clean(project: &Project) -> anyhow::Result<bool> {
    let repository = git2::Repository::open(project.root.as_std_path())
        .map_err(|error| anyhow::anyhow!(t!("cli.version.git_open_failed", error = error)))?;
    let statuses = repository
        .statuses(None)
        .map_err(|error| anyhow::anyhow!(t!("cli.version.git_status_failed", error = error)))?;
    Ok(statuses.iter().all(|entry| {
        matches!(
            entry.status(),
            git2::Status::CURRENT | git2::Status::IGNORED
        )
    }))
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    #[command(about = t!("cli.commands.commit"), visible_alias = "add")]
    Commit(commit::Commit),
    #[command(about = t!("cli.commands.init"))]
    Init(init::Init),
    #[command(about = t!("cli.commands.config"))]
    Config(config::Config),
    #[command(about = t!("cli.commands.version"))]
    Version(version::Version),
    #[command(about = t!("cli.commands.publish"))]
    Publish(publish::Publish),
    #[command(about = t!("cli.commands.ci"))]
    CI(ci::CI),
    #[command(about = t!("cli.commands.status"))]
    Status(status::Status),
    #[command(about = t!("cli.commands.mcp"))]
    Mcp(mcp::McpCommand),
}

fn get_styles() -> clap::builder::Styles {
    styling::Styles::styled()
        .header(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .usage(styling::AnsiColor::Green.on_default() | styling::Effects::BOLD)
        .literal(styling::AnsiColor::Cyan.on_default() | styling::Effects::BOLD)
        .placeholder(styling::AnsiColor::Cyan.on_default())
}

#[derive(Parser, Debug)]
#[command(version, styles = get_styles(), about, long_about = None, arg_required_else_help = true)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(global = true, long, help = t!("cli.flags.dry_run"))]
    pub dry_run: bool,

    #[arg(global = true, long, help = t!("cli.flags.debug"))]
    pub debug: bool,
}
