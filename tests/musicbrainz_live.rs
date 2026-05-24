//! Live-HTTP smoke test for the MusicBrainz resolver.
//!
//! Gated by the `live-network` Cargo feature so default `cargo test`
//! never touches the network. Per AC9.1: no silent-skip pattern — if
//! the feature is enabled but the network is unreachable, the test
//! must fail loudly (and it will, since reqwest's connect timeout
//! surfaces a real error).

#![cfg(all(feature = "musicbrainz", feature = "live-network"))]

use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

use order_playlist::adapters::{Cache, MusicBrainzIsrcResolver, Resolution, Resolver};
use order_playlist::domain::TrackQuery;

#[tokio::test]
async fn resolves_get_lucky_to_isrc() {
    let dir = TempDir::new().unwrap();
    let cache = Arc::new(Mutex::new(
        Cache::load(&dir.path().join("cache.json")).unwrap(),
    ));

    let resolver =
        MusicBrainzIsrcResolver::new(cache, "order_playlist-test/0.1 (test@example.com)".into())
            .unwrap();

    let q = TrackQuery::new("Get Lucky", "Daft Punk");
    let results = resolver.resolve_many(&[q]).await;
    match &results[0] {
        Resolution::Resolved { id, .. } => {
            // Just assert non-empty — the exact ISRC may vary across
            // MusicBrainz updates. The known US ISRC for "Get Lucky"
            // is "USQX91300120" but newer releases may rank higher.
            assert!(!id.get().is_empty(), "expected a non-empty ISRC");
        }
        Resolution::Unresolved { reason, .. } => {
            panic!("expected Resolved, got Unresolved: {}", reason);
        }
    }
}
