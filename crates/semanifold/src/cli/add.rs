use std::fmt;

use clap::{Parser, ValueEnum};
use colored::Colorize;
use inquire::{Select, Text};
use log::{info, warn};

#[derive(clap::ValueEnum, Clone)]
pub(crate) enum Level {
    Patch,
    Minor,
    Major,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Level::Patch => write!(f, "patch"),
            Level::Minor => write!(f, "minor"),
            Level::Major => write!(f, "major"),
        }
    }
}

#[derive(Parser)]
pub(crate) struct Add {
    pub name: Option<String>,
    #[arg(short, long)]
    pub level: Option<Level>,
    #[arg(short, long)]
    pub summary: Option<String>,
}

pub(crate) fn run(add: &Add) -> anyhow::Result<()> {
    let name = if let Some(name) = &add.name {
        name.clone()
    } else {
        loop {
            let name = Text::new("What is the name of the change?")
                .with_initial_value("init")
                .prompt()?;
            if name.is_empty() {
                continue;
            }
            break name;
        }
    };

    let level = if let Some(level) = &add.level {
        level.clone()
    } else {
        Select::new(
            "What kind of change is this?",
            Level::value_variants().to_vec(),
        )
        .prompt()?
    };

    let summary = if let Some(summary) = &add.summary {
        summary.clone()
    } else {
        loop {
            let summary = inquire::prompt_text("Summary")?;
            if summary.is_empty() {
                continue;
            }
            break summary;
        }
    };

    let content = format!(
        r#"
        ---
        "semanifold": {level}
        ---

        {summary}
        "#
    );

    info!("Generated changeset {}: {}", name.green(), content);
    warn!("Semanifold is still in development, not ready for production use.");

    Ok(())
}
