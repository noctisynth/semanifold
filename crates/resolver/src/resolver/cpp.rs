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
    resolver::ResolverType,
    utils,
};

/// C++ resolver for CMake-based projects
pub struct CppResolver;

impl CppResolver {
    fn package_config(path: impl Into<PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Cpp,
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
        let cmake_path = package_path.join("CMakeLists.txt");
        let cmake = std::fs::read_to_string(&cmake_path)?;
        let re = Regex::new(
            r"(?i)(project\s*\([^)]*VERSION\s+)([\d.]+(?:-[a-zA-Z0-9.-]+)?(?:\+[a-zA-Z0-9.-]+)?)",
        )
        .map_err(|error| ResolveError::ParseError {
            path: cmake_path.clone(),
            reason: error.to_string(),
        })?;
        let cmake_updated = re.replace(&cmake, |caps: &regex::Captures| {
            format!("{}{}", &caps[1], version)
        });
        let mut edits = vec![FileEdit {
            path: package.path.join("CMakeLists.txt"),
            expected: FileEditExpectation::Existing {
                hash: FileHash::from_bytes(cmake.as_bytes()),
            },
            new_content: cmake_updated.into_owned(),
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
                path: package.path.join("vcpkg.json"),
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
                    .map(Path::to_path_buf)
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
        let cmake_path = package_path.join("CMakeLists.txt");

        if !cmake_path.exists() {
            return Err(ResolveError::FileOrDirNotFound {
                path: cmake_path.clone(),
            });
        }

        // Read file once and extract both name and version
        let content = std::fs::read_to_string(&cmake_path)?;
        let name = self.extract_name_from_content(&content, &cmake_path)?;
        let version = self.extract_version_from_content(&content, &cmake_path)?;

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
        if !cmake_path.exists() {
            log::warn!(
                "Cannot resolve package in {}, CMakeLists.txt not found.",
                root.display()
            );
            return Ok(vec![]);
        }

        let root_package = self.parse_package(root, &Self::package_config("."))?;

        let mut packages = vec![root_package];
        for member in self.workspace_members(root)? {
            packages.push(self.parse_package(root, &Self::package_config(member))?);
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
            resolver: ResolverType::Cpp,
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
