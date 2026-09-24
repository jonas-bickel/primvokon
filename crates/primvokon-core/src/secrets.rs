//! Secret storage behind a trait: the Freedesktop Secret Service / portal keyring (via `oo7`)
//! when available, otherwise a `0600` JSON file with a visible warning in the UI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::paths;

pub type SharedSecrets = Arc<dyn SecretStore>;

#[async_trait]
pub trait SecretStore: Send + Sync {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>>;
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()>;
    async fn delete(&self, key: &str) -> anyhow::Result<()>;
    /// Human readable backend name for the settings UI.
    fn backend(&self) -> &'static str;
    /// `true` for the insecure file fallback.
    fn is_fallback(&self) -> bool {
        false
    }
}

/// Convenience: set when `value` is non-empty, delete otherwise.
pub async fn set_or_delete(store: &dyn SecretStore, key: &str, value: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        store.delete(key).await
    } else {
        store.set(key, value).await
    }
}

/// Open the best available backend.
pub async fn open() -> SharedSecrets {
    match tokio::time::timeout(Duration::from_secs(5), KeyringStore::new()).await {
        Ok(Ok(store)) => Arc::new(store),
        Ok(Err(e)) => {
            tracing::warn!("secret service unavailable ({e}); using file fallback");
            Arc::new(FileStore::at(paths::secrets_file()))
        }
        Err(_) => {
            tracing::warn!("secret service timed out; using file fallback");
            Arc::new(FileStore::at(paths::secrets_file()))
        }
    }
}

// ---------------------------------------------------------------------------------------

/// Secret Service / portal keyring.
pub struct KeyringStore {
    keyring: oo7::Keyring,
}

const ATTR_APP: &str = "application";
const ATTR_KEY: &str = "primvokon-key";

impl KeyringStore {
    pub async fn new() -> anyhow::Result<Self> {
        let keyring = oo7::Keyring::new().await?;
        Ok(Self { keyring })
    }

    fn attrs(key: &str) -> HashMap<&'static str, String> {
        HashMap::from([(ATTR_APP, paths::APP_ID.to_string()), (ATTR_KEY, key.to_string())])
    }
}

#[async_trait]
impl SecretStore for KeyringStore {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        let items = self.keyring.search_items(&Self::attrs(key)).await?;
        match items.first() {
            Some(item) => {
                let secret = item.secret().await?;
                Ok(Some(String::from_utf8_lossy(secret.as_bytes()).into_owned()))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let label = format!("PRIMVOKON {key}");
        self.keyring
            .create_item(&label, &Self::attrs(key), oo7::Secret::text(value), true)
            .await?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.keyring.delete(&Self::attrs(key)).await?;
        Ok(())
    }

    fn backend(&self) -> &'static str {
        "Secret Service keyring"
    }
}

// ---------------------------------------------------------------------------------------

/// Plain JSON file with `0600` permissions.
pub struct FileStore {
    path: PathBuf,
    cache: Mutex<Option<HashMap<String, String>>>,
}

impl FileStore {
    pub fn at(path: PathBuf) -> Self {
        Self {
            path,
            cache: Mutex::new(None),
        }
    }

    async fn load(&self, cache: &mut Option<HashMap<String, String>>) -> anyhow::Result<()> {
        if cache.is_none() {
            let map = match tokio::fs::read(&self.path).await {
                Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
                Err(_) => HashMap::new(),
            };
            *cache = Some(map);
        }
        Ok(())
    }

    async fn persist(&self, map: &HashMap<String, String>) -> anyhow::Result<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = serde_json::to_vec_pretty(map)?;
        tokio::fs::write(&self.path, bytes).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600)).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl SecretStore for FileStore {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        let mut cache = self.cache.lock().await;
        self.load(&mut cache).await?;
        Ok(cache.as_ref().and_then(|m| m.get(key).cloned()))
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let mut cache = self.cache.lock().await;
        self.load(&mut cache).await?;
        let map = cache.as_mut().expect("loaded");
        map.insert(key.to_string(), value.to_string());
        self.persist(map).await
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        let mut cache = self.cache.lock().await;
        self.load(&mut cache).await?;
        let map = cache.as_mut().expect("loaded");
        if map.remove(key).is_some() {
            self.persist(map).await?;
        }
        Ok(())
    }

    fn backend(&self) -> &'static str {
        "Plain file (no Secret Service found)"
    }

    fn is_fallback(&self) -> bool {
        true
    }
}

/// In-memory store for tests and the API explorer.
#[derive(Default)]
pub struct MemoryStore {
    map: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl SecretStore for MemoryStore {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.map.lock().await.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.map.lock().await.insert(key.into(), value.into());
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.map.lock().await.remove(key);
        Ok(())
    }

    fn backend(&self) -> &'static str {
        "memory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn file_store_roundtrip_and_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::at(dir.path().join("secrets.json"));
        store.set("a", "1").await.unwrap();
        assert_eq!(store.get("a").await.unwrap().as_deref(), Some("1"));
        store.delete("a").await.unwrap();
        assert_eq!(store.get("a").await.unwrap(), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("secrets.json")).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(store.is_fallback());
    }
}
