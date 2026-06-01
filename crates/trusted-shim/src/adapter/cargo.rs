use std::ffi::OsString;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;
use trusted_core::types::{Ecosystem, PackageRef};

use async_trait::async_trait;

use super::{os_args, split_name_version, Adapter};

pub struct CargoAdapter;

#[async_trait]
impl Adapter for CargoAdapter {
    fn is_install_like(&self, args: &[OsString]) -> bool {
        let s = os_args(args);
        s.iter().any(|a| a == "install" || a == "add")
    }

    async fn dry_run_resolve(
        &self,
        real_binary: &str,
        args: &[OsString],
    ) -> Result<Vec<PackageRef>> {
        let s = os_args(args);
        if s.iter().any(|a| a == "add") {
            return Ok(parse_cargo_add(&s));
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
            .context("cargo install dry-run")?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let mut packages = Vec::new();
        for line in combined.lines() {
            if let Some(rest) = line.strip_prefix("Installing ") {
                for token in rest.split_whitespace() {
                    if let Some((name, version)) = parse_cargo_crate(token) {
                        packages.push(PackageRef {
                            ecosystem: Ecosystem::CratesIo,
                            name,
                            version,
                        });
                    }
                }
            }
        }
        if packages.is_empty() {
            packages.extend(parse_cargo_add(&s));
        }
        Ok(packages)
    }

    fn rewrite_for_pins(&self, args: &[OsString], pins: &[(PackageRef, String)]) -> Vec<OsString> {
        let mut out = args.to_vec();
        for (pkg, ver) in pins {
            out.push(format!("{}@{}", pkg.name, ver).into());
        }
        out
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::CratesIo
    }
}

fn parse_cargo_crate(token: &str) -> Option<(String, String)> {
    if let Some((name, ver)) = token.split_once(' ') {
        return Some((name.to_string(), ver.to_string()));
    }
    split_name_version(token)
}

fn parse_cargo_add(args: &[String]) -> Vec<PackageRef> {
    let mut packages = Vec::new();
    let mut after = false;
    for arg in args {
        if arg == "add" || arg == "install" {
            after = true;
            continue;
        }
        if !after || arg.starts_with('-') {
            continue;
        }
        let name = arg.split('@').next().unwrap_or(arg).to_string();
        let version = arg.split('@').nth(1).unwrap_or("latest").to_string();
        if version != "latest" {
            packages.push(PackageRef {
                ecosystem: Ecosystem::CratesIo,
                name,
                version,
            });
        }
    }
    packages
}
