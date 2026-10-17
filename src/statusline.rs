//! `cctune statusline`: the lines Claude Code shows under its prompt. With `refreshInterval: 1`
//! Claude Code runs this every second, so it stays cheap:
//!
//! - It never waits for Music, which takes 0.3 to 1.5 seconds to answer: it draws from the last
//!   answer, moving the position on with the clock, and when that answer is older than 3 seconds
//!   it starts `cctune statusline-refresh` in the background to ask again for the next ticks.
//! - Music closed: nothing is asked of it (`music_running` checks the process list) and the
//!   player disappears.
//! - Any cctune action marks the last answer stale, so a skip or a pause shows within a second.
//! - `--wrap '<command>'` keeps the status line already in place (claude-hud, for example): its
//!   output comes first, and it runs again only when Claude Code's session data changes, not on
//!   every tick.
//!
//! cctune keeps these two answers in its folder of the system's temporary directory.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::actions::clock;
use crate::music::{Now, Player, Repeat, State};

/// How old the last answer of Music may be before it is asked again.
const ASK_EVERY_MS: i64 = 3000;
/// An answer older than this is not shown any more.
const SHOW_FOR_MS: i64 = 30_000;
/// A background question that has not answered after this long is asked again.
const GIVE_UP_MS: i64 = 5000;

fn cache_dir() -> PathBuf {
    // Tests get their own folder, so running them never disturbs the live status line.
    if cfg!(test) { std::env::temp_dir().join(format!("cctune-test-{}", std::process::id())) } else { std::env::temp_dir().join("cctune") }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Makes the next status line ask Music again: something was just changed. The last answer stays
/// on screen until the new one arrives.
pub fn forget_now() {
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(cache_dir().join("stale"), "");
}

/// Keeps Music's answer for the next ticks.
fn remember(n: &Now, at: i64) {
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(cache_dir().join("now"), format!("{at}\t{}", to_line(n)));
    let _ = std::fs::remove_file(cache_dir().join("stale"));
    let _ = std::fs::remove_file(cache_dir().join("asking"));
}

/// `cctune statusline-refresh`, started in the background: asks Music and keeps the answer.
pub fn refresh(player: &dyn Player) {
    let at = now_ms();
    match player.now() {
        Ok(n) => remember(&n, at),
        Err(_) => {
            let _ = std::fs::remove_file(cache_dir().join("asking"));
        }
    }
}

/// Starts a background question to Music, unless one is already under way.
fn ask_in_background(at: i64) {
    let asking = cache_dir().join("asking");
    let since = std::fs::read_to_string(&asking).ok().and_then(|t| t.trim().parse::<i64>().ok());
    if since.is_some_and(|t| (0..GIVE_UP_MS).contains(&(at - t))) {
        return;
    }
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(&asking, at.to_string());
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = Command::new(exe);
    cmd.arg("statusline-refresh").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Its own process group: it outlives this status line run, even when Claude Code cancels it.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let _ = cmd.spawn();
}

fn to_line(n: &Now) -> String {
    let state = match n.state {
        State::Off => return "off".into(),
        State::Stopped => return "stopped".into(),
        State::Playing => "playing",
        State::Paused => "paused",
    };
    let repeat = match n.repeat {
        Repeat::Off => "off",
        Repeat::One => "one",
        Repeat::All => "all",
    };
    // Tabs and line breaks inside names would break the record; they never matter on screen.
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    format!("{state}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{repeat}", clean(&n.name), clean(&n.artist), clean(&n.album), n.duration, n.position, n.volume, n.shuffle)
}

/// What to show from the last answer, moved on in time, and whether Music should be asked again.
fn current(at: i64, running: bool) -> (Option<Now>, bool) {
    if !running {
        let _ = std::fs::remove_file(cache_dir().join("now"));
        return (None, false);
    }
    let Some((when, line)) = std::fs::read_to_string(cache_dir().join("now")).ok().and_then(|t| {
        let (w, l) = t.split_once('\t')?;
        Some((w.parse::<i64>().ok()?, l.to_string()))
    }) else {
        return (None, true);
    };
    let age = (at - when).max(0);
    let mut stale = age >= ASK_EVERY_MS || cache_dir().join("stale").exists();
    let mut n = Now::parse(&line);
    if n.state == State::Playing {
        n.position += (age / 1000) as u32;
        // Past the end, the next track has started: hold at the end until Music says which.
        if n.duration > 0 && n.position >= n.duration {
            n.position = n.duration;
            stale = true;
        }
    }
    ((age < SHOW_FOR_MS).then_some(n), stale)
}

/// Cuts text to `max` characters, with an ellipsis when cut.
fn fit(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";

/// The two lines of the player, in ANSI colors, fitting `cols` columns:
///
/// ```text
/// ♫ Get Lucky — Daft Punk · Random Access Memories
/// ▶ ━━━━━━━━━━━━━╋────────────── 2:14 / 6:09  ⇄ ↻  vol 60%
/// ```
pub fn render(n: &Now, cols: usize) -> Vec<String> {
    if !matches!(n.state, State::Playing | State::Paused) {
        return Vec::new();
    }
    let cols = cols.max(20);
    let title = fit(&n.name, cols.saturating_sub(4));
    let rest = format!(" — {}{}", n.artist, if n.album.is_empty() { String::new() } else { format!(" · {}", n.album) });
    let rest = fit(&rest, cols.saturating_sub(2 + title.chars().count()));
    let line1 = format!("{CYAN}♫{RESET} {BOLD}{title}{RESET}{DIM}{rest}{RESET}");

    let icon = if n.state == State::Playing { format!("{GREEN}▶{RESET}") } else { format!("{DIM}‖{RESET}") };
    let pos = if n.duration > 0 { n.position.min(n.duration) } else { n.position };
    let mut tail = format!(" {} / {}", clock(pos), clock(n.duration));
    let mut marks = String::new();
    if n.shuffle {
        marks.push_str(" ⇄");
    }
    match n.repeat {
        Repeat::All => marks.push_str(" ↻"),
        Repeat::One => marks.push_str(" ↻1"),
        Repeat::Off => {}
    }
    marks.push_str(&format!("  vol {}%", n.volume));
    if cols >= tail.chars().count() + marks.chars().count() + 14 {
        tail.push_str(&marks);
    }
    let width = cols.saturating_sub(2 + tail.chars().count()).clamp(6, 40);
    let done = if n.duration == 0 { 0 } else { ((n.position.min(n.duration) as usize * width) / n.duration as usize).min(width - 1) };
    let color = if n.state == State::Playing { GREEN } else { DIM };
    // The knob is a box-drawing cross, like the line: the terminal draws it centered on the line,
    // where a font's `●` sits off it.
    let bar = format!("{color}{}{BOLD}╋{RESET}{DIM}{}{RESET}", "━".repeat(done), "─".repeat(width - 1 - done));
    vec![line1, format!("{icon} {bar}{DIM}{tail}{RESET}")]
}

/// Claude Code's session data, without what changes on every tick, so that an unchanged session
/// gives the same key.
fn steady(input: &str) -> String {
    let Ok(mut v) = serde_json::from_str::<Value>(input) else { return input.to_string() };
    if let Some(cost) = v.get_mut("cost").and_then(Value::as_object_mut) {
        cost.remove("total_duration_ms");
    }
    v.to_string()
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// How long a status line run waits for the wrapped command before it prints that command's last
/// output instead. Claude Code cancels a run still going when the next one starts (about every
/// second), and a status line like claude-hud takes 0.5 s or more on a long session: a run that
/// waited for it could be cancelled every time, and the screen would stay frozen.
const WRAP_WAIT_MS: i64 = 150;

/// The wrapped status line's output: the last one while the session has not changed; else the
/// command runs again in the background with Claude Code's data on its input, and this run takes
/// its output if it comes within `WRAP_WAIT_MS`, or the last one.
fn wrapped(command: &str, input: &str) -> String {
    let session = serde_json::from_str::<Value>(input).ok().and_then(|v| v.get("session_id").and_then(Value::as_str).map(str::to_string));
    let name = session.as_deref().unwrap_or("none").replace(['/', '\\', '.'], "_");
    let path = cache_dir().join(format!("wrapped-{name}"));
    let key = format!("{:016x}", fnv(&format!("{command}\n{}", steady(input))));
    let cached = || -> (Option<String>, String) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let (k, out) = text.split_once('\n').unwrap_or((text.as_str(), ""));
        (Some(k.to_string()).filter(|k| !k.is_empty()), out.to_string())
    };
    let (had, last) = cached();
    if had.as_deref() == Some(key.as_str()) {
        return last;
    }
    start_wrap(command, input, &path, &key, &cache_dir().join(format!("wrapping-{name}")));
    let until = now_ms() + WRAP_WAIT_MS;
    while now_ms() < until {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let (k, out) = cached();
        if k.as_deref() == Some(key.as_str()) {
            return out;
        }
    }
    last
}

/// Runs the wrapped command in the background, once at a time per session: it outlives this run,
/// so its output is ready for the next one even when Claude Code cancels this one.
fn start_wrap(command: &str, input: &str, path: &std::path::Path, key: &str, busy: &std::path::Path) {
    let at = now_ms();
    let since = std::fs::read_to_string(busy).ok().and_then(|t| t.trim().parse::<i64>().ok());
    if since.is_some_and(|t| (0..GIVE_UP_MS).contains(&(at - t))) {
        return;
    }
    let _ = std::fs::create_dir_all(cache_dir());
    let _ = std::fs::write(busy, at.to_string());
    if cfg!(test) {
        // The test binary has no `statusline-wrap` command: a thread stands for the process.
        let (command, input, path, key, busy) = (command.to_string(), input.to_string(), path.to_path_buf(), key.to_string(), busy.to_path_buf());
        std::thread::spawn(move || wrap_once(&command, &input, &path, &key, &busy));
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let mut cmd = Command::new(exe);
    cmd.arg("statusline-wrap").arg(path).arg(key).arg(busy).arg(command);
    cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    if let Ok(mut child) = cmd.spawn()
        && let Some(mut stdin) = child.stdin.take()
    {
        let _ = stdin.write_all(input.as_bytes());
    }
}

/// `cctune statusline-wrap PATH KEY BUSY COMMAND`, started by `start_wrap` with Claude Code's
/// data on its input.
pub fn wrap_in_background(args: &[&str]) {
    let [path, key, busy, command] = args else { return };
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    wrap_once(command, &input, std::path::Path::new(path), key, std::path::Path::new(busy));
}

/// Runs the wrapped command and keeps its output under `key`, written whole (a run never reads
/// half of it).
fn wrap_once(command: &str, input: &str, path: &std::path::Path, key: &str, busy: &std::path::Path) {
    let child = Command::new("/bin/sh").arg("-c").arg(command).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    if let Ok(mut child) = child {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes());
        }
        if let Ok(out) = child.wait_with_output()
            && out.status.success()
        {
            let text = String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string();
            let part = path.with_extension(format!("part-{}", std::process::id()));
            if std::fs::write(&part, format!("{key}\n{text}")).is_ok() {
                let _ = std::fs::rename(&part, path);
            }
        }
    }
    let _ = std::fs::remove_file(busy);
}

/// Prints the status line: the wrapped one first, then the player.
/// A line that looks empty: Claude Code drops empty lines, and trims spaces, but keeps the Braille
/// blank, which shows as nothing.
const SPACER: &str = "\u{2800}";

/// The wrapped status line, then the player, with a blank line above and below it unless
/// `compact`.
fn compose(wrapped: &str, player: &[String], compact: bool) -> String {
    let mut lines: Vec<&str> = Vec::new();
    if !wrapped.is_empty() {
        lines.push(wrapped);
    }
    if !player.is_empty() {
        if !compact {
            lines.push(SPACER);
        }
        lines.extend(player.iter().map(String::as_str));
        if !compact {
            lines.push(SPACER);
        }
    }
    lines.join("\n")
}

pub fn run(args: &[&str]) {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let wrap = args.iter().position(|a| *a == "--wrap").and_then(|i| args.get(i + 1)).copied();
    let compact = args.contains(&"--compact");
    let cols = std::env::var("COLUMNS").ok().and_then(|c| c.parse().ok()).unwrap_or(80usize);
    let started = now_ms();
    let hud = wrap.map(|cmd| wrapped(cmd, &input)).unwrap_or_default();
    let at = now_ms();
    let running = crate::music::music_running();
    let (now, stale) = current(at, running);
    if stale {
        ask_in_background(at);
    }
    let why = match &now {
        Some(n) => format!("player shown ({:?} at {})", n.state, clock(n.position)),
        None if !running => "no player: Music is not running".to_string(),
        None => "no player: no recent answer from Music yet".to_string(),
    };
    let player = now.map(|n| render(&n, cols)).unwrap_or_default();
    let out = compose(&hud, &player, compact);
    println!("{out}");
    // What the last run printed and why, per session, to explain what Claude Code shows
    // (`cctune doctor`).
    let session = serde_json::from_str::<Value>(&input).ok().and_then(|v| v.get("session_id").and_then(Value::as_str).map(str::to_string));
    let _ = std::fs::write(
        cache_dir().join(format!("last-run-{}", session.as_deref().unwrap_or("none").replace(['/', '\\', '.'], "_"))),
        format!("at {at}\ntook {} ms\ncolumns {cols}\nmusic running {running}\nasked Music again {stale}\n{why}\n--- printed:\n{out}\n", now_ms() - started),
    );
}

/// `cctune doctor`: for each Claude Code session, what its status line printed last, when, and
/// why the player was shown or not. The most recent first.
pub fn doctor() -> String {
    let at = now_ms();
    let mut runs: Vec<(i64, String, String)> = std::fs::read_dir(cache_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let session = name.strip_prefix("last-run-")?.to_string();
            let text = std::fs::read_to_string(e.path()).ok()?;
            let when = text.lines().next()?.strip_prefix("at ")?.parse::<i64>().ok()?;
            Some((when, session, text))
        })
        .collect();
    if runs.is_empty() {
        return "the status line has not run yet (is it set in ~/.claude/settings.json?)".to_string();
    }
    runs.sort_by_key(|r| std::cmp::Reverse(r.0));
    runs.iter()
        .map(|(when, session, text)| {
            let ago = (at - when) as f64 / 1000.0;
            let body: Vec<&str> = text.lines().skip(1).collect();
            format!("session {session}: last run {ago:.1} s ago\n{}", body.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(s: &str) -> String {
        let mut out = String::new();
        let mut esc = false;
        for c in s.chars() {
            match (esc, c) {
                (false, '\x1b') => esc = true,
                (true, 'm') => esc = false,
                (true, _) => {}
                (false, c) => out.push(c),
            }
        }
        out
    }

    fn playing() -> Now {
        Now::parse("playing\tGet Lucky\tDaft Punk\tRandom Access Memories\t369\t134\t60\ttrue\tall")
    }

    #[test]
    fn draws_the_player_in_two_lines() {
        let lines: Vec<String> = render(&playing(), 80).iter().map(|l| strip(l)).collect();
        assert_eq!(lines[0], "♫ Get Lucky — Daft Punk · Random Access Memories");
        assert!(lines[1].starts_with("▶ ━"), "{}", lines[1]);
        assert!(lines[1].contains("╋") && lines[1].ends_with("2:14 / 6:09 ⇄ ↻  vol 60%"), "{}", lines[1]);
        for cols in [20, 40, 80, 200] {
            for l in render(&playing(), cols) {
                assert!(strip(&l).chars().count() <= cols.max(20), "{cols}: {}", strip(&l));
            }
        }
        assert!(render(&Now::idle(State::Off), 80).is_empty(), "nothing when Music is closed");
    }

    #[test]
    fn draws_from_the_last_answer_and_says_when_to_ask_again() {
        // The cache folder is shared: one test holds all the cases, in order.
        let at = 1_790_000_000_000;
        remember(&playing(), at);
        let (n, stale) = current(at + 2_400, true);
        assert_eq!(n.as_ref().map(|n| n.position), Some(136), "the position moves on with the clock");
        assert!(!stale);
        forget_now();
        let (n, stale) = current(at + 2_400, true);
        assert!(n.is_some() && stale, "after an action: shown, and asked again");
        remember(&playing(), at);
        let (n, stale) = current(at + 10_000, true);
        assert!(n.is_some() && stale, "an old answer is shown while Music is asked");
        let (n, _) = current(at + 300_000, true);
        assert!(n.is_none(), "a very old answer is not shown");
        let (n, stale) = current(at + 1_000, false);
        assert!(n.is_none() && !stale, "Music closed: nothing shown, nothing asked");
    }

    #[test]
    fn the_player_gets_room_above_and_below() {
        let player = vec!["♫ A — B".to_string(), "▶ ━╋── 0:10 / 1:00".to_string()];
        assert_eq!(compose("HUD", &player, false), "HUD\n\u{2800}\n♫ A — B\n▶ ━╋── 0:10 / 1:00\n\u{2800}");
        assert_eq!(compose("HUD", &player, true), "HUD\n♫ A — B\n▶ ━╋── 0:10 / 1:00");
        assert_eq!(compose("HUD", &[], false), "HUD", "no player, no room for it");
        assert!(!SPACER.trim().is_empty(), "the spacer survives trimming");
    }

    #[test]
    fn a_record_reads_back_the_same() {
        let n = playing();
        assert_eq!(Now::parse(&to_line(&n)), n);
    }

    #[test]
    fn the_wrapped_line_keeps_its_key_across_ticks() {
        let a = r#"{"session_id":"s","cost":{"total_duration_ms":1000,"total_cost_usd":1.5}}"#;
        let b = r#"{"session_id":"s","cost":{"total_duration_ms":2000,"total_cost_usd":1.5}}"#;
        let c = r#"{"session_id":"s","cost":{"total_duration_ms":2000,"total_cost_usd":1.6}}"#;
        assert_eq!(steady(a), steady(b), "a tick changes nothing");
        assert_ne!(steady(b), steady(c), "new session data does");
    }

    #[test]
    fn the_wrapped_line_runs_again_only_when_the_session_changes() {
        // The command counts its runs in a file, so the test sees when it ran.
        let dir = std::env::temp_dir().join(format!("cctune-wrap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let count = dir.join("runs");
        let cmd = format!("cat > /dev/null; echo x >> '{}'; echo HUD", count.display());
        let runs = || std::fs::read_to_string(&count).map_or(0, |s| s.lines().count());
        let session = format!("test-{}", std::process::id());
        let tick = |ms: u32, cost: f64| format!(r#"{{"session_id":"{session}","cost":{{"total_duration_ms":{ms},"total_cost_usd":{cost}}}}}"#);
        assert_eq!(wrapped(&cmd, &tick(1000, 1.0)), "HUD");
        assert_eq!(wrapped(&cmd, &tick(2000, 1.0)), "HUD");
        assert_eq!(runs(), 1, "a tick reuses the last output");
        assert_eq!(wrapped(&cmd, &tick(3000, 2.0)), "HUD");
        assert_eq!(runs(), 2, "new session data runs it again");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(cache_dir().join(format!("wrapped-{session}")));
    }

    #[test]
    fn a_slow_wrapped_line_never_holds_the_status_line() {
        // Claude Code cancels a run still going after about a second: a slow wrapped command must
        // not hold the run, and its output shows on a later run.
        let session = format!("slow-{}", std::process::id());
        let input = format!(r#"{{"session_id":"{session}"}}"#);
        let cmd = "cat > /dev/null; sleep 0.6; echo SLOW";
        let at = now_ms();
        assert_eq!(wrapped(cmd, &input), "", "nothing yet, and no waiting for it");
        assert!(now_ms() - at < 400, "the run did not wait for the command");
        let mut later = String::new();
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            later = wrapped(cmd, &input);
            if !later.is_empty() {
                break;
            }
        }
        assert_eq!(later, "SLOW", "a later run shows the output");
        let _ = std::fs::remove_file(cache_dir().join(format!("wrapped-{session}")));
    }
}
