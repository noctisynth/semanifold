use std::{fmt, path::Path};

use clap::{Parser, ValueEnum};
use colored::Colorize;
use inquire::{Autocomplete, MultiSelect, Text, autocompletion::Replacement};

use semanifold_resolver::{changeset, config};

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

impl Level {
    pub fn to_bump_level(&self) -> changeset::BumpLevel {
        match self {
            Level::Patch => changeset::BumpLevel::Patch,
            Level::Minor => changeset::BumpLevel::Minor,
            Level::Major => changeset::BumpLevel::Major,
        }
    }
}

#[derive(Parser, Debug)]
pub(crate) struct Commit {
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

fn sanitize_filename(filename: &str) -> String {
    const ILLEGAL_CHARS: [char; 8] = ['<', '>', ':', '"', '/', '\\', '|', ' '];

    filename
        .chars()
        .map(|c| {
            if ILLEGAL_CHARS.contains(&c) {
                '-'
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect()
}

pub(crate) fn run(
    commit: &Commit,
    root_path: &Path,
    config: &config::Config,
) -> anyhow::Result<()> {
    let name = if let Some(name) = &commit.name {
        sanitize_filename(name)
    } else {
        loop {
            let name = Text::new("What is the name of the change?")
                .with_initial_value("init")
                .prompt()?;
            if name.is_empty() {
                continue;
            }
            break sanitize_filename(&name);
        }
    };

    log::debug!("Change name: {name}");

    let mut packages = loop {
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

    let tag = Text::new("What tag should this change be under?")
        .with_autocomplete(TagAutocomplete {
            tags: config.tags.keys().cloned().collect::<Vec<_>>(),
        })
        .prompt()?;

    let mut changeset = changeset::Changeset::new(name.clone(), root_path);
    let level_variants = Level::value_variants().to_vec();
    for variant in level_variants.iter().rev() {
        if packages.is_empty() {
            break;
        }

        let selected_packages = MultiSelect::new(
            &format!(
                "Which packages should be {} bumped?",
                match variant {
                    Level::Patch => "patch".cyan(),
                    Level::Minor => "minor".yellow(),
                    Level::Major => "major".red(),
                }
            ),
            packages.clone(),
        )
        .prompt()?;
        changeset.add_packages(&selected_packages, variant.to_bump_level(), tag.clone());
        packages.retain(|p| !selected_packages.contains(p));
    }

    let summary = if let Some(summary) = &commit.summary {
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
    changeset.summary(summary);

    changeset.commit()?;

    Ok(())
}
