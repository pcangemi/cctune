# Changelog

## Unreleased

- `/cctune` in Claude Code: `/cctune next`, `/cctune playlist "Eras Tour"`… runs cctune and shows
  its answer with no round trip to Claude, even mid-turn. A Claude Code plugin in `claude-plugin/`:
  `/plugin install cctune --marketplace pcangemi/cctune`.
- The player could vanish from the status line on a long session: Claude Code cancels a status
  line run still going when the next one starts, and runs waited for the wrapped status line
  (claude-hud took up to 0.8 s). The wrapped command now runs in the background; a run waits for
  it at most 150 ms and takes about 1 ms otherwise.
- `cctune doctor`: for each Claude Code session, what its status line printed last, how long it
  took, and why the player shows or not.
- Names that Music stores HTML-escaped (`R&amp;B/Soul`, often playlists from the web or Shazam)
  show as written (`R&B/Soul`), and `cctune playlist "R&B"` finds them.

## 0.1.0 - 2026-10-17

First release.

- Commands to control the Music app: what is playing, play, pause, next, previous, volume, seek,
  shuffle, repeat, playlists, library search, play a song.
- `cctune mcp`: an MCP server with the same actions as tools, so Claude Code controls Music when
  asked.
- `cctune statusline`: a live player in Claude Code's status line, refreshed every second
  (`refreshInterval: 1`), drawn from Music's last answer and refreshed in the background so it
  never waits; `--wrap` keeps an existing status line; `--compact` removes the spacing.
- macOS only (Apple Silicon and Intel).
