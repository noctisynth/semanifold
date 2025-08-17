use clap::{Parser, Subcommand};

pub mod add;

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    Add(add::Add),
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(global = true, short, long)]
    pub debug: bool,
}
