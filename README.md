# party-playlist — starter scaffolding

This directory contains the seed material to start the project under the
`ed3d-plan-and-execute` Claude Code workflow.

## What's here

```
party-playlist/
├── README.md                              ← you are here
├── SEED.md                                ← paste into /start-design-plan
└── .ed3d/
    ├── design-plan-guidance.md            ← auto-loaded during design
    └── implementation-plan-guidance.md    ← auto-loaded during planning + review
```

## How to start

1. Copy this whole directory to wherever you keep projects:
   ```
   cp -r party-playlist ~/code/
   cd ~/code/party-playlist
   git init && git add . && git commit -m "Initial scaffolding"
   ```

2. Initialize the Rust project alongside the existing files:
   ```
   cargo init --name party-playlist
   git add . && git commit -m "cargo init"
   ```

3. Install the ed3d marketplace and plugins (one-time, in Claude Code):
   ```
   /plugin marketplace add https://github.com/ed3dai/ed3d-plugins.git
   /plugin install ed3d-plan-and-execute@ed3d-plugins
   /plugin install ed3d-research-agents@ed3d-plugins
   /plugin install ed3d-house-style@ed3d-plugins
   /plugin install ed3d-extending-claude@ed3d-plugins
   ```

4. From inside this directory, launch Claude Code and start the design phase:
   ```
   claude
   ```
   then inside Claude Code:
   ```
   /start-design-plan
   ```
   When it asks for context, paste the body of `SEED.md`. The
   `.ed3d/design-plan-guidance.md` file loads automatically — you don't need
   to mention it.

5. Follow the workflow as the plugin instructs. Roughly:
   - Design phase produces `docs/design-plans/YYYY-MM-DD-party-playlist.md`.
   - `/clear`, then `/start-implementation-plan @docs/design-plans/...md .`
   - `/clear`, then `/execute-implementation-plan @docs/implementation-plans/YYYY-MM-DD-party-playlist`.

## Prerequisites before you start coding

- A Spotify developer app — register at
  https://developer.spotify.com/dashboard. You need the Client ID and
  Client Secret. Drop them in a `.env` file (which `.gitignore` should
  cover — the implementation plan will set this up).
- A small input CSV. Even 10 hand-typed songs as `data/input.csv` with
  `title,artist` columns works to bootstrap.

## On the Apple Music side

v1 is CSV in, CSV out. To get songs out of Apple Music: highlight tracks
in the desktop Music app, File > Library > Export Playlist (XML or
text-tab format), then convert to CSV. There are also tools like SongShift
and Soor. None are great; this is the friction that motivates the v2
MCP integration.

For v2 (not in scope for the design plan you're about to start), the plan
is to integrate `epheterson/applemusic-mcp` so Claude Code can pull and
push playlists directly. Keep that boundary clean during v1 design — the
algorithm should not assume anything about how tracks arrive.

## Why this structure instead of a single CLAUDE.md

The `ed3d-plan-and-execute` plugin uses a research-plan-implement loop.
Design docs describe what+why at the component level and get committed
to git. Implementation plans are generated fresh against the current
codebase right before execution. Putting standing project rails in
`.ed3d/` keeps them out of the design doc (which is per-feature) and
the implementation plan (which is per-execution).

A CLAUDE.md may end up being useful later for cross-cutting reminders,
but the plugin's `project-claude-librarian` agent will manage it for
you. Don't write one up front.
