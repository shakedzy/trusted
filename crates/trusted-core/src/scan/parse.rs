use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::types::{Ecosystem, PackageRef};

#[derive(Debug, Clone)]
pub struct ScanSource {
    pub path: std::path::PathBuf,
    pub manager: String,
    pub packages: Vec<PackageRef>,
}

impl ScanSource {
    pub fn new(path: &Path, manager: &str, packages: Vec<PackageRef>) -> Self {
        Self {
            path: path.to_path_buf(),
            manager: manager.to_string(),
            packages,
        }
    }
}

pub fn parse_npm_lock(path: &Path) -> Result<Vec<PackageRef>> {
    let text = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let mut packages = Vec::new();
    if let Some(pkgs) = v.get("packages").and_then(|p| p.as_object()) {
        for (name, meta) in pkgs {
            if name.is_empty() {
                continue;
            }
            let Some(version) = meta.get("version").and_then(|v| v.as_str()) else {
                continue;
            };
            let pkg_name = name
                .strip_prefix("node_modules/")
                .unwrap_or(name.as_str())
                .rsplit("node_modules/")
                .next()
                .unwrap_or(name.as_str())
                .to_string();
            if !pkg_name.is_empty() {
                packages.push(PackageRef {
                    ecosystem: Ecosystem::Npm,
                    name: pkg_name,
                    version: version.to_string(),
                });
            }
        }
    }
    // lockfile v1
    if packages.is_empty() {
        if let Some(deps) = v.get("dependencies").and_then(|d| d.as_object()) {
            for (name, meta) in deps {
                if let Some(version) = meta.get("version").and_then(|v| v.as_str()) {
                    packages.push(PackageRef {
                        ecosystem: Ecosystem::Npm,
                        name: name.clone(),
                        version: version.to_string(),
                    });
                }
            }
        }
    }
    Ok(dedupe(packages))
}

pub fn parse_pnpm_lock(path: &Path) -> Result<Vec<PackageRef>> {
    let text = std::fs::read_to_string(path)?;
    let mut packages = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(key) = line.strip_suffix(':') {
            if key.contains('@') && !key.starts_with("importers") && !key.starts_with('/') {
                if let Some((name, version)) = key.rsplit_once('@') {
                    if !version.is_empty()
                        && version.chars().next().is_some_and(|c| c.is_ascii_digit())
                    {
                        packages.push(PackageRef {
                            ecosystem: Ecosystem::Npm,
                            name: name.to_string(),
                            version: version.to_string(),
                        });
                    }
                }
            }
        }
    }
    Ok(dedupe(packages))
}

pub fn parse_yarn_lock(path: &Path) -> Result<Vec<PackageRef>> {
    let text = std::fs::read_to_string(path)?;
    let mut packages = Vec::new();
    let mut pending_name: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if !line.starts_with(' ') && !line.starts_with('\t') && trimmed.ends_with(':') {
            let header = trimmed.trim_end_matches(':');
            if let Some(at) = header.find('@') {
                let name = &header[..at];
                pending_name = Some(name.trim_start_matches('"').to_string());
            }
            continue;
        }
        if let Some(name) = &pending_name {
            if let Some(ver) = trimmed.strip_prefix("version ") {
                let version = ver.trim_matches('"');
                packages.push(PackageRef {
                    ecosystem: Ecosystem::Npm,
                    name: name.clone(),
                    version: version.to_string(),
                });
                pending_name = None;
            }
        }
    }
    Ok(dedupe(packages))
}

pub fn parse_uv_lock(path: &Path) -> Result<Vec<PackageRef>> {
    let text = std::fs::read_to_string(path)?;
    let mut packages = Vec::new();
    let mut current_name = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("name = ") {
            current_name = Some(
                line.trim_start_matches("name = ")
                    .trim_matches('"')
                    .to_string(),
            );
        }
        if line.starts_with("version = ") {
            if let Some(name) = current_name.take() {
                let version = line
                    .trim_start_matches("version = ")
                    .trim_matches('"')
                    .to_string();
                packages.push(PackageRef {
                    ecosystem: Ecosystem::PyPI,
                    name,
                    version,
                });
            }
        }
    }
    Ok(dedupe(packages))
}

pub fn parse_pipfile_lock(path: &Path) -> Result<Vec<PackageRef>> {
    let text = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let mut packages = Vec::new();
    for section in ["default", "develop"] {
        if let Some(deps) = v.get(section).and_then(|s| s.as_object()) {
            for (name, meta) in deps {
                let version = meta
                    .get("version")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim_start_matches("=="));
                if let Some(version) = version {
                    packages.push(PackageRef {
                        ecosystem: Ecosystem::PyPI,
                        name: name.clone(),
                        version: version.to_string(),
                    });
                }
            }
        }
    }
    Ok(dedupe(packages))
}

pub fn parse_requirements_txt(path: &Path) -> Result<Vec<PackageRef>> {
    let text = std::fs::read_to_string(path)?;
    let mut packages = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with('-') {
            continue;
        }
        if let Some((name, version)) = line.split_once("==") {
            packages.push(PackageRef {
                ecosystem: Ecosystem::PyPI,
                name: name.trim().to_string(),
                version: version.trim().to_string(),
            });
        }
    }
    Ok(dedupe(packages))
}

pub fn parse_cargo_lock(path: &Path) -> Result<Vec<PackageRef>> {
    let text = std::fs::read_to_string(path)?;
    let lock: CargoLock = toml::from_str(&text).context("parse Cargo.lock")?;
    let packages = lock
        .package
        .into_iter()
        .map(|p| PackageRef {
            ecosystem: Ecosystem::CratesIo,
            name: p.name,
            version: p.version,
        })
        .collect();
    Ok(dedupe(packages))
}

#[derive(Debug, Deserialize)]
struct CargoLock {
    package: Vec<CargoPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
}

pub fn parse_go_mod(path: &Path) -> Result<Vec<PackageRef>> {
    let text = std::fs::read_to_string(path)?;
    let mut packages = Vec::new();
    let mut in_require = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("require (") {
            in_require = true;
            continue;
        }
        if in_require && line == ")" {
            in_require = false;
            continue;
        }
        if line.starts_with("require ") && !line.contains('(') {
            if let Some(pkg) = parse_go_require_line(line.trim_start_matches("require ")) {
                packages.push(pkg);
            }
            continue;
        }
        if in_require {
            if let Some(pkg) = parse_go_require_line(line) {
                packages.push(pkg);
            }
        }
    }
    Ok(dedupe(packages))
}

fn parse_go_require_line(line: &str) -> Option<PackageRef> {
    let line = line.split("//").next()?.trim();
    if line.is_empty() || line.starts_with("module ") {
        return None;
    }
    let mut parts = line.split_whitespace();
    let name = parts.next()?.to_string();
    let version = parts.next()?.to_string();
    if version.starts_with("v") || version.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Some(PackageRef {
            ecosystem: Ecosystem::Go,
            name,
            version,
        })
    } else {
        None
    }
}

fn dedupe(mut packages: Vec<PackageRef>) -> Vec<PackageRef> {
    packages.sort_by(|a, b| {
        a.ecosystem
            .osv_name()
            .cmp(b.ecosystem.osv_name())
            .then(a.name.cmp(&b.name))
            .then(a.version.cmp(&b.version))
    });
    packages
        .dedup_by(|a, b| a.ecosystem == b.ecosystem && a.name == b.name && a.version == b.version);
    packages
}
