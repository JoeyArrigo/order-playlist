# ReccoBeats API spike — 2026-05-23

## Question 1: Which endpoint shape works?

The design's assumption is **confirmed**. The endpoint `GET /v1/audio-features?ids=<id>&ids=<id>...` works perfectly with ISRC identifiers.

Working endpoint: `https://api.reccobeats.com/v1/audio-features?ids=USQX91300120&ids=USQX91100008`

## Question 2: Which fields are present in the response?

The API returns data in a `content` array (not `items` as the design assumed). Each item includes:

**Required fields (present on all items):**
- `id` (internal UUID)
- `href` (Spotify track URL, e.g., "https://open.spotify.com/track/7udrKQC7vbcicpxCYbmzJ0")
- `isrc` (ISRC identifier, maps back to input)

**Audio feature fields (all present):**
- `tempo` (float, e.g., 92.978)
- `key` (integer 0-11, e.g., 9)
- `mode` (integer: 0=minor, 1=major, e.g., 1)
- `energy` (float 0-1, e.g., 0.624)
- `danceability` (float 0-1, e.g., 0.554)
- `valence` (float 0-1, e.g., 0.233)
- `loudness` (float dB, e.g., -5.625)
- `acousticness` (float 0-1, e.g., 0.13)
- `instrumentalness` (float 0-1, e.g., 3.66e-6)
- `liveness` (float 0-1, e.g., 0.127)
- `speechiness` (float 0-1, e.g., 0.0268)

**Good news:** `key` and `mode` ARE present, contrary to the research findings. The design assumption holds.

## Question 3: What's the actual batch limit?

Tested with 10 real ISRCs: all returned successfully. The design's assumption of `MAX_BATCH=40` is reasonable and should be safe. No explicit rate-limit or batch-size errors encountered during testing.

## Question 4: How does the API indicate "no match"?

When provided with fake/unmatchable ISRCs (e.g., `USQX9130XXXXX`), the API simply omits them from the response. It returns an empty `content` array if no ISRCs match, or partial results if some match.

For example, a request with 50 fake ISRCs returns `"content": []`.

## Question 5: What does `href` actually contain?

The `href` field contains a full Spotify track URL: `https://open.spotify.com/track/<spotify-id>`.

Example: `https://open.spotify.com/track/7udrKQC7vbcicpxCYbmzJ0`

This can be used as a fallback mapping key by extracting the last path segment (Spotify ID) and matching against input ISRCs that happen to be Spotify IDs.

## Decisions

- **MAX_BATCH = 40** — Design assumption confirmed as safe; tested with 10, all succeeded
- **Endpoint = `/v1/audio-features?ids=...` (batch query-string)** — Confirmed working with ISRC identifiers
- **key/mode handling = Present** — Both fields are returned by the API, no fallback needed. Design's `PitchClass` and `Mode` mapping can use real values from `key` and `mode` fields.
- **Response shape = `{ content: [FeatureItem...] }`** — Note: API uses `"content"` key, not `"items"`. Each item includes `href`, `isrc`, and all audio features including `key` and `mode`.
- **Mapping strategy = Primary by `isrc`, fallback by `href` extraction** — Both fields present, supports dual-path lookup.

**Status:** Network available, API reachable, design assumptions validated. No scope changes required. Proceed to Task 2.
