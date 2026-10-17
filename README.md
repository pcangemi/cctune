# cctune

[![CI](https://github.com/pcangemi/cctune/actions/workflows/ci.yml/badge.svg)](https://github.com/pcangemi/cctune/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/cctune)](https://crates.io/crates/cctune)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Control Apple Music from the terminal and from Claude Code:

- **Ask Claude**: "play my focus playlist", "skip this one", "what's playing?", "volume 30",
  "put on something by Daft Punk". cctune is an MCP server that Claude calls.
- **See it live** in Claude Code's status line, updated every second:

  ```
  ♫ Get Lucky — Daft Punk · Random Access Memories
  ▶ ━━━━━━━━━━━━━╋────────────── 2:14 / 6:09 ⇄ ↻  vol 60%
  ```

- **Type it**: `cctune next`, or `! cctune next` inside Claude Code, with no round trip to Claude.

A single Rust binary of about 500 KB, for macOS only (Apple Silicon and Intel): it drives the
Music app.

## Install

The prebuilt binary:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/pcangemi/cctune/releases/latest/download/cctune-installer.sh | sh
```

With Homebrew:

```sh
brew install pcangemi/tap/cctune
```

With Cargo (Rust 1.85 or newer):

```sh
cargo install cctune
```

Then:

```sh
cctune now
```

The first command makes macOS ask whether your terminal may control Music: allow it. To change
it later: System Settings › Privacy & Security › Automation.

### In Claude Code

Let Claude control Music:

```sh
claude mcp add cctune -s user -- cctune mcp
```

Show the player in the status line, refreshed every second. In `~/.claude/settings.json`:

```json
"statusLine": {
  "type": "command",
  "command": "cctune statusline",
  "refreshInterval": 1
}
```

Already have a status line? Keep it: pass its command to `--wrap`, and cctune prints its output
first, then the player. It runs your command again only when Claude Code's session data changes,
not every second:

```json
"command": "cctune statusline --wrap 'node ~/.claude/plugins/claude-hud/statusline.mjs'"
```

## Commands

| Command | Does |
|---|---|
| `cctune` or `cctune now` | What is playing |
| `cctune play` | Resume |
| `cctune play get lucky` | Play the best match in your library for a song, artist or album |
| `cctune pause`, `cctune toggle` | Pause, or pause and resume |
| `cctune next`, `cctune prev` | Next or previous track |
| `cctune vol 40`, `vol +10`, `vol -10` | Music's volume (not the Mac's) |
| `cctune seek 90`, `seek +30`, `seek -10` | Jump within the track, in seconds |
| `cctune shuffle [on\|off]` | Shuffle (alone, it switches) |
| `cctune repeat off\|one\|all` | Repeat |
| `cctune playlists` | Your playlists |
| `cctune playlist focus` | Play a playlist, by its name or a part of it |
| `cctune search daft punk` | Find tracks in your library |
| `cctune mcp` | The MCP server for Claude Code |
| `cctune statusline [--wrap 'COMMAND'] [--compact]` | The status line; the player has a blank line above and below it, unless `--compact` |
| `cctune doctor` | For each Claude Code session: what its status line printed last, how long it took, and why the player shows or not |
| `cctune --help`, `--version` | Help, version |

The MCP tools: `music_now_playing`, `music_play`, `music_pause`, `music_toggle`, `music_next`,
`music_previous`, `music_set_volume`, `music_shuffle`, `music_repeat`, `music_seek`,
`music_list_playlists`, `music_play_playlist`, `music_search`, `music_play_song`.

## How it works, and what it costs

cctune sends AppleScript to the Music app through `osascript`. Text you give it (a playlist name,
a search) is passed as an argument of the script, never pasted into it, so quotes cannot break it.

Claude Code runs the status line every second, so it stays fast (about 60 ms a tick) and never
waits for Music, which takes 0.3 to 1.5 seconds to answer:

- It draws from Music's last answer and moves the position on with the clock, so the bar moves
  every second.
- When that answer is more than 3 seconds old, it asks Music again in a background process, ready
  for the next ticks. An AppleScript call costs about 44 ms of CPU: about 1.5% CPU while a track
  plays.
- With Music closed, it checks the process list and asks nothing: close to no cost, and the
  player disappears.
- A wrapped status line runs in the background too. Claude Code cancels a status line run still
  going when the next one starts, and a status line like claude-hud can take half a second on a long
  session: a run that waited for it could be cancelled every time, freezing the screen. cctune
  waits at most 150 ms for it, then prints its last output; the new one shows on the next tick.
- A skip or a pause made through cctune shows within a second; one made elsewhere (keyboard keys,
  the Music window) within about 3 seconds.

To do this, it keeps small files in its folder of the system's temporary directory: the last
answer of Music, and for each session the last output of the wrapped status line and what the last
run printed (read by `cctune doctor`).

## Limits

- macOS only, and only the Music app.
- Your library and playlists: AppleScript cannot search or stream the Apple Music catalog.
- Claude Code hides its status line while a permission prompt or the help menu is open.
- The status line refreshes once a second at most: that is Claude Code's minimum.

## Development

[AGENTS.md](AGENTS.md) is the map for anyone (or any agent) changing the code;
`scripts/verify.sh` checks formatting, lints and tests (CI runs it on macOS).

```sh
git clone https://github.com/pcangemi/cctune.git
cd cctune
scripts/verify.sh
```

### Releasing

Releases are built by [dist](https://axodotdev.github.io/cargo-dist/) from a version tag:

1. Set the version in `Cargo.toml`, add its section to `CHANGELOG.md`, commit to `main`.
2. Tag and push: `git tag -a v0.2.0 -m "cctune 0.2.0" && git push origin v0.2.0`. The release
   workflow builds the two Mac archives, the installer and the Homebrew formula (`cctune.rb`).
3. Copy `cctune.rb` from the release into `Formula/` of
   [pcangemi/homebrew-tap](https://github.com/pcangemi/homebrew-tap) and push it.
4. Publish to crates.io from the tag: `cargo publish`.

## License

Copyright (c) 2026 Pierre CANGEMI. Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE)
or [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
cctune by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
