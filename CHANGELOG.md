# Changelog

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
