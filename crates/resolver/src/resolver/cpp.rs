use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use regex::Regex;
use semifold_core::{
    DependencyKind, EcosystemId, EditSource, FileEdit, FileEditExpectation, FileHash, PackageId,
    PackageSnapshot, VersionMap, VersionSource,
};

use crate::{
    adapter::{
        AdapterError, EcosystemAdapter, EcosystemPlanInput, ManifestDependency, PackageInspection,
        PackageLocation, ParsedPackage,
    },
    config::{PackageConfig, ReleaseChannel},
    error::ResolveError,
    utils,
};

/// C++ resolver for statically analyzable CMake and qmake projects.
pub struct CppResolver;

#[derive(Clone, Debug, Eq, PartialEq)]
enum CppManifest {
    Cmake {
        path: PathBuf,
    },
    Qmake {
        path: PathBuf,
        variable: QmakeVersionVariable,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum QmakeVersionVariable {
    Direct,
    Components {
        major: String,
        minor: String,
        patch: String,
    },
}

impl QmakeVersionVariable {
    fn display_name(&self) -> String {
        match self {
            Self::Direct => "VERSION".to_string(),
            Self::Components {
                major,
                minor,
                patch,
            } => format!("{major}, {minor}, {patch}"),
        }
    }
}

impl CppResolver {
    fn project_relative_path(parent: &camino::Utf8Path, child: &str) -> camino::Utf8PathBuf {
        let parent = parent.as_str().trim_matches('/');
        if parent.is_empty() || parent == "." {
            camino::Utf8PathBuf::from(child)
        } else {
            camino::Utf8PathBuf::from(format!("{parent}/{child}"))
        }
    }

    fn qmake_files(package_path: &Path) -> Result<Vec<PathBuf>, ResolveError> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(package_path)? {
            let path = entry?.path();
            let is_qmake = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("pro") || extension.eq_ignore_ascii_case("pri")
                });
            if path.is_file() && is_qmake {
                files.push(path);
            }
        }
        files.sort();
        Ok(files)
    }

    fn qmake_assignments(content: &str, variable: &str) -> Vec<String> {
        content
            .lines()
            .filter_map(|line| {
                let code = line.split_once('#').map_or(line, |(code, _)| code);
                let (name, value) = code.split_once('=')?;
                if name.trim() != variable {
                    return None;
                }
                let value = value.trim();
                let value = match value.as_bytes() {
                    [b'"', .., b'"'] | [b'\'', .., b'\''] if value.len() >= 2 => {
                        &value[1..value.len() - 1]
                    }
                    _ => value,
                };
                Some(value.to_string())
            })
            .collect()
    }

    fn unique_qmake_assignment(
        content: &str,
        variable: &str,
        path: &Path,
    ) -> Result<Option<String>, ResolveError> {
        let values = Self::qmake_assignments(content, variable);
        match values.as_slice() {
            [] => Ok(None),
            [value] => Ok(Some(value.clone())),
            _ => Err(ResolveError::ParseError {
                path: path.to_path_buf(),
                reason: format!("qmake variable {variable} has multiple assignments"),
            }),
        }
    }

    fn qmake_version_candidate(
        path: &Path,
    ) -> Result<Option<(QmakeVersionVariable, semver::Version)>, ResolveError> {
        let content = std::fs::read_to_string(path)?;
        let direct = Self::unique_qmake_assignment(&content, "VERSION", path)?
            .map(|value| {
                semver::Version::parse(&value)
                    .map(|version| (QmakeVersionVariable::Direct, version))
                    .map_err(|error| ResolveError::ParseError {
                        path: path.to_path_buf(),
                        reason: format!("invalid qmake VERSION `{value}`: {error}"),
                    })
            })
            .transpose()?;
        let component_variables = ["VERSION_MAJOR", "VERSION_MINOR", "VERSION_PATCH"];
        let mut component_values = Vec::new();
        for variable in component_variables {
            if let Some(value) = Self::unique_qmake_assignment(&content, variable, path)? {
                component_values.push(value.parse::<u64>().map_err(|error| {
                    ResolveError::ParseError {
                        path: path.to_path_buf(),
                        reason: format!("invalid qmake {variable} `{value}`: {error}"),
                    }
                })?);
            }
        }
        let components = match component_values.as_slice() {
            [] => None,
            [major, minor, patch] => Some((
                QmakeVersionVariable::Components {
                    major: "VERSION_MAJOR".to_string(),
                    minor: "VERSION_MINOR".to_string(),
                    patch: "VERSION_PATCH".to_string(),
                },
                semver::Version::new(*major, *minor, *patch),
            )),
            _ => {
                return Err(ResolveError::ParseError {
                    path: path.to_path_buf(),
                    reason: "qmake component version requires VERSION_MAJOR, VERSION_MINOR, and VERSION_PATCH"
                        .to_string(),
                });
            }
        };
        match (direct, components) {
            (Some(_), Some(_)) => Err(ResolveError::ParseError {
                path: path.to_path_buf(),
                reason: "qmake defines both VERSION and VERSION_MAJOR/MINOR/PATCH".to_string(),
            }),
            (candidate, None) | (None, candidate) => Ok(candidate),
        }
    }

    fn resolve_manifest(package_path: &Path) -> Result<CppManifest, ResolveError> {
        let cmake_path = package_path.join("CMakeLists.txt");
        if cmake_path.is_file() {
            let content = std::fs::read_to_string(&cmake_path)?;
            if Regex::new(r"(?i)project\s*\([^)]*VERSION\s+")
                .map_err(|error| ResolveError::ParseError {
                    path: cmake_path.clone(),
                    reason: error.to_string(),
                })?
                .is_match(&content)
            {
                return Ok(CppManifest::Cmake { path: cmake_path });
            }
        }

        let mut candidates = Vec::new();
        for path in Self::qmake_files(package_path)? {
            if let Some((variable, version)) = Self::qmake_version_candidate(&path)? {
                candidates.push((path, variable, version));
            }
        }
        match candidates.as_slice() {
            [(path, variable, _)] => Ok(CppManifest::Qmake {
                path: path.clone(),
                variable: variable.clone(),
            }),
            [] => Err(ResolveError::ParseError {
                path: package_path.to_path_buf(),
                reason: "neither a versioned CMake project nor a qmake version assignment (VERSION or VERSION_MAJOR/MINOR/PATCH) was found"
                    .to_string(),
            }),
            candidates => Err(ResolveError::ParseError {
                path: package_path.to_path_buf(),
                reason: format!(
                    "multiple qmake version candidates found: {}",
                    candidates
                        .iter()
                        .map(|(path, variable, _)| format!(
                            "{} ({})",
                            path.display(),
                            variable.display_name()
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            }),
        }
    }

    fn qmake_package_name(
        package_path: &Path,
        version_path: &Path,
    ) -> Result<String, ResolveError> {
        let mut targets = Vec::new();
        for path in Self::qmake_files(package_path)? {
            if !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pro"))
            {
                continue;
            }
            let content = std::fs::read_to_string(&path)?;
            if let Some(target) = Self::unique_qmake_assignment(&content, "TARGET", &path)? {
                if !target.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                }) {
                    return Err(ResolveError::ParseError {
                        path: path.clone(),
                        reason: format!("qmake TARGET `{target}` is not a static identifier"),
                    });
                }
                targets.push(target);
            }
        }
        match targets.as_slice() {
            [target] => Ok(target.clone()),
            [] => version_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
                .ok_or_else(|| ResolveError::ParseError {
                    path: version_path.to_path_buf(),
                    reason: "qmake TARGET and file name are unavailable".to_string(),
                }),
            _ => Err(ResolveError::ParseError {
                path: package_path.to_path_buf(),
                reason: format!(
                    "multiple qmake TARGET candidates found: {}",
                    targets.join(", ")
                ),
            }),
        }
    }

    fn replace_qmake_assignment(
        content: &str,
        variable: &str,
        value: &str,
        path: &Path,
    ) -> Result<String, ResolveError> {
        let pattern = format!(
            r"(?m)^(\s*{}\s*=\s*)([^#\r\n]*)(#.*)?$",
            regex::escape(variable)
        );
        let regex = Regex::new(&pattern).map_err(|error| ResolveError::ParseError {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })?;
        Ok(regex
            .replace(content, |captures: &regex::Captures| {
                let raw_value = captures[2].trim_end();
                let trailing = &captures[2][raw_value.len()..];
                let quote = match raw_value.as_bytes() {
                    [b'"', .., b'"'] => "\"",
                    [b'\'', .., b'\''] => "'",
                    _ => "",
                };
                let comment = captures.get(3).map_or("", |comment| comment.as_str());
                format!("{}{quote}{value}{quote}{trailing}{comment}", &captures[1])
            })
            .into_owned())
    }

    fn replace_qmake_version(
        content: &str,
        variable: &QmakeVersionVariable,
        version: &str,
        path: &Path,
    ) -> Result<String, ResolveError> {
        let replacement = match variable {
            QmakeVersionVariable::Direct => {
                Self::replace_qmake_assignment(content, "VERSION", version, path)?
            }
            QmakeVersionVariable::Components {
                major,
                minor,
                patch,
            } => {
                let parsed = semver::Version::parse(version).map_err(|error| {
                    ResolveError::InvalidVersion {
                        version: version.to_string(),
                        reason: error.to_string(),
                    }
                })?;
                let mut updated = content.to_string();
                for (variable, value) in [
                    (major.as_str(), parsed.major.to_string()),
                    (minor.as_str(), parsed.minor.to_string()),
                    (patch.as_str(), parsed.patch.to_string()),
                ] {
                    updated = Self::replace_qmake_assignment(&updated, variable, &value, path)?;
                }
                updated
            }
        };
        if replacement == content {
            return Err(ResolveError::ParseError {
                path: path.to_path_buf(),
                reason: "qmake version assignment could not be rewritten".to_string(),
            });
        }
        Ok(replacement)
    }

    fn package_config(path: impl Into<PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: EcosystemId::CPP,
            publish: None,
            channel: ReleaseChannel::Stable,
            channel_bump: None,
            assets: Vec::new(),
            github_release: None,
            depends_on: vec![],
        }
    }

    fn package_inspection(
        id: PackageId,
        package: ParsedPackage,
        dependencies: Vec<ManifestDependency>,
    ) -> Result<PackageInspection, AdapterError> {
        let path = camino::Utf8PathBuf::from_path_buf(package.path).map_err(|path| {
            AdapterError::InvalidInput {
                reason: format!("C++ package path is not valid UTF-8: {}", path.display()),
            }
        })?;
        Ok(PackageInspection {
            id,
            manifest_name: package.name,
            version: package.version,
            version_source: package.version_source,
            ecosystem: EcosystemId::CPP,
            path,
            publishable: !package.private,
            dependencies,
        })
    }

    pub fn plan_file_edits(
        root: &Path,
        package: &PackageSnapshot,
        versions: &VersionMap,
    ) -> Result<Vec<FileEdit>, ResolveError> {
        let next_version =
            versions
                .get(&package.id)
                .ok_or_else(|| ResolveError::InvalidConfig {
                    path: root.join(package.path.as_std_path()),
                    reason: format!("missing planned version for {}", package.id),
                })?;
        let version = CppResolver.encode_version(next_version).map_err(|error| {
            ResolveError::InvalidVersion {
                version: next_version.to_string(),
                reason: error.to_string(),
            }
        })?;
        let package_path = root.join(package.path.as_std_path());
        let manifest = Self::resolve_manifest(&package_path)?;
        let (manifest_path, content, new_content) = match manifest {
            CppManifest::Cmake { path } => {
                let content = std::fs::read_to_string(&path)?;
                let re = Regex::new(
                    r"(?i)(project\s*\([^)]*VERSION\s+)([\d.]+(?:-[a-zA-Z0-9.-]+)?(?:\+[a-zA-Z0-9.-]+)?)",
                )
                .map_err(|error| ResolveError::ParseError {
                    path: path.clone(),
                    reason: error.to_string(),
                })?;
                let updated = re
                    .replace(&content, |caps: &regex::Captures| {
                        format!("{}{}", &caps[1], version)
                    })
                    .into_owned();
                (path, content, updated)
            }
            CppManifest::Qmake { path, variable } => {
                let content = std::fs::read_to_string(&path)?;
                let updated = Self::replace_qmake_version(&content, &variable, &version, &path)?;
                (path, content, updated)
            }
        };
        let relative_manifest = manifest_path
            .strip_prefix(root)
            .map(Path::to_path_buf)
            .map_err(|_| ResolveError::InvalidConfig {
                path: manifest_path.clone(),
                reason: "C++ manifest is outside the project root".to_string(),
            })?;
        let relative_manifest =
            camino::Utf8PathBuf::from_path_buf(relative_manifest).map_err(|path| {
                ResolveError::InvalidConfig {
                    path,
                    reason: "C++ manifest path is not valid UTF-8".to_string(),
                }
            })?;
        let relative_manifest =
            camino::Utf8PathBuf::from(relative_manifest.as_str().replace('\\', "/"));
        let mut edits = vec![FileEdit {
            path: relative_manifest,
            expected: FileEditExpectation::Existing {
                hash: FileHash::from_bytes(content.as_bytes()),
            },
            new_content,
            source: EditSource::PackageVersion {
                package: package.id.clone(),
            },
        }];
        let vcpkg_path = package_path.join("vcpkg.json");
        if vcpkg_path.exists() {
            let content = std::fs::read_to_string(&vcpkg_path)?;
            let updated = utils::replace_root_json_string_field(&content, "version", &version)
                .ok_or_else(|| ResolveError::ParseError {
                    path: vcpkg_path.clone(),
                    reason: "vcpkg.json version field could not be replaced".to_string(),
                })?;
            edits.push(FileEdit {
                path: Self::project_relative_path(&package.path, "vcpkg.json"),
                expected: FileEditExpectation::Existing {
                    hash: FileHash::from_bytes(content.as_bytes()),
                },
                new_content: updated,
                source: EditSource::PackageVersion {
                    package: package.id.clone(),
                },
            });
        }
        Ok(edits)
    }
    fn literal_subdirectories(&self, directory: &Path) -> Result<Vec<PathBuf>, ResolveError> {
        let cmake_path = directory.join("CMakeLists.txt");
        let content = std::fs::read_to_string(&cmake_path)?;
        let re =
            Regex::new(r#"(?im)^\s*add_subdirectory\s*\(\s*["']?([^"'\s\)]+)"#).map_err(|e| {
                ResolveError::ParseError {
                    path: cmake_path.clone(),
                    reason: format!("Invalid regex: {e}"),
                }
            })?;

        Ok(re
            .captures_iter(&content)
            .filter_map(|captures| captures.get(1))
            .map(|member| directory.join(member.as_str()))
            .filter(|member| member.join("CMakeLists.txt").exists())
            .collect())
    }

    fn workspace_members(&self, root: &Path) -> Result<Vec<PathBuf>, ResolveError> {
        let canonical_root = std::fs::canonicalize(root)?;
        let mut pending = BTreeSet::from([canonical_root.clone()]);
        let mut visited = BTreeSet::new();
        let mut members = BTreeSet::new();

        while let Some(directory) = pending.pop_first() {
            if !visited.insert(directory.clone()) {
                continue;
            }
            if directory != canonical_root {
                let cmake_path = directory.join("CMakeLists.txt");
                let content = std::fs::read_to_string(&cmake_path)?;
                if self.has_versioned_project(&content, &cmake_path)? {
                    members.insert(directory.clone());
                }
            }
            for child in self.literal_subdirectories(&directory)? {
                let canonical_child = std::fs::canonicalize(&child)?;
                if !canonical_child.starts_with(&canonical_root) {
                    return Err(ResolveError::InvalidConfig {
                        path: child,
                        reason: "add_subdirectory path escapes the project root".to_string(),
                    });
                }
                pending.insert(canonical_child);
            }
        }

        members
            .into_iter()
            .map(|member| {
                member
                    .strip_prefix(&canonical_root)
                    .map(|path| PathBuf::from(path.to_string_lossy().replace('\\', "/")))
                    .map_err(|_| ResolveError::InvalidConfig {
                        path: member,
                        reason: "C++ workspace member is outside the project root".to_string(),
                    })
            })
            .collect()
    }

    fn has_versioned_project(
        &self,
        content: &str,
        cmake_path: &Path,
    ) -> Result<bool, ResolveError> {
        Regex::new(r"(?i)project\s*\([^)]*VERSION\s+")
            .map(|regex| regex.is_match(content))
            .map_err(|error| ResolveError::ParseError {
                path: cmake_path.to_path_buf(),
                reason: format!("Invalid regex: {error}"),
            })
    }

    fn internal_dependencies(
        &self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<Vec<String>, ResolveError> {
        let cmake_path = root.join(&pkg_config.path).join("CMakeLists.txt");
        let content = std::fs::read_to_string(&cmake_path)?;
        let package_name = self.extract_name_from_content(&content, &cmake_path)?;
        let re = Regex::new(&format!(
            r"(?is)target_link_libraries\s*\(\s*{}\s+([^\)]*)\)",
            regex::escape(&package_name)
        ))
        .map_err(|e| ResolveError::ParseError {
            path: cmake_path.clone(),
            reason: format!("Invalid regex: {e}"),
        })?;

        Ok(re
            .captures_iter(&content)
            .filter_map(|captures| captures.get(1))
            .flat_map(|dependencies| {
                dependencies
                    .as_str()
                    .split_whitespace()
                    .map(|dependency| dependency.trim_matches(['\'', '"']))
                    .filter(|dependency| !matches!(*dependency, "PUBLIC" | "PRIVATE" | "INTERFACE"))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect())
    }

    fn manifest_dependencies(
        &self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<Vec<ManifestDependency>, ResolveError> {
        let package_path = root.join(&pkg_config.path);
        if matches!(
            Self::resolve_manifest(&package_path)?,
            CppManifest::Qmake { .. }
        ) {
            return Ok(Vec::new());
        }
        Ok(self
            .internal_dependencies(root, pkg_config)?
            .into_iter()
            .map(|manifest_name| ManifestDependency {
                manifest_name,
                kind: DependencyKind::Runtime,
                requirement: None,
            })
            .collect())
    }

    /// Extract version from CMakeLists.txt content
    fn extract_version_from_content(
        &self,
        content: &str,
        cmake_path: &Path,
    ) -> Result<String, ResolveError> {
        // Match: project(...VERSION x.y.z...)
        let re = Regex::new(
            r"(?i)project\s*\([^)]*VERSION\s+([\d.]+(?:-[a-zA-Z0-9.-]+)?(?:\+[a-zA-Z0-9.-]+)?)",
        )
        .map_err(|e| ResolveError::ParseError {
            path: cmake_path.to_path_buf(),
            reason: format!("Invalid regex: {}", e),
        })?;

        let version = re
            .captures(content)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ResolveError::ParseError {
                path: cmake_path.to_path_buf(),
                reason: "VERSION not found in project() declaration".to_string(),
            })?;

        Ok(version)
    }

    /// Extract project name from CMakeLists.txt content
    fn extract_name_from_content(
        &self,
        content: &str,
        cmake_path: &Path,
    ) -> Result<String, ResolveError> {
        // Match: project(ProjectName ...) or project("project-name" ...)
        let re = Regex::new(r#"(?i)project\s*\(\s*["']?([a-zA-Z0-9_-]+)["']?"#).map_err(|e| {
            ResolveError::ParseError {
                path: cmake_path.to_path_buf(),
                reason: format!("Invalid regex: {}", e),
            }
        })?;

        let name = re
            .captures(content)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ResolveError::ParseError {
                path: cmake_path.to_path_buf(),
                reason: "Project name not found in project() declaration".to_string(),
            })?;

        Ok(name)
    }
}

impl EcosystemAdapter for CppResolver {
    fn ecosystem(&self) -> EcosystemId {
        EcosystemId::CPP
    }

    fn encode_version(&self, version: &semver::Version) -> Result<String, AdapterError> {
        if version.pre.is_empty() && version.build.is_empty() {
            Ok(version.to_string())
        } else {
            Err(AdapterError::InvalidVersion {
                ecosystem: EcosystemId::CPP,
                version: version.clone(),
                reason: "CMake project(VERSION) only accepts stable numeric versions".to_string(),
            })
        }
    }

    fn discover(&self, root: &camino::Utf8Path) -> Result<Vec<PackageInspection>, AdapterError> {
        let packages = self.discover_packages(root.as_std_path())?;
        let mut inspections = packages
            .into_iter()
            .map(|package| {
                let dependencies = self.manifest_dependencies(
                    root.as_std_path(),
                    &Self::package_config(package.path.clone()),
                )?;
                Self::package_inspection(
                    PackageId::new(package.name.clone()),
                    package,
                    dependencies,
                )
            })
            .collect::<Result<Vec<_>, AdapterError>>()?;
        inspections.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.path.cmp(&right.path))
        });
        Ok(inspections)
    }

    fn inspect(&self, location: &PackageLocation) -> Result<PackageInspection, AdapterError> {
        if location.path.is_absolute()
            || location
                .path
                .components()
                .any(|component| component == camino::Utf8Component::ParentDir)
        {
            return Err(AdapterError::InvalidInput {
                reason: format!(
                    "C++ package path must be relative to the project root: {}",
                    location.path
                ),
            });
        }
        let config = Self::package_config(location.path.as_std_path());
        let package = self.parse_package(location.project_root.as_std_path(), &config)?;
        let dependencies =
            self.manifest_dependencies(location.project_root.as_std_path(), &config)?;
        Self::package_inspection(location.id.clone(), package, dependencies)
    }

    fn plan_edits(&self, input: EcosystemPlanInput<'_>) -> Result<Vec<FileEdit>, AdapterError> {
        if input
            .workspace_packages
            .iter()
            .any(|package| package.ecosystem != EcosystemId::CPP)
        {
            return Err(AdapterError::InvalidInput {
                reason: "C++ edit planning received a non-C++ workspace package".to_string(),
            });
        }
        let workspace_packages = input
            .workspace_packages
            .iter()
            .map(|package| (package.id.clone(), package))
            .collect::<BTreeMap<_, _>>();
        let released_packages = input.released_packages.iter().collect::<BTreeSet<_>>();

        released_packages
            .into_iter()
            .map(|id| {
                let package = workspace_packages.get(id).copied().ok_or_else(|| {
                    AdapterError::InvalidInput {
                        reason: format!("released C++ package {id} is not in the workspace"),
                    }
                })?;
                Ok(Self::plan_file_edits(
                    input.project_root.as_std_path(),
                    package,
                    input.versions,
                )?)
            })
            .collect::<Result<Vec<_>, AdapterError>>()
            .map(|edits| edits.into_iter().flatten().collect())
    }
}

impl CppResolver {
    fn parse_package(
        &self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<ParsedPackage, ResolveError> {
        let package_path = root.join(&pkg_config.path);
        let manifest = Self::resolve_manifest(&package_path)?;
        let (name, version) = match manifest {
            CppManifest::Cmake { path } => {
                let content = std::fs::read_to_string(&path)?;
                (
                    self.extract_name_from_content(&content, &path)?,
                    self.extract_version_from_content(&content, &path)?,
                )
            }
            CppManifest::Qmake { path, variable: _ } => {
                let (_, version) = Self::qmake_version_candidate(&path)?.ok_or_else(|| {
                    ResolveError::ParseError {
                        path: path.clone(),
                        reason: "qmake version candidate disappeared during inspection".to_string(),
                    }
                })?;
                (
                    Self::qmake_package_name(&package_path, &path)?,
                    version.to_string(),
                )
            }
        };

        Ok(ParsedPackage {
            name,
            version: semver::Version::parse(&version)?,
            version_source: VersionSource::PackageManifest,
            path: pkg_config.path.clone(),
            private: false,
        })
    }

    fn discover_packages(&self, root: &Path) -> Result<Vec<ParsedPackage>, ResolveError> {
        let cmake_path = root.join("CMakeLists.txt");
        let has_qmake_files = !Self::qmake_files(root)?.is_empty();
        if !cmake_path.exists() && !has_qmake_files {
            log::warn!(
                "Cannot resolve C++ package in {}; no CMakeLists.txt, .pro, or .pri file found.",
                root.display()
            );
            return Ok(vec![]);
        }

        let root_manifest = Self::resolve_manifest(root)?;
        let root_package = self.parse_package(root, &Self::package_config("."))?;

        let mut packages = vec![root_package];
        if matches!(root_manifest, CppManifest::Cmake { .. }) {
            for member in self.workspace_members(root)? {
                packages.push(self.parse_package(root, &Self::package_config(member))?);
            }
        }

        Ok(packages)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        adapter::{AdapterError, EcosystemAdapter, EcosystemPlanInput, PackageLocation},
        config::{PackageConfig, ReleaseChannel},
        error::ResolveError,
        resolver::ResolverType,
    };
    use semifold_core::{EcosystemId, PackageId, PackageSnapshot, VersionMap, VersionSource};

    use super::CppResolver;

    fn temp_dir(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "semifold-cpp-resolver-{test_name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn package_config(path: impl Into<PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Cpp.into(),
            publish: None,
            channel: ReleaseChannel::Stable,
            channel_bump: None,
            assets: vec![],
            github_release: None,
            depends_on: vec![],
        }
    }

    fn write_cmake_project(root: &Path, path: &str, name: &str, version: &str) {
        let package_root = root.join(path);
        fs::create_dir_all(&package_root).unwrap();
        fs::write(
            package_root.join("CMakeLists.txt"),
            format!(
                "cmake_minimum_required(VERSION 3.20)\nproject({name} VERSION {version} LANGUAGES CXX)\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn resolves_a_single_cmake_project() {
        let root = temp_dir("single-package");
        write_cmake_project(&root, ".", "demo_library", "1.2.3-alpha.1+build.7");

        let package = CppResolver
            .parse_package(&root, &package_config("."))
            .unwrap();

        assert_eq!(package.name, "demo_library");
        assert_eq!(
            package.version,
            semver::Version::parse("1.2.3-alpha.1+build.7").unwrap()
        );
        assert_eq!(package.path, PathBuf::from("."));
        assert!(!package.private);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_the_root_cmake_project_only() {
        let root = temp_dir("root-discovery");
        write_cmake_project(&root, ".", "root-project", "1.0.0");
        write_cmake_project(&root, "libraries/child", "child-project", "2.0.0");

        let packages = CppResolver.discover_packages(&root).unwrap();

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "root-project");
        assert_eq!(packages[0].path, PathBuf::from("."));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_recursively_discovers_and_inspects_literal_subdirectories() {
        let root = temp_dir("adapter-workspace");
        write_cmake_project(&root, ".", "root-project", "1.0.0");
        fs::write(
            root.join("CMakeLists.txt"),
            "project(root-project VERSION 1.0.0)\nadd_subdirectory(groups)\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("groups")).unwrap();
        fs::write(
            root.join("groups/CMakeLists.txt"),
            "add_subdirectory(../libraries/core)\nadd_subdirectory(../applications/app)\n",
        )
        .unwrap();
        write_cmake_project(&root, "libraries/core", "core", "1.0.0");
        write_cmake_project(&root, "applications/app", "app", "1.0.0");
        fs::write(
            root.join("applications/app/CMakeLists.txt"),
            "project(app VERSION 1.0.0)\ntarget_link_libraries(app PRIVATE core external)\n",
        )
        .unwrap();
        let project_root = camino::Utf8PathBuf::from_path_buf(root.clone()).unwrap();

        let discovered = CppResolver.discover(&project_root).unwrap();
        assert_eq!(
            discovered
                .iter()
                .map(|package| package.id.as_str())
                .collect::<Vec<_>>(),
            ["app", "core", "root-project"]
        );
        let app = CppResolver
            .inspect(&PackageLocation {
                id: PackageId::new("configured-app"),
                project_root,
                path: "applications/app".into(),
            })
            .unwrap();

        assert_eq!(app.id, PackageId::new("configured-app"));
        assert_eq!(app.manifest_name, "app");
        assert_eq!(
            app.dependencies
                .iter()
                .map(|dependency| dependency.manifest_name.as_str())
                .collect::<Vec<_>>(),
            ["core", "external"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_rejects_a_literal_subdirectory_outside_the_project_root() {
        let root = temp_dir("adapter-escape");
        let external = temp_dir("adapter-escape-external");
        let external_name = external.file_name().unwrap().to_string_lossy();
        write_cmake_project(&external, ".", "external", "1.0.0");
        fs::write(
            root.join("CMakeLists.txt"),
            format!("project(root VERSION 1.0.0)\nadd_subdirectory(../{external_name})\n"),
        )
        .unwrap();
        let project_root = camino::Utf8PathBuf::from_path_buf(root.clone()).unwrap();

        assert!(matches!(
            CppResolver.discover(&project_root),
            Err(AdapterError::Manifest(ResolveError::InvalidConfig { .. }))
        ));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(external).unwrap();
    }

    #[test]
    fn plans_cmake_and_optional_vcpkg_edits_without_writing() {
        let root = temp_dir("plan-file-edits");
        write_cmake_project(&root, "library", "demo-library", "1.0.0");
        fs::write(
            root.join("library/vcpkg.json"),
            "{\"version\": \"1.0.0\"}\n",
        )
        .unwrap();
        let package = PackageSnapshot {
            id: PackageId::new("demo-library"),
            manifest_name: "demo-library".to_string(),
            version: semver::Version::new(1, 0, 0),
            version_source: VersionSource::PackageManifest,
            ecosystem: EcosystemId::CPP,
            path: "library".into(),
            publishable: true,
            dependencies: vec![],
        };

        let versions = VersionMap::from([(
            PackageId::new("demo-library"),
            semver::Version::new(1, 0, 1),
        )]);
        let edits = CppResolver
            .plan_edits(EcosystemPlanInput {
                project_root: camino::Utf8Path::from_path(&root).unwrap(),
                workspace_packages: std::slice::from_ref(&package),
                released_packages: std::slice::from_ref(&package.id),
                versions: &versions,
            })
            .unwrap();

        assert_eq!(edits.len(), 2);
        assert!(
            edits
                .iter()
                .any(|edit| edit.path == "library/CMakeLists.txt"
                    && edit.new_content.contains("VERSION 1.0.1"))
        );
        assert!(edits.iter().any(|edit| edit.path == "library/vcpkg.json" && edit.new_content.contains("1.0.1")));
        assert!(
            fs::read_to_string(root.join("library/CMakeLists.txt"))
                .unwrap()
                .contains("VERSION 1.0.0")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_and_plans_a_quoted_qmake_version() {
        let root = temp_dir("qmake-direct");
        fs::write(
            root.join("demo.pro"),
            "TEMPLATE = app\nTARGET = demo-app\nVERSION = \"1.2.3\" # release\n",
        )
        .unwrap();

        let inspection = CppResolver
            .inspect(&PackageLocation {
                id: PackageId::new("demo"),
                project_root: camino::Utf8PathBuf::from_path_buf(root.clone()).unwrap(),
                path: ".".into(),
            })
            .unwrap();
        assert_eq!(inspection.manifest_name, "demo-app");
        assert_eq!(inspection.version, semver::Version::new(1, 2, 3));

        let snapshot = PackageSnapshot {
            id: inspection.id,
            manifest_name: inspection.manifest_name,
            version: inspection.version,
            version_source: inspection.version_source,
            ecosystem: inspection.ecosystem,
            path: inspection.path,
            publishable: inspection.publishable,
            dependencies: vec![],
        };
        let versions = VersionMap::from([(snapshot.id.clone(), semver::Version::new(1, 2, 4))]);
        let edits = CppResolver
            .plan_edits(EcosystemPlanInput {
                project_root: camino::Utf8Path::from_path(&root).unwrap(),
                workspace_packages: std::slice::from_ref(&snapshot),
                released_packages: std::slice::from_ref(&snapshot.id),
                versions: &versions,
            })
            .unwrap();

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].path, "demo.pro");
        assert!(
            edits[0]
                .new_content
                .contains("VERSION = \"1.2.4\" # release")
        );
        assert!(
            fs::read_to_string(root.join("demo.pro"))
                .unwrap()
                .contains("1.2.3")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_and_rewrites_qmake_component_variables_in_pri() {
        let root = temp_dir("qmake-components");
        fs::write(
            root.join("demo.pro"),
            "TEMPLATE = app\nTARGET = demo-components\ninclude(version.pri)\n",
        )
        .unwrap();
        fs::write(
            root.join("version.pri"),
            "VERSION_MAJOR = \"2\"\nVERSION_MINOR = '4'\nVERSION_PATCH = 6\n",
        )
        .unwrap();

        let package = CppResolver
            .parse_package(&root, &package_config("."))
            .unwrap();
        assert_eq!(package.name, "demo-components");
        assert_eq!(package.version, semver::Version::new(2, 4, 6));

        let snapshot = PackageSnapshot {
            id: PackageId::new("version"),
            manifest_name: "version".to_string(),
            version: package.version,
            version_source: VersionSource::PackageManifest,
            ecosystem: EcosystemId::CPP,
            path: ".".into(),
            publishable: true,
            dependencies: vec![],
        };
        let versions = VersionMap::from([(snapshot.id.clone(), semver::Version::new(3, 0, 1))]);
        let edits = CppResolver
            .plan_edits(EcosystemPlanInput {
                project_root: camino::Utf8Path::from_path(&root).unwrap(),
                workspace_packages: std::slice::from_ref(&snapshot),
                released_packages: std::slice::from_ref(&snapshot.id),
                versions: &versions,
            })
            .unwrap();

        assert!(edits[0].new_content.contains("VERSION_MAJOR = \"3\""));
        assert!(edits[0].new_content.contains("VERSION_MINOR = '0'"));
        assert!(edits[0].new_content.contains("VERSION_PATCH = 1"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_an_invalid_static_qmake_version() {
        let root = temp_dir("qmake-invalid-version");
        fs::write(root.join("app.pro"), "TARGET = app\nVERSION = 1.2\n").unwrap();

        let error = CppResolver
            .parse_package(&root, &package_config("."))
            .unwrap_err();
        assert!(error.to_string().contains("invalid qmake VERSION `1.2`"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_duplicate_and_dynamic_qmake_targets() {
        let duplicate_root = temp_dir("qmake-duplicate-target");
        fs::write(
            duplicate_root.join("app.pro"),
            "TARGET = first\nTARGET = second\nVERSION = 1.2.3\n",
        )
        .unwrap();
        let duplicate_error = CppResolver
            .parse_package(&duplicate_root, &package_config("."))
            .unwrap_err();
        assert!(
            duplicate_error
                .to_string()
                .contains("qmake variable TARGET has multiple assignments")
        );
        fs::remove_dir_all(duplicate_root).unwrap();

        let duplicate_files_root = temp_dir("qmake-duplicate-target-files");
        fs::write(
            duplicate_files_root.join("app.pro"),
            "TARGET = shared\nVERSION = 1.2.3\n",
        )
        .unwrap();
        fs::write(duplicate_files_root.join("tool.pro"), "TARGET = shared\n").unwrap();
        let duplicate_files_error = CppResolver
            .parse_package(&duplicate_files_root, &package_config("."))
            .unwrap_err();
        assert!(
            duplicate_files_error
                .to_string()
                .contains("multiple qmake TARGET candidates found: shared, shared")
        );
        fs::remove_dir_all(duplicate_files_root).unwrap();

        let dynamic_root = temp_dir("qmake-dynamic-target");
        fs::write(
            dynamic_root.join("app.pro"),
            "TARGET = app $$TARGET_SUFFIX\nVERSION = 1.2.3\n",
        )
        .unwrap();
        let dynamic_error = CppResolver
            .parse_package(&dynamic_root, &package_config("."))
            .unwrap_err();
        assert!(
            dynamic_error
                .to_string()
                .contains("qmake TARGET `app $$TARGET_SUFFIX` is not a static identifier")
        );
        fs::remove_dir_all(dynamic_root).unwrap();

        let dynamic_version_root = temp_dir("qmake-dynamic-version");
        fs::write(
            dynamic_version_root.join("app.pro"),
            "TARGET = app\nVERSION = 1.2.3 $$VERSION_SUFFIX\n",
        )
        .unwrap();
        let dynamic_version_error = CppResolver
            .parse_package(&dynamic_version_root, &package_config("."))
            .unwrap_err();
        assert!(
            dynamic_version_error
                .to_string()
                .contains("invalid qmake VERSION `1.2.3 $$VERSION_SUFFIX`")
        );
        fs::remove_dir_all(dynamic_version_root).unwrap();
    }

    #[test]
    fn rejects_ambiguous_qmake_version_candidates() {
        let root = temp_dir("qmake-ambiguous");
        fs::write(root.join("app.pro"), "VERSION = 1.0.0\n").unwrap();
        fs::write(root.join("shared.pri"), "VERSION = 2.0.0\n").unwrap();

        let error = CppResolver
            .parse_package(&root, &package_config("."))
            .unwrap_err();
        assert!(matches!(error, ResolveError::ParseError { .. }));
        assert!(
            error
                .to_string()
                .contains("multiple qmake version candidates")
        );
        assert!(error.to_string().contains("app.pro"));
        assert!(error.to_string().contains("shared.pri"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn versioned_cmake_takes_precedence_over_qmake() {
        let root = temp_dir("cmake-qmake-precedence");
        write_cmake_project(&root, ".", "cmake-app", "1.0.0");
        fs::write(
            root.join("qmake-app.pro"),
            "TARGET = qmake-app\nVERSION = 2.0.0\n",
        )
        .unwrap();

        let package = CppResolver
            .parse_package(&root, &package_config("."))
            .unwrap();
        assert_eq!(package.name, "cmake-app");
        assert_eq!(package.version, semver::Version::new(1, 0, 0));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_named_channels_for_cmake_versions() {
        let version = semver::Version::parse("1.2.3-alpha.0").unwrap();

        assert!(matches!(
            CppResolver.encode_version(&version),
            Err(AdapterError::InvalidVersion {
                ecosystem,
                ..
            }) if ecosystem == EcosystemId::CPP
        ));
    }
}
