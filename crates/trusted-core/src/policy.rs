use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use chrono::{Duration, Utc};
use futures::stream::{self, StreamExt};
use semver::Version;

use crate::config::Config;
use crate::osv::{OsvCheckResult, OsvClient};
use crate::progress::{print_audit_progress, print_audit_progress_done};
use crate::registry::RegistryClient;
use crate::types::{ClosestSafeNoCandidate, PackageRef, UnsafeAction};

#[derive(Debug, Clone)]
pub enum ViolationKind {
    Osv { ids: Vec<String> },
    TooNew { age_days: u32, minimum_days: u32 },
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub package: PackageRef,
    pub kind: ViolationKind,
    pub hint: Option<String>,
}

#[derive(Debug, Clone)]
pub enum PolicyOutcome {
    Allow,
    Block { violations: Vec<Violation> },
    Ask { violations: Vec<Violation> },
    Repin { pins: Vec<(PackageRef, String)> },
}

pub struct PolicyEngine {
    config: Config,
    osv: OsvClient,
    registry: RegistryClient,
}

impl PolicyEngine {
    pub fn new(config: Config) -> Result<Self> {
        let osv = OsvClient::new(&config)?;
        let registry = RegistryClient::new(&config)?;
        Ok(Self {
            config,
            osv,
            registry,
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Audit-only: collect every violation (ignores `unsafe_action` / no re-pin).
    pub async fn audit(&self, packages: &[PackageRef]) -> Result<Vec<Violation>> {
        const OSV_CHUNK: usize = 500;
        const AGE_CONCURRENCY: usize = 32;

        let mut violations = Vec::new();
        let total = packages.len();
        let mut processed = 0usize;

        for chunk in packages.chunks(OSV_CHUNK) {
            print_audit_progress(processed, total, "Querying OSV…");
            let osv_results = self.osv.check_packages(chunk).await?;

            let mut age_checks = Vec::new();
            for (pkg, osv) in chunk.iter().zip(osv_results.iter()) {
                processed += 1;
                print_audit_progress(processed, total, "Querying OSV…");
                if self.config.is_allowed(pkg) {
                    continue;
                }
                if osv.vulnerable {
                    violations.push(self.violation_osv(pkg, osv));
                    continue;
                }
                if self.config.min_release_age_days > 0 {
                    age_checks.push(pkg.clone());
                }
            }

            if !age_checks.is_empty() {
                let age_base = processed - age_checks.len();
                let age_done = Arc::new(AtomicUsize::new(0));
                let registry = &self.registry;
                let min_days = self.config.min_release_age_days;
                let age_violations: Vec<Violation> = stream::iter(age_checks)
                    .map(|pkg| {
                        let age_done = age_done.clone();
                        async move {
                            let result = Self::check_release_age(registry, min_days, &pkg).await;
                            let n = age_base + age_done.fetch_add(1, Ordering::Relaxed) + 1;
                            print_audit_progress(n, total, "Checking release dates…");
                            result
                        }
                    })
                    .buffer_unordered(AGE_CONCURRENCY)
                    .filter_map(|r| async move { r.ok().flatten() })
                    .collect()
                    .await;
                violations.extend(age_violations);
            }
        }

        print_audit_progress(total, total, "Complete");
        print_audit_progress_done();
        Ok(violations)
    }

    pub async fn evaluate(&self, packages: &[PackageRef]) -> Result<PolicyOutcome> {
        let mut violations = Vec::new();
        let osv_results = self.osv.check_packages(packages).await?;

        for (pkg, osv) in packages.iter().zip(osv_results.iter()) {
            if self.config.is_allowed(pkg) {
                continue;
            }
            if let Some(v) = self.check_one(pkg, osv).await? {
                violations.push(v);
            }
        }

        if violations.is_empty() {
            return Ok(PolicyOutcome::Allow);
        }

        match self.config.unsafe_action {
            UnsafeAction::Block => Ok(PolicyOutcome::Block { violations }),
            UnsafeAction::Ask => {
                if is_tty() {
                    Ok(PolicyOutcome::Ask { violations })
                } else {
                    Ok(PolicyOutcome::Block { violations })
                }
            }
            UnsafeAction::ClosestSafe => {
                let pins = self.compute_pins(&violations).await?;
                if pins.is_empty() {
                    match self.config.closest_safe_no_candidate {
                        ClosestSafeNoCandidate::Block => Ok(PolicyOutcome::Block { violations }),
                        ClosestSafeNoCandidate::Ask if is_tty() => {
                            Ok(PolicyOutcome::Ask { violations })
                        }
                        _ => Ok(PolicyOutcome::Block { violations }),
                    }
                } else {
                    Ok(PolicyOutcome::Repin { pins })
                }
            }
        }
    }

    async fn check_one(&self, pkg: &PackageRef, osv: &OsvCheckResult) -> Result<Option<Violation>> {
        if osv.vulnerable {
            let hint = self
                .osv
                .affected_versions_hint(pkg)
                .await
                .ok()
                .flatten()
                .or_else(|| Some(default_osv_hint().to_string()));
            return Ok(Some(Violation {
                package: pkg.clone(),
                kind: ViolationKind::Osv {
                    ids: osv.vulns.iter().map(|v| v.id.clone()).collect(),
                },
                hint,
            }));
        }
        if self.config.min_release_age_days > 0 {
            return Self::check_release_age(&self.registry, self.config.min_release_age_days, pkg)
                .await;
        }
        Ok(None)
    }

    fn violation_osv(&self, pkg: &PackageRef, osv: &OsvCheckResult) -> Violation {
        Violation {
            package: pkg.clone(),
            kind: ViolationKind::Osv {
                ids: osv.vulns.iter().map(|v| v.id.clone()).collect(),
            },
            hint: Some(default_osv_hint().to_string()),
        }
    }

    async fn check_release_age(
        registry: &RegistryClient,
        min_release_age_days: u32,
        pkg: &PackageRef,
    ) -> Result<Option<Violation>> {
        if min_release_age_days == 0 {
            return Ok(None);
        }
        if let Some(published) = registry.published_at(pkg).await? {
            let age = Utc::now().signed_duration_since(published);
            let min = Duration::days(min_release_age_days as i64);
            if age < min {
                let age_days = age.num_days().max(0) as u32;
                return Ok(Some(Violation {
                    package: pkg.clone(),
                    kind: ViolationKind::TooNew {
                        age_days,
                        minimum_days: min_release_age_days,
                    },
                    hint: Some(format!(
                        "wait {} more day(s) or lower min_release_age_days",
                        (min - age).num_days().max(1)
                    )),
                }));
            }
        }
        Ok(None)
    }

    async fn compute_pins(&self, violations: &[Violation]) -> Result<Vec<(PackageRef, String)>> {
        let mut pins = Vec::new();
        for v in violations {
            if let Some(safe) = self.find_closest_safe(&v.package).await? {
                pins.push((v.package.clone(), safe));
            }
        }
        Ok(pins)
    }

    async fn find_closest_safe(&self, pkg: &PackageRef) -> Result<Option<String>> {
        let ceiling = &pkg.version;
        let versions = self.registry.list_versions(pkg).await?;
        let osv = &self.osv;
        let min_age = self.config.min_release_age_days;

        let mut candidates: Vec<_> = versions
            .into_iter()
            .filter(|v| version_lte(&v.version, ceiling))
            .collect();
        candidates.sort_by(|a, b| {
            compare_versions(&b.version, &a.version).unwrap_or(std::cmp::Ordering::Equal)
        });

        for ver in candidates {
            let candidate = PackageRef {
                ecosystem: pkg.ecosystem,
                name: pkg.name.clone(),
                version: ver.version.clone(),
            };
            if self.config.is_allowed(&candidate) {
                return Ok(Some(ver.version));
            }
            let results = osv.check_packages(std::slice::from_ref(&candidate)).await?;
            let vulnerable = results.first().map(|r| r.vulnerable).unwrap_or(false);
            if vulnerable {
                continue;
            }
            if min_age > 0 {
                let age = Utc::now().signed_duration_since(ver.published_at);
                if age < Duration::days(min_age as i64) {
                    continue;
                }
            }
            return Ok(Some(ver.version));
        }
        Ok(None)
    }
}

fn version_lte(candidate: &str, ceiling: &str) -> bool {
    match (parse_semver(candidate), parse_semver(ceiling)) {
        (Some(c), Some(ce)) => c <= ce,
        _ => candidate == ceiling || candidate <= ceiling,
    }
}

fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    match (parse_semver(a), parse_semver(b)) {
        (Some(a), Some(b)) => Some(a.cmp(&b)),
        _ => None,
    }
}

fn parse_semver(s: &str) -> Option<Version> {
    Version::parse(s.trim_start_matches('v')).ok()
}

fn default_osv_hint() -> &'static str {
    "see OSV advisories for this package; try an older version"
}

fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}
