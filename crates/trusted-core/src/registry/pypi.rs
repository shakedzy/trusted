use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::http_client::registry_http_client;
use crate::registry::VersionInfo;

#[derive(Clone)]
pub struct PyPIRegistry {
    http: reqwest::Client,
}

impl PyPIRegistry {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: registry_http_client()?,
        })
    }

    pub async fn version_published_at(
        &self,
        name: &str,
        version: &str,
    ) -> Result<Option<DateTime<Utc>>> {
        let url = format!(
            "https://pypi.org/pypi/{}/{}/json",
            urlencoding::encode(name),
            urlencoding::encode(version)
        );
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        let v: serde_json::Value = resp.json().await?;
        let uploads = v
            .get("urls")
            .and_then(|u| u.as_array())
            .and_then(|arr| arr.first())
            .and_then(|u| u.get("upload_time"))
            .and_then(|t| t.as_str())
            .or_else(|| {
                v.get("info")
                    .and_then(|i| i.get("release_date"))
                    .and_then(|t| t.as_str())
            });
        Ok(uploads.and_then(parse_pypi_time))
    }

    pub async fn list_versions(&self, name: &str) -> Result<Vec<VersionInfo>> {
        let url = format!("https://pypi.org/pypi/{}/json", urlencoding::encode(name));
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let v: serde_json::Value = resp.json().await?;
        let releases = v
            .get("releases")
            .and_then(|r| r.as_object())
            .context("pypi releases object")?;
        let mut out = Vec::new();
        for (version, files) in releases {
            if let Some(arr) = files.as_array() {
                if arr.is_empty() {
                    continue;
                }
                let published_at = arr
                    .iter()
                    .filter_map(|f| f.get("upload_time").and_then(|t| t.as_str()))
                    .filter_map(parse_pypi_time)
                    .min()
                    .unwrap_or_else(Utc::now);
                out.push(VersionInfo {
                    version: version.clone(),
                    published_at,
                });
            }
        }
        Ok(out)
    }
}

fn parse_pypi_time(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .map(|n| n.and_utc())
}
