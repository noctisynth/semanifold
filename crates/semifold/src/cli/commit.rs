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

use crate::cli::terminal::{StepOutcome, Terminal};

#[derive(clap::ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Level {
    Patch,
    Minor,
    Major,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PackageArgument {
    package: String,
    level: Option<Level>,
}

fn parse_package_argument(value: &str) -> Result<PackageArgument, String> {
    let (package, level) = value
        .split_once('=')
        .map_or((value, None), |(package, level)| (package, Some(level)));
    let package = package.trim();
    if package.is_empty() {
        return Err(t!("cli.commit.invalid_package_argument").into_owned());
    }
    let level = level
        .map(|level| {
            Level::from_str(level, true)
                .map_err(|_| t!("cli.commit.invalid_package_level", level = level).into_owned())
        })
        .transpose()?;
    Ok(PackageArgument {
        package: package.to_string(),
        level,
    })
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
    pub fn to_bump_level(self) -> BumpLevel {
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
    #[arg(short = 'm', long, help = t!("cli.commit.flags.summary"))]
    pub summary: Option<String>,
    #[arg(
        short = 'p',
        long = "package",
        value_name = "PACKAGE[=LEVEL]",
        value_parser = parse_package_argument,
        help = t!("cli.commit.flags.package")
    )]
    pub packages: Vec<PackageArgument>,
    #[arg(long, conflicts_with = "no_tag", help = t!("cli.commit.flags.tag"))]
    pub tag: Option<String>,
    #[arg(long, conflicts_with = "tag", help = t!("cli.commit.flags.no_tag"))]
    pub no_tag: bool,
}

pub(crate) fn run(commit: &Commit, project: &Project) -> anyhow::Result<()> {
    let terminal = Terminal::detect();
    terminal.heading(&t!("cli.commit.heading"));
    let config = &project.config;

    let name = if let Some(name) = &commit.name {
        name.clone()
    } else {
        super::require_interactive(&t!("cli.commit.query_name"), "--name")?;
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
    let selected_packages = if commit.packages.is_empty() {
        if all_packages.len() == 1 {
            all_packages
                .into_iter()
                .map(|package| PackageArgument {
                    package,
                    level: None,
                })
                .collect()
        } else {
            super::require_interactive(&t!("cli.commit.query_packages"), "--package")?;
            loop {
                let packages =
                    MultiSelect::new(&t!("cli.commit.query_packages"), all_packages.clone())
                        .prompt()?;
                if packages.is_empty() {
                    terminal.warning(&t!("cli.commit.warn_no_packages"));
                    continue;
                }
                break packages
                    .into_iter()
                    .map(|package| PackageArgument {
                        package,
                        level: None,
                    })
                    .collect();
            }
        }
    } else {
        commit.packages.clone()
    };

    let tag = if let Some(tag) = &commit.tag {
        Some(tag.clone())
    } else if commit.no_tag || config.tags.is_empty() {
        None
    } else {
        super::require_interactive(&t!("cli.commit.query_tags"), "--tag or --no-tag")?;
        Some(
            Select::new(
                &t!("cli.commit.query_tags"),
                config.tags.keys().cloned().collect::<Vec<_>>(),
            )
            .prompt()?,
        )
    };

    let mut package_inputs = Vec::new();
    let mut packages = Vec::new();
    for package in selected_packages {
        if let Some(level) = package.level.or(commit.level) {
            package_inputs.push(ChangesetPackageInput {
                package: PackageId::new(package.package),
                bump: level.to_bump_level(),
                tag: tag.clone(),
            });
        } else {
            packages.push(package.package);
        }
    }

    if !packages.is_empty() {
        super::require_interactive(
            &t!("cli.commit.query_pkg_bump", level = "patch/minor/major"),
            "--level or --package PACKAGE=LEVEL",
        )?;
    }
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
                    tag: tag.clone(),
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
        super::require_interactive(&t!("cli.commit.query_summary"), "--summary")?;
        loop {
            let summary = inquire::prompt_text(&t!("cli.commit.query_summary"))?;
            if summary.is_empty() {
                terminal.warning(&t!("cli.commit.empty_summary"));
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
    terminal.summary(
        StepOutcome::Success,
        &t!("cli.commit.complete", name = name),
    );

    Ok(())
}
