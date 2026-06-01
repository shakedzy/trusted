use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use std::time::Duration;

use crate::cache::{osv_cache_key, DiskCache};
use crate::config::Config;
use crate::types::PackageRef;

const OSV_BATCH_URL: &str = "https://api.osv.dev/v1/querybatch";

#[derive(Debug, Clone, Serialize)]
struct Query {
    package: OsvPackage,
    version: String,
}

#[derive(Debug, Clone, Serialize)]
struct OsvPackage {
    name: String,
    ecosystem: String,
}

#[derive(Debug, Deserialize)]
struct BatchResponse {
    results: Vec<QueryResult>,
}

#[derive(Debug, Deserialize)]
struct QueryResult {
    #[serde(default)]
    vulns: Vec<Vuln>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vuln {
    pub id: String,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsvCheckResult {
    pub vulnerable: bool,
    pub vulns: Vec<Vuln>,
}

pub struct OsvClient {
    http: reqwest::Client,
    api_key: Option<String>,
    cache: DiskCache,
}

impl OsvClient {
    pub fn new(config: &Config) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10));
        if let Some(key) = &config.osv.api_key {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                "X-API-Key",
                reqwest::header::HeaderValue::from_str(key).context("invalid OSV API key")?,
            );
            builder = builder.default_headers(headers);
        }
        Ok(Self {
            http: builder.build()?,
            api_key: config.osv.api_key.clone(),
            cache: DiskCache::new(config.cache.ttl_seconds)?,
        })
    }

    pub async fn check_packages(&self, packages: &[PackageRef]) -> Result<Vec<OsvCheckResult>> {
        let mut results = Vec::with_capacity(packages.len());
        let mut uncached = Vec::new();
        let mut uncached_indices = Vec::new();

        for (i, pkg) in packages.iter().enumerate() {
            let key = osv_cache_key(pkg);
            if let Some(cached) = self.cache.get("osv", &key)? {
                results.push(cached);
            } else {
                results.push(OsvCheckResult {
                    vulnerable: false,
                    vulns: vec![],
                });
                uncached.push(pkg.clone());
                uncached_indices.push(i);
            }
        }

        if uncached.is_empty() {
            return Ok(results);
        }

        let queries: Vec<Query> = uncached
            .iter()
            .map(|p| Query {
                package: OsvPackage {
                    name: p.name.clone(),
                    ecosystem: p.ecosystem.osv_name().to_string(),
                },
                version: p.version.clone(),
            })
            .collect();

        let body = serde_json::json!({ "queries": queries });
        let mut req = self.http.post(OSV_BATCH_URL).json(&body);
        if let Some(key) = &self.api_key {
            req = req.header("X-API-Key", key);
        }
        let resp = req.send().await.context("OSV batch request")?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("OSV API error {status}: {text}");
        }
        let batch: BatchResponse = serde_json::from_str(&text).context("parse OSV response")?;

        for (idx, (pkg, qr)) in uncached_indices
            .into_iter()
            .zip(uncached.into_iter().zip(batch.results))
        {
            let vulns = qr.vulns;
            let result = OsvCheckResult {
                vulnerable: !vulns.is_empty(),
                vulns,
            };
            let key = osv_cache_key(&pkg);
            self.cache.set("osv", &key, &result)?;
            results[idx] = result;
        }

        Ok(results)
    }

    pub async fn affected_versions_hint(&self, pkg: &PackageRef) -> Result<Option<String>> {
        let body = serde_json::json!({
            "package": {
                "name": pkg.name,
                "ecosystem": pkg.ecosystem.osv_name(),
            }
        });
        let url = "https://api.osv.dev/v1/query";
        let resp = self
            .http
            .post(url)
            .json(&body)
            .send()
            .await
            .context("OSV query for hints")?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        let text = resp.text().await?;
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        let vulns = parsed.get("vulns").and_then(|v| v.as_array());
        if vulns.is_none() {
            return Ok(None);
        }
        Ok(Some(
            "see OSV advisories for this package; try an older version".to_string(),
        ))
    }
}
