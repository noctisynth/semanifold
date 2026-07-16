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

    /// Update version in CMakeLists.txt
    fn update_cmake_version(
        &self,
        package_path: &Path,
        new_version: &str,
    ) -> Result<(), ResolveError> {
        let cmake_path = package_path.join("CMakeLists.txt");
        let content = std::fs::read_to_string(&cmake_path)?;

        // Replace version in project() declaration
        let re = Regex::new(
            r"(?i)(project\s*\([^)]*VERSION\s+)([\d.]+(?:-[a-zA-Z0-9.-]+)?(?:\+[a-zA-Z0-9.-]+)?)",
        )
        .map_err(|e| ResolveError::ParseError {
            path: cmake_path.clone(),
            reason: format!("Invalid regex: {}", e),
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
        } else {
            return Err(ResolveError::ParseError {
                path: vcpkg_path.clone(),
                reason: "vcpkg.json root must be an object".to_string(),
            });
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        config::{PackageConfig, VersionMode},
        context::Context,
        resolver::{ResolvedPackage, Resolver, ResolverType},
    };

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
            version_mode: VersionMode::Semantic,
            assets: vec![],
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

        let package = CppResolver.resolve(&root, &package_config(".")).unwrap();

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

        let packages = CppResolver.resolve_all(&root).unwrap();

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].name, "root-project");
        assert_eq!(packages[0].path, PathBuf::from("."));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bumps_cmake_and_vcpkg_versions_without_removing_vcpkg_fields() {
        let root = temp_dir("bump-with-vcpkg");
        write_cmake_project(&root, "library", "demo-library", "1.0.0");
        fs::write(
            root.join("library/vcpkg.json"),
            r#"{
  "name": "demo-library",
  "version": "1.0.0",
  "dependencies": ["fmt"],
  "custom": { "preserved": true }
}
"#,
        )
        .unwrap();
        let package = ResolvedPackage {
            name: "demo-library".to_string(),
            version: semver::Version::parse("1.0.0").unwrap(),
            path: PathBuf::from("library"),
            private: false,
        };

        CppResolver
            .bump(
                &Context::default(),
                &root,
                &package,
                &semver::Version::parse("1.1.0").unwrap(),
            )
            .unwrap();

        let cmake = fs::read_to_string(root.join("library/CMakeLists.txt")).unwrap();
        assert!(cmake.contains("project(demo-library VERSION 1.1.0 LANGUAGES CXX)"));
        let vcpkg = serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(root.join("library/vcpkg.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(vcpkg["version"], "1.1.0");
        assert_eq!(vcpkg["dependencies"], serde_json::json!(["fmt"]));
        assert_eq!(vcpkg["custom"]["preserved"], true);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bumps_a_cmake_project_without_an_optional_vcpkg_manifest() {
        let root = temp_dir("bump-without-vcpkg");
        write_cmake_project(&root, ".", "standalone", "1.0.0");
        let package = ResolvedPackage {
            name: "standalone".to_string(),
            version: semver::Version::parse("1.0.0").unwrap(),
            path: PathBuf::from("."),
            private: false,
        };

        CppResolver
            .bump(
                &Context::default(),
                &root,
                &package,
                &semver::Version::parse("1.0.1").unwrap(),
            )
            .unwrap();

        let cmake = fs::read_to_string(root.join("CMakeLists.txt")).unwrap();
        assert!(cmake.contains("VERSION 1.0.1"));
        assert!(!root.join("vcpkg.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
