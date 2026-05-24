# order_playlist

A small Rust CLI that reorders a CSV playlist to follow a target energy
arc (slow start → peak in the back half → mellow finish). It pulls audio
features from public APIs, runs simulated annealing over the track order
against a cost function (arc deviation, Camelot key compatibility, tempo
continuity, energy continuity, artist spacing), and writes the result
back as CSV.

## Quick start

```bash
cargo build --release

./target/release/order_playlist \
  --input  playlist.csv \
  --output reordered.csv \
  --musicbrainz-contact you@example.com \
  --seed 42
```

The first run is online (MusicBrainz + ReccoBeats); MusicBrainz throttles
to ~1 req/sec so expect ~1 minute per 60 tracks. Subsequent runs read
from `playlist.cache.json` next to the input and are instant + offline.

## Input

A CSV with `title,artist` columns. Header row required.

```csv
title,artist
Get Lucky,Daft Punk
Dancing Queen,ABBA
```

## Outputs

| File | Contents |
|---|---|
| `reordered.csv` | Resolved tracks in the new order, with `position, title, artist, tempo, key, mode, energy, danceability, valence, loudness, isrc` |
| `unresolved.csv` | Tracks that couldn't be resolved, with a `reason` column |
| `<input>.cache.json` | Feature cache. Safe to delete; will be rebuilt |

stdout also gets an ASCII energy chart and a summary:

```
Energy arc
0.9 |                            #   #   # #      #
0.7 |      ## # ##    #    # #   # ###   #####   ##
0.5 |      #### ##### # # ############  ######## ##
0.3 |########## ####################################
    +-----------------------------------------------
     1...

Summary
  resolved:     47
  unresolved:  132
  seed:        42 (supplied)
  total cost:  before 478.414  after 57.430
  artist clashes remaining: 0
```

## Caveats: resolution rate

Track resolution depends on:

1. MusicBrainz having a recording for your `title + artist` query
2. That recording having an attached ISRC
3. ReccoBeats having audio features for that ISRC

In practice, expect **30–60% of a typical pop/hip-hop library to
resolve**. Common reasons for misses:

- Live versions, remasters, deluxe-edition reissues, and feature-credit
  variants often lack ISRCs on MusicBrainz
- ReccoBeats's feature DB doesn't cover every ISRC, even for hits
- MusicBrainz rate-limits aggressively; tracks rejected with
  `musicbrainz error: RateLimit` will retry on the next run (failures
  from rate limiting are not cached)

Stripping qualifiers like `(Live)`, `(Deluxe Edition)`, `(2024 Remaster)`
from your input materially improves match rates.

## Apple Music workflow

Music.app (Mac) → select playlist → **File → Library → Export Playlist…**
→ format **Plain Text**. Convert the tab-separated export to the
`title,artist` CSV:

```bash
tr '\r' '\n' < export.txt | awk -F'\t' '
NR==1 {
  for (i=1;i<=NF;i++) { if ($i=="Name") n=i; if ($i=="Artist") a=i }
  print "title,artist"; next
}
NF >= 2 && $n != "" {
  t=$n; ar=$a
  gsub(/"/,"\"\"",t); gsub(/"/,"\"\"",ar)
  printf "\"%s\",\"%s\"\n", t, ar
}' > playlist.csv
```

To get the reordered playlist back into Music.app, export the original
playlist as **XML** as well, then use a small script to rewrite the
playlist's track-order list using the resolved IDs and import the new
XML via **File → Library → Import Playlist…**.

## Build & test

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                                  # default features
cargo test --no-default-features            # proves core builds without adapters
cargo build --release
```

Live-network tests (real HTTP against MusicBrainz / ReccoBeats) are
gated behind the `live-network` feature:

```bash
cargo test --features live-network,musicbrainz,reccobeats
```

## CLI reference

```
order_playlist --input <CSV> --output <CSV> [options]

  --input <PATH>                Input CSV with title,artist columns
  --output <PATH>               Output CSV (resolved + reordered)
  --unresolved <PATH>           Sidecar CSV [default: unresolved.csv next to output]
  --cache <PATH>                Feature cache JSON [default: <input>.cache.json]
  --seed <U64>                  RNG seed for reproducible runs [default: system time]
  --artist-window <U8>          Window for artist-spacing constraint, 0 disables [default: 4]
  --musicbrainz-contact <STR>   Contact string for MusicBrainz UA [default: anonymous@example.com]
  -v, --verbose                 -v=DEBUG, -vv=TRACE [default: INFO]
```

## Repository layout

```
src/
  domain/     pure types (tracks, Camelot codes, features)
  algo/       pure cost / annealer / arc functions
  adapters/   IO: csv, cache, musicbrainz, reccobeats
  cli/        clap args, ASCII chart, summary report
  run.rs      orchestration entry point (library-visible)
  main.rs     binary entry: tracing + clap + std::process::exit
tests/
  *.rs        integration tests (cache, exit codes, zero-network, etc.)
  fixtures/   small_party.csv and its precomputed cache
docs/
  design-plans/         per-feature design docs
  implementation-plans/ per-feature task plans
  test-plans/           manual test plans
```

Architectural rules (FCIS layering, determinism invariants, exit-code
contract) live in `CLAUDE.md`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 2 | Bad CLI arguments |
| 3 | Input CSV missing or malformed |
| 4 | Cache corrupt or schema mismatch |
| 5 | Nothing resolved from input |
| 6 | Adapter exhausted retries (rate limit, etc.) |

## Status

v1. Single feature set (MusicBrainz + ReccoBeats), CSV in, CSV out. The
codebase is structured so adding a new feature source (Spotify, etc.) is
a new adapter behind a feature flag — `domain/` and `algo/` should not
change.
