use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::time::Instant;

use zbus::blocking::Connection;
use zbus::blocking::connection::Builder;
use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const PATH: &str = "/org/mpris/MediaPlayer2";

pub(crate) const ACTION_PLAY: u32 = 1;
pub(crate) const ACTION_PAUSE: u32 = 2;
pub(crate) const ACTION_PLAY_PAUSE: u32 = 3;
pub(crate) const ACTION_STOP: u32 = 4;
pub(crate) const ACTION_NEXT: u32 = 5;
pub(crate) const ACTION_PREVIOUS: u32 = 6;
pub(crate) const ACTION_SEEK_BY: u32 = 7;
pub(crate) const ACTION_SEEK_TO: u32 = 8;
pub(crate) const ACTION_RAISE: u32 = 9;

const KEY_PLAY: u32 = 1;
const KEY_PAUSE: u32 = 2;
const KEY_NEXT: u32 = 4;
const KEY_PREVIOUS: u32 = 8;
const KEY_SEEK: u32 = 16;

pub(crate) type Deliver = Box<dyn Fn(u32, f64) + Send + Sync>;

struct Root {
    identity: String,
    desktop_entry: String,
    deliver: Deliver,
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        (self.deliver)(ACTION_RAISE, 0.0);
    }

    fn quit(&self) {}

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        self.identity.clone()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        self.desktop_entry.clone()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        Vec::new()
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }
}

#[derive(Default)]
pub(crate) struct Track {
    pub(crate) title: String,
    pub(crate) artist: String,
    pub(crate) album: String,
    pub(crate) art: String,
}

struct Player {
    deliver: Deliver,
    status: String,
    keys: u32,
    track: Track,
    track_number: u32,
    duration: f64,
    position: f64,
    rate: f64,
    reported: Instant,
}

impl Player {
    fn can(&self, key: u32) -> bool {
        self.keys & key != 0
    }

    fn current_position(&self) -> f64 {
        match self.status.as_str() {
            "Playing" => self.position + self.reported.elapsed().as_secs_f64() * self.rate,
            _ => self.position,
        }
    }
}

fn micros(seconds: f64) -> i64 {
    (seconds * 1_000_000.0) as i64
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn play(&self) {
        (self.deliver)(ACTION_PLAY, 0.0);
    }

    fn pause(&self) {
        (self.deliver)(ACTION_PAUSE, 0.0);
    }

    fn play_pause(&self) {
        (self.deliver)(ACTION_PLAY_PAUSE, 0.0);
    }

    fn stop(&self) {
        (self.deliver)(ACTION_STOP, 0.0);
    }

    fn next(&self) {
        (self.deliver)(ACTION_NEXT, 0.0);
    }

    fn previous(&self) {
        (self.deliver)(ACTION_PREVIOUS, 0.0);
    }

    fn seek(&self, offset: i64) {
        (self.deliver)(ACTION_SEEK_BY, offset as f64 / 1_000_000.0);
    }

    fn set_position(&self, _track: ObjectPath<'_>, position: i64) {
        (self.deliver)(ACTION_SEEK_TO, position as f64 / 1_000_000.0);
    }

    fn open_uri(&self, _uri: String) {}

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.status.clone()
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        if self.rate > 0.0 { self.rate } else { 1.0 }
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        1.0
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        micros(self.current_position())
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let mut metadata = HashMap::new();
        let mut insert = |key: &str, value: Value<'_>| {
            if let Ok(value) = OwnedValue::try_from(value) {
                metadata.insert(key.to_owned(), value);
            }
        };
        if let Ok(track) =
            ObjectPath::try_from(format!("/org/bande/Browser/track/{}", self.track_number))
        {
            insert("mpris:trackid", Value::from(track));
        }
        if !self.track.title.is_empty() {
            insert("xesam:title", Value::from(self.track.title.as_str()));
        }
        if !self.track.artist.is_empty() {
            insert("xesam:artist", Value::from(vec![self.track.artist.as_str()]));
        }
        if !self.track.album.is_empty() {
            insert("xesam:album", Value::from(self.track.album.as_str()));
        }
        if !self.track.art.is_empty() {
            insert("mpris:artUrl", Value::from(self.track.art.as_str()));
        }
        if self.duration > 0.0 && self.duration.is_finite() {
            insert("mpris:length", Value::from(micros(self.duration)));
        }
        metadata
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.can(KEY_NEXT)
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.can(KEY_PREVIOUS)
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.can(KEY_PLAY)
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        self.can(KEY_PAUSE)
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.can(KEY_SEEK)
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

pub(crate) struct Mpris {
    connection: Connection,
}

pub(crate) enum Change {
    Status(String),
    Track(Track),
    Keys(u32),
    Position { duration: f64, position: f64, rate: f64 },
}

impl Mpris {
    pub(crate) fn open(name: &str, identity: &str, desktop_entry: &str, deliver: impl Fn(u32, f64) + Send + Sync + Clone + 'static) -> Option<Self> {
        let connection = Builder::session()
            .ok()?
            .name(format!("org.mpris.MediaPlayer2.{name}"))
            .ok()?
            .serve_at(
                PATH,
                Root {
                    identity: identity.to_owned(),
                    desktop_entry: desktop_entry.to_owned(),
                    deliver: Box::new(deliver.clone()),
                },
            )
            .ok()?
            .serve_at(
                PATH,
                Player {
                    deliver: Box::new(deliver),
                    status: "Stopped".to_owned(),
                    keys: 0,
                    track: Track::default(),
                    track_number: 0,
                    duration: 0.0,
                    position: 0.0,
                    rate: 1.0,
                    reported: Instant::now(),
                },
            )
            .ok()?
            .build()
            .ok()?;
        Some(Self { connection })
    }

    pub(crate) fn change(&self, change: Change) {
        let Ok(player) = self.connection.object_server().interface::<_, Player>(PATH) else {
            return;
        };
        let emitter = player.signal_emitter().clone();
        let mut state = player.get_mut();
        let _ = match change {
            Change::Status(status) => {
                state.position = state.current_position();
                state.reported = Instant::now();
                state.status = status;
                zbus::block_on(state.playback_status_changed(&emitter))
            }
            Change::Track(track) => {
                state.track = track;
                state.track_number += 1;
                zbus::block_on(state.metadata_changed(&emitter))
            }
            Change::Keys(keys) => {
                state.keys = keys;
                let _ = zbus::block_on(state.can_play_changed(&emitter));
                let _ = zbus::block_on(state.can_pause_changed(&emitter));
                let _ = zbus::block_on(state.can_go_next_changed(&emitter));
                let _ = zbus::block_on(state.can_seek_changed(&emitter));
                zbus::block_on(state.can_go_previous_changed(&emitter))
            }
            Change::Position { duration, position, rate } => {
                let jumped = (state.current_position() - position).abs() > 1.0;
                let length_changed = (state.duration - duration).abs() > f64::EPSILON;
                state.duration = duration;
                state.position = position;
                state.rate = rate;
                state.reported = Instant::now();
                if length_changed {
                    let _ = zbus::block_on(state.metadata_changed(&emitter));
                }
                match jumped {
                    true => zbus::block_on(Player::seeked(&emitter, micros(position))),
                    false => Ok(()),
                }
            }
        };
    }
}

pub(crate) fn deliver_to(sender: Sender<crate::Queued>, wake: std::os::unix::net::UnixStream) -> impl Fn(u32, f64) + Send + Sync + Clone + 'static {
    let wake = std::sync::Arc::new(wake);
    let sender = std::sync::Arc::new(std::sync::Mutex::new(sender));
    move |action, value| {
        let Ok(sender) = sender.lock() else {
            return;
        };
        crate::answer(
            &sender,
            &wake,
            crate::Queued {
                kind: crate::EVENT_MEDIA_KEY,
                a: action,
                x: value,
                ..crate::Queued::default()
            },
        );
    }
}
