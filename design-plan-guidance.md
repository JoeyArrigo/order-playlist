# Design plan guidance for party-playlist

This file is auto-loaded by `/start-design-plan` from `ed3d-plan-and-execute`.
It defines project-wide rails — terminology, tech choices, architectural
constraints, and scope boundaries. Keep it short and durable; it should still
be accurate months from now.

## Domain terminology

- **Track / song** — a single audio item with metadata (title, artist) and
  audio features (tempo, key, mode, energy, danceability, valence).
- **Playlist** — an ordered list of tracks.
- **Ordering** — a specific permutation of a playlist being evaluated.
- **Cost** — scalar quality measure of an ordering; lower is better.
- **Arc** — the target energy curve over playlist position [0, 1].
- **Anchor** — a track pinned to a specific position; annealing must respect it.
- **Camelot code** — DJ notation for musical key (1A–12B), used to compute
  harmonic distance between adjacent tracks.
- **2-swap** — annealing's neighbor operation: exchange two tracks' positions.
- **Delta-cost** — incremental cost change from a 2-swap, computed without
  re-summing the full playlist cost.

## Technology choices (non-negotiable for v1)

- **Language:** Rust (stable, current edition).
- **Async runtime:** Tokio.
- **HTTP:** reqwest.
- **Serialization:** serde + serde_json + csv.
- **Randomness:** rand (with a seedable RNG for determinism).
- **Error handling:** anyhow at the binary boundary, thiserror for library
  errors with structure.
- **CLI:** clap (derive API).
- **Logging:** tracing + tracing-subscriber.
- **Optional later:** rayon (parallel restarts), argmin (only if it's clearly
  cleaner than a 30-line hand-rolled SA loop).

## Architectural constraints

- **Pure-core, side-effect-shell.** The cost function, annealing loop, and
  Camelot math live in pure modules with no I/O. Spotify, CSV, and (later)
  MCP integration are isolated in adapter modules. This makes the algorithm
  trivially testable and lets us swap feature sources without rework.
- **No global state.** Configuration flows through structs passed explicitly.
- **Delta-cost is mandatory.** Any annealing implementation that recomputes
  the full cost per iteration is a bug, not an optimization opportunity.
  This is the single most performance-sensitive design point.
- **Determinism with `--seed`.** Same inputs + same seed = same output, every
  time. Test this explicitly.
- **Cache aggressively.** Resolved Spotify IDs and audio features go in a
  sidecar JSON next to the input CSV. Re-runs should hit zero network.

## Scope boundaries (v1)

In scope:
- CSV in, CSV out.
- Spotify client-credentials auth and track lookup.
- Simulated annealing with delta-cost.
- Camelot wheel distance.
- Energy-arc targeting (configurable curve).
- Artist-spacing hard constraint.
- Anchor song support.
- Deterministic seeded runs.
- Caching of resolved tracks and features.

Out of scope for v1 (acknowledge as future work, do not design around):
- Apple Music MCP integration (v2).
- Local audio analysis (symphonia/aubio).
- Web UI or GUI.
- ML / embedding-based recommendation.
- Real-time crowd-response re-ordering.
- Multi-language support, accessibility features.

## Style preferences

- Functions ≤ 50 lines unless there's a clear reason.
- One pub item per file when the file's purpose is that item (e.g.
  `Annealer` in `anneal.rs`); otherwise group related items.
- Doc comments on every `pub` item.
- Unit tests in the same file under `#[cfg(test)] mod tests`.
- Integration tests in `tests/` for end-to-end CSV-in CSV-out runs.
- No `unwrap()` outside of tests and `main()`.

## External API notes

- **Spotify `/audio-features` may be unavailable** for newly-registered apps
  as of late 2024. The design must include a fallback path (ReccoBeats free
  API or GetSongBPM) selectable at compile time or runtime. Verify which
  endpoint we have access to during the design phase, not after writing the
  client.
- **Rate limits:** Spotify's are generous for client-credentials flow but
  exist; back off on 429.
- **Search ambiguity:** `q=track:X artist:Y` returns multiple candidates.
  v1 picks the highest-popularity match where artist name fuzzy-matches; logs
  the choice. Manual override via cache file lets the user fix mismatches.

## Acceptance-criteria mapping notes

The implementation plan should map each acceptance criterion to either an
automated test (preferred) or a documented manual verification step. The
determinism criterion (`--seed` reproducibility) and the artist-spacing
constraint are both easy to assert in tests — make sure they actually are.
