mod discover;
pub mod parse;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

pub use discover::discover;
pub use parse::ScanSource;

use crate::policy::{PolicyEngine, Violation};
use crate::types::PackageRef;

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub root: PathBuf,
    pub sources: Vec<ScanSource>,
    pub unique_packages: usize,
    pub violations: Vec<ScanViolation>,
}

#[derive(Debug, Clone)]
pub struct ScanViolation {
    pub violation: Violation,
    /// Lockfile/manifest paths that declared this package@version.
    pub sources: Vec<PathBuf>,
}

pub async fn scan_repo(root: &Path) -> Result<ScanReport> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let sources = discover(&root)?;
    let (packages, pkg_sources) = merge_sources(&sources);

    if sources.is_empty() {
        eprintln!("trusted: no lockfiles found to scan.");
    } else {
        eprintln!(
            "trusted: checking {} package(s) total (from {} lockfile(s))…",
            packages.len(),
            sources.len()
        );
    }

    let engine = PolicyEngine::new(crate::config::Config::load()?)?;
    let violations = engine.audit(&packages).await?;

    let scan_violations = violations
        .into_iter()
        .map(|v| {
            let key = package_key(&v.package);
            let sources = pkg_sources.get(&key).cloned().unwrap_or_default();
            ScanViolation {
                violation: v,
                sources,
            }
        })
        .collect();

    Ok(ScanReport {
        unique_packages: packages.len(),
        sources,
        violations: scan_violations,
        root,
    })
}

fn merge_sources(sources: &[ScanSource]) -> (Vec<PackageRef>, HashMap<String, Vec<PathBuf>>) {
    let mut packages = Vec::new();
    let mut seen = HashSet::new();
    let mut pkg_sources: HashMap<String, Vec<PathBuf>> = HashMap::new();

    for source in sources {
        for pkg in &source.packages {
            let key = package_key(pkg);
            pkg_sources
                .entry(key.clone())
                .or_default()
                .push(source.path.clone());
            if seen.insert(key.clone()) {
                packages.push(pkg.clone());
            }
        }
    }

    for paths in pkg_sources.values_mut() {
        paths.sort();
        paths.dedup();
    }

    (packages, pkg_sources)
}

fn package_key(pkg: &PackageRef) -> String {
    format!("{}:{}@{}", pkg.ecosystem.osv_name(), pkg.name, pkg.version)
}

pub fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}
