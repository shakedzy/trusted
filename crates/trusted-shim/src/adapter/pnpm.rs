use std::ffi::OsString;

use anyhow::Result;
use trusted_core::types::Ecosystem;

use async_trait::async_trait;

use super::npm::NpmAdapter;
use super::Adapter;

pub struct PnpmAdapter;

#[async_trait]
impl Adapter for PnpmAdapter {
    fn is_install_like(&self, args: &[std::ffi::OsString]) -> bool {
        let npm = NpmAdapter;
        npm.is_install_like(args)
    }

    async fn dry_run_resolve(
        &self,
        real_binary: &str,
        args: &[OsString],
    ) -> Result<Vec<trusted_core::types::PackageRef>> {
        let npm = NpmAdapter;
        npm.dry_run_resolve(real_binary, args).await
    }

    fn rewrite_for_pins(
        &self,
        args: &[OsString],
        pins: &[(trusted_core::types::PackageRef, String)],
    ) -> Vec<OsString> {
        let npm = NpmAdapter;
        npm.rewrite_for_pins(args, pins)
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Npm
    }
}
