use std::fmt;

use clap::{Parser, ValueEnum};
use colored::Colorize;
use inquire::{Confirm, MultiSelect, Select, Text};

use rust_i18n::t;
use semifold_core::{BumpLevel, PackageId};
use semifold_engine::{
    AppError, ChangesetCreateError, ChangesetDraft, ChangesetPackageInput, Project,
    SemifoldService, SystemDependencies,
};

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
    pub fn to_bump_level(&self) -> BumpLevel {
        match self {
            Level::Patch => BumpLevel::Patch,
            Level::Minor => BumpLevel::Minor,
            Level::Major => BumpLevel::Major,
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

pub(crate) fn run(commit: &Commit, project: &Project) -> anyhow::Result<()> {
    let config = &project.config;

    let name = if let Some(name) = &commit.name {
        name.clone()
    } else {
        loop {
            let name = Text::new(&t!("cli.commit.query_name"))
                .prompt()?
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            break name;
        }
    };

    log::debug!("Change name: {name}");

    let all_packages = config.packages.keys().cloned().collect::<Vec<_>>();
    let mut packages = loop {
        if all_packages.len() == 1 {
            break all_packages;
        }
        let packages =
            MultiSelect::new(&t!("cli.commit.query_packages"), all_packages.clone()).prompt()?;
        if packages.is_empty() {
            log::warn!("{}", t!("cli.commit.warn_no_packages"));
            continue;
        }
        break packages;
    };

    let tag = Select::new(
        &t!("cli.commit.query_tags"),
        config.tags.keys().cloned().collect::<Vec<_>>(),
    )
    .prompt()?;

    let mut package_inputs = Vec::new();
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
        package_inputs.extend(
            selected_packages
                .iter()
                .map(|package| ChangesetPackageInput {
                    package: PackageId::new(package),
                    bump: variant.to_bump_level(),
                    tag: Some(tag.clone()),
                }),
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
    SemifoldService::new(SystemDependencies)
        .create_changeset(
            project,
            ChangesetDraft {
                name: name.clone(),
                packages: package_inputs,
                summary,
            },
        )
        .map_err(|error| match error {
            AppError::ChangesetCreate(ChangesetCreateError::EmptyName) => {
                anyhow::anyhow!(t!("cli.commit.empty_name"))
            }
            AppError::ChangesetCreate(ChangesetCreateError::AlreadyExists { .. }) => {
                anyhow::anyhow!(t!("cli.commit.commit_exists", name = name))
            }
            error => anyhow::anyhow!(t!("cli.commit.create_failed", error = error)),
        })?;

    Ok(())
}
