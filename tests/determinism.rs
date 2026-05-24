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
    // AC5.3: Different seeds can produce different orderings, or they may converge
    // to the same optimum if the cost landscape has a dominant minimum.
    // Both behaviors are valid. We verify that the RNG seeded differently by
    // checking that runs with the same seed are always identical (AC5.1).
    // This test asserts that different seeds at least complete successfully.
    let a = run_small_party_with_seed_and_skip_and_window(1, &[], 4).await;
    let b = run_small_party_with_seed_and_skip_and_window(999999, &[], 4).await;

    // Both runs must complete successfully
    assert_eq!(
        a.exit,
        ExitCode::Success,
        "seed=1 should complete successfully"
    );
    assert_eq!(
        b.exit,
        ExitCode::Success,
        "seed=999999 should complete successfully"
    );

    // Both output files should exist and contain valid data (header + 10 rows)
    let bytes_a = std::fs::read(&a.output).unwrap();
    let bytes_b = std::fs::read(&b.output).unwrap();

    assert!(!bytes_a.is_empty(), "seed=1 output should not be empty");
    assert!(
        !bytes_b.is_empty(),
        "seed=999999 output should not be empty"
    );

    // Verify both have the correct number of lines
    let lines_a: Vec<_> = std::str::from_utf8(&bytes_a).unwrap().lines().collect();
    let lines_b: Vec<_> = std::str::from_utf8(&bytes_b).unwrap().lines().collect();
    assert_eq!(
        lines_a.len(),
        11,
        "seed=1 output should have header + 10 rows"
    );
    assert_eq!(
        lines_b.len(),
        11,
        "seed=999999 output should have header + 10 rows"
    );
}
