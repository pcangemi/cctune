//! The Music app, driven through AppleScript (`osascript`). Text from the user (a playlist name,
//! a search) is passed as a script argument (`on run argv`), never pasted into the script, so
//! quotes in a name cannot break or change it. Nothing here starts the Music app by itself:
//! reading what plays answers `off` when Music is not running.

use std::fmt;
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// The Music app is not running.
    Off,
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Repeat {
    Off,
    One,
    All,
}

impl Repeat {
    pub fn parse(s: &str) -> Option<Repeat> {
        match s {
            "off" | "none" => Some(Repeat::Off),
            "one" | "track" | "song" => Some(Repeat::One),
            "all" | "on" | "playlist" => Some(Repeat::All),
            _ => None,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Repeat::Off => "off",
            Repeat::One => "one",
            Repeat::All => "all",
        }
    }
}

/// What is playing, with everything the status line shows.
#[derive(Clone, PartialEq, Debug)]
pub struct Now {
    pub state: State,
    pub name: String,
    pub artist: String,
    pub album: String,
    /// Seconds.
    pub duration: u32,
    pub position: u32,
    pub volume: u8,
    pub shuffle: bool,
    pub repeat: Repeat,
}

impl Now {
    pub fn idle(state: State) -> Now {
        Now {
            state,
            name: String::new(),
            artist: String::new(),
            album: String::new(),
            duration: 0,
            position: 0,
            volume: 0,
            shuffle: false,
            repeat: Repeat::Off,
        }
    }

    /// Reads the line `NOW_SCRIPT` prints: `off`, `stopped`, or tab-separated fields.
    pub fn parse(line: &str) -> Now {
        let f: Vec<&str> = line.trim_end_matches(['\r', '\n']).split('\t').collect();
        let state = match f[0] {
            "off" => return Now::idle(State::Off),
            "playing" | "fast forwarding" | "rewinding" => State::Playing,
            "paused" => State::Paused,
            _ => return Now::idle(State::Stopped),
        };
        let num = |i: usize| f.get(i).and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0).max(0.0);
        Now {
            state,
            name: unescape(f.get(1).unwrap_or(&"")),
            artist: unescape(f.get(2).unwrap_or(&"")),
            album: unescape(f.get(3).unwrap_or(&"")),
            duration: num(4) as u32,
            position: num(5) as u32,
            volume: num(6).min(100.0) as u8,
            shuffle: f.get(7) == Some(&"true"),
            repeat: f.get(8).and_then(|r| Repeat::parse(r)).unwrap_or(Repeat::Off),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Track {
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration: u32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Playlist {
    pub name: String,
    pub tracks: u32,
}

#[derive(Debug)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Everything cctune asks of a music player; the tests use a fake one.
pub trait Player {
    fn now(&self) -> Result<Now, Error>;
    fn play(&self) -> Result<(), Error>;
    fn pause(&self) -> Result<(), Error>;
    fn toggle(&self) -> Result<(), Error>;
    fn next(&self) -> Result<(), Error>;
    fn previous(&self) -> Result<(), Error>;
    fn set_volume(&self, level: u8) -> Result<(), Error>;
    fn set_shuffle(&self, on: bool) -> Result<(), Error>;
    fn set_repeat(&self, mode: Repeat) -> Result<(), Error>;
    fn seek(&self, seconds: u32) -> Result<(), Error>;
    fn playlists(&self) -> Result<Vec<Playlist>, Error>;
    /// Plays the playlist with this name, or the first whose name contains it; returns its name.
    fn play_playlist(&self, name: &str) -> Result<String, Error>;
    fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>, Error>;
    /// Plays the first track of the library matching the query.
    fn play_song(&self, query: &str) -> Result<Track, Error>;
}

/// The Music app of this Mac.
pub struct AppleMusic;

// Variable names are long on purpose: AppleScript reserves short words that look harmless (`st`,
// `nd`, `rd`, `th` are ordinal suffixes). The test `every_script_compiles` checks each script.
// Values are read with `get` before being converted: inside `tell application "Music"`, a
// command like `round` would receive a reference to the property, not its value.
const NOW_SCRIPT: &str = r#"
if application "Music" is not running then return "off"
tell application "Music"
    set playerState to (get player state) as text
    if playerState is "stopped" then return "stopped"
    try
        set theTrack to current track
    on error
        return "stopped"
    end try
    set trackLength to 0
    try
        set trackLength to (get duration of theTrack) as integer
    end try
    set playedSeconds to 0
    try
        set playedSeconds to (get player position) as integer
    end try
    set trackName to (get name of theTrack)
    set trackArtist to (get artist of theTrack)
    set trackAlbum to (get album of theTrack)
    set volumeLevel to (get sound volume)
    set shuffleOn to (get shuffle enabled)
    set repeatMode to (get song repeat) as text
end tell
return playerState & tab & trackName & tab & trackArtist & tab & trackAlbum & tab & trackLength & tab & playedSeconds & tab & volumeLevel & tab & shuffleOn & tab & repeatMode
"#;

const PLAYLISTS_SCRIPT: &str = r#"
tell application "Music"
    set listing to ""
    repeat with thePlaylist in (every user playlist)
        set listing to listing & (name of thePlaylist) & tab & (count of tracks of thePlaylist) & linefeed
    end repeat
    return listing
end tell
"#;

const PLAY_PLAYLIST_SCRIPT: &str = r#"
on run argv
    set wanted to item 1 of argv
    tell application "Music"
        set found to (every user playlist whose name is wanted)
        if found is {} then set found to (every user playlist whose name contains wanted)
        if found is {} then return ""
        set thePlaylist to item 1 of found
        play thePlaylist
        return name of thePlaylist
    end tell
end run
"#;

const SEARCH_SCRIPT: &str = r#"
on run argv
    set wanted to item 1 of argv
    set maxCount to (item 2 of argv) as integer
    set playFirst to (item 3 of argv) is "play"
    tell application "Music"
        set found to (search library playlist 1 for wanted)
        if playFirst and found is not {} then play item 1 of found
        set listing to ""
        set counted to 0
        repeat with theTrack in found
            set counted to counted + 1
            if counted > maxCount then exit repeat
            set trackLength to 0
            try
                set trackLength to (get duration of theTrack) as integer
            end try
            set listing to listing & (get name of theTrack) & tab & (get artist of theTrack) & tab & (get album of theTrack) & tab & trackLength & linefeed
        end repeat
        return listing
    end tell
end run
"#;

impl AppleMusic {
    /// Runs a script with its arguments and returns what it printed, or a readable error.
    fn run(script: &str, args: &[&str]) -> Result<String, Error> {
        let out = Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .args(args)
            .output()
            .map_err(|e| Error(format!("cannot run osascript ({e}); cctune needs macOS")))?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).trim_end_matches('\n').to_string());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("-1743") || err.contains("Not authorized") {
            return Err(Error(
                "macOS has not allowed this terminal to control Music. Open System Settings › Privacy & Security › \
                 Automation, and allow Music for your terminal app."
                    .into(),
            ));
        }
        if err.contains("-600") || err.contains("isn’t running") || err.contains("isn't running") {
            return Err(Error("the Music app is not running".into()));
        }
        Err(Error(format!("Music answered: {}", err.trim())))
    }

    fn tell(verb: &str) -> Result<(), Error> {
        Self::run(&format!("tell application \"Music\" to {verb}"), &[]).map(|_| ())
    }
}

fn tracks(out: &str) -> Vec<Track> {
    out.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            Track {
                name: unescape(f.first().unwrap_or(&"")),
                artist: unescape(f.get(1).unwrap_or(&"")),
                album: unescape(f.get(2).unwrap_or(&"")),
                duration: f.get(3).and_then(|d| d.trim().parse().ok()).unwrap_or(0),
            }
        })
        .collect()
}

/// What to say when nothing matches. AppleScript reaches the library only: a catalog song plays
/// once it is added to the library in Music, and opening its link shows it without playing it.
pub fn no_match(query: &str) -> String {
    format!("nothing in your library matches \"{query}\" (cctune plays your library and playlists, not the Apple Music catalog)")
}

impl Player for AppleMusic {
    fn now(&self) -> Result<Now, Error> {
        if !music_running() {
            return Ok(Now::idle(State::Off));
        }
        Self::run(NOW_SCRIPT, &[]).map(|line| Now::parse(&line))
    }

    fn play(&self) -> Result<(), Error> {
        Self::tell("play")
    }

    fn pause(&self) -> Result<(), Error> {
        Self::tell("pause")
    }

    fn toggle(&self) -> Result<(), Error> {
        Self::tell("playpause")
    }

    fn next(&self) -> Result<(), Error> {
        Self::tell("next track")
    }

    fn previous(&self) -> Result<(), Error> {
        Self::tell("previous track")
    }

    fn set_volume(&self, level: u8) -> Result<(), Error> {
        Self::tell(&format!("set sound volume to {}", level.min(100)))
    }

    fn set_shuffle(&self, on: bool) -> Result<(), Error> {
        Self::tell(&format!("set shuffle enabled to {on}"))
    }

    fn set_repeat(&self, mode: Repeat) -> Result<(), Error> {
        Self::tell(&format!("set song repeat to {}", mode.word()))
    }

    fn seek(&self, seconds: u32) -> Result<(), Error> {
        Self::tell(&format!("set player position to {seconds}"))
    }

    fn playlists(&self) -> Result<Vec<Playlist>, Error> {
        let out = Self::run(PLAYLISTS_SCRIPT, &[])?;
        Ok(out
            .lines()
            .filter_map(|l| {
                let (name, n) = l.rsplit_once('\t')?;
                Some(Playlist { name: unescape(name), tracks: n.trim().parse().unwrap_or(0) })
            })
            .collect())
    }

    fn play_playlist(&self, name: &str) -> Result<String, Error> {
        // Some names are stored HTML-escaped (`R&amp;B`): try the name as typed, then escaped.
        let mut tries = vec![name.to_string()];
        if escape(name) != name {
            tries.push(escape(name));
        }
        for wanted in tries {
            let found = Self::run(PLAY_PLAYLIST_SCRIPT, &[&wanted])?;
            if !found.is_empty() {
                return Ok(unescape(&found));
            }
        }
        Err(Error(format!("no playlist named like \"{name}\"")))
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<Track>, Error> {
        Self::run(SEARCH_SCRIPT, &[query, &limit.max(1).to_string(), "list"]).map(|o| tracks(&o))
    }

    fn play_song(&self, query: &str) -> Result<Track, Error> {
        let found = Self::run(SEARCH_SCRIPT, &[query, "1", "play"]).map(|o| tracks(&o))?;
        found.into_iter().next().ok_or_else(|| Error(no_match(query)))
    }
}

/// Music returns some names HTML-escaped (`R&amp;B/Soul`, often playlists that came from the web
/// or Shazam): they are shown as people wrote them.
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&amp;", "&")
}

/// The escaped spelling of a name, to find one stored that way.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Whether the Music app runs, asked of the system without starting it or sending it anything,
/// so the status line costs almost nothing while Music is closed.
#[cfg(target_os = "macos")]
pub fn music_running() -> bool {
    unsafe extern "C" {
        fn proc_listallpids(buffer: *mut i32, buffersize: i32) -> i32;
        fn proc_name(pid: i32, buffer: *mut u8, buffersize: u32) -> i32;
    }
    let mut pids = vec![0i32; 8192];
    // SAFETY: the buffer holds `pids.len()` i32 and its size is given in bytes.
    let n = unsafe { proc_listallpids(pids.as_mut_ptr(), (pids.len() * 4) as i32) };
    if n <= 0 {
        // Unknown: let AppleScript check, which does not start Music either.
        return true;
    }
    let mut name = [0u8; 256];
    pids[..(n as usize).min(pids.len())].iter().any(|&pid| {
        // SAFETY: `name` holds 256 bytes, its size is passed, and the length returned is checked.
        let len = unsafe { proc_name(pid, name.as_mut_ptr(), name.len() as u32) };
        len > 0 && &name[..(len as usize).min(name.len())] == b"Music"
    })
}

#[cfg(not(target_os = "macos"))]
pub fn music_running() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_plays() {
        let now = Now::parse("playing\tGet Lucky\tDaft Punk\tRandom Access Memories\t369\t134\t60\ttrue\tall\n");
        assert_eq!(now.state, State::Playing);
        assert_eq!((now.name.as_str(), now.artist.as_str()), ("Get Lucky", "Daft Punk"));
        assert_eq!((now.duration, now.position, now.volume), (369, 134, 60));
        assert!(now.shuffle);
        assert_eq!(now.repeat, Repeat::All);
        assert_eq!(Now::parse("off").state, State::Off);
        assert_eq!(Now::parse("stopped").state, State::Stopped);
        assert_eq!(Now::parse("paused\tA\tB\tC\t10\t2\t40\tfalse\toff").state, State::Paused);
    }

    /// Compiles every script without running it: Music is not contacted, but a syntax error (a
    /// reserved word used as a name, a typo) fails here instead of in the person's terminal.
    #[cfg(target_os = "macos")]
    #[test]
    fn every_script_compiles() {
        let out = std::env::temp_dir().join(format!("cctune-compile-{}.scpt", std::process::id()));
        let tells = [
            "play",
            "pause",
            "playpause",
            "next track",
            "previous track",
            "set sound volume to 50",
            "set shuffle enabled to true",
            "set song repeat to all",
            "set player position to 30",
        ];
        let mut scripts: Vec<(String, String)> =
            [("now", NOW_SCRIPT), ("playlists", PLAYLISTS_SCRIPT), ("play playlist", PLAY_PLAYLIST_SCRIPT), ("search", SEARCH_SCRIPT)]
                .iter()
                .map(|(n, s)| (n.to_string(), s.to_string()))
                .collect();
        scripts.extend(tells.iter().map(|v| (v.to_string(), format!("tell application \"Music\" to {v}"))));
        for (name, script) in scripts {
            let res = Command::new("/usr/bin/osacompile").arg("-o").arg(&out).arg("-e").arg(&script).output().expect("osacompile");
            assert!(res.status.success(), "the {name} script does not compile: {}", String::from_utf8_lossy(&res.stderr));
        }
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn shows_escaped_names_as_written() {
        assert_eq!(unescape("R&amp;B/RAP 2000'S"), "R&B/RAP 2000'S");
        assert_eq!(unescape("Tom &amp; Jerry &lt;3"), "Tom & Jerry <3");
        assert_eq!(unescape("AC/DC"), "AC/DC");
        assert_eq!(escape("R&B/Soul"), "R&amp;B/Soul");
        assert_eq!(Now::parse("playing\tR&amp;B Song\tA &amp; B\tX\t10\t1\t50\tfalse\toff").artist, "A & B");
    }

    #[test]
    fn reads_track_lists() {
        let t = tracks("One\tU2\tAchtung Baby\t276\nTwo\tX\tY\t0\n");
        assert_eq!(t.len(), 2);
        assert_eq!(t[0], Track { name: "One".into(), artist: "U2".into(), album: "Achtung Baby".into(), duration: 276 });
    }
}
