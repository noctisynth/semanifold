use clap::{Parser, Subcommand, builder::styling};
use rust_i18n::t;

pub mod ci;
pub mod commit;
pub mod init;
pub mod publish;
pub mod status;
pub mod version;

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    #[command(about = t!("cli.commands.commit"), visible_alias = "add")]
    Commit(commit::Commit),
    #[command(about = t!("cli.commands.init"))]
    Init(init::Init),
    #[command(about = t!("cli.commands.version"))]
    Version(version::Version),
    #[command(about = t!("cli.commands.publish"))]
    Publish(publish::Publish),
    #[command(about = t!("cli.commands.ci"))]
    CI(ci::CI),
    #[command(about = t!("cli.commands.status"))]
    Status(status::Status),
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
