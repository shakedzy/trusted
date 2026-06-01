use std::ffi::OsString;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;
use trusted_core::types::{Ecosystem, PackageRef};

use async_trait::async_trait;

use super::{os_args, parse_would_install_line, Adapter};

pub struct PipAdapter;

#[async_trait]
impl Adapter for PipAdapter {
    fn is_install_like(&self, args: &[OsString]) -> bool {
        let s = os_args(args);
        s.iter().any(|a| a == "install" || a == "sync")
    }

    async fn dry_run_resolve(
        &self,
        real_binary: &str,
        args: &[OsString],
    ) -> Result<Vec<PackageRef>> {
        let mut cmd_args: Vec<OsString> = args.to_vec();
        if !cmd_args.iter().any(|a| a == "--dry-run") {
            cmd_args.push("--dry-run".into());
        }
        let output = Command::new(real_binary)
            .args(&cmd_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("pip dry-run")?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut packages = Vec::new();
        for line in stderr.lines().chain(stdout.lines()) {
            packages.extend(parse_would_install_line(line, Ecosystem::PyPI));
        }
        if packages.is_empty() {
            packages.extend(parse_from_requirement_args(&os_args(args)));
        }
        Ok(dedupe_packages(packages))
    }

    fn rewrite_for_pins(&self, args: &[OsString], pins: &[(PackageRef, String)]) -> Vec<OsString> {
        let mut out = args.to_vec();
        for (pkg, ver) in pins {
            out.push(format!("{}=={}", pkg.name, ver).into());
        }
        out
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::PyPI
    }
}

fn parse_from_requirement_args(args: &[String]) -> Vec<PackageRef> {
    let mut packages = Vec::new();
    let mut after_install = false;
    for arg in args {
        if arg == "install" || arg == "sync" {
            after_install = true;
            continue;
        }
        if !after_install || arg.starts_with('-') {
            continue;
        }
        if let Some((name, version)) = super::split_name_version(arg) {
            packages.push(PackageRef {
                ecosystem: Ecosystem::PyPI,
                name,
                version,
            });
        } else if !arg.contains('/') && !arg.contains('\\') {
            packages.push(PackageRef {
                ecosystem: Ecosystem::PyPI,
                name: arg.clone(),
                version: "latest".to_string(),
            });
        }
    }
    packages
}

fn dedupe_packages(mut packages: Vec<PackageRef>) -> Vec<PackageRef> {
    packages.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
    packages.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    packages.retain(|p| p.version != "latest");
    packages
}
