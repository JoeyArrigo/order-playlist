//! In-memory `Resolver` test double.
//!
//! Used by Phase 7's integration tests to exercise the pipeline without
//! touching the network. The double also includes `PanicOnCallResolver`
//! to prove the warm-cache path issues zero network calls.

use std::collections::HashMap;

use playlistize::adapters::{Resolution, Resolver};
use playlistize::domain::{TrackId, TrackQuery};

/// In-memory resolver for testing.
///
/// Looks up queries in a provided HashMap and returns Unresolved if not found.
pub struct InMemoryResolver {
    pub map: HashMap<TrackQuery, TrackId>,
}

impl InMemoryResolver {
    /// Create a new InMemoryResolver from an iterator of (query, id) pairs.
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
