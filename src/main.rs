// pattern: Imperative Shell
// Reason: This is the entry point for the binary. It handles CLI argument parsing,
// logging initialization, dependency construction, and process exit codes.
// All business logic is delegated to run::run (pure) and adapters (I/O boundaries).

//! `playlistize` binary entry point.

use clap::Parser;
use playlistize::adapters::{Cache, FeatureSource, Resolver};
use playlistize::cli::Args;
use playlistize::run::{run, RunDeps};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> miette::Result<()> {
    // AC9.3: install miette panic hook for source spans + help text.
    miette::set_panic_hook();

    let args = Args::parse().resolve();
    init_tracing(args.verbose);

    if !args.seed_was_supplied {
        tracing::info!(
            seed = args.seed,
            "no --seed supplied; derived from system time"
        );
    }

    let cache_path = args.cache.clone();
    let cache = Arc::new(Mutex::new(
        Cache::load(&cache_path).map_err(miette::Report::new)?,
    ));

    let deps = build_deps(&args, cache.clone())?;
    let (exit, report) = run(args, deps).await?;
    if !report.message.is_empty() {
        eprintln!("error: {}", report.message);
    }

    std::process::exit(exit as i32);
}

/// `init_tracing` is `main.rs`-only. Integration tests call `run()`
/// directly and don't install a global subscriber, so they see only
/// the default no-op subscriber — no double-init panic.
fn init_tracing(verbose: u8) {
    use tracing_subscriber::EnvFilter;
    let default_level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("playlistize={}", default_level)));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// Construct the resolver + feature source based on enabled Cargo features.
/// AC8.3: when no provider feature is enabled, emit a clear error.
fn build_deps(
    args: &playlistize::cli::ResolvedArgs,
    cache: Arc<Mutex<Cache>>,
) -> miette::Result<RunDeps> {
    #[cfg(all(feature = "musicbrainz", feature = "reccobeats"))]
    {
        let resolver = Box::new(
            playlistize::adapters::MusicBrainzIsrcResolver::new(
                cache.clone(),
                format!(
                    "playlistize/{} ({})",
                    env!("CARGO_PKG_VERSION"),
                    args.musicbrainz_contact
                ),
            )
            .map_err(|e| miette::miette!("failed to build MusicBrainz client: {e}"))?,
        ) as Box<dyn Resolver>;

        let feature_source = Box::new(
            playlistize::adapters::ReccoBeatsFeatures::new(cache.clone())
                .map_err(|e| miette::miette!("failed to build ReccoBeats client: {e}"))?,
        ) as Box<dyn FeatureSource>;

        Ok(RunDeps {
            resolver,
            feature_source,
        })
    }

    #[cfg(not(all(feature = "musicbrainz", feature = "reccobeats")))]
    {
        let _ = (args, cache);
        Err(miette::miette!(
            "no resolver/feature source compiled in; build with `--features musicbrainz,reccobeats`"
        ))
    }
}
