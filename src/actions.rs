//! What cctune can do, in one place: the commands (`cctune next`) and the MCP tools ("skip this
//! song", said to Claude) both become an `Action`, run here, and answer with the same text.

use serde_json::Value;

use crate::music::{Error, Now, Player, Repeat, State};

#[derive(Clone, PartialEq, Debug)]
pub enum Action {
    Now,
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Volume(Change),
    Shuffle(Option<bool>),
    Repeat(Repeat),
    Seek(Change),
    Playlists,
    Playlist(String),
    Search(String, usize),
    Song(String),
}

/// A level set outright, or moved up or down from where it is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Change {
    To(u32),
    Up(u32),
    Down(u32),
}

impl Change {
    fn parse(s: &str) -> Option<Change> {
        let n = |t: &str| t.trim_end_matches('%').trim_end_matches('s').parse::<u32>().ok();
        match s.chars().next()? {
            '+' => n(&s[1..]).map(Change::Up),
            '-' => n(&s[1..]).map(Change::Down),
            _ => n(s).map(Change::To),
        }
    }

    fn apply(self, from: u32, max: u32) -> u32 {
        match self {
            Change::To(v) => v,
            Change::Up(d) => from.saturating_add(d),
            Change::Down(d) => from.saturating_sub(d),
        }
        .min(max)
    }
}

pub const SEARCH_LIMIT: usize = 10;

/// `cctune <command> [args]`.
pub fn from_cli(cmd: &str, rest: &[&str]) -> Result<Action, String> {
    let text = rest.join(" ");
    let need = |what: &str| if text.trim().is_empty() { Err(format!("`cctune {cmd}` needs {what}")) } else { Ok(text.trim().to_string()) };
    Ok(match cmd {
        "now" | "status" => Action::Now,
        "play" if rest.is_empty() => Action::Play,
        "play" => Action::Song(text),
        "pause" => Action::Pause,
        "toggle" | "pp" => Action::Toggle,
        "next" | "skip" => Action::Next,
        "prev" | "previous" | "back" => Action::Previous,
        "vol" | "volume" => match rest.first() {
            None => Action::Now,
            Some(v) => Action::Volume(Change::parse(v).ok_or("volume is a number from 0 to 100, or +10 / -10")?),
        },
        "shuffle" => Action::Shuffle(match rest.first().copied() {
            None => None,
            Some("on") => Some(true),
            Some("off") => Some(false),
            Some(_) => return Err("shuffle takes on or off (or nothing, to switch it)".into()),
        }),
        "repeat" => Action::Repeat(rest.first().and_then(|r| Repeat::parse(r)).ok_or("repeat takes off, one or all")?),
        "seek" => Action::Seek(rest.first().and_then(|s| Change::parse(s)).ok_or("seek takes seconds: 90, +30 or -10")?),
        "playlists" => Action::Playlists,
        "playlist" => Action::Playlist(need("a playlist name")?),
        "search" | "find" => Action::Search(need("something to search")?, SEARCH_LIMIT),
        "song" => Action::Song(need("a song, an artist or an album")?),
        other => return Err(format!("unknown command `{other}` (see cctune --help)")),
    })
}

/// An MCP tool call: its name and arguments.
pub fn from_tool(name: &str, args: &Value) -> Result<Action, String> {
    let s = |k: &str| args.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let n = |k: &str| args.get(k).and_then(Value::as_f64);
    Ok(match name {
        "music_now_playing" => Action::Now,
        "music_play" => Action::Play,
        "music_pause" => Action::Pause,
        "music_toggle" => Action::Toggle,
        "music_next" => Action::Next,
        "music_previous" => Action::Previous,
        "music_set_volume" => Action::Volume(Change::To(n("level").ok_or("level (0 to 100) is required")?.clamp(0.0, 100.0) as u32)),
        "music_shuffle" => Action::Shuffle(args.get("on").and_then(Value::as_bool)),
        "music_repeat" => Action::Repeat(s("mode").and_then(|m| Repeat::parse(&m)).ok_or("mode is off, one or all")?),
        "music_seek" => Action::Seek(Change::To(n("seconds").ok_or("seconds is required")?.max(0.0) as u32)),
        "music_list_playlists" => Action::Playlists,
        "music_play_playlist" => Action::Playlist(s("name").ok_or("name is required")?),
        "music_search" => Action::Search(s("query").ok_or("query is required")?, n("limit").map_or(SEARCH_LIMIT, |l| l.clamp(1.0, 50.0) as usize)),
        "music_play_song" => Action::Song(s("query").ok_or("query is required")?),
        other => return Err(format!("unknown tool {other}")),
    })
}

pub fn clock(s: u32) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

/// One line about what plays: `▶ Get Lucky — Daft Punk · Random Access Memories  2:14 / 6:09`.
pub fn describe(now: &Now) -> String {
    match now.state {
        State::Off => "Music is not running".into(),
        State::Stopped => "nothing is playing".into(),
        State::Playing | State::Paused => {
            let icon = if now.state == State::Playing { "▶" } else { "‖ paused:" };
            let mut out = format!("{icon} {} — {}", now.name, now.artist);
            if !now.album.is_empty() {
                out.push_str(&format!(" · {}", now.album));
            }
            out.push_str(&format!("  {} / {}  · volume {}%", clock(now.position), clock(now.duration), now.volume));
            if now.shuffle {
                out.push_str(" · shuffle");
            }
            match now.repeat {
                Repeat::One => out.push_str(" · repeat one"),
                Repeat::All => out.push_str(" · repeat all"),
                Repeat::Off => {}
            }
            out
        }
    }
}

/// Runs an action and says what happened.
pub fn run(player: &dyn Player, action: &Action) -> Result<String, Error> {
    let after = |player: &dyn Player| player.now().map(|n| describe(&n));
    match action {
        Action::Now => after(player),
        Action::Play => player.play().and_then(|_| after(player)),
        Action::Pause => player.pause().map(|_| "paused".into()),
        Action::Toggle => player.toggle().and_then(|_| after(player)),
        Action::Next => player.next().and_then(|_| after(player)),
        Action::Previous => player.previous().and_then(|_| after(player)),
        Action::Volume(c) => {
            let level = c.apply(player.now()?.volume as u32, 100) as u8;
            player.set_volume(level).map(|_| format!("volume {level}%"))
        }
        Action::Shuffle(on) => {
            let on = match on {
                Some(v) => *v,
                None => !player.now()?.shuffle,
            };
            player.set_shuffle(on).map(|_| format!("shuffle {}", if on { "on" } else { "off" }))
        }
        Action::Repeat(mode) => player.set_repeat(*mode).map(|_| format!("repeat {mode:?}").to_lowercase()),
        Action::Seek(c) => {
            let now = player.now()?;
            let to = c.apply(now.position, now.duration.max(1));
            player.seek(to).map(|_| format!("at {} / {}", clock(to), clock(now.duration)))
        }
        Action::Playlists => {
            let lists = player.playlists()?;
            if lists.is_empty() {
                return Ok("no playlist in your library".into());
            }
            Ok(lists.iter().map(|p| format!("{}  ({} tracks)", p.name, p.tracks)).collect::<Vec<_>>().join("\n"))
        }
        Action::Playlist(name) => player.play_playlist(name).map(|n| format!("playing the playlist {n}")),
        Action::Search(q, limit) => {
            let found = player.search(q, *limit)?;
            if found.is_empty() {
                return Ok(crate::music::no_match(q));
            }
            Ok(found.iter().map(|t| format!("{} — {} · {} ({})", t.name, t.artist, t.album, clock(t.duration))).collect::<Vec<_>>().join("\n"))
        }
        Action::Song(q) => player.play_song(q).map(|t| format!("▶ {} — {} · {}", t.name, t.artist, t.album)),
    }
}

/// Whether an action changes what plays (so a cached now-playing must be read again).
pub fn changes_playback(action: &Action) -> bool {
    !matches!(action, Action::Now | Action::Playlists | Action::Search(..))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_commands() {
        assert_eq!(from_cli("next", &[]), Ok(Action::Next));
        assert_eq!(from_cli("vol", &["+10"]), Ok(Action::Volume(Change::Up(10))));
        assert_eq!(from_cli("vol", &["35%"]), Ok(Action::Volume(Change::To(35))));
        assert_eq!(from_cli("seek", &["-15"]), Ok(Action::Seek(Change::Down(15))));
        assert_eq!(from_cli("playlist", &["Deep", "Focus"]), Ok(Action::Playlist("Deep Focus".into())));
        assert_eq!(from_cli("play", &["get", "lucky"]), Ok(Action::Song("get lucky".into())));
        assert_eq!(from_cli("shuffle", &[]), Ok(Action::Shuffle(None)));
        assert!(from_cli("repeat", &["sometimes"]).is_err());
        assert!(from_cli("playlist", &[]).is_err());
        assert!(from_cli("dance", &[]).is_err());
    }

    #[test]
    fn levels_stay_in_range() {
        assert_eq!(Change::Up(30).apply(90, 100), 100);
        assert_eq!(Change::Down(30).apply(10, 100), 0);
        assert_eq!(Change::To(250).apply(0, 100), 100);
    }

    #[test]
    fn reads_tool_calls() {
        assert_eq!(from_tool("music_set_volume", &serde_json::json!({"level": 140})), Ok(Action::Volume(Change::To(100))));
        assert_eq!(from_tool("music_play_playlist", &serde_json::json!({"name": " Chill "})), Ok(Action::Playlist("Chill".into())));
        assert!(from_tool("music_play_playlist", &serde_json::json!({})).is_err());
    }
}
