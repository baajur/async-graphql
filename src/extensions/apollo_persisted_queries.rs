//! Apollo persisted queries extension.

use crate::extensions::{Error, Extension, ExtensionContext, ExtensionFactory};
use crate::{Request, Result};
use futures::lock::Mutex;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
struct PersistedQuery {
    version: i32,
    #[serde(rename = "sha256Hash")]
    sha256_hash: String,
}

/// Cache storage for persisted queries.
#[async_trait::async_trait]
pub trait CacheStorage: Send + Sync + Clone + 'static {
    /// Load the query by `key`.
    async fn get(&self, key: String) -> Option<String>;

    /// Save the query by `key`.
    async fn set(&self, key: String, query: String);
}

/// Memory-based LRU cache.
#[derive(Clone)]
pub struct LruCacheStorage(Arc<Mutex<lru::LruCache<String, String>>>);

impl LruCacheStorage {
    /// Creates a new LRU Cache that holds at most `cap` items.
    pub fn new(cap: usize) -> Self {
        Self(Arc::new(Mutex::new(lru::LruCache::new(cap))))
    }
}

#[async_trait::async_trait]
impl CacheStorage for LruCacheStorage {
    async fn get(&self, key: String) -> Option<String> {
        let mut cache = self.0.lock().await;
        cache.get(&key).cloned()
    }

    async fn set(&self, key: String, query: String) {
        let mut cache = self.0.lock().await;
        cache.put(key, query);
    }
}

/// Apollo persisted queries extension.
///
/// [Reference](https://www.apollographql.com/docs/react/api/link/persisted-queries/)
#[cfg_attr(feature = "nightly", doc(cfg(feature = "apollo_persisted_queries")))]
pub struct ApolloPersistedQueries<T>(T);

impl<T: CacheStorage> ApolloPersistedQueries<T> {
    /// Creates an apollo persisted queries extension.
    pub fn new(cache_storage: T) -> ApolloPersistedQueries<T> {
        Self(cache_storage)
    }
}

impl<T: CacheStorage> ExtensionFactory for ApolloPersistedQueries<T> {
    fn create(&self) -> Box<dyn Extension> {
        Box::new(ApolloPersistedQueriesExtension {
            storage: self.0.clone(),
        })
    }
}

struct ApolloPersistedQueriesExtension<T> {
    storage: T,
}

#[async_trait::async_trait]
impl<T: CacheStorage> Extension for ApolloPersistedQueriesExtension<T> {
    async fn prepare_request(
        &mut self,
        _ctx: &ExtensionContext<'_>,
        mut request: Request,
    ) -> Result<Request> {
        if let Some(value) = request.extensions.remove("persistedQuery") {
            let persisted_query: PersistedQuery = serde_json::from_value(value).map_err(|_| {
                Error::Other("Invalid \"PersistedQuery\" extension configuration.".to_string())
            })?;
            if persisted_query.version != 1 {
                return Err(Error::Other (
                    format!("Only the \"PersistedQuery\" extension of version \"1\" is supported, and the current version is \"{}\".", persisted_query.version),
                    ));
            }

            if request.query.is_empty() {
                if let Some(query) = self.storage.get(persisted_query.sha256_hash).await {
                    Ok(Request { query, ..request })
                } else {
                    Err(Error::Other("PersistedQueryNotFound".to_string()))
                }
            } else {
                self.storage
                    .set(persisted_query.sha256_hash, request.query.clone())
                    .await;
                Ok(request)
            }
        } else {
            Ok(request)
        }
    }
}

#[cfg(test)]
mod tests {
    #[async_std::test]
    async fn test() {
        use super::*;
        use crate::*;

        struct Query;

        #[Object(internal)]
        impl Query {
            async fn value(&self) -> i32 {
                100
            }
        }

        let schema = Schema::build(Query, EmptyMutation, EmptySubscription)
            .extension(ApolloPersistedQueries::new(LruCacheStorage::new(256)))
            .finish();

        let mut request = Request::new("{ value }");
        request.extensions.insert(
            "persistedQuery".to_string(),
            serde_json::json!({
                "version": 1,
                "sha256Hash": "abc",
            }),
        );

        assert_eq!(
            schema.execute(request).await.into_result().unwrap().data,
            serde_json::json!({
                "value": 100
            })
        );

        let mut request = Request::new("");
        request.extensions.insert(
            "persistedQuery".to_string(),
            serde_json::json!({
                "version": 1,
                "sha256Hash": "abc",
            }),
        );

        assert_eq!(
            schema.execute(request).await.into_result().unwrap().data,
            serde_json::json!({
                "value": 100
            })
        );

        let mut request = Request::new("");
        request.extensions.insert(
            "persistedQuery".to_string(),
            serde_json::json!({
                "version": 1,
                "sha256Hash": "def",
            }),
        );

        assert_eq!(
            schema.execute(request).await.into_result().unwrap_err(),
            Error::Other("PersistedQueryNotFound".to_string())
        );
    }
}
