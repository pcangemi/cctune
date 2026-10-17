//! cctune: control Apple Music from the terminal and from Claude Code. The same actions are
//! commands (`cctune next`), tools of an MCP server (`cctune mcp`) that Claude calls when asked,
//! and a live player in Claude Code's status line (`cctune statusline`).

mod actions;
mod mcp;
mod music;
mod statusline;

use std::process::ExitCode;

const HELP: &str = "\
cctune: control Apple Music from the terminal and from Claude Code

USAGE:
    cctune [COMMAND]

COMMANDS:
    now                  What is playing (the default)
    play [SONG]          Resume, or play the best match in your library for a song, artist or album
    pause | toggle       Pause, or pause and resume
    next | prev          Next or previous track
    vol [N|+N|-N]        Music's volume, from 0 to 100
    seek N|+N|-N         Jump to a position, in seconds
    shuffle [on|off]     Turn shuffle on or off (or switch it)
    repeat off|one|all   Repeat mode
    playlists            Your playlists
    playlist NAME        Play a playlist, by its name or a part of it
    search QUERY         Find tracks in your library

FOR CLAUDE CODE:
    mcp                  Run as an MCP server, so Claude can control Music when asked:
                           claude mcp add cctune -s user -- cctune mcp
    statusline [--wrap 'COMMAND'] [--compact]
                         Print the player for Claude Code's status line, after the output of
                         COMMAND (your current status line, if you have one), with a blank line
                         above and below it unless --compact. In settings.json:
                           \"statusLine\": {\"type\": \"command\", \"command\": \"cctune statusline\",
                                          \"refreshInterval\": 1}

    doctor               For each session: what the status line printed last, how long it took, and
                         why the player shows or not

    -h, --help           Show this help
    -V, --version        Show the version

macOS only: cctune drives the Music app through AppleScript. The first command makes macOS ask
whether your terminal may control Music. It works with your library and playlists; the Apple
Music catalog cannot be searched this way.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let player = music::AppleMusic;
    let (cmd, rest) = args.split_first().map_or(("now", &[][..]), |(c, r)| (*c, r));
    match cmd {
        "-h" | "--help" | "help" => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        "-V" | "--version" => {
            println!("cctune {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "mcp" => match mcp::serve(&player, std::io::stdin().lock(), std::io::stdout().lock()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("cctune: {e}");
                ExitCode::FAILURE
            }
        },
        "statusline" => {
            // A status line always exits cleanly: a failure would blank Claude Code's line.
            statusline::run(rest);
            ExitCode::SUCCESS
        }
        "doctor" => {
            println!("{}", statusline::doctor());
            ExitCode::SUCCESS
        }
        // Started by the status line, in the background: runs the wrapped status line and keeps
        // its output.
        "statusline-wrap" => {
            statusline::wrap_in_background(rest);
            ExitCode::SUCCESS
        }
        // Started by the status line, in the background: asks Music and keeps the answer.
        "statusline-refresh" => {
            statusline::refresh(&player);
            ExitCode::SUCCESS
        }
        _ => {
            let action = match actions::from_cli(cmd, rest) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("cctune: {e}");
                    return ExitCode::from(2);
                }
            };
            match actions::run(&player, &action) {
                Ok(text) => {
                    if actions::changes_playback(&action) {
                        statusline::forget_now();
                    }
                    println!("{text}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("cctune: {e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}
