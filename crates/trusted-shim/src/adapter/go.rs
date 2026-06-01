use std::ffi::OsString;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;
use trusted_core::types::{Ecosystem, PackageRef};

use async_trait::async_trait;

use super::{os_args, Adapter};

pub struct GoAdapter;

#[async_trait]
impl Adapter for GoAdapter {
    fn is_install_like(&self, args: &[OsString]) -> bool {
        let s = os_args(args);
        s.iter().any(|a| a == "get" || a == "install")
    }

    async fn dry_run_resolve(
        &self,
        real_binary: &str,
        args: &[OsString],
    ) -> Result<Vec<PackageRef>> {
        let parsed = parse_go_modules(&os_args(args));
        if !parsed.is_empty() {
            return Ok(parsed);
        }
        let mut cmd_args: Vec<OsString> = args.to_vec();
        cmd_args.push("-n".into());
        let output = Command::new(real_binary)
            .args(&cmd_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .context("go dry-run")?;
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let mut packages = Vec::new();
        for line in combined.lines() {
            if line.contains("@") {
                packages.extend(parse_go_module_token(line));
            }
        }
        Ok(packages)
    }

    fn rewrite_for_pins(&self, args: &[OsString], pins: &[(PackageRef, String)]) -> Vec<OsString> {
        let mut out: Vec<OsString> = args
            .iter()
            .filter(|a| {
                let s = a.to_string_lossy();
                !pins.iter().any(|(p, _)| s.contains(&p.name))
            })
            .cloned()
            .collect();
        for (pkg, ver) in pins {
            out.push(format!("{}@{}", pkg.name, ver).into());
        }
        out
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Go
    }
}

fn parse_go_modules(args: &[String]) -> Vec<PackageRef> {
    let mut packages = Vec::new();
    let mut after = false;
    for arg in args {
        if arg == "get" || arg == "install" {
            after = true;
            continue;
        }
        if !after || arg.starts_with('-') {
            continue;
        }
        packages.extend(parse_go_module_token(arg));
    }
    packages
}

fn parse_go_module_token(token: &str) -> Vec<PackageRef> {
    let token = token.split_whitespace().last().unwrap_or(token);
    if let Some((name, version)) = token.rsplit_once('@') {
        return vec![PackageRef {
            ecosystem: Ecosystem::Go,
            name: name.to_string(),
            version: version.to_string(),
        }];
    }
    vec![]
}
