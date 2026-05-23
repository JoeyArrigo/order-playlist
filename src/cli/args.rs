//! CLI argument parser using `clap` derive.
//!
//! Defaults are filled in by `Args::resolve()` after parsing — clap
//! can't compute defaults that depend on other args (e.g.,
//! `unresolved` defaults to `output` with a different extension).

use std::path::PathBuf;

#[derive(clap::Parser, Debug)]
#[command(
    name = "playlistize",
    version,
    about = "Reorder a CSV playlist to follow a target energy arc."
)]
pub struct Args {
    /// Input CSV file with `title,artist` columns.
    #[arg(long)]
    pub input: PathBuf,

    /// Output CSV file with reordered tracks + feature columns.
    #[arg(long)]
    pub output: PathBuf,

    /// Sidecar CSV for tracks that couldn't be resolved.
    /// Defaults to `unresolved.csv` next to `--output`.
    #[arg(long)]
    pub unresolved: Option<PathBuf>,

    /// Feature cache JSON. Defaults to `<input>.cache.json`.
    #[arg(long)]
    pub cache: Option<PathBuf>,

    /// Seed for the deterministic RNG. If absent, derived from system
    /// time and logged at INFO.
    #[arg(long)]
    pub seed: Option<u64>,

    /// Window for the artist-spacing constraint. `0` disables.
    #[arg(long, default_value_t = 4)]
    pub artist_window: u8,

    /// Increase log verbosity (-v=DEBUG, -vv=TRACE). Default is INFO.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// User-agent contact string for MusicBrainz (e.g., email or URL).
    /// MusicBrainz throttles aggressively without a real contact.
    #[arg(long, default_value = "anonymous@example.com")]
    pub musicbrainz_contact: String,
}

/// `Args` with all defaults resolved (no more `Option`s on
/// path/seed fields). Produced by `Args::resolve`.
pub struct ResolvedArgs {
    pub input: PathBuf,
    pub output: PathBuf,
    pub unresolved: PathBuf,
    pub cache: PathBuf,
    pub seed: u64,
    pub seed_was_supplied: bool,
    pub artist_window: u8,
    pub verbose: u8,
    pub musicbrainz_contact: String,
}

impl Args {
    pub fn resolve(self) -> ResolvedArgs {
        let unresolved = self.unresolved.unwrap_or_else(|| {
            self.output
                .parent()
                .map(|p| p.join("unresolved.csv"))
                .unwrap_or_else(|| PathBuf::from("unresolved.csv"))
        });
        let cache = self.cache.unwrap_or_else(|| {
            let mut p = self.input.clone();
            p.set_extension("cache.json");
            p
        });
        let (seed, seed_was_supplied) = match self.seed {
            Some(s) => (s, true),
            None => {
                let s = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() as u64)
                    .unwrap_or(0);
                (s, false)
            }
        };
        ResolvedArgs {
            input: self.input,
            output: self.output,
            unresolved,
            cache,
            seed,
            seed_was_supplied,
            artist_window: self.artist_window,
            verbose: self.verbose,
            musicbrainz_contact: self.musicbrainz_contact,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_required_args() {
        let args =
            Args::try_parse_from(["playlistize", "--input", "in.csv", "--output", "out.csv"])
                .unwrap();
        let r = args.resolve();
        assert_eq!(r.input, PathBuf::from("in.csv"));
        assert_eq!(r.output, PathBuf::from("out.csv"));
        assert_eq!(r.artist_window, 4);
        assert!(!r.seed_was_supplied);
    }

    #[test]
    fn defaults_unresolved_to_output_dir() {
        let args = Args::try_parse_from([
            "playlistize",
            "--input",
            "in.csv",
            "--output",
            "/tmp/out.csv",
        ])
        .unwrap();
        let r = args.resolve();
        assert_eq!(r.unresolved, PathBuf::from("/tmp/unresolved.csv"));
    }

    #[test]
    fn defaults_cache_to_input_extension() {
        let args = Args::try_parse_from([
            "playlistize",
            "--input",
            "/data/songs.csv",
            "--output",
            "out.csv",
        ])
        .unwrap();
        let r = args.resolve();
        assert_eq!(r.cache, PathBuf::from("/data/songs.cache.json"));
    }

    #[test]
    fn seed_passthrough() {
        let args = Args::try_parse_from([
            "playlistize",
            "--input",
            "in.csv",
            "--output",
            "out.csv",
            "--seed",
            "42",
        ])
        .unwrap();
        let r = args.resolve();
        assert_eq!(r.seed, 42);
        assert!(r.seed_was_supplied);
    }

    #[test]
    fn artist_window_zero_accepted() {
        let args = Args::try_parse_from([
            "playlistize",
            "--input",
            "in.csv",
            "--output",
            "out.csv",
            "--artist-window",
            "0",
        ])
        .unwrap();
        assert_eq!(args.resolve().artist_window, 0);
    }

    #[test]
    fn missing_required_arg_errors() {
        let result = Args::try_parse_from(["playlistize", "--input", "in.csv"]);
        assert!(result.is_err());
    }
}
