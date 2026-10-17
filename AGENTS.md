# Working on cctune

cctune controls the Apple Music app from the terminal and from Claude Code. One list of actions
(`src/actions.rs`) is reached three ways: commands (`src/main.rs`), MCP tools (`src/mcp.rs`) and
Claude Code's status line (`src/statusline.rs`). `src/music.rs` is the only file that talks to
Music, through `osascript`.

## Rules

- **User text never goes inside a script.** Names and searches are passed as script arguments
  (`on run argv`). A playlist named `"; do shell script "…` must stay a name.
- **Every script compiles.** AppleScript reserves short words that look like fine names (`st`,
  `nd`, `rd`, `th`): use long, explicit variable names, and add every new script to the test
  `every_script_compiles` (it compiles them with `osacompile` without contacting Music).
- **Nothing starts the Music app by itself.** Reading what plays answers `off` when Music is
  closed; `music_running` checks the process list without sending Music anything.
- **The status line runs every second**: it must stay fast and must not fail (a failure blanks
  Claude Code's line). Ask Music at most every 3 seconds; reuse the wrapped command's output while
  Claude Code's session data is the same.
- **A new action is added once**, in `actions.rs` (`Action`, `from_cli`, `from_tool`, `run`), then
  exposed as a command in `HELP` and as a tool in `mcp::tools`. Update the README tables too.
- One runtime dependency (`serde_json`): the MCP protocol is small enough to speak by hand.

## Commands

| Command | When |
|---|---|
| `scripts/verify.sh` | Before every commit: formatting, clippy, tests. |
| `cargo test` | While working. Tests use a fake player; they never touch Music. |
| `cargo run -- now` | Against the real Music app (macOS asks once for permission). |

On a Mac whose Command Line Tools linker is older than its SDK (`tapi error ... arm64e`), prefix
builds with `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.sdk`.

## Never

- Change the person's music in a test: use the fake player of `src/mcp.rs`.
- Edit `~/.claude/settings.json` or register the MCP server without the person's go.
