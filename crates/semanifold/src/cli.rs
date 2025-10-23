use clap::{Parser, Subcommand, builder::styling};

pub mod ci;
pub mod commit;
pub mod init;
pub mod publish;
pub mod version;

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    #[command(about = "Commit a new change", visible_alias = "add")]
    Commit(commit::Commit),
    #[command(about = "Initialize semanifold changesets config")]
    Init(init::Init),
    #[command(about = "Bump version of packages")]
    Version(version::Version),
    #[command(about = "Publish packages")]
    Publish(publish::Publish),
    #[command(about = "Run CI tasks")]
    CI(ci::CI),
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

    #[arg(global = true, short, long, help = "Enable debug mode")]
    pub debug: bool,
}
