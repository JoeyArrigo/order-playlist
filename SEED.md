## Rough idea

I want a Rust CLI that takes a list of songs and computes the best play order
for a house party. It optimizes for an energy arc (warm up, peak, gentle decline),
smooth harmonic and tempo transitions between adjacent tracks, variety (no artist
clustering), and pacing of well-known "banger" moments.

Source library lives on Apple Music. Apple's API does not expose audio features
(tempo, key, energy, danceability) — those come from Spotify's API by matching
on title+artist or ISRC. Playback stays on Apple Music.

## Decisions already made (do not re-litigate in clarification)

- **Language:** Rust.
- **Algorithm family:** simulated annealing over a weighted cost function.
  Considered and rejected for v1: exact TSP (too slow at n=80), greedy with
  lookahead (worse local optima), RL (overkill, no training data).
- **Feature source:** Spotify Web API `/audio-features` endpoint, accessed via
  client-credentials flow (no user login needed for public catalog reads).
- **Playback target:** Apple Music. v1 ships a CSV in/out. v2 integrates with
  the `epheterson/applemusic-mcp` MCP server (or a fork) for end-to-end flow.
- **Cost function shape:** sum of pairwise costs (tempo delta, Camelot wheel
  key distance, energy jump, same-artist-within-N penalty) plus per-position
  arc cost (squared deviation from a target energy curve).
- **Camelot wheel:** encoded as a 24×24 lookup table for harmonic distance.
- **Delta-cost optimization:** a 2-swap only mutates 4 pairwise terms and 2
  arc terms — don't recompute the whole sum each annealing step. This is
  required for performance at n=80.

## Constraints

- v1 must be runnable end-to-end (Apple Music CSV → reordered CSV) within a
  weekend of work. Optimize for time-to-working, not feature completeness.
- No Apple Developer account ($99/yr) for v1. Spotify-only auth.
- Cache resolved Spotify track IDs and features in a sidecar JSON to make
  reruns fast and avoid API quota churn.
- Hard constraint during annealing: no two songs by the same artist within
  N positions (N=4 default, configurable).
- Optional: anchor songs fixed at specific positions for known peak moments.

## Acceptance criteria for v1 (Definition of Done)

1. Given `data/input.csv` with `title,artist` columns, produce `data/output.csv`
   with the same songs in optimized order plus their resolved features.
2. End-to-end run on a 40-song playlist completes in under 30 seconds on a
   modern laptop.
3. The reordered playlist visibly follows the target energy arc when its
   energy values are plotted (an ASCII chart in the CLI output is fine).
4. No two adjacent songs share an artist; no two songs by the same artist
   within 4 positions.
5. Unresolved tracks (Spotify match failed) are logged with title+artist and
   skipped without crashing.
6. Deterministic runs: a `--seed` flag produces identical output across runs.

## URLs that may be relevant

- Spotify Web API docs: https://developer.spotify.com/documentation/web-api
- Spotify `/audio-features` endpoint reference (note: deprecated for *new*
  apps as of late 2024; verify access before relying on it. Fallback: the
  ReccoBeats free API, or GetSongBPM)
- `epheterson/applemusic-mcp` (for v2 integration): https://github.com/epheterson/applemusic-mcp
- Camelot wheel reference (any DJ harmonic-mixing guide)

## Open questions worth raising during /start-design-plan clarification

- How are anchor songs specified — CLI flag, sidecar file, or marker in
  the input CSV?
- Should the cost function weights be CLI flags, a config file, or both?
- What happens when Spotify search returns multiple plausible matches —
  fail loud, pick the most popular, or prompt interactively?
- Should v1 just write the reordered CSV, or also emit a small report
  (resolved/unresolved counts, before/after arc, total cost)?
