use std::ffi::OsString;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::process::Command;

pub async fn exec_real(
    binary: &str,
    args: &[OsString],
    env_overrides: &[(&str, &str)],
) -> Result<i32> {
    let mut cmd = Command::new(binary);
    cmd.args(args);
    cmd.stdin(Stdio::inherit());
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    for (k, v) in env_overrides {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .await
        .with_context(|| format!("exec {binary}"))?;
    Ok(status.code().unwrap_or(1))
}

pub fn find_real_binary(tool: &str) -> Result<String> {
    if let Ok(override_path) = std::env::var(format!("TRUSTED_REAL_{}", tool.to_uppercase())) {
        return Ok(override_path);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let shim_dir = trusted_core::config::shims_dir().ok();
    for dir in std::env::split_paths(&path) {
        if shim_dir.as_ref().is_some_and(|s| dir == *s) {
            continue;
        }
        let candidate = dir.join(tool);
        if candidate.is_file() {
            if let Ok(canonical) = std::fs::canonicalize(&candidate) {
                if let Ok(self_exe) = std::env::current_exe() {
                    if let Ok(self_canon) = self_exe.canonicalize() {
                        if canonical == self_canon {
                            continue;
                        }
                    }
                }
            }
            return Ok(candidate.to_string_lossy().into_owned());
        }
    }
    anyhow::bail!("could not find real `{tool}` binary on PATH (outside trusted shims)")
}
