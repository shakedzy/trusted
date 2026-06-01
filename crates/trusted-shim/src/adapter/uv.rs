use std::ffi::OsString;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;
use trusted_core::types::{Ecosystem, PackageRef};

use async_trait::async_trait;

use super::{os_args, parse_would_install_line, Adapter};

pub struct UvAdapter;

#[async_trait]
impl Adapter for UvAdapter {
    fn is_install_like(&self, args: &[OsString]) -> bool {
        let s = os_args(args);
        if s.first().map(|a| a.as_str()) == Some("pip") {
            return s.iter().any(|a| a == "install" || a == "sync");
        }
        s.iter().any(|a| a == "sync" || a == "add")
    }

    async fn dry_run_resolve(
        &self,
        real_binary: &str,
        args: &[OsString],
    ) -> Result<Vec<PackageRef>> {
        let s = os_args(args);
        let mut cmd_args: Vec<OsString> = args.to_vec();
        if s.first().map(|a| a.as_str()) == Some("pip") {
            if !cmd_args.iter().any(|a| a == "--dry-run") {
                cmd_args.push("--dry-run".into());
            }
        } else if s.iter().any(|a| a == "sync") {
            // uv sync resolves from lockfile
            return read_uv_lock();
        } else if !cmd_args.iter().any(|a| a == "--dry-run") {
            cmd_args.push("--dry-run".into());
        }
        let output = Command::new(real_binary)
            .args(&cmd_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("uv dry-run")?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let mut packages = Vec::new();
        for line in combined.lines() {
            packages.extend(parse_would_install_line(line, Ecosystem::PyPI));
        }
        Ok(packages)
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

fn read_uv_lock() -> Result<Vec<PackageRef>> {
    let path = std::env::current_dir()?.join("uv.lock");
    if !path.exists() {
        return Ok(vec![]);
    }
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
    Ok(packages)
}
