//! Media page backend: Windows System Media Transport Controls
//! (GlobalSystemMediaTransportControlsSessionManager), album art read through
//! Windows.Storage.Streams.DataReader.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

#[derive(Clone, Default)]
pub struct MediaState {
    pub has_session: bool,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// Position (seconds) at `polled`.
    pub position: f64,
    pub duration: f64,
    pub polled: Option<Instant>,
    pub art: Option<Arc<Vec<u8>>>,
    pub art_id: u64,
}

impl MediaState {
    /// Position extrapolated to "now" while playing.
    pub fn pos_now(&self) -> f64 {
        let mut p = self.position;
        if self.playing {
            if let Some(t) = self.polled {
                p += t.elapsed().as_secs_f64();
            }
        }
        if self.duration > 0.0 {
            p.min(self.duration)
        } else {
            p
        }
    }

    pub fn key(&self) -> String {
        format!("{}\u{1}{}", self.title, self.artist)
    }
}

#[derive(Clone, Copy)]
pub enum MediaCmd {
    PlayPause,
    Next,
    Prev,
    Seek(f64),
}

static STATE: Mutex<Option<MediaState>> = Mutex::new(None);
static DEMO: AtomicBool = AtomicBool::new(false);

pub const DEMO_TITLE: &str = "Senja di Ujung Kota";
pub const DEMO_ARTIST: &str = "Kuro Nesos (demo)";

pub fn set_demo(on: bool) {
    DEMO.store(on, Ordering::SeqCst);
}

pub fn snapshot() -> MediaState {
    STATE.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

fn utc_now_100ns() -> i64 {
    let ft = unsafe { GetSystemTimeAsFileTime() };
    ((ft.dwHighDateTime as i64) << 32) | ft.dwLowDateTime as i64
}

fn read_art(session: &Session) -> Option<Vec<u8>> {
    read_art_steps(session).ok()
}

/// Reads the thumbnail bytes; the error names the step that failed (for --test-media).
fn read_art_steps(session: &Session) -> Result<Vec<u8>, String> {
    let e = |step: &'static str| move |err: windows::core::Error| format!("{}: {}", step, err.message());
    let props = session.TryGetMediaPropertiesAsync().and_then(|op| op.get()).map_err(e("properties"))?;
    let thumb = props.Thumbnail().map_err(e("thumbnail"))?;
    let stream = thumb.OpenReadAsync().and_then(|op| op.get()).map_err(e("open"))?;
    let size = stream.Size().map_err(e("size"))? as u32;
    if size == 0 || size > 16 * 1024 * 1024 {
        return Err(format!("size {}", size));
    }
    let input = stream.GetInputStreamAt(0).map_err(e("input"))?;
    let reader = DataReader::CreateDataReader(&input).map_err(e("reader"))?;
    let loaded = reader.LoadAsync(size).and_then(|op| op.get()).map_err(e("load"))?;
    let mut buf = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut buf).map_err(e("read"))?;
    Ok(buf)
}

/// Polls the current session. Track metadata (a cross-process async call) is only re-read
/// when `refresh_props` is set; playback state and timeline are read every time.
fn poll(manager: &Manager, prev: &MediaState, refresh_props: bool) -> MediaState {
    let Ok(session) = manager.GetCurrentSession() else { return MediaState::default() };
    let mut st = MediaState { has_session: true, ..Default::default() };
    if refresh_props || !prev.has_session {
        if let Ok(props) = session.TryGetMediaPropertiesAsync().and_then(|op| op.get()) {
            st.title = props.Title().map(|s| s.to_string()).unwrap_or_default();
            st.artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
        }
    } else {
        st.title = prev.title.clone();
        st.artist = prev.artist.clone();
    }
    if let Ok(info) = session.GetPlaybackInfo() {
        st.playing = info.PlaybackStatus().map(|s| s == Status::Playing).unwrap_or(false);
    }
    if let Ok(tl) = session.GetTimelineProperties() {
        let pos = tl.Position().map(|t| t.Duration).unwrap_or(0) as f64 / 1e7;
        let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0) as f64 / 1e7;
        let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0) as f64 / 1e7;
        let mut p = pos - start;
        if st.playing {
            if let Ok(updated) = tl.LastUpdatedTime() {
                let age = (utc_now_100ns() - updated.UniversalTime) as f64 / 1e7;
                if (0.0..3600.0).contains(&age) {
                    p += age;
                }
            }
        }
        st.duration = (end - start).max(0.0);
        st.position = if st.duration > 0.0 { p.clamp(0.0, st.duration) } else { p.max(0.0) };
    }
    st.polled = Some(Instant::now());
    if st.key() == prev.key() && prev.art.is_some() {
        st.art = prev.art.clone();
        st.art_id = prev.art_id;
    } else if !st.title.is_empty() {
        st.art = read_art(&session).map(Arc::new);
        st.art_id = prev.art_id + 1;
    }
    st
}

/// One-shot read of the current session, for `--test-media`.
pub fn describe_current() -> String {
    let Ok(manager) = Manager::RequestAsync().and_then(|op| op.get()) else { return "media manager unavailable".into() };
    let st = poll(&manager, &MediaState::default(), true);
    if !st.has_session {
        return "no media session".into();
    }
    format!(
        "title='{}' artist='{}' playing={} position={:.1}s duration={:.1}s art={}",
        st.title,
        st.artist,
        st.playing,
        st.position,
        st.duration,
        match manager.GetCurrentSession().map_err(|e| e.message().to_string()).and_then(|s| read_art_steps(&s)) {
            Ok(b) => format!("{} bytes", b.len()),
            Err(why) => format!("none ({})", why),
        }
    )
}

struct Demo {
    playing: bool,
    base: f64,
    since: Instant,
}

impl Demo {
    fn pos(&self) -> f64 {
        let p = self.base + if self.playing { self.since.elapsed().as_secs_f64() } else { 0.0 };
        p % 96.0
    }

    fn state(&self) -> MediaState {
        MediaState {
            has_session: true,
            title: DEMO_TITLE.into(),
            artist: DEMO_ARTIST.into(),
            playing: self.playing,
            position: self.pos(),
            duration: 96.0,
            polled: Some(Instant::now()),
            art: None,
            art_id: 0,
        }
    }

    fn apply(&mut self, cmd: MediaCmd) {
        let p = self.pos();
        match cmd {
            MediaCmd::PlayPause => self.playing = !self.playing,
            MediaCmd::Next | MediaCmd::Prev => {
                self.base = 0.0;
                self.since = Instant::now();
                return;
            }
            MediaCmd::Seek(s) => {
                self.base = s;
                self.since = Instant::now();
                return;
            }
        }
        self.base = p;
        self.since = Instant::now();
    }
}

pub fn spawn() -> Sender<MediaCmd> {
    let (tx, rx) = channel::<MediaCmd>();
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let mut demo = Demo { playing: true, base: 0.0, since: Instant::now() };
        let manager = Manager::RequestAsync().and_then(|op| op.get()).ok();
        let mut last = MediaState::default();
        let mut n: u64 = 0;
        loop {
            n += 1;
            let wait = if last.has_session || DEMO.load(Ordering::SeqCst) { 500 } else { 1000 };
            let cmd = match rx.recv_timeout(Duration::from_millis(wait)) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            };
            if DEMO.load(Ordering::SeqCst) {
                if let Some(c) = cmd {
                    demo.apply(c);
                }
                last = demo.state();
            } else if let Some(manager) = &manager {
                if let (Some(c), Ok(session)) = (cmd, manager.GetCurrentSession()) {
                    let _ = match c {
                        MediaCmd::PlayPause => session.TryTogglePlayPauseAsync().map(|_| ()),
                        MediaCmd::Next => session.TrySkipNextAsync().map(|_| ()),
                        MediaCmd::Prev => session.TrySkipPreviousAsync().map(|_| ()),
                        MediaCmd::Seek(s) => session.TryChangePlaybackPositionAsync((s * 1e7) as i64).map(|_| ()),
                    };
                    std::thread::sleep(Duration::from_millis(150));
                }
                // metadata every 2 s, or right after a command (track may have changed)
                last = poll(manager, &last, cmd.is_some() || n % 4 == 0 || !last.has_session);
            }
            if let Ok(mut g) = STATE.lock() {
                *g = Some(last.clone());
            }
        }
    });
    tx
}
