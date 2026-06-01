use std::ffi::OsString;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;
use trusted_core::types::{Ecosystem, PackageRef};

use async_trait::async_trait;

use super::{os_args, parse_would_install_line, split_name_version, Adapter};

pub struct NpmAdapter;

#[async_trait]
impl Adapter for NpmAdapter {
    fn is_install_like(&self, args: &[OsString]) -> bool {
        let s = os_args(args);
        s.iter().any(|a| a == "install" || a == "ci" || a == "add")
    }

    async fn dry_run_resolve(
        &self,
        real_binary: &str,
        args: &[OsString],
    ) -> Result<Vec<PackageRef>> {
        if let Ok(pkgs) = read_package_lock() {
            if !pkgs.is_empty() && os_args(args).iter().any(|a| a == "ci") {
                return Ok(pkgs);
            }
        }
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
            .context("npm dry-run")?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let mut packages = Vec::new();
        for line in combined.lines() {
            packages.extend(parse_would_install_line(line, Ecosystem::Npm));
            if let Some(pkg) = parse_npm_add_line(line) {
                packages.push(pkg);
            }
        }
        if packages.is_empty() {
            packages.extend(parse_cli_packages(&os_args(args), Ecosystem::Npm));
        }
        Ok(dedupe(packages))
    }

    fn rewrite_for_pins(&self, args: &[OsString], pins: &[(PackageRef, String)]) -> Vec<OsString> {
        let mut out: Vec<OsString> = args
            .iter()
            .filter(|a| {
                let s = a.to_string_lossy();
                !s.starts_with("npm@") && !pins.iter().any(|(p, _)| s.starts_with(&p.name))
            })
            .cloned()
            .collect();
        for (pkg, ver) in pins {
            out.push(format!("{}@{}", pkg.name, ver).into());
        }
        out
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Npm
    }
}

fn parse_npm_add_line(line: &str) -> Option<PackageRef> {
    let line = line.trim();
    if line.contains("add") {
        for token in line.split_whitespace() {
            if let Some((name, version)) = split_name_version(token) {
                return Some(PackageRef {
                    ecosystem: Ecosystem::Npm,
                    name,
                    version,
                });
            }
        }
    }
    None
}

fn parse_cli_packages(args: &[String], ecosystem: Ecosystem) -> Vec<PackageRef> {
    let mut packages = Vec::new();
    let mut after = false;
    for arg in args {
        if arg == "install" || arg == "add" || arg == "ci" {
            after = true;
            continue;
        }
        if !after || arg.starts_with('-') {
            continue;
        }
        if let Some((name, version)) = split_name_version(arg) {
            packages.push(PackageRef {
                ecosystem,
                name,
                version,
            });
        }
    }
    packages
}

fn read_package_lock() -> Result<Vec<PackageRef>> {
    let path = std::env::current_dir()?.join("package-lock.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let text = std::fs::read_to_string(path)?;
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let mut packages = Vec::new();
    if let Some(pkgs) = v.get("packages").and_then(|p| p.as_object()) {
        for (name, meta) in pkgs {
            if name.is_empty() {
                continue;
            }
            let version = meta.get("version").and_then(|v| v.as_str());
            if let Some(version) = version {
                let pkg_name = name
                    .strip_prefix("node_modules/")
                    .unwrap_or(name)
                    .rsplit("node_modules/")
                    .next()
                    .unwrap_or(name)
                    .to_string();
                packages.push(PackageRef {
                    ecosystem: Ecosystem::Npm,
                    name: pkg_name,
                    version: version.to_string(),
                });
            }
        }
    }
    Ok(packages)
}

fn dedupe(mut packages: Vec<PackageRef>) -> Vec<PackageRef> {
    packages.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
    packages.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    packages
}
