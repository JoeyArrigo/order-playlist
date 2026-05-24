mod support;

use playlistize::run::ExitCode;
use support::common::{
    run_small_party_with_seed, run_small_party_with_seed_and_skip,
    run_small_party_with_seed_and_skip_and_window,
};

#[tokio::test]
async fn two_runs_same_seed_produce_byte_identical_output() {
    let a = run_small_party_with_seed(42).await;
    let b = run_small_party_with_seed(42).await;
    assert_eq!(a.exit, ExitCode::Success);
    assert_eq!(b.exit, ExitCode::Success);

    let bytes_a = std::fs::read(&a.output).unwrap();
    let bytes_b = std::fs::read(&b.output).unwrap();
    assert_eq!(bytes_a, bytes_b, "AC5.1: outputs must be byte-identical");
}

#[tokio::test]
async fn two_runs_same_seed_produce_byte_identical_unresolved() {
    // Force two queries to be unresolved by hiding them from the resolver map.
    let skip = ["Get Lucky", "Dancing Queen"];
    let a = run_small_party_with_seed_and_skip(42, &skip).await;
    let b = run_small_party_with_seed_and_skip(42, &skip).await;
    assert_eq!(a.exit, ExitCode::Success);

    // Both runs must produce unresolved.csv with the same content.
    assert!(
        a.unresolved.exists(),
        "unresolved.csv should be created when there are unresolved tracks"
    );
    assert!(
        b.unresolved.exists(),
        "unresolved.csv should be created when there are unresolved tracks"
    );

    let bytes_a = std::fs::read(&a.unresolved).unwrap();
    let bytes_b = std::fs::read(&b.unresolved).unwrap();
    assert_eq!(
        bytes_a, bytes_b,
        "AC5.2: unresolved.csv must be byte-identical"
    );
}

#[tokio::test]
async fn different_seeds_may_produce_different_orderings() {
    // AC5.3: Different seeds may produce different orderings (though they may converge
    // to the same optimum if the cost landscape has a dominant minimum).
    // This test tries multiple seed pairs and verifies that AT LEAST ONE pair
    // produces different orderings, confirming the RNG seed has an observable effect.
    // If all pairs converge to the same ordering, that's also valid (dominant optimum),
    // but we verify by checking if any pair differs; at least one should.

    let pairs = [(1u64, 99999u64), (42u64, 1000000u64), (7u64, 12345u64)];

    let mut any_differ = false;
    for (s1, s2) in pairs {
        let a = run_small_party_with_seed_and_skip_and_window(s1, &[], 4).await;
        let b = run_small_party_with_seed_and_skip_and_window(s2, &[], 4).await;

        assert_eq!(
            a.exit,
            ExitCode::Success,
            "seed={} should complete successfully",
            s1
        );
        assert_eq!(
            b.exit,
            ExitCode::Success,
            "seed={} should complete successfully",
            s2
        );

        let bytes_a = std::fs::read(&a.output).unwrap();
        let bytes_b = std::fs::read(&b.output).unwrap();

        assert!(
            !bytes_a.is_empty(),
            "seed={} output should not be empty",
            s1
        );
        assert!(
            !bytes_b.is_empty(),
            "seed={} output should not be empty",
            s2
        );

        // Verify both have the correct number of lines (header + 10 rows)
        let lines_a: Vec<_> = std::str::from_utf8(&bytes_a).unwrap().lines().collect();
        let lines_b: Vec<_> = std::str::from_utf8(&bytes_b).unwrap().lines().collect();
        assert_eq!(
            lines_a.len(),
            11,
            "seed={} output should have header + 10 rows",
            s1
        );
        assert_eq!(
            lines_b.len(),
            11,
            "seed={} output should have header + 10 rows",
            s2
        );

        // Check if this pair produces different orderings
        if bytes_a != bytes_b {
            any_differ = true;
            break;
        }
    }

    assert!(
        any_differ,
        "AC5.3: at least one seed pair should produce different orderings (RNG must affect optimization)"
    );
}
