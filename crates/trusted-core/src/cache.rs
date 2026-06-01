use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use crate::config::trusted_cache_dir;

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct CacheEnvelope<T> {
    expires_at: u64,
    value: T,
}

#[derive(Clone)]
pub struct DiskCache {
    dir: PathBuf,
    ttl: Duration,
}

impl DiskCache {
    pub fn new(ttl_seconds: u64) -> Result<Self> {
        let dir = trusted_cache_dir()?;
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            ttl: Duration::from_secs(ttl_seconds),
        })
    }

    fn path_for<K: Hash>(&self, namespace: &str, key: &K) -> PathBuf {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        namespace.hash(&mut h);
        let hash = h.finish();
        self.dir.join(format!("{namespace}-{hash:016x}.json"))
    }

    pub fn get<K, V>(&self, namespace: &str, key: &K) -> Result<Option<V>>
    where
        K: Hash + Serialize,
        V: DeserializeOwned,
    {
        let path = self.path_for(namespace, key);
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path)?;
        let env: CacheEnvelope<V> = serde_json::from_str(&text)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now > env.expires_at {
            let _ = std::fs::remove_file(&path);
            return Ok(None);
        }
        Ok(Some(env.value))
    }

    pub fn set<K, V>(&self, namespace: &str, key: &K, value: &V) -> Result<()>
    where
        K: Hash + Serialize,
        V: Serialize,
    {
        let path = self.path_for(namespace, key);
        let expires_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + self.ttl.as_secs();
        let env = CacheEnvelope { expires_at, value };
        let text = serde_json::to_string(&env)?;
        std::fs::write(path, text).context("write cache file")?;
        Ok(())
    }
}

pub type OsvCacheKey = (String, String, String);

pub fn osv_cache_key(pkg: &crate::types::PackageRef) -> OsvCacheKey {
    (
        pkg.ecosystem.osv_name().to_string(),
        pkg.name.clone(),
        pkg.version.clone(),
    )
}

pub type ReleaseCacheKey = OsvCacheKey;

#[derive(Debug, Clone, Default)]
pub struct MemoryCache<K, V>
where
    K: Eq + Hash,
{
    inner: HashMap<K, V>,
}

impl<K, V> MemoryCache<K, V>
where
    K: Eq + Hash,
{
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.inner.get(key)
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.inner.insert(key, value);
    }
}
