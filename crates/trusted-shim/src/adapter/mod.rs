mod cargo;
mod go;
pub mod npm;
mod pip;
mod pnpm;
mod uv;

use std::ffi::OsString;

use anyhow::Result;
use async_trait::async_trait;
use trusted_core::types::{Ecosystem, PackageRef};

pub use cargo::CargoAdapter;
pub use go::GoAdapter;
pub use npm::NpmAdapter;
pub use pip::PipAdapter;
pub use pnpm::PnpmAdapter;
pub use uv::UvAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pip,
    Uv,
    Npm,
    Pnpm,
    Cargo,
    Go,
}

impl Tool {
    pub fn from_argv0(name: &str) -> Option<Self> {
        match name {
            "pip" | "pip3" => Some(Tool::Pip),
            "uv" => Some(Tool::Uv),
            "npm" => Some(Tool::Npm),
            "pnpm" => Some(Tool::Pnpm),
            "cargo" => Some(Tool::Cargo),
            "go" => Some(Tool::Go),
            _ => None,
        }
    }

    pub fn binary_name(self) -> &'static str {
        match self {
            Tool::Pip => "pip",
            Tool::Uv => "uv",
            Tool::Npm => "npm",
            Tool::Pnpm => "pnpm",
            Tool::Cargo => "cargo",
            Tool::Go => "go",
        }
    }

    pub fn adapter(self) -> Box<dyn Adapter> {
        match self {
            Tool::Pip => Box::new(PipAdapter),
            Tool::Uv => Box::new(UvAdapter),
            Tool::Npm => Box::new(NpmAdapter),
            Tool::Pnpm => Box::new(PnpmAdapter),
            Tool::Cargo => Box::new(CargoAdapter),
            Tool::Go => Box::new(GoAdapter),
        }
    }
}

#[async_trait]
pub trait Adapter: Send + Sync {
    fn is_install_like(&self, args: &[OsString]) -> bool;
    async fn dry_run_resolve(
        &self,
        real_binary: &str,
        args: &[OsString],
    ) -> Result<Vec<PackageRef>>;
    fn rewrite_for_pins(&self, args: &[OsString], pins: &[(PackageRef, String)]) -> Vec<OsString>;
    fn ecosystem(&self) -> Ecosystem;
}

pub fn os_args(args: &[OsString]) -> Vec<String> {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

pub fn parse_would_install_line(line: &str, ecosystem: Ecosystem) -> Vec<PackageRef> {
    let mut packages = Vec::new();
    if let Some(rest) = line.strip_prefix("Would install ") {
        for token in rest.split_whitespace() {
            if let Some((name, version)) = split_name_version(token) {
                packages.push(PackageRef {
                    ecosystem,
                    name,
                    version,
                });
            }
        }
    }
    packages
}

pub fn split_name_version(token: &str) -> Option<(String, String)> {
    if let Some((name, ver)) = token.split_once('@') {
        return Some((name.to_string(), ver.to_string()));
    }
    if let Some(idx) = token.rfind('-') {
        let (name, ver) = token.split_at(idx);
        let ver = ver.trim_start_matches('-');
        if !name.is_empty()
            && !ver.is_empty()
            && ver.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            return Some((name.to_string(), ver.to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::split_name_version;

    #[test]
    fn split_pypi_wheel_name() {
        let (name, ver) = split_name_version("requests-2.32.0").unwrap();
        assert_eq!(name, "requests");
        assert_eq!(ver, "2.32.0");
    }

    #[test]
    fn split_npm_at() {
        let (name, ver) = split_name_version("lodash@4.17.21").unwrap();
        assert_eq!(name, "lodash");
        assert_eq!(ver, "4.17.21");
    }
}
