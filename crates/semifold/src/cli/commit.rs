use std::{fmt, path::Path};

use clap::{Parser, ValueEnum};
use colored::Colorize;
use inquire::{Autocomplete, Confirm, MultiSelect, Text, autocompletion::Replacement};

use rust_i18n::t;
use semifold_resolver::{changeset, context::Context};

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
    #[arg(short, long, help = t!("cli.commit.flags.name"))]
    pub name: Option<String>,
    #[arg(short, long, help = t!("cli.commit.flags.level"))]
    pub level: Option<Level>,
    #[arg(short, long, help = t!("cli.commit.flags.summary"))]
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

fn file_exists(root_path: &Path, filename: &str) -> bool {
    let path = root_path.join(format!("{filename}.md"));
    path.exists()
}

pub(crate) fn run(commit: &Commit, ctx: &Context) -> anyhow::Result<()> {
    let Context {
        config: Some(config),
        changeset_root: Some(changeset_root),
        ..
    } = ctx
    else {
        return Err(anyhow::anyhow!(t!("cli.not_initialized")));
    };

    let name = if let Some(name) = &commit.name {
        let sanitized_name = sanitize_filename(name);
        if sanitized_name.is_empty() {
            return Err(anyhow::anyhow!(t!("cli.commit.empty_name")));
        }
        if file_exists(changeset_root, &sanitized_name) {
            return Err(anyhow::anyhow!(t!("cli.commit.commit_exists", name = name)));
        }
        sanitized_name
    } else {
        loop {
            let name = Text::new(&t!("cli.commit.query_name"))
                .prompt()?
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            break sanitize_filename(&name);
        }
    };

    log::debug!("Change name: {name}");

    let mut packages = loop {
        let packages = MultiSelect::new(
            &t!("cli.commit.query_packages"),
            config.packages.keys().cloned().collect::<Vec<_>>(),
        )
        .prompt()?;
        if packages.is_empty() {
            log::warn!("{}", t!("cli.commit.warn_no_packages"));
            continue;
        }
        break packages;
    };

    let tag = Text::new(&t!("cli.commit.query_tags"))
        .with_autocomplete(TagAutocomplete {
            tags: config.tags.keys().cloned().collect::<Vec<_>>(),
        })
        .prompt()?;

    let mut changeset = changeset::Changeset::new(name.clone(), changeset_root);
    let level_variants = Level::value_variants().iter().rev();
    for variant in level_variants {
        if packages.is_empty() {
            break;
        }

        let selected_packages = MultiSelect::new(
            &format!(
                "{}",
                t!(
                    "cli.commit.query_pkg_bump",
                    level = match variant {
                        Level::Patch => "patch".cyan(),
                        Level::Minor => "minor".yellow(),
                        Level::Major => "major".red(),
                    }
                ),
            ),
            packages.clone(),
        )
        .with_help_message(&match variant {
            Level::Patch => t!("cli.commit.help_patch"),
            Level::Minor => t!("cli.commit.help_minor"),
            Level::Major => t!("cli.commit.help_major"),
        })
        .with_default(if matches!(variant, Level::Patch) {
            let default_packages = (0..packages.len()).collect::<Vec<_>>();
            default_packages.leak()
        } else {
            &[]
        })
        .prompt()?;
        changeset.add_packages(
            &selected_packages,
            variant.to_bump_level(),
            Some(tag.clone()),
        );
        packages.retain(|p| !selected_packages.contains(p));
    }

    if !packages.is_empty()
        && !Confirm::new(&t!("cli.commit.warn_incomplete_select"))
            .with_default(false)
            .prompt()?
    {
        return Ok(());
    }

    let summary = if let Some(summary) = &commit.summary {
        summary.clone()
    } else {
        loop {
            let summary = inquire::prompt_text(&t!("cli.commit.query_summary"))?;
            if summary.is_empty() {
                log::warn!("{}", t!("cli.commit.empty_summary"));
                continue;
            }
            break summary;
        }
    };
    changeset.summary(summary);

    changeset.commit()?;

    Ok(())
}
