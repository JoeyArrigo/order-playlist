// pattern: Imperative Shell
// Reason: This module orchestrates I/O operations (cache reads, resolver calls,
// feature source calls, file writes) with the pure algorithm core. Side effects
// are inherent to the purpose (reading input, writing output, cache persistence).

//! Orchestration: read CSV → cache-partition → resolve → fetch features → anneal → write outputs.
//!
//! Exposed to integration tests so the pipeline can be exercised with
//! `InMemoryResolver` + `InMemoryFeatureSource` (or `PanicOnCall*` doubles).

use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::adapters::{read_input, write_output, write_unresolved, Unresolved};
use crate::adapters::{Cache, FeatureSource, Resolution, Resolver};
use crate::algo::{optimize, AnnealConfig, CamelotTable, CostContext, CostWeights, EnergyArc};
use crate::cli::{
    format_summary, render_arc,
    report::{count_artist_clashes, SummaryInputs},
    ResolvedArgs,
};
use crate::domain::{Track, TrackId, TrackQuery};

/// Semantic exit codes (per design).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    Success = 0,
    Other = 1,
    BadArgs = 2,
    InputError = 3,
    CacheError = 4,
    NothingResolved = 5,
    NetworkExhausted = 6,
}

pub struct RunDeps {
    pub resolver: Box<dyn Resolver>,
    pub feature_source: Box<dyn FeatureSource>,
}

/// Diagnostic payload returned alongside an `ExitCode`. Lets `main.rs`
/// (or test harnesses) format their own user-facing message rather than
/// having `run()` `eprintln!` from inside the library — keeps the
/// CLI/lib separation that `cli/mod.rs` declares.
#[derive(Debug, Clone, Default)]
pub struct RunReport {
    /// Human-readable message attached to a non-success exit (or empty).
    pub message: String,
}

pub async fn run(
    args: ResolvedArgs,
    deps: RunDeps,
) -> Result<(ExitCode, RunReport), miette::Report> {
    // 1. Read input.
    let queries = read_input(&args.input).map_err(miette::Report::new)?;
    tracing::info!(count = queries.len(), input = %args.input.display(), "loaded input");

    // 2. Load cache.
    let cache = Arc::new(Mutex::new(
        Cache::load(&args.cache).map_err(miette::Report::new)?,
    ));

    // 3. AC7.1: Partition queries against the cache BEFORE calling the resolver.
    //    Cached "explicit failure" (empty TrackId) → goes straight to `unresolved`.
    //    Cached "success" (non-empty TrackId) → goes straight to `resolved_pairs`.
    //    Anything else → into `to_resolve`, which the resolver sees only when non-empty.
    let mut resolved_pairs: Vec<(TrackQuery, TrackId)> = Vec::new();
    let mut unresolved: Vec<Unresolved> = Vec::new();
    let mut to_resolve: Vec<TrackQuery> = Vec::new();
    {
        let cache_lock = cache.lock().await;
        for q in queries {
            match cache_lock.get_resolution(&q) {
                Some(id) if !id.get().is_empty() => resolved_pairs.push((q, id.clone())),
                Some(_) => {
                    tracing::warn!(title = %q.title, artist = %q.artist, "unresolved: cached prior failure");
                    unresolved.push(Unresolved {
                        query: q,
                        reason: "cached: no ISRC on prior run".into(),
                    });
                }
                None => to_resolve.push(q),
            }
        }
    }

    if !to_resolve.is_empty() {
        for r in deps.resolver.resolve_many(&to_resolve).await {
            match r {
                Resolution::Resolved { query, id } => resolved_pairs.push((query, id)),
                Resolution::Unresolved { query, reason } => {
                    tracing::warn!(title = %query.title, artist = %query.artist, %reason, "unresolved");
                    unresolved.push(Unresolved { query, reason });
                }
            }
        }
    }

    // 4. AC7.1: Partition IDs against the feature cache. Same logic, applied
    //    to feature lookup. Only un-cached IDs are passed to `features_for`.
    let mut tracks: Vec<Track> = Vec::new();
    let mut to_fetch: Vec<TrackId> = Vec::new();
    let mut pending: Vec<(TrackQuery, TrackId)> = Vec::new();
    {
        let cache_lock = cache.lock().await;
        for (q, id) in resolved_pairs {
            match cache_lock.get_features(&id) {
                Some(f) => tracks.push(Track {
                    query: q,
                    id,
                    features: f.clone(),
                }),
                None => {
                    pending.push((q, id.clone()));
                    to_fetch.push(id);
                }
            }
        }
    }

    if !to_fetch.is_empty() {
        let fetched = deps.feature_source.features_for(&to_fetch).await;
        // `fetched` is in the same order as `to_fetch`; zip with `pending`.
        for ((q, id), (_, feat)) in pending.into_iter().zip(fetched) {
            match feat {
                Some(f) => tracks.push(Track {
                    query: q,
                    id,
                    features: f,
                }),
                None => {
                    tracing::warn!(title = %q.title, artist = %q.artist, "unresolved: feature lookup returned None");
                    unresolved.push(Unresolved {
                        query: q,
                        reason: "feature lookup returned None".into(),
                    });
                }
            }
        }
    }

    // 5. Save cache (best-effort).
    {
        let c = cache.lock().await;
        if let Err(e) = c.save_atomic() {
            tracing::warn!(error = %format!("{e:?}"), "cache save failed; continuing");
        }
    }

    // 6. Always write unresolved.csv when there's content (AC4.1 + AC4.5).
    if !unresolved.is_empty() {
        write_unresolved(&args.unresolved, &unresolved).map_err(miette::Report::new)?;
    }

    // 7. Bail out if nothing resolved (AC4.4). `main.rs` formats the message.
    if tracks.is_empty() {
        return Ok((
            ExitCode::NothingResolved,
            RunReport {
                message: "no tracks resolved; nothing to anneal".into(),
            },
        ));
    }

    // 8. Anneal.
    let ctx = CostContext {
        tracks: &tracks,
        weights: CostWeights {
            artist_window: args.artist_window,
            ..Default::default()
        },
        arc: EnergyArc,
        camelot_table: CamelotTable::new(),
    };
    let initial: Vec<usize> = (0..tracks.len()).collect();
    let before_cost = ctx.total_cost(&initial);
    let before_arc_dev = compute_arc_dev(&tracks, &initial);

    let mut rng = ChaCha20Rng::seed_from_u64(args.seed);
    let ordering = optimize(initial, &ctx, &AnnealConfig::default(), &mut rng);
    let after_cost = ctx.total_cost(&ordering);
    let after_arc_dev = compute_arc_dev(&tracks, &ordering);

    // 9. Write output CSV.
    write_output(&args.output, &ordering, &tracks).map_err(miette::Report::new)?;

    // 10. Print chart + summary to stdout. (run.rs is permitted to write
    //     to stdout for the deliverables; only stderr error printing is
    //     forbidden — see RunReport above.)
    print!("{}", render_arc(&tracks, &ordering, 12));
    let breakdown = ctx.cost_breakdown(&ordering);
    let summary = format_summary(&SummaryInputs {
        resolved: tracks.len(),
        unresolved: unresolved.len(),
        unresolved_path: &args.unresolved,
        seed: args.seed,
        seed_was_supplied: args.seed_was_supplied,
        before_cost,
        after_cost,
        before_arc_dev,
        after_arc_dev,
        cost_breakdown: breakdown,
        remaining_clashes: count_artist_clashes(&tracks, &ordering, args.artist_window),
    });
    print!("{}", summary);

    Ok((ExitCode::Success, RunReport::default()))
}

fn compute_arc_dev(tracks: &[Track], ordering: &[usize]) -> f32 {
    let arc = EnergyArc;
    let n = ordering.len();
    ordering
        .iter()
        .enumerate()
        .map(|(i, &idx)| arc.deviation_cost(i, n, tracks[idx].features.energy))
        .sum()
}
