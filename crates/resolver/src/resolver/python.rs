use std::{collections::BTreeMap, path::Path};

use semifold_core::{
    DependencyKind, EcosystemId, EditSource, FileEdit, FileEditExpectation, FileHash, PackageId,
    PackageSnapshot, VersionMap, VersionSource,
};
use semver::{Prerelease, Version};
use serde::{Deserialize, Serialize};

use crate::{
    adapter::{
        AdapterError, EcosystemAdapter, EcosystemPlanInput, ManifestDependency, PackageInspection,
        PackageLocation, ParsedPackage,
    },
    config::{PackageConfig, ReleaseChannel},
    error::ResolveError,
};

#[derive(Serialize, Deserialize, Debug)]
struct PyProjectToml {
    pub project: Option<ProjectMetadata>,
    pub tool: Option<ToolMetadata>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ProjectMetadata {
    pub name: String,
    pub version: Option<String>,
    pub dynamic: Option<Vec<String>>,
    pub dependencies: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct ToolMetadata {
    pub poetry: Option<PoetryMetadata>,
    pub hatch: Option<HatchMetadata>,
}

#[derive(Serialize, Deserialize, Debug)]
struct PoetryMetadata {
    pub name: Option<String>,
    pub version: Option<String>,
    pub dependencies: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Serialize, Deserialize, Debug)]
struct HatchMetadata {
    pub version: Option<HatchVersion>,
}

#[derive(Serialize, Deserialize, Debug)]
struct HatchVersion {
    pub path: Option<String>,
}

/// Cargo.toml 结构（用于 maturin/PyO3 项目）
#[derive(Serialize, Deserialize, Debug)]
struct CargoToml {
    pub package: Option<CargoPackage>,
}

#[derive(Serialize, Deserialize, Debug)]
struct CargoPackage {
    pub name: Option<String>,
    pub version: Option<String>,
}

pub struct PythonResolver;

fn parse_python_version(value: &str) -> Result<Version, ResolveError> {
    if let Ok(version) = Version::parse(value) {
        return Ok(version);
    }

    let (base, channel, sequence) = if let Some((base, sequence)) = value.rsplit_once(".post") {
        (base, "post", sequence)
    } else if let Some((base, sequence)) = value.rsplit_once("rc") {
        (base, "rc", sequence)
    } else if let Some((base, sequence)) = value.rsplit_once('a') {
        (base, "alpha", sequence)
    } else if let Some((base, sequence)) = value.rsplit_once('b') {
        (base, "beta", sequence)
    } else {
        return Err(ResolveError::InvalidVersion {
            version: value.to_string(),
            reason: "expected SemVer or a supported PEP 440 pre/post-release".to_string(),
        });
    };
    if sequence.is_empty() || !sequence.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ResolveError::InvalidVersion {
            version: value.to_string(),
            reason: "PEP 440 release sequence must be a non-negative integer".to_string(),
        });
    }
    let mut version = Version::parse(base).map_err(|error| ResolveError::InvalidVersion {
        version: value.to_string(),
        reason: format!("invalid PEP 440 release base: {error}"),
    })?;
    version.pre = Prerelease::new(&format!("{channel}.{sequence}"))?;
    Ok(version)
}

fn encode_python_version(version: &Version) -> Result<String, AdapterError> {
    if version.pre.is_empty() {
        return Ok(version.to_string());
    }
    if !version.build.is_empty() {
        return Err(AdapterError::InvalidVersion {
            ecosystem: EcosystemId::PYTHON,
            version: version.clone(),
            reason: "PEP 440 named channels do not support SemVer build metadata".to_string(),
        });
    }
    let Some((channel, sequence)) = version.pre.as_str().split_once('.') else {
        return Err(AdapterError::InvalidVersion {
            ecosystem: EcosystemId::PYTHON,
            version: version.clone(),
            reason: "named release channels require a numeric sequence".to_string(),
        });
    };
    if sequence.is_empty()
        || !sequence.bytes().all(|byte| byte.is_ascii_digit())
        || sequence.contains('.')
    {
        return Err(AdapterError::InvalidVersion {
            ecosystem: EcosystemId::PYTHON,
            version: version.clone(),
            reason: "named release channel sequences must be non-negative integers".to_string(),
        });
    }

    let base = format!("{}.{}.{}", version.major, version.minor, version.patch);
    let encoded = match channel {
        "alpha" => format!("{base}a{sequence}"),
        "beta" => format!("{base}b{sequence}"),
        "rc" => format!("{base}rc{sequence}"),
        "post" => format!("{base}.post{sequence}"),
        _ => {
            return Err(AdapterError::InvalidVersion {
                ecosystem: EcosystemId::PYTHON,
                version: version.clone(),
                reason: format!("unsupported Python release channel {channel}"),
            });
        }
    };
    Ok(encoded)
}

impl PythonResolver {
    fn package_config(path: impl Into<std::path::PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: EcosystemId::PYTHON,
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
                reason: format!("Python package path is not valid UTF-8: {}", path.display()),
            }
        })?;
        Ok(PackageInspection {
            id,
            manifest_name: package.name,
            version: package.version,
            version_source: package.version_source,
            ecosystem: EcosystemId::PYTHON,
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
        let version = PythonResolver
            .encode_version(next_version)
            .map_err(|error| ResolveError::InvalidVersion {
                version: next_version.to_string(),
                reason: error.to_string(),
            })?;
        let mut edits = Vec::new();
        let mut configured_version_path = None;

        let pyproject_path = package.path.join("pyproject.toml");
        let pyproject_absolute = root.join(pyproject_path.as_std_path());
        if pyproject_absolute.exists() {
            let original = std::fs::read_to_string(&pyproject_absolute)?;
            let mut document = original
                .parse::<toml_edit::DocumentMut>()
                .map_err(|error| ResolveError::ParseError {
                    path: pyproject_absolute.clone(),
                    reason: error.to_string(),
                })?;
            if let Some(project) = document
                .get_mut("project")
                .and_then(|item| item.as_table_mut())
            {
                let version_is_dynamic = project
                    .get("dynamic")
                    .and_then(|item| item.as_array())
                    .is_some_and(|fields| {
                        fields.iter().any(|field| field.as_str() == Some("version"))
                    });
                if !version_is_dynamic {
                    project.insert("version", toml_edit::value(&version));
                }
            }
            configured_version_path = document
                .get("tool")
                .and_then(|item| item.as_table())
                .and_then(|tool| tool.get("hatch"))
                .and_then(|item| item.as_table())
                .and_then(|hatch| hatch.get("version"))
                .and_then(|item| item.as_table())
                .and_then(|version| version.get("path"))
                .and_then(|item| item.as_str())
                .map(str::to_string);
            if let Some(poetry) = document
                .get_mut("tool")
                .and_then(|item| item.as_table_mut())
                .and_then(|tool| tool.get_mut("poetry"))
                .and_then(|item| item.as_table_mut())
            {
                poetry.insert("version", toml_edit::value(&version));
            }
            let new_content = document.to_string();
            if new_content != original {
                edits.push(Self::file_edit(
                    package,
                    pyproject_path.as_str(),
                    original,
                    new_content,
                ));
            }
        }

        let setup_path = package.path.join("setup.cfg");
        let setup_absolute = root.join(setup_path.as_std_path());
        if setup_absolute.exists() {
            let original = std::fs::read_to_string(&setup_absolute)?;
            let updated = original
                .lines()
                .map(|line| {
                    let is_version = line
                        .split_once('=')
                        .is_some_and(|(key, _)| key.trim() == "version");
                    if is_version {
                        format!("version = {version}")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if updated != original {
                edits.push(Self::file_edit(
                    package,
                    setup_path.as_str(),
                    original,
                    updated,
                ));
            }
        }

        let package_name = package.manifest_name.replace('-', "_");
        let mut version_paths = configured_version_path
            .map(|path| vec![package.path.join(path)])
            .unwrap_or_default();
        version_paths.extend([
            package.path.join(&package_name).join("__init__.py"),
            package
                .path
                .join("src")
                .join(&package_name)
                .join("__init__.py"),
            package.path.join(&package_name).join("__version__.py"),
            package
                .path
                .join("src")
                .join(&package_name)
                .join("__version__.py"),
            package.path.join("src").join("__init__.py"),
        ]);
        version_paths.dedup();
        for relative in version_paths {
            let absolute = root.join(relative.as_std_path());
            if !absolute.exists() {
                continue;
            }
            let original = std::fs::read_to_string(&absolute)?;
            if let Some(updated) = Self::static_version_content(&original, &version) {
                edits.push(Self::file_edit(
                    package,
                    relative.as_str(),
                    original,
                    updated,
                ));
                break;
            }
        }

        Ok(edits)
    }

    fn file_edit(
        package: &PackageSnapshot,
        path: &str,
        original: String,
        new_content: String,
    ) -> FileEdit {
        FileEdit {
            path: path.into(),
            expected: FileEditExpectation::Existing {
                hash: FileHash::from_bytes(original.as_bytes()),
            },
            new_content,
            source: EditSource::PackageVersion {
                package: package.id.clone(),
            },
        }
    }

    fn static_version_content(content: &str, version: &str) -> Option<String> {
        let mut output = String::new();
        let mut updated = false;
        for line in content.lines() {
            let trimmed = line.trim();
            let dynamic = trimmed.contains("version(")
                || trimmed.contains("get_version()")
                || trimmed.contains("importlib")
                || trimmed.contains("pkg_resources");
            let static_assignment = trimmed
                .strip_prefix("__version__")
                .and_then(|rest| rest.find('=').map(|position| rest[position + 1..].trim()))
                .is_some_and(|value| {
                    (value.starts_with('"') && value.ends_with('"'))
                        || (value.starts_with('\'') && value.ends_with('\''))
                });
            if !dynamic && static_assignment {
                output.push_str(&format!("__version__ = \"{version}\"\n"));
                updated = true;
            } else {
                output.push_str(line);
                output.push('\n');
            }
        }
        updated.then_some(output)
    }

    fn pep_dependency(specification: String) -> ManifestDependency {
        let boundary = specification
            .char_indices()
            .find_map(|(index, character)| {
                matches!(character, '<' | '>' | '=' | '~' | '!' | ';' | '[' | '@')
                    .then_some(index)
                    .or_else(|| character.is_whitespace().then_some(index))
            })
            .unwrap_or(specification.len());
        let manifest_name = specification[..boundary].trim().to_string();
        let requirement = specification[boundary..].trim();
        let requirement = (!requirement.is_empty()).then(|| requirement.to_string());
        ManifestDependency {
            manifest_name,
            kind: DependencyKind::Runtime,
            requirement,
        }
    }

    fn manifest_dependencies(
        &self,
        root: &Path,
        pkg_path: &Path,
    ) -> Result<Vec<ManifestDependency>, ResolveError> {
        let pyproject_path = root.join(pkg_path).join("pyproject.toml");
        if !pyproject_path.exists() {
            return Ok(vec![]);
        }

        let pyproject: PyProjectToml =
            toml_edit::de::from_str(&std::fs::read_to_string(&pyproject_path)?).map_err(|e| {
                ResolveError::ParseError {
                    path: pyproject_path,
                    reason: e.to_string(),
                }
            })?;
        let mut dependencies = Vec::new();
        if let Some(project) = pyproject.project {
            dependencies.extend(
                project
                    .dependencies
                    .unwrap_or_default()
                    .into_iter()
                    .map(Self::pep_dependency),
            );
        }
        if let Some(poetry) = pyproject.tool.and_then(|tool| tool.poetry) {
            dependencies.extend(
                poetry
                    .dependencies
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|(name, _)| name != "python")
                    .map(|(manifest_name, requirement)| ManifestDependency {
                        manifest_name,
                        kind: DependencyKind::Runtime,
                        requirement: requirement
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| Some(requirement.to_string())),
                    }),
            );
        }
        Ok(dependencies)
    }

    fn resolve_pyproject(
        &self,
        root: &Path,
        pkg_path: &Path,
    ) -> Result<ParsedPackage, ResolveError> {
        let pyproject_path = root.join(pkg_path).join("pyproject.toml");
        if !pyproject_path.exists() {
            return Err(ResolveError::FileOrDirNotFound {
                path: pyproject_path.clone(),
            });
        }

        let pyproject_str = std::fs::read_to_string(&pyproject_path)?;
        let pyproject: PyProjectToml =
            toml_edit::de::from_str(&pyproject_str).map_err(|e| ResolveError::ParseError {
                path: pyproject_path.clone(),
                reason: e.to_string(),
            })?;

        let (name, version) = if let Some(project) = pyproject.project {
            // PEP 621 标准格式
            let name = project.name.clone();

            let is_version_dynamic = project
                .dynamic
                .as_ref()
                .map(|d| d.iter().any(|field| field == "version"))
                .unwrap_or(false);

            let version = if is_version_dynamic {
                // version 是动态的，尝试从其他地方获取
                log::debug!(
                    "Version is declared as dynamic in {}, attempting to extract from source files",
                    pyproject_path.display()
                );
                self.extract_version_from_source(root, pkg_path, &name)
                    .unwrap_or_else(|e| {
                        log::warn!("Failed to extract dynamic version: {}, using default", e);
                        "0.0.0".to_string()
                    })
            } else {
                project.version.unwrap_or_else(|| "0.0.0".to_string())
            };

            (name, version)
        } else if let Some(tool) = pyproject.tool {
            if let Some(poetry) = tool.poetry {
                // Poetry 格式
                let name = poetry.name.ok_or(ResolveError::InvalidConfig {
                    path: pyproject_path.clone(),
                    reason: "Poetry project name not found".to_string(),
                })?;
                let version = poetry.version.unwrap_or_else(|| "0.0.0".to_string());
                (name, version)
            } else {
                return Err(ResolveError::InvalidConfig {
                    path: pyproject_path.clone(),
                    reason: "No project metadata found in pyproject.toml".to_string(),
                });
            }
        } else {
            return Err(ResolveError::InvalidConfig {
                path: pyproject_path.clone(),
                reason: "No project metadata found in pyproject.toml".to_string(),
            });
        };

        Ok(ParsedPackage {
            name,
            version: parse_python_version(&version)?,
            version_source: VersionSource::PackageManifest,
            path: pkg_path.to_path_buf(),
            private: false,
        })
    }

    /// 从 setup.cfg 解析元数据文件（fallback）
    fn resolve_setup_cfg(
        &self,
        root: &Path,
        pkg_path: &Path,
    ) -> Result<ParsedPackage, ResolveError> {
        let setup_cfg_path = root.join(pkg_path).join("setup.cfg");
        if !setup_cfg_path.exists() {
            return Err(ResolveError::FileOrDirNotFound {
                path: setup_cfg_path.clone(),
            });
        }

        let setup_cfg_str = std::fs::read_to_string(&setup_cfg_path)?;
        // ini parse
        let mut name: Option<String> = None;
        let mut version: Option<String> = None;
        let mut in_metadata = false;

        for line in setup_cfg_str.lines() {
            let trimmed = line.trim();
            if trimmed == "[metadata]" {
                in_metadata = true;
                continue;
            }
            if trimmed.starts_with('[') {
                in_metadata = false;
            }
            if in_metadata {
                if let Some(rest) = trimmed.strip_prefix("name") {
                    if let Some(val) = rest.trim().strip_prefix('=') {
                        name = Some(val.trim().to_string());
                    }
                } else if let Some(rest) = trimmed.strip_prefix("version")
                    && let Some(val) = rest.trim().strip_prefix('=')
                {
                    version = Some(val.trim().to_string());
                }
            }
        }

        let name = name.ok_or(ResolveError::InvalidConfig {
            path: setup_cfg_path.clone(),
            reason: "Package name not found in setup.cfg".to_string(),
        })?;
        let version = version.unwrap_or_else(|| "0.0.0".to_string());

        Ok(ParsedPackage {
            name,
            version: parse_python_version(&version)?,
            version_source: VersionSource::PackageManifest,
            path: pkg_path.to_path_buf(),
            private: false,
        })
    }

    fn resolve_package(&self, root: &Path, pkg_path: &Path) -> Result<ParsedPackage, ResolveError> {
        let setup_cfg_exists = root.join(pkg_path).join("setup.cfg").exists();
        if root.join(pkg_path).join("pyproject.toml").exists() {
            match self.resolve_pyproject(root, pkg_path) {
                Ok(package) => return Ok(package),
                Err(ResolveError::InvalidConfig { .. }) if setup_cfg_exists => {}
                Err(error) => return Err(error),
            }
        }
        if setup_cfg_exists {
            return self.resolve_setup_cfg(root, pkg_path);
        }
        Err(ResolveError::FileOrDirNotFound {
            path: root.join(pkg_path),
        })
    }

    /// 从源文件中提取动态版本号
    /// 当 pyproject.toml 中声明 `dynamic = ["version"]` 时使用
    ///
    /// 尝试从以下位置提取版本号（按优先级）：
    /// 1. `<package>/__init__.py` 中的 `__version__`
    /// 2. `src/<package>/__init__.py` 中的 `__version__`
    /// 3. `<package>/__version__.py` 中的 `__version__`
    /// 4. `src/<package>/__version__.py` 中的 `__version__`
    /// 5. `Cargo.toml` 中的 version（用于 maturin/PyO3 项目）
    /// 6. Hatch 配置中的 version.path
    fn extract_version_from_source(
        &self,
        root: &Path,
        pkg_path: &Path,
        package_name: &str,
    ) -> Result<String, ResolveError> {
        // 尝试从常见位置提取 __version__
        let version_file_paths = vec![
            root.join(pkg_path).join(package_name).join("__init__.py"),
            root.join(pkg_path)
                .join("src")
                .join(package_name)
                .join("__init__.py"),
            root.join(pkg_path)
                .join(package_name)
                .join("__version__.py"),
            root.join(pkg_path)
                .join("src")
                .join(package_name)
                .join("__version__.py"),
        ];

        for file_path in &version_file_paths {
            if file_path.exists()
                && let Ok(content) = std::fs::read_to_string(file_path)
                && let Some(version) = self.extract_version_from_content(&content)
            {
                log::debug!(
                    "Extracted version '{}' from {}",
                    version,
                    file_path.display()
                );
                return Ok(version);
            }
        }

        // 尝试从 Cargo.toml 获取版本（用于 maturin/PyO3 项目）
        let cargo_toml_path = root.join(pkg_path).join("Cargo.toml");
        if cargo_toml_path.exists() {
            log::debug!("Found Cargo.toml, attempting to extract version for maturin/PyO3 project");
            if let Ok(cargo_str) = std::fs::read_to_string(&cargo_toml_path)
                && let Ok(cargo_toml) = toml_edit::de::from_str::<CargoToml>(&cargo_str)
                && let Some(version) = cargo_toml.package.and_then(|p| p.version)
            {
                log::debug!(
                    "Extracted version '{}' from Cargo.toml for maturin/PyO3 project",
                    version
                );
                return Ok(version);
            }
        }

        // 尝试从 Hatch 配置中获取 version.path
        let pyproject_path = root.join(pkg_path).join("pyproject.toml");
        if pyproject_path.exists()
            && let Ok(pyproject_str) = std::fs::read_to_string(&pyproject_path)
            && let Ok(pyproject) = toml_edit::de::from_str::<PyProjectToml>(&pyproject_str)
            && let Some(tool) = pyproject.tool
            && let Some(hatch) = tool.hatch
            && let Some(version_config) = hatch.version
            && let Some(version_path) = version_config.path
        {
            let hatch_version_file = root.join(pkg_path).join(version_path);
            if hatch_version_file.exists()
                && let Ok(content) = std::fs::read_to_string(&hatch_version_file)
                && let Some(version) = self.extract_version_from_content(&content)
            {
                log::debug!(
                    "Extracted version '{}' from Hatch version.path: {}",
                    version,
                    hatch_version_file.display()
                );
                return Ok(version);
            }
        }

        Err(ResolveError::InvalidConfig {
            path: root.join(pkg_path).to_path_buf(),
            reason: format!(
                "Could not extract version from source files for package '{}'. \
                 Version is declared as dynamic but no __version__ found in common locations \
                 (checked: __init__.py, __version__.py, Cargo.toml, Hatch version.path).",
                package_name
            ),
        })
    }

    /// 从文件内容中提取 __version__ 值
    /// 支持的格式：
    /// - `__version__ = "1.0.0"`
    /// - `__version__ = '1.0.0'`
    /// - `__version__: str = "1.0.0"`
    fn extract_version_from_content(&self, content: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("__version__") {
                // 排除动态获取的情况
                if trimmed.contains("version(")
                    || trimmed.contains("get_version()")
                    || trimmed.contains("importlib")
                    || trimmed.contains("pkg_resources")
                {
                    continue;
                }

                // 提取静态版本号
                if let Some(pos) = trimmed.find('=') {
                    let value_part = trimmed[pos + 1..].trim();

                    // 处理单引号或双引号
                    if let Some(version) = value_part
                        .strip_prefix('"')
                        .and_then(|s| s.strip_suffix('"'))
                    {
                        return Some(version.to_string());
                    }
                    if let Some(version) = value_part
                        .strip_prefix('\'')
                        .and_then(|s| s.strip_suffix('\''))
                    {
                        return Some(version.to_string());
                    }
                }
            }
        }
        None
    }
}

impl EcosystemAdapter for PythonResolver {
    fn ecosystem(&self) -> EcosystemId {
        EcosystemId::PYTHON
    }

    fn encode_version(&self, version: &Version) -> Result<String, AdapterError> {
        encode_python_version(version)
    }

    fn discover(&self, root: &camino::Utf8Path) -> Result<Vec<PackageInspection>, AdapterError> {
        let packages = self.discover_packages(root.as_std_path())?;
        let mut inspections = packages
            .into_iter()
            .map(|package| {
                let dependencies = self.manifest_dependencies(root.as_std_path(), &package.path)?;
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
                    "Python package path must be relative to the project root: {}",
                    location.path
                ),
            });
        }
        let package = self.resolve_package(
            location.project_root.as_std_path(),
            location.path.as_std_path(),
        )?;
        let dependencies = self.manifest_dependencies(
            location.project_root.as_std_path(),
            location.path.as_std_path(),
        )?;
        Self::package_inspection(location.id.clone(), package, dependencies)
    }

    fn plan_edits(&self, input: EcosystemPlanInput<'_>) -> Result<Vec<FileEdit>, AdapterError> {
        if input
            .workspace_packages
            .iter()
            .any(|package| package.ecosystem != EcosystemId::PYTHON)
        {
            return Err(AdapterError::InvalidInput {
                reason: "Python edit planning received a non-Python workspace package".to_string(),
            });
        }
        let workspace_packages = input
            .workspace_packages
            .iter()
            .map(|package| (package.id.clone(), package))
            .collect::<BTreeMap<_, _>>();
        let released_packages = input
            .released_packages
            .iter()
            .collect::<std::collections::BTreeSet<_>>();

        released_packages
            .into_iter()
            .map(|id| {
                let package = workspace_packages.get(id).copied().ok_or_else(|| {
                    AdapterError::InvalidInput {
                        reason: format!("released Python package {id} is not in the workspace"),
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

impl PythonResolver {
    fn parse_package(
        &self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<ParsedPackage, ResolveError> {
        self.resolve_package(root, &pkg_config.path)
    }

    fn discover_packages(&self, root: &Path) -> Result<Vec<ParsedPackage>, ResolveError> {
        let mut packages = Vec::new();

        // 检查是否是单包项目
        if root.join("pyproject.toml").exists() || root.join("setup.cfg").exists() {
            packages.push(self.parse_package(root, &Self::package_config("."))?);
        }

        // 检查常见的 monorepo 结构
        let common_patterns = vec!["packages/*", "libs/*", "apps/*"];

        for pattern in common_patterns {
            let glob_pattern = root.join(pattern).display().to_string();
            let paths = glob::glob(&glob_pattern)?
                .map(|path| {
                    path.map_err(|error| ResolveError::ParseError {
                        path: error.path().to_path_buf(),
                        reason: error.to_string(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            for path in paths {
                if path.join("pyproject.toml").exists() || path.join("setup.cfg").exists() {
                    let rel_path = pathdiff::diff_paths(&path, root).unwrap_or(path.clone());
                    packages.push(self.parse_package(root, &Self::package_config(rel_path))?);
                }
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
        adapter::{EcosystemAdapter, EcosystemPlanInput, PackageLocation},
        config::{PackageConfig, ReleaseChannel},
        error::ResolveError,
        resolver::ResolverType,
    };
    use semifold_core::{EcosystemId, PackageId, PackageSnapshot, VersionMap, VersionSource};

    use super::{PythonResolver, parse_python_version};

    fn temp_dir(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "semifold-python-resolver-{test_name}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn package_config(path: impl Into<PathBuf>) -> PackageConfig {
        PackageConfig {
            path: path.into(),
            resolver: ResolverType::Python.into(),
            publish: None,
            channel: ReleaseChannel::Stable,
            channel_bump: None,
            assets: vec![],
            github_release: None,
            depends_on: vec![],
        }
    }

    fn write_pyproject(root: &Path, path: &str, content: &str) {
        let package_root = root.join(path);
        fs::create_dir_all(&package_root).unwrap();
        fs::write(package_root.join("pyproject.toml"), content).unwrap();
    }

    #[test]
    fn resolves_pep_621_project_metadata() {
        let root = temp_dir("pep-621");
        write_pyproject(
            &root,
            ".",
            "[project]\nname = \"example\"\nversion = \"1.2.3\"\n",
        );

        let package = PythonResolver
            .parse_package(&root, &package_config("."))
            .unwrap();

        assert_eq!(package.name, "example");
        assert_eq!(package.version, semver::Version::parse("1.2.3").unwrap());
        assert_eq!(package.path, PathBuf::from("."));
        assert!(!package.private);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn encodes_and_parses_supported_pep_440_release_channels() {
        let cases = [
            ("1.2.3-alpha.4", "1.2.3a4"),
            ("1.2.3-beta.4", "1.2.3b4"),
            ("1.2.3-rc.4", "1.2.3rc4"),
            ("1.2.3-post.4", "1.2.3.post4"),
        ];

        for (domain, encoded) in cases {
            let domain = semver::Version::parse(domain).unwrap();
            assert_eq!(PythonResolver.encode_version(&domain).unwrap(), encoded);
            assert_eq!(parse_python_version(encoded).unwrap(), domain);
        }
    }

    #[test]
    fn rejects_python_channels_without_a_pep_440_encoding() {
        let version = semver::Version::parse("1.2.3-nightly.0").unwrap();

        assert!(matches!(
            PythonResolver.encode_version(&version),
            Err(crate::adapter::AdapterError::InvalidVersion {
                ecosystem,
                ..
            }) if ecosystem == EcosystemId::PYTHON
        ));
    }

    #[test]
    fn plans_pep_440_post_release_versions() {
        let root = temp_dir("pep-440-post-release-plan");
        write_pyproject(
            &root,
            ".",
            "[project]\nname = \"example\"\nversion = \"1.2.3\"\n",
        );
        let package = PackageSnapshot {
            id: PackageId::new("example"),
            manifest_name: "example".to_string(),
            version: semver::Version::new(1, 2, 3),
            version_source: VersionSource::PackageManifest,
            ecosystem: EcosystemId::PYTHON,
            path: ".".into(),
            publishable: true,
            dependencies: vec![],
        };
        let versions = VersionMap::from([(
            package.id.clone(),
            semver::Version::parse("1.2.3-post.0").unwrap(),
        )]);

        let edits = PythonResolver
            .plan_edits(EcosystemPlanInput {
                project_root: camino::Utf8Path::from_path(&root).unwrap(),
                workspace_packages: std::slice::from_ref(&package),
                released_packages: std::slice::from_ref(&package.id),
                versions: &versions,
            })
            .unwrap();

        assert!(
            edits
                .iter()
                .any(|edit| edit.new_content.contains("version = \"1.2.3.post0\""))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_poetry_project_metadata() {
        let root = temp_dir("poetry");
        write_pyproject(
            &root,
            ".",
            "[tool.poetry]\nname = \"poetry-example\"\nversion = \"2.3.4\"\n",
        );

        let package = PythonResolver
            .parse_package(&root, &package_config("."))
            .unwrap();

        assert_eq!(package.name, "poetry-example");
        assert_eq!(package.version, semver::Version::parse("2.3.4").unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_hatch_dynamic_version_from_configured_source_file() {
        let root = temp_dir("hatch-dynamic-version");
        write_pyproject(
            &root,
            ".",
            "[project]\nname = \"hatch-example\"\ndynamic = [\"version\"]\n\n[tool.hatch.version]\npath = \"src/hatch_example/__init__.py\"\n",
        );
        let source_dir = root.join("src/hatch_example");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(
            source_dir.join("__init__.py"),
            "__version__: str = \"3.4.5\"\n",
        )
        .unwrap();

        let package = PythonResolver
            .parse_package(&root, &package_config("."))
            .unwrap();

        assert_eq!(package.name, "hatch-example");
        assert_eq!(package.version, semver::Version::parse("3.4.5").unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn falls_back_to_setup_cfg_metadata() {
        let root = temp_dir("setup-cfg");
        fs::write(
            root.join("pyproject.toml"),
            "[build-system]\nrequires = [\"setuptools\"]\n",
        )
        .unwrap();
        fs::write(
            root.join("setup.cfg"),
            "[metadata]\nname = cfg-example\nversion = 4.5.6\n\n[options]\npackages = find:\n",
        )
        .unwrap();

        let package = PythonResolver
            .parse_package(&root, &package_config("."))
            .unwrap();

        assert_eq!(package.name, "cfg-example");
        assert_eq!(package.version, semver::Version::parse("4.5.6").unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovers_root_and_common_monorepo_directories() {
        let root = temp_dir("monorepo");
        write_pyproject(
            &root,
            ".",
            "[project]\nname = \"root\"\nversion = \"1.0.0\"\n",
        );
        write_pyproject(
            &root,
            "packages/core",
            "[project]\nname = \"core\"\nversion = \"1.0.0\"\n",
        );
        write_pyproject(
            &root,
            "libs/helpers",
            "[project]\nname = \"helpers\"\nversion = \"1.0.0\"\n",
        );
        let app_dir = root.join("apps/cli");
        fs::create_dir_all(&app_dir).unwrap();
        fs::write(
            app_dir.join("setup.cfg"),
            "[metadata]\nname = cli\nversion = 1.0.0\n",
        )
        .unwrap();

        let mut packages = PythonResolver.discover_packages(&root).unwrap();
        packages.sort_by(|left, right| left.name.cmp(&right.name));

        assert_eq!(packages.len(), 4);
        assert_eq!(packages[0].name, "cli");
        assert_eq!(packages[0].path, PathBuf::from("apps/cli"));
        assert_eq!(packages[1].name, "core");
        assert_eq!(packages[1].path, PathBuf::from("packages/core"));
        assert_eq!(packages[2].name, "helpers");
        assert_eq!(packages[2].path, PathBuf::from("libs/helpers"));
        assert_eq!(packages[3].name, "root");
        assert_eq!(packages[3].path, PathBuf::from("."));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_discovers_and_inspects_manifest_dependencies_before_id_binding() {
        let root = temp_dir("adapter-inspection");
        write_pyproject(
            &root,
            ".",
            "[project]\nname = \"root\"\nversion = \"1.0.0\"\n",
        );
        write_pyproject(
            &root,
            "packages/core",
            "[project]\nname = \"core\"\nversion = \"1.0.0\"\n",
        );
        write_pyproject(
            &root,
            "packages/app",
            "[project]\nname = \"app\"\nversion = \"1.0.0\"\ndependencies = [\"core>=1\", \"requests>=2\"]\n",
        );
        let project_root = camino::Utf8PathBuf::from_path_buf(root.clone()).unwrap();

        let discovered = PythonResolver.discover(&project_root).unwrap();
        assert_eq!(
            discovered
                .iter()
                .map(|package| package.id.as_str())
                .collect::<Vec<_>>(),
            ["app", "core", "root"]
        );
        let app = PythonResolver
            .inspect(&PackageLocation {
                id: PackageId::new("configured-app"),
                project_root,
                path: "packages/app".into(),
            })
            .unwrap();

        assert_eq!(app.id, PackageId::new("configured-app"));
        assert_eq!(app.manifest_name, "app");
        assert_eq!(
            app.dependencies
                .iter()
                .map(|dependency| dependency.manifest_name.as_str())
                .collect::<Vec<_>>(),
            ["core", "requests"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_propagates_an_invalid_pyproject_instead_of_falling_back() {
        let root = temp_dir("invalid-pyproject");
        fs::write(root.join("pyproject.toml"), "[project\nname = \"broken\"\n").unwrap();
        let project_root = camino::Utf8PathBuf::from_path_buf(root.clone()).unwrap();

        assert!(matches!(
            PythonResolver.inspect(&PackageLocation {
                id: PackageId::new("broken"),
                project_root,
                path: ".".into(),
            }),
            Err(crate::adapter::AdapterError::Manifest(
                ResolveError::ParseError { .. }
            ))
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_pep_621_and_poetry_internal_dependencies() {
        let root = temp_dir("dependencies");
        write_pyproject(
            &root,
            "packages/pep",
            "[project]\nname = \"pep\"\nversion = \"1.0.0\"\ndependencies = [\"core>=1.0.0\", \"helpers ~= 2.0\"]\n",
        );
        write_pyproject(
            &root,
            "packages/poetry",
            "[tool.poetry]\nname = \"poetry\"\nversion = \"1.0.0\"\n\n[tool.poetry.dependencies]\npython = \"^3.11\"\ncore = \"^1.0.0\"\nhelpers = { path = \"../helpers\" }\n",
        );

        let resolver = PythonResolver;
        assert_eq!(
            resolver
                .manifest_dependencies(&root, Path::new("packages/pep"))
                .unwrap()
                .into_iter()
                .map(|dependency| dependency.manifest_name)
                .collect::<Vec<_>>(),
            vec!["core", "helpers"]
        );
        assert_eq!(
            resolver
                .manifest_dependencies(&root, Path::new("packages/poetry"))
                .unwrap()
                .into_iter()
                .map(|dependency| dependency.manifest_name)
                .collect::<Vec<_>>(),
            vec!["core", "helpers"]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_pyproject_and_static_init_version_without_writing() {
        let root = temp_dir("bump-pyproject");
        write_pyproject(
            &root,
            "packages/example",
            "[project]\nname = \"example\"\nversion = \"1.0.0\"\ndependencies = [\"requests>=2\"]\n\n[tool.custom]\npreserved = true\n",
        );
        let source_dir = root.join("packages/example/src/example");
        fs::create_dir_all(&source_dir).unwrap();
        fs::write(source_dir.join("__init__.py"), "__version__ = '1.0.0'\n").unwrap();
        let package = PackageSnapshot {
            id: PackageId::new("example"),
            manifest_name: "example".to_string(),
            version: semver::Version::new(1, 0, 0),
            version_source: VersionSource::PackageManifest,
            ecosystem: EcosystemId::PYTHON,
            path: "packages/example".into(),
            publishable: true,
            dependencies: vec![],
        };

        let edits = PythonResolver::plan_file_edits(
            &root,
            &package,
            &VersionMap::from([(PackageId::new("example"), semver::Version::new(1, 1, 0))]),
        )
        .unwrap();

        assert_eq!(edits.len(), 2);
        let pyproject = &edits
            .iter()
            .find(|edit| edit.path == "packages/example/pyproject.toml")
            .unwrap()
            .new_content;
        assert!(pyproject.contains("version = \"1.1.0\""));
        assert!(pyproject.contains("dependencies = [\"requests>=2\"]"));
        assert!(pyproject.contains("[tool.custom]"));
        assert!(pyproject.contains("preserved = true"));
        assert_eq!(
            edits
                .iter()
                .find(|edit| edit.path == "packages/example/src/example/__init__.py")
                .unwrap()
                .new_content,
            "__version__ = \"1.1.0\"\n"
        );
        assert!(
            fs::read_to_string(root.join("packages/example/pyproject.toml"))
                .unwrap()
                .contains("version = \"1.0.0\"")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_setup_cfg_version() {
        let root = temp_dir("bump-setup-cfg");
        fs::write(
            root.join("setup.cfg"),
            "[metadata]\nname = cfg-example\nversion = 1.0.0\nversion_file = VERSION\n\n[options]\npackages = find:\n",
        )
        .unwrap();
        let package = PackageSnapshot {
            id: PackageId::new("cfg-example"),
            manifest_name: "cfg-example".to_string(),
            version: semver::Version::new(1, 0, 0),
            version_source: VersionSource::PackageManifest,
            ecosystem: EcosystemId::PYTHON,
            path: ".".into(),
            publishable: true,
            dependencies: vec![],
        };

        let edits = PythonResolver::plan_file_edits(
            &root,
            &package,
            &VersionMap::from([(PackageId::new("cfg-example"), semver::Version::new(1, 0, 1))]),
        )
        .unwrap();

        assert_eq!(edits.len(), 1);
        let setup_cfg = &edits[0].new_content;
        assert!(setup_cfg.contains("name = cfg-example"));
        assert!(setup_cfg.contains("version = 1.0.1"));
        assert!(setup_cfg.contains("version_file = VERSION"));
        assert!(
            fs::read_to_string(root.join("setup.cfg"))
                .unwrap()
                .contains("version = 1.0.0")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn plans_the_configured_hatch_version_source() {
        let root = temp_dir("hatch-version-source");
        write_pyproject(
            &root,
            ".",
            "[project]\nname = \"hatch-example\"\ndynamic = [\"version\"]\n\n[tool.hatch.version]\npath = \"src/hatch_example/version.py\"\n",
        );
        let source = root.join("src/hatch_example/version.py");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "__version__ = \"1.0.0\"\n").unwrap();
        let package = PackageSnapshot {
            id: PackageId::new("hatch-example"),
            manifest_name: "hatch-example".to_string(),
            version: semver::Version::new(1, 0, 0),
            version_source: VersionSource::PackageManifest,
            ecosystem: EcosystemId::PYTHON,
            path: ".".into(),
            publishable: true,
            dependencies: vec![],
        };

        let edits = PythonResolver::plan_file_edits(
            &root,
            &package,
            &VersionMap::from([(
                PackageId::new("hatch-example"),
                semver::Version::new(1, 0, 1),
            )]),
        )
        .unwrap();

        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].path, "./src/hatch_example/version.py");
        assert_eq!(edits[0].new_content, "__version__ = \"1.0.1\"\n");
        assert_eq!(
            fs::read_to_string(source).unwrap(),
            "__version__ = \"1.0.0\"\n"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dynamic_python_version_never_plans_a_cargo_edit() {
        let root = temp_dir("dynamic-cargo-version");
        write_pyproject(
            &root,
            ".",
            "[project]\nname = \"native-example\"\ndynamic = [\"version\"]\n",
        );
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"native-example\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        let package = PackageSnapshot {
            id: PackageId::new("native-example"),
            manifest_name: "native-example".to_string(),
            version: semver::Version::new(1, 0, 0),
            version_source: VersionSource::PackageManifest,
            ecosystem: EcosystemId::PYTHON,
            path: ".".into(),
            publishable: true,
            dependencies: vec![],
        };

        let versions = VersionMap::from([(
            PackageId::new("native-example"),
            semver::Version::new(1, 0, 1),
        )]);
        let edits = PythonResolver
            .plan_edits(EcosystemPlanInput {
                project_root: camino::Utf8Path::from_path(&root).unwrap(),
                workspace_packages: std::slice::from_ref(&package),
                released_packages: std::slice::from_ref(&package.id),
                versions: &versions,
            })
            .unwrap();

        assert!(edits.is_empty());
        assert!(
            fs::read_to_string(root.join("Cargo.toml"))
                .unwrap()
                .contains("version = \"1.0.0\"")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
