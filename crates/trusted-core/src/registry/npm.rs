use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::http_client::registry_http_client;
use crate::registry::VersionInfo;

#[derive(Clone)]
pub struct NpmRegistry {
    http: reqwest::Client,
}

impl NpmRegistry {
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
            "https://registry.npmjs.org/{}/{}",
            urlencoding::encode(name),
            urlencoding::encode(version)
        );
        let resp = self.http.get(&url).send().await?;
        if resp.status().is_success() {
            let v: serde_json::Value = resp.json().await?;
            if let Some(time) = v.get("time").and_then(|t| t.as_str()) {
                return Ok(parse_npm_time(time));
            }
        }
        let meta_url = format!("https://registry.npmjs.org/{}", urlencoding::encode(name));
        let resp = self.http.get(&meta_url).send().await?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        let v: serde_json::Value = resp.json().await?;
        let time = v
            .get("time")
            .and_then(|t| t.get(version))
            .and_then(|t| t.as_str());
        Ok(time.and_then(parse_npm_time))
    }

    pub async fn list_versions(&self, name: &str) -> Result<Vec<VersionInfo>> {
        let url = format!("https://registry.npmjs.org/{}", urlencoding::encode(name));
        let resp = self.http.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let v: serde_json::Value = resp.json().await?;
        let times = v.get("time").and_then(|t| t.as_object());
        let mut out = Vec::new();
        if let Some(times) = times {
            let versions = v
                .get("versions")
                .and_then(|vers| vers.as_object())
                .map(|m| m.keys().cloned().collect::<std::collections::HashSet<_>>())
                .unwrap_or_default();
            for (version, time_val) in times {
                if version == "created" || version == "modified" {
                    continue;
                }
                if !versions.is_empty() && !versions.contains(version) {
                    continue;
                }
                if let Some(ts) = time_val.as_str().and_then(parse_npm_time) {
                    out.push(VersionInfo {
                        version: version.clone(),
                        published_at: ts,
                    });
                }
            }
        }
        Ok(out)
    }
}

fn parse_npm_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}
