//! `cctune mcp`: an MCP server on standard input and output, so Claude Code can control Music
//! when asked ("play my focus playlist", "skip this one"). JSON-RPC 2.0, one message per line;
//! only the parts of the protocol a tools server needs: initialize, ping, tools/list, tools/call.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::actions;
use crate::music::Player;

const INSTRUCTIONS: &str = "Controls the Apple Music app on this Mac: what is playing, play, pause, next, previous, \
volume, shuffle, repeat, seeking, the user's playlists, and songs of the user's library (searching the Apple Music \
catalog is not possible). Prefer music_play_playlist or music_play_song when the user names something to listen to.";

fn tools() -> Value {
    let none = json!({"type": "object", "properties": {}});
    let tool = |name: &str, description: &str, schema: &Value| json!({"name": name, "description": description, "inputSchema": schema});
    json!([
        tool("music_now_playing", "What Music is playing now: track, artist, album, position, volume, shuffle and repeat.", &none),
        tool("music_play", "Start or resume playback.", &none),
        tool("music_pause", "Pause playback.", &none),
        tool("music_toggle", "Pause if playing, play if paused.", &none),
        tool("music_next", "Skip to the next track.", &none),
        tool("music_previous", "Go back to the previous track.", &none),
        tool(
            "music_set_volume",
            "Set Music's volume (not the Mac's).",
            &json!({
                "type": "object", "properties": {"level": {"type": "number", "minimum": 0, "maximum": 100, "description": "Volume from 0 to 100"}}, "required": ["level"]
            })
        ),
        tool(
            "music_shuffle",
            "Turn shuffle on or off; without `on`, switch it.",
            &json!({
                "type": "object", "properties": {"on": {"type": "boolean"}}
            })
        ),
        tool(
            "music_repeat",
            "Set repeat: off, one (the current track) or all.",
            &json!({
                "type": "object", "properties": {"mode": {"type": "string", "enum": ["off", "one", "all"]}}, "required": ["mode"]
            })
        ),
        tool(
            "music_seek",
            "Jump to a position in the current track.",
            &json!({
                "type": "object", "properties": {"seconds": {"type": "number", "minimum": 0}}, "required": ["seconds"]
            })
        ),
        tool("music_list_playlists", "List the user's playlists with their number of tracks.", &none),
        tool(
            "music_play_playlist",
            "Play one of the user's playlists, by its exact name or a part of it.",
            &json!({
                "type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]
            })
        ),
        tool(
            "music_search",
            "Search the user's library for tracks by name, artist or album.",
            &json!({
                "type": "object", "properties": {"query": {"type": "string"}, "limit": {"type": "number", "minimum": 1, "maximum": 50}}, "required": ["query"]
            })
        ),
        tool(
            "music_play_song",
            "Play the best match in the user's library for a song, artist or album.",
            &json!({
                "type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]
            })
        ),
    ])
}

/// Answers one message; `None` for notifications, which get no answer.
pub fn handle(player: &dyn Player, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            // The client's version is accepted as it is: a tools server only needs the basics.
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or("2025-06-18"),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "cctune", "version": env!("CARGO_PKG_VERSION")},
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let outcome = actions::from_tool(name, &args).map_err(|e| e.to_string()).and_then(|action| {
                let done = actions::run(player, &action).map_err(|e| e.to_string());
                if done.is_ok() && actions::changes_playback(&action) {
                    crate::statusline::forget_now();
                }
                done
            });
            Ok(match outcome {
                Ok(text) => json!({"content": [{"type": "text", "text": text}], "isError": false}),
                Err(text) => json!({"content": [{"type": "text", "text": text}], "isError": true}),
            })
        }
        other => Err(json!({"code": -32601, "message": format!("method not found: {other}")})),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

/// Serves until the input ends.
pub fn serve(player: &dyn Player, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(player, &msg),
            Err(_) => Some(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}})),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::{Error, Now, Playlist, Repeat, State, Track};
    use std::cell::RefCell;

    /// A player that remembers what it was asked.
    struct Fake {
        now: RefCell<Now>,
        log: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new() -> Fake {
            let mut now = Now::idle(State::Playing);
            now.name = "Get Lucky".into();
            now.artist = "Daft Punk".into();
            now.album = "Random Access Memories".into();
            now.duration = 369;
            now.position = 134;
            now.volume = 60;
            Fake { now: RefCell::new(now), log: RefCell::new(Vec::new()) }
        }
    }

    impl Player for Fake {
        fn now(&self) -> Result<Now, Error> {
            Ok(self.now.borrow().clone())
        }
        fn play(&self) -> Result<(), Error> {
            self.log.borrow_mut().push("play".into());
            Ok(())
        }
        fn pause(&self) -> Result<(), Error> {
            self.log.borrow_mut().push("pause".into());
            Ok(())
        }
        fn toggle(&self) -> Result<(), Error> {
            Ok(())
        }
        fn next(&self) -> Result<(), Error> {
            self.log.borrow_mut().push("next".into());
            Ok(())
        }
        fn previous(&self) -> Result<(), Error> {
            Ok(())
        }
        fn set_volume(&self, level: u8) -> Result<(), Error> {
            self.log.borrow_mut().push(format!("volume {level}"));
            Ok(())
        }
        fn set_shuffle(&self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        fn set_repeat(&self, _: Repeat) -> Result<(), Error> {
            Ok(())
        }
        fn seek(&self, s: u32) -> Result<(), Error> {
            self.log.borrow_mut().push(format!("seek {s}"));
            Ok(())
        }
        fn playlists(&self) -> Result<Vec<Playlist>, Error> {
            Ok(vec![Playlist { name: "Deep Focus".into(), tracks: 42 }])
        }
        fn play_playlist(&self, name: &str) -> Result<String, Error> {
            if "Deep Focus".to_lowercase().contains(&name.to_lowercase()) {
                Ok("Deep Focus".into())
            } else {
                Err(Error(format!("no playlist named like \"{name}\"")))
            }
        }
        fn search(&self, _: &str, _: usize) -> Result<Vec<Track>, Error> {
            Ok(vec![])
        }
        fn play_song(&self, q: &str) -> Result<Track, Error> {
            Err(Error(format!("nothing in your library matches \"{q}\"")))
        }
    }

    fn talk(player: &Fake, msgs: &[Value]) -> Vec<Value> {
        let input: String = msgs.iter().map(|m| format!("{m}\n")).collect();
        let mut out = Vec::new();
        serve(player, input.as_bytes(), &mut out).unwrap();
        String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    #[test]
    fn speaks_the_protocol() {
        let fake = Fake::new();
        let replies = talk(
            &fake,
            &[
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test"}}}),
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
                json!({"jsonrpc": "2.0", "id": 3, "method": "resources/list"}),
            ],
        );
        assert_eq!(replies.len(), 3, "the notification gets no answer");
        assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(replies[0]["result"]["serverInfo"]["name"], "cctune");
        let names: Vec<&str> = replies[1]["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"music_play_playlist") && names.contains(&"music_now_playing"));
        assert_eq!(replies[2]["error"]["code"], -32601);
    }

    #[test]
    fn runs_tools_and_reports_errors_as_tool_results() {
        let fake = Fake::new();
        let call = |id: u32, name: &str, args: Value| json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}});
        let replies = talk(
            &fake,
            &[
                call(1, "music_now_playing", json!({})),
                call(2, "music_set_volume", json!({"level": 35})),
                call(3, "music_play_playlist", json!({"name": "focus"})),
                call(4, "music_play_playlist", json!({"name": "polka"})),
                call(5, "music_teleport", json!({})),
            ],
        );
        let text = |r: &Value| r["result"]["content"][0]["text"].as_str().unwrap().to_string();
        assert!(text(&replies[0]).contains("Get Lucky — Daft Punk") && text(&replies[0]).contains("2:14 / 6:09"));
        assert_eq!(text(&replies[1]), "volume 35%");
        assert_eq!(text(&replies[2]), "playing the playlist Deep Focus");
        assert_eq!(replies[3]["result"]["isError"], true);
        assert_eq!(replies[4]["result"]["isError"], true);
        assert_eq!(*fake.log.borrow(), vec!["volume 35".to_string()]);
    }
}
