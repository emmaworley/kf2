//! In-memory cache of authed `ProviderSession`s keyed by
//! `(kf2_session_id, provider_id)`.
//!
//! Entries survive for the lifetime of the process; callers evict explicitly
//! when credentials change or a session is unconfigured. **Note: there is
//! no TTL or capacity bound** — multi-tenant deployments should swap this
//! for an LRU (e.g. `moka` or the `lru` crate) before rolling out widely.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::provider::ProviderSession;
use crate::provider::types::ProviderId;

/// Storage key. Constructed once on insert from a `&str + ProviderId`.
type CacheKey = (Arc<str>, ProviderId);

pub struct ProviderCache {
    inner: RwLock<HashMap<CacheKey, Arc<dyn ProviderSession>>>,
}

impl Default for ProviderCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get(
        &self,
        session_id: &str,
        provider_id: ProviderId,
    ) -> Option<Arc<dyn ProviderSession>> {
        let key: CacheKey = (Arc::from(session_id), provider_id);
        self.inner.read().await.get(&key).cloned()
    }

    /// Insert a fresh value, overwriting any existing entry. Returns the
    /// inserted `Arc` (a clone of `value`).
    pub async fn insert(
        &self,
        session_id: &str,
        provider_id: ProviderId,
        value: Arc<dyn ProviderSession>,
    ) -> Arc<dyn ProviderSession> {
        let key: CacheKey = (Arc::from(session_id), provider_id);
        let returned = value.clone();
        self.inner.write().await.insert(key, value);
        returned
    }

    /// Insert only if no entry exists for `(session_id, provider_id)`. If a
    /// racing caller inserted first, their value wins and we drop our
    /// candidate. Returns the value currently in the cache for this key.
    pub async fn get_or_insert(
        &self,
        session_id: &str,
        provider_id: ProviderId,
        candidate: Arc<dyn ProviderSession>,
    ) -> Arc<dyn ProviderSession> {
        let key: CacheKey = (Arc::from(session_id), provider_id);
        let mut guard = self.inner.write().await;
        guard.entry(key).or_insert(candidate).clone()
    }

    /// Remove the entry for `(session_id, provider_id)`, if any.
    pub async fn evict(&self, session_id: &str, provider_id: ProviderId) {
        let key: CacheKey = (Arc::from(session_id), provider_id);
        let mut guard = self.inner.write().await;
        guard.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::mock::MockProvider;
    use crate::provider::types::ProviderConfig;
    use std::sync::atomic::Ordering;

    fn sample_config() -> ProviderConfig {
        ProviderConfig::BasicAuth {
            username: "u".into(),
            password: "p".into(),
        }
    }

    #[tokio::test]
    async fn get_returns_none_when_empty() {
        let cache = ProviderCache::new();
        assert!(cache.get("nope", ProviderId::Dam).await.is_none());
    }

    #[tokio::test]
    async fn get_or_insert_keeps_the_first_writer() {
        let (provider, control) = MockProvider::new(ProviderId::Dam);
        let cache = ProviderCache::new();

        let first = provider.configure(Some(&sample_config())).await.unwrap();
        let second = provider.configure(Some(&sample_config())).await.unwrap();

        // First insert wins; second insert is dropped.
        let a = cache
            .get_or_insert("s1", ProviderId::Dam, first.clone())
            .await;
        let b = cache
            .get_or_insert("s1", ProviderId::Dam, second.clone())
            .await;
        assert!(Arc::ptr_eq(&a, &b));
        assert!(Arc::ptr_eq(&a, &first));

        // Two successful configures should have happened.
        assert_eq!(control.configure_success_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn evict_removes_entry() {
        let (provider, _) = MockProvider::new(ProviderId::Dam);
        let cache = ProviderCache::new();

        let configured = provider.configure(Some(&sample_config())).await.unwrap();
        cache.insert("s1", ProviderId::Dam, configured).await;
        assert!(cache.get("s1", ProviderId::Dam).await.is_some());

        cache.evict("s1", ProviderId::Dam).await;
        assert!(cache.get("s1", ProviderId::Dam).await.is_none());
    }
}
