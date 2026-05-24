mod support;

use order_playlist::adapters::{Resolution, Resolver};
use order_playlist::domain::{TrackId, TrackQuery};
use support::in_memory::InMemoryResolver;

#[tokio::test]
async fn in_memory_resolver_returns_known_id() {
    let q = TrackQuery::new("Get Lucky", "Daft Punk");
    let id = TrackId::new("USQX91300120");
    let r = InMemoryResolver::new([(q.clone(), id.clone())]);
    let results = r.resolve_many(std::slice::from_ref(&q)).await;
    match &results[0] {
        Resolution::Resolved { query, id: got_id } => {
            assert_eq!(query, &q);
            assert_eq!(got_id, &id);
        }
        _ => panic!("expected Resolved"),
    }
}

#[tokio::test]
async fn in_memory_resolver_returns_unresolved_for_missing() {
    let r = InMemoryResolver::new([]);
    let q = TrackQuery::new("Unknown", "Nobody");
    let results = r.resolve_many(std::slice::from_ref(&q)).await;
    assert!(matches!(&results[0], Resolution::Unresolved { .. }));
}
