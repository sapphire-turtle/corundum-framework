//! Linux and the BSDs: an MPRIS player (`org.mpris.MediaPlayer2`) on the D-Bus session bus.
//!
//! zbus serves the player's two interfaces on its own executor thread, where the OS's calls
//! come in and go to the app as events. What the app tells the session goes to a thread of
//! the session's own, which updates the interfaces and sends the bus their change signals,
//! so the frame never waits on the bus. The cover is a URL to MPRIS: a file's is its path,
//! and bytes are written to a file of the session's first.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use raw_window_handle::HasWindowHandle;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::object_server::{InterfaceRef, SignalEmitter};
use zbus::zvariant::{ObjectPath, Value};
use zbus::{Connection, interface};

use super::{Cover, EventSender, MediaEvent, MediaSessionSpec, Metadata, Playback};

/// Where MPRIS players are served.
const PATH: &str = "/org/mpris/MediaPlayer2";

/// How far the position told may be from where the clock put it before it is a seek, which
/// MPRIS clients hear of through `Seeked`: they read the position again on it.
const SEEK_TOLERANCE: Duration = Duration::from_millis(500);

pub(super) struct Backend {
    updates: Option<mpsc::Sender<Update>>,
    worker: Option<JoinHandle<()>>,
}

/// What the app told the session, on its way to the session's thread.
enum Update {
    Metadata(Metadata),
    Playback(Playback),
}

impl Backend {
    pub(super) fn new(
        spec: &MediaSessionSpec<'_>,
        _window: Option<&dyn HasWindowHandle>,
        sender: EventSender,
    ) -> io::Result<Option<Backend>> {
        let element = bus_name_element(spec.id);
        let root = Root {
            identity: spec.name.to_owned(),
            desktop_entry: spec.desktop_entry.unwrap_or_default().to_owned(),
        };
        let player = Player {
            state: State::new(),
            sender,
        };
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (updates, rx) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("corundum media session".into())
            .spawn(move || {
                let served = zbus::block_on(serve(&element, root, player));
                let (connection, art) = match served {
                    Ok(served) => {
                        let _ = ready_tx.send(Ok(()));
                        served
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                let mut art = art;
                for update in rx {
                    if let Err(err) = zbus::block_on(apply(&connection, &mut art, update)) {
                        tracing::warn!("media session: MPRIS update failed: {err}");
                    }
                }
                // The session was dropped: the player leaves the bus with the connection,
                // and its cover file with it.
                art.remove();
            })?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Some(Backend {
                updates: Some(updates),
                worker: Some(worker),
            })),
            Ok(Err(err)) => {
                let _ = worker.join();
                Err(io::Error::other(err))
            }
            Err(_) => Err(io::Error::other("the media session's thread stopped")),
        }
    }

    pub(super) fn set_metadata(&mut self, metadata: &Metadata) -> io::Result<()> {
        self.send(Update::Metadata(metadata.clone()))
    }

    pub(super) fn set_playback(&mut self, playback: Playback) -> io::Result<()> {
        self.send(Update::Playback(playback))
    }

    fn send(&self, update: Update) -> io::Result<()> {
        self.updates
            .as_ref()
            .and_then(|updates| updates.send(update).ok())
            .ok_or_else(|| io::Error::other("the media session's thread stopped"))
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        // Closing the channel ends the thread's loop.
        self.updates = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Connect to the session bus, serve the player there, and take its name: the app's own,
/// or, when another of its processes has that, the same with `.instance<pid>`, as MPRIS
/// asks of a second instance.
async fn serve(element: &str, root: Root, player: Player) -> zbus::Result<(Connection, ArtFile)> {
    let connection = zbus::connection::Builder::session()?
        .serve_at(PATH, root)?
        .serve_at(PATH, player)?
        .build()
        .await?;
    let name = format!("org.mpris.MediaPlayer2.{element}");
    let flags = RequestNameFlags::DoNotQueue.into();
    let reply = connection
        .request_name_with_flags(name.as_str(), flags)
        .await?;
    let name = if reply == RequestNameReply::Exists {
        let name = format!("{name}.instance{}", std::process::id());
        connection
            .request_name_with_flags(name.as_str(), flags)
            .await?;
        name
    } else {
        name
    };
    tracing::debug!("media session: MPRIS player {name}");
    Ok((connection, ArtFile::new(element)))
}

/// Put `update` in the player and tell the bus what changed.
async fn apply(connection: &Connection, art: &mut ArtFile, update: Update) -> zbus::Result<()> {
    let player: InterfaceRef<Player> = connection.object_server().interface(PATH).await?;
    let emitter = player.signal_emitter();
    match update {
        Update::Metadata(metadata) => {
            let art_url = match &metadata.cover {
                Some(Cover::Path(path)) => Some(file_url(path)),
                Some(Cover::Bytes(bytes)) => art.write(bytes).map(|path| file_url(&path)),
                None => None,
            };
            if !matches!(metadata.cover, Some(Cover::Bytes(_))) {
                art.remove();
            }
            {
                let mut player = player.get_mut().await;
                player.state.track += 1;
                player.state.art_url = art_url;
                player.state.metadata = metadata;
            }
            player.get().await.metadata_changed(emitter).await?;
        }
        Update::Playback(playback) => {
            let (seeked, status_changed) = {
                let mut player = player.get_mut().await;
                let state = &mut player.state;
                let expected = state.position();
                let status_changed = status(state.playback) != status(playback);
                state.playback = playback;
                state.since = Instant::now();
                let seeked = playback
                    .position()
                    .filter(|&told| jumped(expected, told))
                    .map(micros);
                (seeked, status_changed)
            };
            if status_changed {
                player.get().await.playback_status_changed(emitter).await?;
            }
            if let Some(position) = seeked {
                Player::seeked(emitter, position).await?;
            }
        }
    }
    Ok(())
}

/// The player's root interface: its name and what it cannot do (raise a window, quit).
struct Root {
    identity: String,
    desktop_entry: String,
}

#[interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {}

    fn quit(&self) {}

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> &str {
        &self.identity
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> &str {
        &self.desktop_entry
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

/// What the player shows, as the app last told it.
struct State {
    metadata: Metadata,
    /// Counts the tracks shown, for each one's `mpris:trackid`: `SetPosition` names the
    /// track it is for, and one meant for the last track is let go.
    track: u64,
    art_url: Option<String>,
    playback: Playback,
    /// When `playback` was told, from which the position is moved on while it plays.
    since: Instant,
}

impl State {
    fn new() -> State {
        State {
            metadata: Metadata::default(),
            track: 0,
            art_url: None,
            playback: Playback::Stopped,
            since: Instant::now(),
        }
    }

    fn track_id(&self) -> String {
        format!("/net/fireturtle/corundum/track/{}", self.track)
    }

    /// The position now: as told, moved on by the time since while playing.
    fn position(&self) -> Option<Duration> {
        position_at(self.playback, self.since.elapsed(), self.metadata.duration)
    }
}

/// The `org.mpris.MediaPlayer2.Player` interface: the controls, which go to the app, and
/// what plays.
struct Player {
    state: State,
    sender: EventSender,
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.sender.send(MediaEvent::Next);
    }

    fn previous(&self) {
        self.sender.send(MediaEvent::Previous);
    }

    fn pause(&self) {
        self.sender.send(MediaEvent::Pause);
    }

    fn play_pause(&self) {
        self.sender.send(MediaEvent::Toggle);
    }

    fn stop(&self) {
        self.sender.send(MediaEvent::Stop);
    }

    fn play(&self) {
        self.sender.send(MediaEvent::Play);
    }

    /// Moves by `offset` microseconds, back where it is negative.
    fn seek(&self, offset: i64) {
        let by = Duration::from_micros(offset.unsigned_abs());
        self.sender.send(if offset < 0 {
            MediaEvent::SeekBackward(by)
        } else {
            MediaEvent::SeekForward(by)
        });
    }

    /// Goes to `position` microseconds in `track_id`; MPRIS asks that a call for another
    /// track, or for a position outside the track, be let go.
    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        if track_id.as_str() != self.state.track_id() || position < 0 {
            return;
        }
        let to = Duration::from_micros(position.unsigned_abs());
        if self
            .state
            .metadata
            .duration
            .is_some_and(|length| to > length)
        {
            return;
        }
        self.sender.send(MediaEvent::SeekTo(to));
    }

    #[zbus(signal)]
    async fn seeked(emitter: &SignalEmitter<'_>, position: i64) -> zbus::Result<()>;

    #[zbus(property)]
    fn playback_status(&self) -> &'static str {
        status(self.state.playback)
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
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
    fn metadata(&self) -> HashMap<&'static str, Value<'_>> {
        let state = &self.state;
        let mut map = HashMap::new();
        if let Ok(path) = ObjectPath::try_from(state.track_id()) {
            map.insert("mpris:trackid", Value::from(path));
        }
        let metadata = &state.metadata;
        if let Some(duration) = metadata.duration {
            map.insert("mpris:length", Value::from(micros(duration)));
        }
        if let Some(url) = &state.art_url {
            map.insert("mpris:artUrl", Value::from(url.as_str()));
        }
        if let Some(title) = &metadata.title {
            map.insert("xesam:title", Value::from(title.as_str()));
        }
        if let Some(artist) = &metadata.artist {
            map.insert("xesam:artist", Value::from(vec![artist.as_str()]));
        }
        if let Some(album) = &metadata.album {
            map.insert("xesam:album", Value::from(album.as_str()));
        }
        map
    }

    /// Read by a client when it draws: where the clock has the position now. MPRIS sends no
    /// signal as it moves.
    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        self.state.position().map_or(0, micros)
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn can_control(&self) -> bool {
        true
    }
}

/// The cover given as bytes, written to a file MPRIS clients can read. Each image is a new
/// file, named after its bytes, so a client does not keep showing the last one it read.
struct ArtFile {
    dir: PathBuf,
    prefix: String,
    current: Option<PathBuf>,
}

impl ArtFile {
    /// Under `$XDG_RUNTIME_DIR`, which only the user reads and which goes with the
    /// session, or else the temporary directory.
    fn new(element: &str) -> ArtFile {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        ArtFile {
            dir,
            prefix: format!("corundum-cover-{element}-{}", std::process::id()),
            current: None,
        }
    }

    /// Write `bytes` (unless the file is already theirs) and take away the last image's.
    fn write(&mut self, bytes: &[u8]) -> Option<PathBuf> {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        let path = self.dir.join(format!(
            "{}-{:016x}.{}",
            self.prefix,
            hasher.finish(),
            image_extension(bytes)
        ));
        if self.current.as_ref() == Some(&path) {
            return Some(path);
        }
        self.remove();
        match std::fs::write(&path, bytes) {
            Ok(()) => {
                self.current = Some(path.clone());
                Some(path)
            }
            Err(err) => {
                tracing::warn!("media session: cannot write the cover to {path:?}: {err}");
                None
            }
        }
    }

    fn remove(&mut self) {
        if let Some(path) = self.current.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The extension of an image's file, from its first bytes, as clients that go by the name
/// want it.
fn image_extension(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        "jpg"
    } else {
        "img"
    }
}

/// MPRIS's name for the playback.
fn status(playback: Playback) -> &'static str {
    match playback {
        Playback::Stopped => "Stopped",
        Playback::Paused(_) => "Paused",
        Playback::Playing(_) => "Playing",
    }
}

/// The position `elapsed` after `playback` was told: moved on while playing, no further
/// than the track's end.
fn position_at(
    playback: Playback,
    elapsed: Duration,
    duration: Option<Duration>,
) -> Option<Duration> {
    let at = match playback {
        Playback::Stopped => return None,
        Playback::Paused(at) => at,
        Playback::Playing(at) => at + elapsed,
    };
    Some(duration.map_or(at, |length| at.min(length)))
}

/// Whether the position told is a seek: not where the clock put playback, or there was no
/// position before.
fn jumped(expected: Option<Duration>, told: Duration) -> bool {
    expected.is_none_or(|expected| expected.abs_diff(told) > SEEK_TOLERANCE)
}

/// A duration in MPRIS's microseconds.
fn micros(duration: Duration) -> i64 {
    i64::try_from(duration.as_micros()).unwrap_or(i64::MAX)
}

/// `id` as the last element of a D-Bus name: letters, digits and `_` only, not starting
/// with a digit. Dots are kept, as reverse-domain ids have them, but an empty element
/// between them is not.
fn bus_name_element(id: &str) -> String {
    let parts: Vec<String> = id
        .split('.')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut out: String = part
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            if out.starts_with(|c: char| c.is_ascii_digit()) {
                out.insert(0, '_');
            }
            out
        })
        .collect();
    if parts.is_empty() {
        "corundum".to_owned()
    } else {
        parts.join(".")
    }
}

/// `path` as a `file://` URL, with what a URL may not hold percent-encoded.
fn file_url(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut url = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(&byte) {
            url.push(byte as char);
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_becomes_a_bus_name_element() {
        assert_eq!(bus_name_element("corundum-demo"), "corundum_demo");
        assert_eq!(
            bus_name_element("net.fireturtle.Asteria"),
            "net.fireturtle.Asteria"
        );
        assert_eq!(bus_name_element("9lives..app"), "_9lives.app");
        assert_eq!(bus_name_element(""), "corundum");
    }

    #[test]
    fn a_path_becomes_a_file_url() {
        assert_eq!(
            file_url(Path::new("/tmp/a cover/ジャケット.png")),
            "file:///tmp/a%20cover/%E3%82%B8%E3%83%A3%E3%82%B1%E3%83%83%E3%83%88.png"
        );
    }

    #[test]
    fn the_position_moves_on_only_while_playing() {
        let at = Duration::from_secs(10);
        let later = Duration::from_secs(5);
        assert_eq!(
            position_at(Playback::Playing(at), later, None),
            Some(Duration::from_secs(15))
        );
        assert_eq!(position_at(Playback::Paused(at), later, None), Some(at));
        assert_eq!(position_at(Playback::Stopped, later, None), None);
        // Not past the end.
        assert_eq!(
            position_at(Playback::Playing(at), later, Some(Duration::from_secs(12))),
            Some(Duration::from_secs(12))
        );
    }

    #[test]
    fn only_a_jump_is_a_seek() {
        let at = Duration::from_secs(30);
        assert!(!jumped(Some(at), at + Duration::from_millis(100)));
        assert!(jumped(Some(at), Duration::from_secs(60)));
        assert!(jumped(Some(at), Duration::from_secs(1)));
        assert!(jumped(None, Duration::ZERO));
    }

    /// A player on the real session bus, driven as a media panel would: skipped where there
    /// is no session bus (a container, CI without one).
    #[test]
    fn a_player_on_the_session_bus_shows_its_track_and_takes_calls() {
        use crate::media_session::MediaSession;
        use std::sync::Arc;
        use zbus::proxy::CacheProperties;
        use zbus::zvariant::OwnedValue;

        let Ok(bus) = zbus::block_on(Connection::session()) else {
            eprintln!("no D-Bus session bus: skipped");
            return;
        };
        let id = format!("corundum_test_{}", std::process::id());
        let (woken, wakes) = mpsc::channel();
        let mut session = MediaSession::new(
            &MediaSessionSpec {
                id: &id,
                name: "Corundum test",
                desktop_entry: Some("corundum-test"),
            },
            None,
            move || {
                let _ = woken.send(());
            },
        )
        .unwrap();
        assert!(session.is_connected());
        let png: Arc<[u8]> = Arc::from(&b"\x89PNG\r\n\x1a\nnot really"[..]);
        session
            .set_metadata(&Metadata {
                title: Some("Song".into()),
                artist: Some("Band".into()),
                album: Some("Album".into()),
                duration: Some(Duration::from_secs(180)),
                cover: Some(Cover::Bytes(png)),
            })
            .unwrap();
        session
            .set_playback(Playback::Playing(Duration::from_secs(10)))
            .unwrap();

        let player = zbus::block_on(
            zbus::proxy::Builder::<zbus::Proxy<'_>>::new(&bus)
                .destination(format!("org.mpris.MediaPlayer2.{id}"))
                .unwrap()
                .path(PATH)
                .unwrap()
                .interface("org.mpris.MediaPlayer2.Player")
                .unwrap()
                .cache_properties(CacheProperties::No)
                .build(),
        )
        .unwrap();
        let property = |name: &str| zbus::block_on(player.get_property::<OwnedValue>(name));

        // The updates reach the bus on the session's thread: wait for them.
        let deadline = Instant::now() + Duration::from_secs(5);
        while property("PlaybackStatus")
            .ok()
            .and_then(|v| String::try_from(v).ok())
            != Some("Playing".into())
        {
            assert!(Instant::now() < deadline, "the player never played");
            std::thread::sleep(Duration::from_millis(20));
        }
        let metadata: HashMap<String, OwnedValue> =
            HashMap::try_from(property("Metadata").unwrap()).unwrap();
        let text = |key: &str| String::try_from(metadata[key].try_clone().unwrap()).unwrap();
        assert_eq!(text("xesam:title"), "Song");
        assert_eq!(text("xesam:album"), "Album");
        assert_eq!(
            i64::try_from(metadata["mpris:length"].try_clone().unwrap()).unwrap(),
            180_000_000
        );
        let art = text("mpris:artUrl");
        assert!(art.starts_with("file://") && art.ends_with(".png"), "{art}");
        let position = i64::try_from(property("Position").unwrap()).unwrap();
        assert!((10_000_000..20_000_000).contains(&position), "{position}");
        let track = ObjectPath::try_from(metadata["mpris:trackid"].try_clone().unwrap()).unwrap();

        let expect = |event: MediaEvent| {
            wakes.recv_timeout(Duration::from_secs(5)).expect("woken");
            assert_eq!(session.poll(), Some(event));
        };
        zbus::block_on(player.call_method("PlayPause", &())).unwrap();
        expect(MediaEvent::Toggle);
        zbus::block_on(player.call_method("Next", &())).unwrap();
        expect(MediaEvent::Next);
        zbus::block_on(player.call_method("Seek", &(-5_000_000i64))).unwrap();
        expect(MediaEvent::SeekBackward(Duration::from_secs(5)));
        zbus::block_on(player.call_method("SetPosition", &(&track, 30_000_000i64))).unwrap();
        expect(MediaEvent::SeekTo(Duration::from_secs(30)));
        // Another track's, or past the end: let go.
        let other = ObjectPath::try_from("/net/fireturtle/corundum/track/999").unwrap();
        zbus::block_on(player.call_method("SetPosition", &(&other, 1i64))).unwrap();
        zbus::block_on(player.call_method("SetPosition", &(&track, 999_000_000i64))).unwrap();
        zbus::block_on(player.call_method("Stop", &())).unwrap();
        expect(MediaEvent::Stop);

        // Dropped, the player leaves the bus and its cover file goes.
        drop(session);
        let path = Path::new(art.trim_start_matches("file://"));
        assert!(!path.exists(), "{path:?}");
    }

    #[test]
    fn a_cover_is_named_by_its_kind() {
        assert_eq!(image_extension(b"\x89PNG\r\n"), "png");
        assert_eq!(image_extension(&[0xff, 0xd8, 0xff]), "jpg");
        assert_eq!(image_extension(b"GIF89a"), "img");
    }
}
