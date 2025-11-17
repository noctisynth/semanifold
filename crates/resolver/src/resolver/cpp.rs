use std::path::Path;

use regex::Regex;

use crate::{
    config::{PackageConfig, ResolverConfig, VersionMode},
    context,
    error::ResolveError,
    resolver::{ResolvedPackage, Resolver, ResolverType},
    utils,
};

/// C++ resolver for CMake-based projects
pub struct CppResolver;

impl CppResolver {
    /// Read version from CMakeLists.txt
    fn read_cmake_version(&self, package_path: &Path) -> Result<String, ResolveError> {
        let cmake_path = package_path.join("CMakeLists.txt");
        if !cmake_path.exists() {
            return Err(ResolveError::FileOrDirNotFound {
                path: cmake_path.clone(),
            });
        }

        let content = std::fs::read_to_string(&cmake_path)?;

        // Match: project(...VERSION x.y.z...)
        let re = Regex::new(r"project\s*\([^)]*VERSION\s+([\d.]+)").map_err(|e| {
            ResolveError::ParseError {
                path: cmake_path.clone(),
                reason: format!("Invalid regex: {}", e),
            }
        })?;

        let version = re
            .captures(&content)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ResolveError::ParseError {
                path: cmake_path.clone(),
                reason: "VERSION not found in project() declaration".to_string(),
            })?;

        log::debug!("Read version {} from {:?}", version, cmake_path);
        Ok(version)
    }

    /// Update version in CMakeLists.txt
    fn update_cmake_version(
        &self,
        package_path: &Path,
        new_version: &str,
    ) -> Result<(), ResolveError> {
        let cmake_path = package_path.join("CMakeLists.txt");
        let content = std::fs::read_to_string(&cmake_path)?;

        // Replace version in project() declaration
        let re = Regex::new(r"(project\s*\([^)]*VERSION\s+)([\d.]+)").map_err(|e| {
            ResolveError::ParseError {
                path: cmake_path.clone(),
                reason: format!("Invalid regex: {}", e),
            }
        })?;

        let updated_content = re.replace(&content, |caps: &regex::Captures| {
            format!("{}{}", &caps[1], new_version)
        });

        std::fs::write(&cmake_path, updated_content.as_ref())?;
        log::info!("Updated {:?} to version {}", cmake_path, new_version);
        Ok(())
    }

    /// Update version in vcpkg.json if it exists (optional)
    fn update_vcpkg_version(
        &self,
        package_path: &Path,
        new_version: &str,
    ) -> Result<(), ResolveError> {
        let vcpkg_path = package_path.join("vcpkg.json");

        if !vcpkg_path.exists() {
            log::debug!("Skipping optional file {:?} (not found)", vcpkg_path);
            return Ok(());
        }

        let content = std::fs::read_to_string(&vcpkg_path)?;
        let mut vcpkg_json: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| ResolveError::ParseError {
                path: vcpkg_path.clone(),
                reason: e.to_string(),
            })?;

        if let Some(obj) = vcpkg_json.as_object_mut() {
            obj.insert(
                "version".to_string(),
                serde_json::Value::String(new_version.to_string()),
            );
        }

        let updated_content =
            serde_json::to_string_pretty(&vcpkg_json).map_err(|e| ResolveError::ParseError {
                path: vcpkg_path.clone(),
                reason: e.to_string(),
            })?;

        std::fs::write(&vcpkg_path, updated_content)?;
        log::info!("Updated {:?} to version {}", vcpkg_path, new_version);
        Ok(())
    }
}

impl Resolver for CppResolver {
    fn resolve(
        &mut self,
        root: &Path,
        pkg_config: &PackageConfig,
    ) -> Result<ResolvedPackage, ResolveError> {
        let package_path = root.join(&pkg_config.path);
        let version = self.read_cmake_version(&package_path)?;

        // Extract project name from CMakeLists.txt
        let cmake_path = package_path.join("CMakeLists.txt");
        let content = std::fs::read_to_string(&cmake_path)?;

        // Match: project(ProjectName ...)
        let re = Regex::new(r"project\s*\(\s*(\w+)").map_err(|e| ResolveError::ParseError {
            path: cmake_path.clone(),
            reason: format!("Invalid regex: {}", e),
        })?;

        let name = re
            .captures(&content)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .ok_or_else(|| ResolveError::ParseError {
                path: cmake_path.clone(),
                reason: "Project name not found in project() declaration".to_string(),
            })?;

        Ok(ResolvedPackage {
            name,
            version: semver::Version::parse(&version)?,
            path: pkg_config.path.clone(),
            private: false,
        })
    }

    fn resolve_all(&mut self, root: &Path) -> Result<Vec<ResolvedPackage>, ResolveError> {
        let cmake_path = root.join("CMakeLists.txt");
        if !cmake_path.exists() {
            log::warn!(
                "Cannot resolve package in {}, CMakeLists.txt not found.",
                root.display()
            );
            return Ok(vec![]);
        }

        // C++ projects typically don't have workspace concept like Rust/Node.js
        // So we just resolve the single package at root
        let package = self.resolve(
            root,
            &PackageConfig {
                path: ".".into(),
                resolver: ResolverType::Cpp,
                version_mode: VersionMode::Semantic,
                assets: vec![],
            },
        )?;

        Ok(vec![package])
    }

    fn bump(
        &mut self,
        ctx: &context::Context,
        root: &Path,
        package: &ResolvedPackage,
        version: &semver::Version,
    ) -> Result<(), ResolveError> {
        let bumped_version = version.to_string();
        let package_path = root.join(&package.path);

        if ctx.dry_run {
            log::warn!(
                "Skip bump for {} to version {} due to dry run",
                package.name,
                bumped_version
            );
            return Ok(());
        }

        // Update CMakeLists.txt (required)
        self.update_cmake_version(&package_path, &bumped_version)?;

        // Update vcpkg.json if it exists (optional)
        self.update_vcpkg_version(&package_path, &bumped_version)?;

        Ok(())
    }

    fn sort_packages(
        &mut self,
        _root: &Path,
        _packages: &mut Vec<(String, PackageConfig)>,
    ) -> Result<(), ResolveError> {
        // C++ projects don't typically have internal package dependencies
        // that need sorting, so this is a no-op
        Ok(())
    }

    fn publish(
        &mut self,
        package: &ResolvedPackage,
        resolver_config: &ResolverConfig,
        dry_run: bool,
    ) -> Result<(), ResolveError> {
        if package.private {
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
            if dry_run && !prepublish.dry_run.unwrap_or(false) {
                log::warn!(
                    "Skip prepublish command {} {} due to dry run",
                    prepublish.command,
                    args.join(" ")
                );
                continue;
            }
            log::info!("Running {} {}", prepublish.command, args.join(" "));
            utils::run_command(prepublish, &package.path)?;
        }

        log::info!("Running publish commands for {}", package.name);
        for publish in &resolver_config.publish {
            let args = publish.args.clone().unwrap_or_default();
            if dry_run && !publish.dry_run.unwrap_or(false) {
                log::warn!(
                    "Skip publish command {} {} due to dry run",
                    publish.command,
                    args.join(" ")
                );
                continue;
            }
            log::info!("Running {} {}", publish.command, args.join(" "));
            utils::run_command(publish, &package.path)?;
        }

        Ok(())
    }
}
