use core::fmt;
use std::path::{Path, PathBuf};

use saphyr::{LoadableYamlNode, Mapping, Yaml, YamlEmitter};
use serde::{Deserialize, Serialize};

use crate::{context::Context, error::ResolveError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum BumpLevel {
    Major = 3,
    Minor = 2,
    Patch = 1,
    Unchanged = 0,
}

impl fmt::Display for BumpLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BumpLevel::Major => write!(f, "major"),
            BumpLevel::Minor => write!(f, "minor"),
            BumpLevel::Patch => write!(f, "patch"),
            BumpLevel::Unchanged => write!(f, "unchanged"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangePackage {
    pub name: String,
    pub level: BumpLevel,
    pub tag: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Changeset {
    pub name: String,
    pub packages: Vec<ChangePackage>,
    pub summary: String,
    pub root_path: PathBuf,
    pub path: Option<PathBuf>,
}

impl Changeset {
    pub fn new(name: String, root_path: &Path) -> Self {
        Self {
            name,
            packages: Vec::new(),
            summary: String::new(),
            root_path: root_path.to_path_buf(),
            path: None,
        }
    }

    #[inline]
    pub fn add_package(&mut self, name: String, level: BumpLevel, tag: Option<String>) {
        self.packages.push(ChangePackage { name, level, tag })
    }

    pub fn add_packages(&mut self, packages: &[String], level: BumpLevel, tag: Option<String>) {
        packages.iter().for_each(|package| {
            self.add_package(package.clone(), level, tag.clone());
        })
    }

    pub fn summary(&mut self, summary: String) {
        self.summary = summary;
    }

    pub fn from_file(ctx: &Context, path: &PathBuf) -> Result<Self, ResolveError> {
        let changeset_str = std::fs::read_to_string(path)?;
        let separator = "---";

        let sep_idx = changeset_str
            .rfind(separator)
            .ok_or(ResolveError::InvalidChangeset {
                path: path.to_path_buf(),
                reason: "Invalid changeset".to_string(),
            })?;

        let (left_part, right_part) = changeset_str.split_at(sep_idx);
        let fm = Yaml::load_from_str(left_part).map_err(|e| ResolveError::InvalidChangeset {
            path: path.to_path_buf(),
            reason: format!("Failed to parse changeset front matter: {e}"),
        })?;
        let packages_map = fm.first().and_then(|f| f.as_mapping());

        let mut packages = Vec::new();
        if let Some(map) = packages_map {
            map.into_iter().try_for_each(|(k, v)| {
                let name = k
                    .as_str()
                    .ok_or(ResolveError::InvalidChangeset {
                        path: path.to_path_buf(),
                        reason: format!("Failed to parse package name: {k:?}"),
                    })?
                    .to_string();
                if !ctx.has_package(&name) {
                    return Err(ResolveError::InvalidChangeset {
                        path: path.to_path_buf(),
                        reason: format!("Package {name} is not defined in config"),
                    });
                }

                let mark = v
                    .as_str()
                    .ok_or(ResolveError::InvalidChangeset {
                        path: path.to_path_buf(),
                        reason: format!("Failed to parse package mark: {v:?}"),
                    })?
                    .to_string();
                let mut mark = mark.split(':');
                let level = mark.next().ok_or(ResolveError::InvalidChangeset {
                    path: path.to_path_buf(),
                    reason: format!("Failed to parse package mark: {v:?}"),
                })?;
                let tag = mark.next().map(|s| s.to_string());
                let level = match level {
                    "major" => BumpLevel::Major,
                    "minor" => BumpLevel::Minor,
                    "patch" => BumpLevel::Patch,
                    _ => {
                        return Err(ResolveError::InvalidChangeset {
                            path: path.to_path_buf(),
                            reason: format!("Invalid bump level: {level}"),
                        });
                    }
                };
                packages.push(ChangePackage { name, level, tag });
                Ok(())
            })?;
        }

        let summary = right_part[3..].trim().to_string();

        Ok(Self {
            name: path
                .file_stem()
                .ok_or(ResolveError::InvalidChangeset {
                    path: path.to_path_buf(),
                    reason: "Invalid changeset".to_string(),
                })?
                .to_string_lossy()
                .to_string(),
            packages,
            summary,
            root_path: path.parent().unwrap().to_path_buf(),
            path: Some(path.to_path_buf()),
        })
    }

    pub fn commit_to(&mut self, changeset_path: &Path) -> Result<(), ResolveError> {
        log::debug!("Commit changeset: {self:?}");

        let file_path = changeset_path.join(format!("{}.md", self.name));

        let mut fm = String::new();
        let mut emitter = YamlEmitter::new(&mut fm);
        let mut fm_map = Mapping::new();
        for package in &self.packages {
            let mark = if let Some(tag) = &package.tag {
                format!("{}:{}", package.level, tag)
            } else {
                format!("{}", package.level)
            };

            fm_map.insert(
                Yaml::value_from_str(&package.name),
                Yaml::value_from_str(mark.leak()),
            );
        }
        emitter
            .dump(&Yaml::Mapping(fm_map))
            .map_err(|e| ResolveError::ParseError {
                path: file_path.clone(),
                reason: e.to_string(),
            })?;

        let content = format!("{fm}\n---\n\n{}\n", self.summary);
        std::fs::write(&file_path, content)?;

        self.path = Some(file_path);

        Ok(())
    }

    pub fn commit(&mut self) -> Result<(), ResolveError> {
        self.commit_to(&self.root_path.clone())
    }

    pub fn clean(&self) -> Result<(), ResolveError> {
        let file_path = self.root_path.join(format!("{}.md", self.name));
        std::fs::remove_file(file_path)?;
        Ok(())
    }
}
