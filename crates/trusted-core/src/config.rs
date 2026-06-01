use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dirs::{cache_dir, config_dir};
use serde::{Deserialize, Serialize};

use crate::types::{ClosestSafeNoCandidate, UnsafeAction};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_min_release_age_days")]
    pub min_release_age_days: u32,
    #[serde(default)]
    pub unsafe_action: UnsafeAction,
    #[serde(default)]
    pub closest_safe_no_candidate: ClosestSafeNoCandidate,
    #[serde(default)]
    pub osv: OsvConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub allow: Vec<AllowEntry>,
}

fn default_min_release_age_days() -> u32 {
    7
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsvConfig {
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_ttl_seconds")]
    pub ttl_seconds: u64,
}

fn default_ttl_seconds() -> u64 {
    3600
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            ttl_seconds: default_ttl_seconds(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllowEntry {
    pub ecosystem: String,
    pub package: String,
    pub version: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_release_age_days: default_min_release_age_days(),
            unsafe_action: UnsafeAction::Block,
            closest_safe_no_candidate: ClosestSafeNoCandidate::Block,
            osv: OsvConfig::default(),
            cache: CacheConfig::default(),
            allow: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let mut cfg = Self::default();
        if let Some(path) = global_config_path() {
            if path.exists() {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("read config {}", path.display()))?;
                let file: Config = toml::from_str(&text).context("parse global config")?;
                cfg.merge(file);
            }
        }
        if let Some(path) = project_config_path() {
            if path.exists() {
                let text = std::fs::read_to_string(&path)
                    .with_context(|| format!("read config {}", path.display()))?;
                let file: Config = toml::from_str(&text).context("parse project config")?;
                cfg.merge(file);
            }
        }
        Ok(cfg)
    }

    fn merge(&mut self, other: Config) {
        self.min_release_age_days = other.min_release_age_days;
        self.unsafe_action = other.unsafe_action;
        self.closest_safe_no_candidate = other.closest_safe_no_candidate;
        if other.osv.api_key.is_some() {
            self.osv = other.osv;
        }
        self.cache = other.cache;
        if !other.allow.is_empty() {
            self.allow = other.allow;
        }
    }

    pub fn is_allowed(&self, pkg: &crate::types::PackageRef) -> bool {
        self.allow.iter().any(|a| {
            a.ecosystem.eq_ignore_ascii_case(pkg.ecosystem.osv_name())
                && a.package == pkg.name
                && a.version.as_ref().is_none_or(|v| v == &pkg.version)
        })
    }
}

pub fn global_config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("trusted").join("config.toml"))
}

pub fn project_config_path() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .map(|d| d.join(".trusted.toml"))
}

pub fn trusted_cache_dir() -> Result<PathBuf> {
    cache_dir()
        .map(|d| d.join("trusted"))
        .context("could not determine cache directory")
}

pub fn trusted_home() -> Result<PathBuf> {
    dirs::home_dir()
        .map(|d| d.join(".trusted"))
        .context("could not determine home directory")
}

pub fn shims_dir() -> Result<PathBuf> {
    Ok(trusted_home()?.join("shims"))
}

pub fn write_default_config(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let example = include_str!("../../../config.example.toml");
    if !path.exists() {
        std::fs::write(path, example)?;
    }
    Ok(())
}
