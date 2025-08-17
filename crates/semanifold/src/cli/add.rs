use std::fmt;

use clap::{Parser, ValueEnum};
use colored::Colorize;
use inquire::{Autocomplete, MultiSelect, Select, Text, autocompletion::Replacement};
use log::{info, warn};
use saphyr::{Mapping, Yaml, YamlEmitter};
use semanifold_resolver::config;

#[derive(clap::ValueEnum, Clone, Debug)]
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

#[derive(Parser, Debug)]
pub(crate) struct Add {
    pub name: Option<String>,
    #[arg(short, long)]
    pub level: Option<Level>,
    #[arg(short, long)]
    pub summary: Option<String>,
}

#[derive(Clone)]
pub(crate) struct TagAutocomplete {
    tags: Vec<String>,
}

impl Autocomplete for TagAutocomplete {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, inquire::CustomUserError> {
        let suggestions = self
            .tags
            .iter()
            .filter(|tag| tag.starts_with(input))
            .cloned()
            .collect::<Vec<_>>();
        Ok(suggestions)
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, inquire::CustomUserError> {
        let completion = if let Some(highlighted_suggestion) = highlighted_suggestion {
            highlighted_suggestion
        } else {
            self.tags
                .iter()
                .find(|tag| tag.starts_with(input))
                .cloned()
                .unwrap_or(input.to_string())
        };
        Ok(Replacement::Some(completion))
    }
}

pub(crate) fn run(add: &Add, config: &config::Config) -> anyhow::Result<()> {
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

    let packages = loop {
        let packages = MultiSelect::new(
            "What packages are affected by this change?",
            config.packages.keys().cloned().collect::<Vec<_>>(),
        )
        .prompt()?;
        if packages.is_empty() {
            log::warn!("No packages selected.");
            continue;
        }
        break packages;
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

    let tag = Text::new("What tag should this change be under?")
        .with_autocomplete(TagAutocomplete {
            tags: config.tags.keys().cloned().collect::<Vec<_>>(),
        })
        .prompt()?;

    let summary = if let Some(summary) = &add.summary {
        summary.clone()
    } else {
        loop {
            let summary = inquire::prompt_text("Summary:")?;
            if summary.is_empty() {
                log::warn!("Summary cannot be empty.");
                continue;
            }
            break summary;
        }
    };

    let mut fm = String::new();
    let mut emitter = YamlEmitter::new(&mut fm);
    let mut fm_map = Mapping::new();
    for package in packages {
        fm_map.insert(
            Yaml::value_from_str(package.leak()),
            Yaml::value_from_str(
                if tag.is_empty() {
                    format!("{}", level)
                } else {
                    format!("{}:{}", level, tag)
                }
                .leak(),
            ),
        );
    }
    emitter.dump(&Yaml::Mapping(fm_map))?;

    let content = format!("{fm}\n---\n\n{summary}\n");

    info!("Generated changeset {}:\n{}", name.green(), content);
    warn!("Semanifold is still in development, not ready for production use.");

    Ok(())
}
