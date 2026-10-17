# Working on cctune

cctune controls the Apple Music app from the terminal and from Claude Code. One list of actions
(`src/actions.rs`) is reached three ways: commands (`src/main.rs`), MCP tools (`src/mcp.rs`) and
Claude Code's status line (`src/statusline.rs`). `src/music.rs` is the only file that talks to
Music, through `osascript`. `claude-plugin/` is the `/cctune` slash command, a Claude Code plugin in
TypeScript that runs the `cctune` program with the words typed after it; `.claude-plugin/
marketplace.json` lists it, so people install it with `/plugin install cctune --marketplace
pcangemi/cctune`.

## Rules

- **User text never goes inside a script.** Names and searches are passed as script arguments
  (`on run argv`). A playlist named `"; do shell script "…` must stay a name.
- **Every script compiles.** AppleScript reserves short words that look like fine names (`st`,
  `nd`, `rd`, `th`): use long, explicit variable names, and add every new script to the test
  `every_script_compiles` (it compiles them with `osacompile` without contacting Music).
- **Nothing starts the Music app by itself.** Reading what plays answers `off` when Music is
  closed; `music_running` checks the process list without sending Music anything.
- **The status line runs every second**: it must stay fast and must not fail (a failure blanks
  Claude Code's line). Claude Code cancels a run still going when the next one starts, so a run
  never waits for Music, and waits at most `WRAP_WAIT_MS` for the wrapped command: both run in the
  background. Ask Music at most every 3 seconds; reuse the wrapped command's output while Claude
  Code's session data is the same. `cctune doctor` shows what each session's last run printed.
- **A new action is added once**, in `actions.rs` (`Action`, `from_cli`, `from_tool`, `run`), then
  exposed as a command in `HELP` and as a tool in `mcp::tools`. Update the README tables too.
- One runtime dependency (`serde_json`): the MCP protocol is small enough to speak by hand.
- **The plugin stays a thin pass-through**: it runs `cctune` and shows its answer, so a new action
  needs no change there. Its tests answer for cctune with a fake; they never run it.

## Commands

| Command | When |
|---|---|
| `scripts/verify.sh` | Before every commit: formatting, clippy, tests. |
| `cargo test` | While working. Tests use a fake player; they never touch Music. |
| `cargo run -- now` | Against the real Music app (macOS asks once for permission). |
| `claude plugin test claude-plugin` | The `/cctune` plugin's tests (also in `scripts/verify.sh`). |
| `claude --plugin-dir claude-plugin` | A Claude Code session with the plugin loaded from this folder. |

On a Mac whose Command Line Tools linker is older than its SDK (`tapi error ... arm64e`), prefix
builds with `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.sdk`.

## Never

- Change the person's music in a test: use the fake player of `src/mcp.rs`.
- Edit `~/.claude/settings.json` or register the MCP server without the person's go.
