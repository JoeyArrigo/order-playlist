//! In-memory test doubles for `Resolver` and `FeatureSource`.
//!
//! Used by Phase 7's integration tests to exercise the pipeline without
//! touching the network. The doubles also include panic-on-call variants
//! to prove the warm-cache path issues zero network calls.

use std::collections::HashMap;

use order_playlist::adapters::{FeatureSource, Resolution, Resolver};
use order_playlist::domain::{TrackFeatures, TrackId, TrackQuery};

/// In-memory resolver for testing.
///
/// Looks up queries in a provided HashMap and returns Unresolved if not found.
#[allow(dead_code)]
pub struct InMemoryResolver {
    pub map: HashMap<TrackQuery, TrackId>,
}

impl InMemoryResolver {
    /// Create a new InMemoryResolver from an iterator of (query, id) pairs.
    #[allow(dead_code)]
    pub fn new(pairs: impl IntoIterator<Item = (TrackQuery, TrackId)>) -> Self {
        Self {
            map: pairs.into_iter().collect(),
        }
    }
}

#[async_trait::async_trait]
impl Resolver for InMemoryResolver {
    async fn resolve_many(&self, queries: &[TrackQuery]) -> Vec<Resolution> {
        queries
            .iter()
            .map(|q| match self.map.get(q) {
                Some(id) => Resolution::Resolved {
                    query: q.clone(),
                    id: id.clone(),
                },
                None => Resolution::Unresolved {
                    query: q.clone(),
                    reason: "no fixture entry".into(),
                },
            })
            .collect()
    }
}

/// Panic-on-call resolver for testing warm-cache paths.
///
/// Panics if resolve_many is called, ensuring that cache warm paths
/// never invoke the resolver at all.
#[allow(dead_code)]
pub struct PanicOnCallResolver;

#[async_trait::async_trait]
impl Resolver for PanicOnCallResolver {
    async fn resolve_many(&self, _queries: &[TrackQuery]) -> Vec<Resolution> {
        panic!("PanicOnCallResolver was invoked — this should not happen on a warm-cache path");
    }
}

/// In-memory feature source for testing.
///
/// Looks up track IDs in a provided HashMap and returns their features,
/// or `None` if not found.
#[allow(dead_code)]
pub struct InMemoryFeatureSource {
    pub map: HashMap<TrackId, TrackFeatures>,
}

impl InMemoryFeatureSource {
    /// Create a new InMemoryFeatureSource from an iterator of (id, features) pairs.
    #[allow(dead_code)]
    pub fn new(pairs: impl IntoIterator<Item = (TrackId, TrackFeatures)>) -> Self {
        Self {
            map: pairs.into_iter().collect(),
        }
    }
}

#[async_trait::async_trait]
impl FeatureSource for InMemoryFeatureSource {
    async fn features_for(&self, ids: &[TrackId]) -> Vec<(TrackId, Option<TrackFeatures>)> {
        ids.iter()
            .map(|id| (id.clone(), self.map.get(id).cloned()))
            .collect()
    }
}

/// Panic-on-call feature source for testing warm-cache paths.
///
/// Panics if features_for is called, ensuring that cache warm paths
/// never invoke the feature source at all.
#[allow(dead_code)]
pub struct PanicOnCallFeatureSource;

#[async_trait::async_trait]
impl FeatureSource for PanicOnCallFeatureSource {
    async fn features_for(&self, _ids: &[TrackId]) -> Vec<(TrackId, Option<TrackFeatures>)> {
        panic!("PanicOnCallFeatureSource was invoked — warm cache should bypass this");
    }
}
