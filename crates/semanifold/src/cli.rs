use clap::{Parser, Subcommand};

pub mod commit;
pub mod init;

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    #[command(about = "Commit a new change", visible_alias="add")]
    Commit(commit::Commit),
    #[command(about = "Initialize semanifold changesets config")]
    Init(init::Init),
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(global = true, short, long, help = "Enable debug mode")]
    pub debug: bool,
}
