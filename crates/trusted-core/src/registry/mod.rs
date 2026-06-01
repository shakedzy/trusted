mod npm;
mod pypi;

pub use npm::NpmRegistry;
pub use pypi::PyPIRegistry;

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::cache::{DiskCache, ReleaseCacheKey};
use crate::config::Config;
use crate::types::{Ecosystem, PackageRef};

#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub published_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct RegistryClient {
    cache: DiskCache,
    pypi: PyPIRegistry,
    npm: NpmRegistry,
}

impl RegistryClient {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            cache: DiskCache::new(config.cache.ttl_seconds)?,
            pypi: PyPIRegistry::new()?,
            npm: NpmRegistry::new()?,
        })
    }

    pub async fn published_at(&self, pkg: &PackageRef) -> Result<Option<DateTime<Utc>>> {
        let key: ReleaseCacheKey = (
            pkg.ecosystem.osv_name().to_string(),
            pkg.name.clone(),
            pkg.version.clone(),
        );
        if let Some(ts) = self.cache.get::<_, i64>("release", &key)? {
            return Ok(Some(
                DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now),
            ));
        }
        let at = match pkg.ecosystem {
            Ecosystem::PyPI => {
                self.pypi
                    .version_published_at(&pkg.name, &pkg.version)
                    .await?
            }
            Ecosystem::Npm => {
                self.npm
                    .version_published_at(&pkg.name, &pkg.version)
                    .await?
            }
            Ecosystem::CratesIo => crates_io_published_at(&pkg.name, &pkg.version).await?,
            Ecosystem::Go => go_module_published_at(&pkg.name, &pkg.version).await?,
        };
        if let Some(at) = at {
            self.cache.set("release", &key, &at.timestamp())?;
        }
        Ok(at)
    }

    pub async fn list_versions(&self, pkg: &PackageRef) -> Result<Vec<VersionInfo>> {
        match pkg.ecosystem {
            Ecosystem::PyPI => self.pypi.list_versions(&pkg.name).await,
            Ecosystem::Npm => self.npm.list_versions(&pkg.name).await,
            Ecosystem::CratesIo => list_crates_io_versions(&pkg.name).await,
            Ecosystem::Go => list_go_versions(&pkg.name).await,
        }
    }
}

async fn crates_io_published_at(name: &str, version: &str) -> Result<Option<DateTime<Utc>>> {
    let url = format!("https://crates.io/api/v1/crates/{name}/{version}");
    let resp = reqwest::get(&url).await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let v: serde_json::Value = resp.json().await?;
    let created = v
        .get("version")
        .and_then(|x| x.get("created_at"))
        .and_then(|x| x.as_str());
    Ok(created.and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    }))
}

async fn list_crates_io_versions(name: &str) -> Result<Vec<VersionInfo>> {
    let url = format!("https://crates.io/api/v1/crates/{name}/versions");
    let resp = reqwest::get(&url).await?;
    if !resp.status().is_success() {
        return Ok(vec![]);
    }
    let v: serde_json::Value = resp.json().await?;
    let mut out = Vec::new();
    if let Some(versions) = v.get("versions").and_then(|x| x.as_array()) {
        for ver in versions {
            let version = ver.get("num").and_then(|x| x.as_str()).unwrap_or_default();
            let created = ver.get("created_at").and_then(|x| x.as_str());
            if let (version, Some(created)) = (version, created) {
                if let Ok(dt) = DateTime::parse_from_rfc3339(created) {
                    out.push(VersionInfo {
                        version: version.to_string(),
                        published_at: dt.with_timezone(&Utc),
                    });
                }
            }
        }
    }
    Ok(out)
}

async fn go_module_published_at(name: &str, version: &str) -> Result<Option<DateTime<Utc>>> {
    let module_path = if name.contains('/') {
        name.to_string()
    } else {
        return Ok(None);
    };
    let url = format!(
        "https://proxy.golang.org/{}/@v/{version}.info",
        module_path,
        version = urlencoding::encode(version)
    );
    let resp = reqwest::get(&url).await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let v: serde_json::Value = resp.json().await?;
    let time = v.get("Time").and_then(|x| x.as_str());
    Ok(time.and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    }))
}

async fn list_go_versions(name: &str) -> Result<Vec<VersionInfo>> {
    let url = format!("https://proxy.golang.org/{}/@v/list", name);
    let resp = reqwest::get(&url).await?;
    if !resp.status().is_success() {
        return Ok(vec![]);
    }
    let text = resp.text().await?;
    let mut out = Vec::new();
    for version in text.lines() {
        if version.ends_with("/@v/list") {
            continue;
        }
        let v = version.trim();
        if v.is_empty() {
            continue;
        }
        if let Some(published_at) = go_module_published_at(name, v).await? {
            out.push(VersionInfo {
                version: v.to_string(),
                published_at,
            });
        }
    }
    Ok(out)
}
