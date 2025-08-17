use clap::{Parser, Subcommand};

pub mod add;

#[derive(Subcommand)]
pub(crate) enum Commands {
    Add(add::Add),
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[arg(global = true, short, long)]
    pub debug: bool,
}
