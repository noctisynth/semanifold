use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{
    config::{PackageConfig, ResolverConfig, VersionMode},
    error::ResolveError,
    resolver::{ResolvedPackage, Resolver, ResolverType},
    utils,
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

impl PythonResolver {
    fn resolve_pyproject(
        &self,
        root: &Path,
        pkg_path: &Path,
    ) -> Result<ResolvedPackage, ResolveError> {
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

        Ok(ResolvedPackage {
            name,
            version,
            path: pkg_path.to_path_buf(),
            private: false,
        })
    }

    /// 从 setup.cfg 解析元数据文件（fallback）
    fn resolve_setup_cfg(
        &self,
        root: &Path,
        pkg_path: &Path,
    ) -> Result<ResolvedPackage, ResolveError> {
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

        Ok(ResolvedPackage {
            name,
            version,
            path: pkg_path.to_path_buf(),
            private: false,
        })
    }

    fn update_pyproject_version(
        &self,
        root: &Path,
        pkg_path: &Path,
        version: &str,
    ) -> Result<(), ResolveError> {
        let pyproject_path = root.join(pkg_path).join("pyproject.toml");
        let pyproject_str = std::fs::read_to_string(&pyproject_path)?;

        let mut doc = pyproject_str
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| ResolveError::ParseError {
                path: pyproject_path.clone(),
                reason: e.to_string(),
            })?;

        if let Some(project) = doc.get_mut("project")
            && let Some(project_table) = project.as_table_mut()
        {
            project_table.insert("version", toml_edit::value(version));
        }

        // tool.poetry.version
        if let Some(tool) = doc.get_mut("tool")
            && let Some(tool_table) = tool.as_table_mut()
            && let Some(poetry) = tool_table.get_mut("poetry")
            && let Some(poetry_table) = poetry.as_table_mut()
        {
            poetry_table.insert("version", toml_edit::value(version));
        }

        std::fs::write(&pyproject_path, doc.to_string())?;

        // 如果存在 Cargo.toml（maturin/PyO3 项目），也更新它
        self.update_cargo_version(root, pkg_path, version)?;

        Ok(())
    }

    /// 更新 Cargo.toml 中的版本号（用于 maturin/PyO3 项目）
    fn update_cargo_version(
        &self,
        root: &Path,
        pkg_path: &Path,
        version: &str,
    ) -> Result<(), ResolveError> {
        let cargo_path = root.join(pkg_path).join("Cargo.toml");

        // 如果没有 Cargo.toml，不是错误，直接返回
        if !cargo_path.exists() {
            return Ok(());
        }

        log::debug!("Found Cargo.toml, updating version for maturin/PyO3 project");

        let cargo_str = std::fs::read_to_string(&cargo_path)?;
        let mut doc =
            cargo_str
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| ResolveError::ParseError {
                    path: cargo_path.clone(),
                    reason: e.to_string(),
                })?;

        if let Some(package) = doc.get_mut("package")
            && let Some(package_table) = package.as_table_mut()
        {
            package_table.insert("version", toml_edit::value(version));
            std::fs::write(&cargo_path, doc.to_string())?;
            log::info!("Updated version in Cargo.toml to {}", version);
        }

        Ok(())
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

    /// 更新 `__init__.py` 中的 `__version__`
    /// 仅当 `__version__` 是硬编码的版本号字符串时才更新
    fn update_init_version(
        &self,
        root: &Path,
        pkg_path: &Path,
        package_name: &str,
        version: &str,
    ) -> Result<(), ResolveError> {
        let init_paths = vec![
            root.join(pkg_path).join(package_name).join("__init__.py"),
            root.join(pkg_path)
                .join("src")
                .join(package_name)
                .join("__init__.py"),
            root.join(pkg_path).join("src").join("__init__.py"),
            // root.join(pkg_path).join("__init__.py"), // 待定
        ];

        for init_path in init_paths {
            if !init_path.exists() {
                continue;
            }

            let content = std::fs::read_to_string(&init_path)?;
            let mut new_content = String::new();
            let mut updated = false;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("__version__") {
                    // 检查是否是动态版本获取
                    // 包含函数调用的都认为是动态获取，比如:
                    // - `__version__ = version("package")`
                    // - `__version__ = importlib_metadata.version("package")`
                    // - `__version__ = get_version()`
                    if trimmed.contains("version(")
                        || trimmed.contains("get_version()")
                        || trimmed.contains("importlib")
                        || trimmed.contains("pkg_resources")
                    {
                        log::debug!(
                            "Skipping __version__ update in {} - detected dynamic version retrieval: {}",
                            init_path.display(),
                            trimmed
                        );
                        new_content.push_str(line);
                        new_content.push('\n');
                        continue;
                    }

                    // 检查是否是静态版本号字符串
                    if let Some(pos) = trimmed.find('=') {
                        let value_part = trimmed[pos + 1..].trim();
                        if (value_part.starts_with('"') && value_part.ends_with('"'))
                            || (value_part.starts_with('\'') && value_part.ends_with('\''))
                        {
                            new_content.push_str(&format!("__version__ = \"{}\"\n", version));
                            updated = true;
                            log::debug!(
                                "Updated __version__ in {} from {} to {}",
                                init_path.display(),
                                value_part,
                                version
                            );
                            continue;
                        }
                    }

                    log::debug!(
                        "Skipping __version__ update in {} - unrecognized format: {}",
                        init_path.display(),
                        trimmed
                    );
                    new_content.push_str(line);
                    new_content.push('\n');
                } else {
                    new_content.push_str(line);
                    new_content.push('\n');
                }
            }

            if updated {
                std::fs::write(&init_path, new_content)?;
                log::info!("Updated __version__ in {}", init_path.display());
                return Ok(());
            } else {
                log::debug!(
                    "No static __version__ found to update in {}",
                    init_path.display()
                );
            }
        }

        Ok(())
    }

    fn parse_dependencies(
        &self,
        root: &Path,
        pkg_path: &Path,
    ) -> Result<Vec<String>, ResolveError> {
        let pyproject_path = root.join(pkg_path).join("pyproject.toml");
        if !pyproject_path.exists() {
            return Ok(vec![]);
        }

        let pyproject_str = std::fs::read_to_string(&pyproject_path)?;
        let pyproject: PyProjectToml =
            toml_edit::de::from_str(&pyproject_str).map_err(|e| ResolveError::ParseError {
                path: pyproject_path.clone(),
                reason: e.to_string(),
            })?;

        let mut deps = Vec::new();

        // PEP 621 Dependencies
        if let Some(project) = pyproject.project
            && let Some(dependencies) = project.dependencies
        {
            for dep in dependencies {
                // "requests>=2.0.0" -> "requests"
                if let Some(name) = dep.split(&['>', '<', '=', '~', '!'][..]).next() {
                    deps.push(name.trim().to_string());
                }
            }
        }

        // Poetry Dependencies
        if let Some(tool) = pyproject.tool
            && let Some(poetry) = tool.poetry
            && let Some(dependencies) = poetry.dependencies
        {
            for (name, _) in dependencies {
                if name != "python" {
                    deps.push(name);
                }
            }
        }

        Ok(deps)
    }
}

impl Resolver for PythonResolver {
    fn resolve(
        &mut self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<ResolvedPackage, ResolveError> {
        if let Ok(package) = self.resolve_pyproject(root, &pkg_config.path) {
            return Ok(package);
        }

        if let Ok(package) = self.resolve_setup_cfg(root, &pkg_config.path) {
            return Ok(package);
        }

        Err(ResolveError::FileOrDirNotFound {
            path: root.join(&pkg_config.path),
        })
    }

    fn resolve_all(&mut self, root: &Path) -> Result<Vec<ResolvedPackage>, ResolveError> {
        let mut packages = Vec::new();

        // 检查是否是单包项目
        if root.join("pyproject.toml").exists() || root.join("setup.cfg").exists() {
            match self.resolve(
                root,
                &PackageConfig {
                    path: ".".into(),
                    resolver: ResolverType::Python,
                    version_mode: VersionMode::Semantic,
                    assets: vec![],
                },
            ) {
                Ok(package) => packages.push(package),
                Err(e) => log::warn!("Failed to resolve root package: {}", e),
            }
        }

        // 检查常见的 monorepo 结构
        let common_patterns = vec!["packages/*", "libs/*", "apps/*"];

        for pattern in common_patterns {
            let glob_pattern = format!("{}/{}", root.display(), pattern);
            if let Ok(paths) = glob::glob(&glob_pattern) {
                for path in paths.flatten() {
                    if path.join("pyproject.toml").exists() || path.join("setup.cfg").exists() {
                        let rel_path = pathdiff::diff_paths(&path, root).unwrap_or(path.clone());
                        match self.resolve(
                            root,
                            &PackageConfig {
                                path: rel_path,
                                resolver: ResolverType::Python,
                                version_mode: VersionMode::Semantic,
                                assets: vec![],
                            },
                        ) {
                            Ok(package) => packages.push(package),
                            Err(e) => {
                                log::warn!("Failed to resolve package at {}: {}", path.display(), e)
                            }
                        }
                    }
                }
            }
        }

        Ok(packages)
    }

    fn bump(
        &mut self,
        root: &Path,
        package: &ResolvedPackage,
        version: &semver::Version,
        dry_run: bool,
    ) -> Result<(), ResolveError> {
        let bumped_version = version.to_string();

        if dry_run {
            log::warn!(
                "Skip bump for {} to version {} due to dry run",
                package.name,
                bumped_version
            );
            return Ok(());
        }

        // 更新 pyproject.toml
        let pyproject_path = root.join(&package.path).join("pyproject.toml");
        if pyproject_path.exists() {
            self.update_pyproject_version(root, &package.path, &bumped_version)?;
            log::info!("Updated pyproject.toml for {}", package.name);
        }

        // 更新 setup.cfg（如果存在）
        let setup_cfg_path = root.join(&package.path).join("setup.cfg");
        if setup_cfg_path.exists() {
            let content = std::fs::read_to_string(&setup_cfg_path)?;
            let new_content = content
                .lines()
                .map(|line| {
                    if line.trim().starts_with("version") {
                        format!("version = {}", bumped_version)
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&setup_cfg_path, new_content)?;
            log::info!("Updated setup.cfg for {}", package.name);
        }

        // 尝试更新 __init__.py 中的 __version__
        let package_dir_name = package.name.replace('-', "_");
        if let Err(e) =
            self.update_init_version(root, &package.path, &package_dir_name, &bumped_version)
        {
            log::debug!("Could not update __init__.py: {}", e);
        }

        Ok(())
    }

    fn sort_packages(
        &mut self,
        root: &Path,
        packages: &mut Vec<(String, PackageConfig)>,
    ) -> Result<(), ResolveError> {
        let cached_deps: HashMap<String, Vec<String>> = packages
            .iter()
            .filter(|(_, cfg)| cfg.resolver == ResolverType::Python)
            .fold(HashMap::new(), |mut acc, (name, cfg)| {
                match self.parse_dependencies(root, &cfg.path) {
                    Ok(deps) => {
                        acc.insert(name.clone(), deps);
                    }
                    Err(e) => {
                        log::warn!("Failed to parse dependencies for {}: {}", name, e);
                        acc.insert(name.clone(), vec![]);
                    }
                }
                acc
            });

        packages.sort_by(|(a, a_cfg), (b, b_cfg)| {
            if a_cfg.resolver == ResolverType::Python
                && b_cfg.resolver == ResolverType::Python
                && let (Some(a_deps), Some(b_deps)) = (cached_deps.get(a), cached_deps.get(b))
            {
                if a_deps.iter().any(|dep| dep == b) {
                    return std::cmp::Ordering::Greater;
                }
                if b_deps.iter().any(|dep| dep == a) {
                    return std::cmp::Ordering::Less;
                }
            }
            std::cmp::Ordering::Equal
        });

        Ok(())
    }

    fn publish(
        &mut self,
        package: &ResolvedPackage,
        resolver_config: &ResolverConfig,
        dry_run: bool,
    ) -> Result<(), ResolveError> {
        if dry_run {
            log::warn!(
                "Skip publish {} {} due to dry run",
                package.name,
                format_args!("v{}", package.version)
            );
            return Ok(());
        } else if package.private {
            log::warn!(
                "Skip publish {} {} due to private flag",
                package.name,
                format_args!("v{}", package.version)
            );
            return Ok(());
        }

        log::info!("Running prepublish commands for {}", package.name);
        for prepublish in &resolver_config.prepublish {
            let args = prepublish.args.clone().unwrap_or_default();
            log::info!("Running {} {}", prepublish.command, args.join(" "));
            utils::run_command(prepublish, &package.path)?;
        }

        log::info!("Running publish commands for {}", package.name);
        for publish in &resolver_config.publish {
            let args = publish.args.clone().unwrap_or_default();
            log::info!("Running {} {}", publish.command, args.join(" "));
            utils::run_command(publish, &package.path)?;
        }

        Ok(())
    }
}
