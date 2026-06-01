use std::ffi::OsString;
use std::io::{self, Write};

use crate::adapter::Tool;
use crate::exec::{exec_real, find_real_binary};
use anyhow::{bail, Result};
use trusted_core::config::Config;
use std::path::Path;

use trusted_core::policy::{PolicyEngine, PolicyOutcome};
use trusted_core::{
    print_check_failed, print_check_ok, print_install_ask_prompt,
    print_install_blocked, print_install_declined, print_repin, print_scan_results, scan_repo,
};

pub async fn run_shim(tool: Tool, args: Vec<OsString>) -> Result<i32> {
    let adapter = tool.adapter();
    if !adapter.is_install_like(&args) {
        let binary = find_real_binary(tool.binary_name())?;
        return exec_real(&binary, &args, &[]).await;
    }

    let config = Config::load()?;
    let binary = find_real_binary(tool.binary_name())?;
    let packages = adapter.dry_run_resolve(&binary, &args).await?;

    if packages.is_empty() {
        tracing::debug!("no packages resolved; passthrough");
        return exec_real(&binary, &args, &[]).await;
    }

    let engine = PolicyEngine::new(config)?;
    let outcome = engine.evaluate(&packages).await?;

    match outcome {
        PolicyOutcome::Allow => exec_real(&binary, &args, &[]).await,
        PolicyOutcome::Block { violations } => {
            print_install_blocked(Some(tool.binary_name()), &violations);
            Ok(1)
        }
        PolicyOutcome::Ask { violations } => {
            print_install_ask_prompt(Some(tool.binary_name()), &violations);
            io::stderr().flush()?;
            let mut buf = String::new();
            io::stdin().read_line(&mut buf)?;
            if buf.trim().eq_ignore_ascii_case("y") {
                exec_real(&binary, &args, &[]).await
            } else {
                print_install_declined();
                Ok(1)
            }
        }
        PolicyOutcome::Repin { pins } => {
            print_repin(Some(tool.binary_name()), &pins);
            let new_args = adapter.rewrite_for_pins(&args, &pins);
            exec_real(&binary, &new_args, &[("TRUSTED_REPIN", "1")]).await
        }
    }
}

pub async fn run_scan(root: &Path) -> Result<i32> {
    let report = scan_repo(root).await?;
    print_scan_results(&report);
    if report.sources.is_empty() {
        return Ok(1);
    }
    if report.violations.is_empty() {
        Ok(0)
    } else {
        Ok(1)
    }
}

pub async fn run_check(packages: Vec<trusted_core::types::PackageRef>) -> Result<i32> {
    if packages.is_empty() {
        bail!("no packages to check");
    }
    let engine = PolicyEngine::new(Config::load()?)?;
    let violations = engine.audit(&packages).await?;
    if violations.is_empty() {
        print_check_ok(packages.len());
        Ok(0)
    } else {
        print_check_failed(None, &violations);
        Ok(1)
    }
}
