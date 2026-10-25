//! Notch state machine: App state, periodic logic and the notch shape for each mode.
//! Sizes are 96-DPI logical units. Painting, input, pages, alerts, music, mascot and
//! window helpers live in the child modules declared below.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Graphics::Gdi::{CreateRectRgn, SetWindowRgn};
use windows::Win32::System::SystemInformation::{GetLocalTime, GetTickCount};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetLastInputInfo, SetFocus, LASTINPUTINFO, VK_CAPITAL};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::gfx::{rotation, Align, Gfx};
use crate::lyrics::{self, Status as LyricsStatus};
use crate::media::{self, MediaCmd, MediaState};
use crate::model::{self, Dock, Settings, Task, Water, WaterEvent, GOALS};
use crate::shelf::{self, Kind, Shelf};
use crate::sys::{self, glyph, SysAlert};

mod alerts;
mod anim;
mod home;
mod input;
mod mascot;
mod music;
mod pages;
mod paint;
mod vertical;
mod window;

use anim::{ease, mix, Fx, Spring};
use home::page_of;
pub use window::{activate, open_path, place_window};
use window::{cursor_screen, mouse_buttons, set_click_through};

/// Window size (logical px) for each dock: wide and short at the top edge, narrow and tall
/// against a side edge (the side layouts are vertical, see ui_vertical.rs).
pub fn window_size(dock: Dock) -> (f32, f32) {
    match dock {
        Dock::Top => (640.0, 190.0),
        Dock::Left | Dock::Right => (270.0, 480.0),
    }
}
const SHOULDER: f32 = 6.0;

// palette
const WHITE: u32 = 0xFFFFFF;
const GREY: u32 = 0x8C8C8C;
const DIM: u32 = 0x5E5E5E;
const RED: u32 = 0xFF4F4F;
const BLUE: u32 = 0x4DB6FF;
const CYAN: u32 = 0x66DBFF;
const GREEN: u32 = 0x40B854;
const BATTERY_GREEN: u32 = 0x34E05C;
const ORANGE: u32 = 0xFF9F0A;
// Kuro amber: black surfaces, hairlines, and one accent for whatever is active
const AMBER: u32 = 0xF5C542;
/// Text and icons drawn on amber.
const ON_AMBER: u32 = 0x1A1300;
const CARD: u32 = 0x131313;
const HAIR: u32 = 0x222222;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Home,
    Media,
    Pomodoro,
    Water,
    Shelf,
    Todo,
    Settings,
}

const PAGES: [Page; 7] = [Page::Home, Page::Media, Page::Pomodoro, Page::Water, Page::Shelf, Page::Todo, Page::Settings];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Act {
    PagePrev,
    PageNext,
    Dot(usize),
    MediaPrev,
    MediaPlay,
    MediaNext,
    MediaSeek,
    Lyrics,
    Romaji,
    PomoToggle,
    PomoReset,
    PomoShort,
    PomoFocus,
    PomoLong,
    WaterOn,
    WaterDrink,
    WaterSettings,
    WaterBack,
    Bottle(i32),
    Goal(usize),
    Start(i32),
    End(i32),
    WaterReset,
    ShelfItem(usize),
    ShelfClear,
    /// The small "x" on a Shelf card.
    ShelfRemove(usize),
    TodoAdd,
    TodoEdit,
    TodoRow(usize),
    TodoCheck(usize),
    TodoDelete,
    TodoCancel,
    TodoSave,
    TodoTitle,
    TodoTime,
    Alert(usize),
    /// The panel header: drag it to move the notch along an edge or to another edge.
    Grab,
    /// A Home tile: opens that page.
    Tile(usize),
    /// A Settings row (0 autostart, 1 cat, 2 romaji, 3 lyrics, 4 minimal, 5 screenshots, 6 reset position).
    Setting(u8),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Collapsed,
    Minimal,
    Expanded,
    Alert,
    Live,
    Lyrics,
    Timer,
    Mascot,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum AlertAction {
    Drank,
    Refilled,
    Snooze,
    TaskStart(usize),
    TaskLater(usize),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum AlertIcon {
    Glyph(u16, u32),
    CapsOn,
    CapsOff,
    Battery(bool, u8),
}

#[derive(Clone)]
struct Alert {
    icon: AlertIcon,
    title: String,
    value: String,
    color: u32,
    actions: Vec<(String, AlertAction)>,
    ttl: Duration,
    born: Option<Instant>,
    sound: bool,
    /// Two-line card (used for mirrored Windows notifications).
    card: bool,
}

impl Alert {
    fn simple(icon: AlertIcon, title: &str, value: &str, color: u32) -> Self {
        Alert { icon, title: title.into(), value: value.into(), color, actions: Vec::new(), ttl: Duration::from_millis(2600), born: None, sound: false, card: false }
    }
    fn big(&self) -> bool {
        self.card || !self.actions.is_empty()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum PMode {
    Focus,
    Short,
    Long,
}

impl PMode {
    fn secs(self) -> f64 {
        match self {
            PMode::Focus => 25.0 * 60.0,
            PMode::Short => 5.0 * 60.0,
            PMode::Long => 15.0 * 60.0,
        }
    }
    fn label(self) -> &'static str {
        match self {
            PMode::Focus => "Focus time",
            PMode::Short => "Short break",
            PMode::Long => "Long break",
        }
    }
}

struct Pomodoro {
    mode: PMode,
    remaining: f64,
    running: bool,
    last: Instant,
    sessions: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum MascotState {
    Off,
    Sleeping(Instant),
    Waking(Instant),
}

struct Form {
    title: String,
    digits: String,
    field: u8,
    edit: Option<usize>,
}

struct Hit {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    act: Act,
}

pub enum Effect {
    DragOut(PathBuf),
    Activate,
    Deactivate,
}

#[derive(Default, Clone)]
pub struct Demo {
    pub alerts: bool,
    pub lyrics: bool,
    pub mascot: bool,
    pub pin: bool,
    pub page: Option<usize>,
}

pub struct App {
    pub hwnd: HWND,
    /// None while the renderer is being recreated after a device loss.
    gfx: Option<Gfx>,
    gfx_lost: Option<Instant>,
    px: (u32, u32),
    /// Window size in logical px (depends on the dock).
    win: (f32, f32),
    fs_since: Option<Instant>,
    scale: f32,
    w: Spring,
    h: Spring,
    r: Spring,
    shown: Mode,
    content_a: f32,
    hover: bool,
    hover_leave: Option<Instant>,
    /// Whole-notch opacity: the closed notch fades while the cursor is over it.
    ghost: f32,
    /// The window takes mouse input; otherwise clicks go through to the window below.
    solid: bool,
    /// Cursor over the notch shape / within reach of it.
    inside: bool,
    near: bool,
    /// Where and since when the cursor rests on the closed notch (it opens after a moment).
    dwell: Option<(Instant, (f32, f32))>,
    /// A mouse button was already down when the cursor arrived (a file is being dragged in).
    drag_in: bool,
    /// A click went through to the window below: stay closed until the cursor leaves.
    blocked: bool,
    /// Moving the notch by its header: (grab offset along the edge in px, moved yet, start).
    drag: Option<(f32, bool, POINT)>,
    page: usize,
    slide: Spring,
    /// Animated pagination indicator position (page index).
    dot: Spring,
    water_settings: bool,
    /// When "Drink" was last pressed (a drop falls into the bottle).
    drink_at: Option<Instant>,
    /// When the track last changed (the album art bumps).
    track_change: Instant,
    reset_confirm: Option<Instant>,
    tasks: Vec<Task>,
    task_sel: Option<usize>,
    task_scroll: usize,
    form: Option<Form>,
    media: MediaState,
    media_tx: Sender<MediaCmd>,
    art: Option<(u64, ID2D1Bitmap)>,
    lyrics: lyrics::Shared,
    lyric_idx: Option<usize>,
    lyric_change: Instant,
    pomo: Pomodoro,
    water: Water,
    shelf: Shelf,
    shelf_drag: bool,
    /// Decoded thumbnails of image items on the Shelf (path, bitmap), most recent last.
    thumbs: RefCell<Vec<(PathBuf, Option<ID2D1Bitmap>)>>,
    /// Loudness and beat of the music, for the equalizer bars.
    beat: crate::audio::Beat,
    press: Option<(f32, f32, Act)>,
    settings: Settings,
    /// "Start with Windows" (registry), cached for the Settings page.
    autostart: bool,
    queue: VecDeque<Alert>,
    alert: Option<Alert>,
    mascot: MascotState,
    caps: bool,
    keyboard: String,
    battery: (bool, u8),
    fullscreen_hidden: bool,
    hits: RefCell<Vec<Hit>>,
    /// Hover/press/toggle animations of the panel controls.
    fx: Fx,
    off: Cell<(f32, f32)>,
    /// Top of the notch shape in window coordinates (0 when docked at the top edge).
    top: Cell<f32>,
    hot: Option<Act>,
    mouse: (f32, f32),
    sys_rx: Receiver<SysAlert>,
    last_tick: Instant,
    last_slow: Instant,
    last_check: Instant,
    /// Clock stamp of the last frame (seconds or minutes, see `clock_stamp`).
    last_stamp: u32,
    region: (i32, i32, i32, i32),
    dirty: bool,
    pub effect: Option<Effect>,
    /// Timer interval the window should use next (ms): fast only while something moves.
    pub interval: u32,
    idle_since: Option<Instant>,
    last_trim: Option<Instant>,
    demo: Demo,
    demo_start: Instant,
    demo_step: usize,
    started: Instant,
}

fn idle_ms() -> u32 {
    unsafe {
        let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if GetLastInputInfo(&mut li).as_bool() {
            GetTickCount().wrapping_sub(li.dwTime)
        } else {
            0
        }
    }
}

fn caps_on() -> bool {
    unsafe { GetKeyState(VK_CAPITAL.0 as i32) & 1 == 1 }
}

fn date_strings() -> (String, String) {
    let t = unsafe { GetLocalTime() };
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    (
        format!("{} {} {} {}", DAYS[(t.wDayOfWeek % 7) as usize], t.wDay, MONTHS[(t.wMonth.clamp(1, 12) - 1) as usize], t.wYear),
        format!("{:02}:{:02}", t.wHour, t.wMinute),
    )
}

fn mmss(s: f64) -> String {
    let s = s.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Largest font size between `max` and `min` (0.5 steps) at which `s` fits in `width`.
fn fit_size(g: &Gfx, s: &str, width: f32, max: f32, min: f32, weight: u32) -> f32 {
    let mut size = max;
    while size > min && g.measure(s, size, weight) > width {
        size -= 0.5;
    }
    size
}

impl App {
    pub fn new(hwnd: HWND, gfx: Gfx, px: (u32, u32), scale: f32, sys_rx: Receiver<SysAlert>, demo: Demo) -> Self {
        let media_tx = media::spawn();
        if demo.lyrics {
            media::set_demo(true);
        }
        let b = sys::battery();
        let settings = Settings::load();
        let win = window_size(settings.dock);
        let page = demo.page.unwrap_or(0).min(PAGES.len() - 1);
        App {
            hwnd,
            gfx: Some(gfx),
            gfx_lost: None,
            px,
            win,
            fs_since: None,
            scale,
            w: Spring::new(150.0),
            h: Spring::new(30.0),
            r: Spring::new(10.0),
            shown: Mode::Collapsed,
            content_a: 1.0,
            hover: false,
            hover_leave: None,
            ghost: 1.0,
            solid: false,
            inside: false,
            near: false,
            dwell: None,
            drag_in: false,
            blocked: false,
            drag: None,
            page,
            slide: Spring::new(0.0),
            dot: Spring::new(page as f32),
            water_settings: false,
            drink_at: None,
            track_change: Instant::now(),
            reset_confirm: None,
            tasks: model::load_tasks(),
            task_sel: None,
            task_scroll: 0,
            form: None,
            media: MediaState::default(),
            media_tx,
            art: None,
            lyrics: lyrics::shared(),
            lyric_idx: None,
            lyric_change: Instant::now(),
            pomo: Pomodoro { mode: PMode::Focus, remaining: PMode::Focus.secs(), running: false, last: Instant::now(), sessions: 0 },
            water: Water::load(),
            shelf: Shelf::load(),
            shelf_drag: false,
            thumbs: RefCell::new(Vec::new()),
            beat: crate::audio::Beat::new(),
            press: None,
            settings,
            autostart: model::autostart_enabled(),
            queue: VecDeque::new(),
            alert: None,
            mascot: MascotState::Off,
            caps: caps_on(),
            keyboard: sys::keyboard_layout(),
            battery: (b.plugged, b.percent),
            fullscreen_hidden: false,
            hits: RefCell::new(Vec::new()),
            fx: Fx::default(),
            off: Cell::new((0.0, 0.0)),
            top: Cell::new(0.0),
            hot: None,
            mouse: (-1.0, -1.0),
            sys_rx,
            last_tick: Instant::now(),
            last_slow: Instant::now(),
            last_check: Instant::now(),
            last_stamp: u32::MAX,
            region: (0, 0, 0, 0),
            dirty: true,
            effect: None,
            interval: 16,
            idle_since: None,
            last_trim: None,
            demo,
            demo_start: Instant::now(),
            demo_step: 0,
            started: Instant::now(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.clone()
    }

    pub fn toggle_pet(&mut self) {
        self.settings.pet = !self.settings.pet;
        self.settings.save();
        self.mascot = MascotState::Off;
    }

    /// Re-reads "Start with Windows" after the tray menu changed it.
    pub fn sync_autostart(&mut self) {
        self.autostart = model::autostart_enabled();
        self.dirty = true;
    }

    pub fn toggle_minimal(&mut self) {
        self.settings.minimal = !self.settings.minimal;
        self.settings.save();
    }

    pub fn toggle_romaji(&mut self) {
        self.settings.romaji = !self.settings.romaji;
        self.settings.save();
        self.dirty = true;
    }

    pub fn set_dock(&mut self, dock: Dock) {
        self.settings.dock = dock;
        self.settings.save();
        self.win = window_size(dock);
        self.px = ((self.win.0 * self.scale).round() as u32, (self.win.1 * self.scale).round() as u32);
        // the swap chain has the window's size, so the renderer is rebuilt for the new one
        self.gfx_lost = Instant::now().checked_sub(Duration::from_secs(1));
        self.region = (0, 0, 0, 0);
        self.dirty = true;
    }

    /// Top-left of a w x h notch shape in window coordinates for the current dock.
    fn shape_pos(&self, w: f32, h: f32) -> (f32, f32) {
        match self.settings.dock {
            Dock::Top => (self.win.0 / 2.0 - w / 2.0, 0.0),
            Dock::Left => (0.0, (self.win.1 - h) / 2.0),
            Dock::Right => (self.win.0 - w, (self.win.1 - h) / 2.0),
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32().min(0.05);
        self.last_tick = now;
        if self.gfx_lost.is_some() {
            self.recover_renderer();
            if self.gfx.is_none() {
                self.interval = 250;
                return;
            }
        }

        while let Ok(s) = self.sys_rx.try_recv() {
            self.sys_alert(s);
        }
        if self.demo.alerts {
            self.demo_script();
        }

        if now.duration_since(self.last_slow) >= Duration::from_millis(250) {
            self.last_slow = now;
            self.slow_checks();
        }
        if now.duration_since(self.last_check) >= Duration::from_secs(5) {
            self.last_check = now;
            self.schedule_checks();
        }

        self.update_media();
        self.beat.update(dt, self.media.playing);
        self.update_pomodoro();
        self.update_alerts(now);
        self.update_mascot(now);
        self.update_hover(now);

        // shape animation
        let target = self.target_mode();
        let (tw, th, tr) = self.shape_for(target);
        let mut moving = self.w.step(tw, dt);
        moving |= self.h.step(th, dt);
        moving |= self.r.step(tr, dt);
        moving |= self.slide.step(0.0, dt);
        moving |= self.dot.step(self.page as f32, dt);
        moving |= self.fx.step(self.hot, dt);
        moving |= self.drink_at.is_some_and(|t| t.elapsed() < Duration::from_millis(1000));
        if target != self.shown {
            self.content_a = (self.content_a - dt * 12.0).max(0.0);
            if self.content_a <= 0.0 {
                self.shown = target;
            }
            moving = true;
        } else if self.content_a < 1.0 {
            let close = (self.w.v - tw).abs() < 60.0 && (self.h.v - th).abs() < 30.0;
            if close {
                self.content_a = (self.content_a + dt * 7.0).min(1.0);
            }
            moving = true;
        }
        // the closed notch fades while the cursor is over it, so what is underneath shows
        let ghost_to = if self.inside && !self.hover && !self.drag_in && !self.wants_input() { 0.16 } else { 1.0 };
        if (self.ghost - ghost_to).abs() > 0.001 {
            let step = dt * 7.0;
            self.ghost = if ghost_to < self.ghost { (self.ghost - step).max(ghost_to) } else { (self.ghost + step).min(ghost_to) };
            moving = true;
        }
        let solid = self.wants_input();
        if solid != self.solid {
            self.solid = solid;
            set_click_through(self.hwnd, !solid);
        }
        self.update_region();

        let stamp = self.clock_stamp();
        let live = self.live_animation();
        let hovering_soon = self.dwell.is_some() || self.hover_leave.is_some();
        if self.fullscreen_hidden {
            // nothing is visible while a fullscreen app (e.g. a game) is in front
            self.interval = 250;
            return;
        }
        if moving || live || stamp != self.last_stamp || self.dirty {
            self.last_stamp = stamp;
            self.dirty = false;
            if !self.render() && self.gfx_lost.is_none() {
                self.gfx_lost = Some(Instant::now());
            }
        }
        let sleeping = matches!(self.mascot, MascotState::Sleeping(t) if t.elapsed() > Duration::from_secs(1));
        // Frame pacing (see AGENTS.md): display refresh only while something moves; looping
        // decorations (equalizer, alert timer) at 30 fps; the sleeping cat and the open panel
        // at 10 fps; the resting notch wakes 4x a second (it only redraws when the minute turns).
        let resting = matches!(self.shown, Mode::Collapsed | Mode::Minimal) && !self.near && !self.hover;
        self.interval = if moving || hovering_soon {
            16
        } else if (live && !sleeping) || (self.near && !self.hover) {
            // near the notch: poll often enough to notice clicks that go through it
            33
        } else if resting {
            250
        } else {
            100
        };

        // give unused memory back: after 3 s of calm (at most once a minute), and every
        // 2 minutes anyway (e.g. while music keeps the notch animating)
        let calm = self.interval >= 100;
        if calm {
            self.idle_since.get_or_insert(now);
        } else {
            self.idle_since = None;
        }
        let calm_long = self.idle_since.is_some_and(|t| t.elapsed() >= Duration::from_secs(3));
        let since_trim = self.last_trim.map_or(self.started.elapsed(), |t| t.elapsed());
        if (calm_long && since_trim >= Duration::from_secs(60)) || since_trim >= Duration::from_secs(120) || (self.last_trim.is_none() && since_trim >= Duration::from_secs(5)) {
            sys::trim_memory();
            self.last_trim = Some(now);
        }
    }

    /// Seconds when something on screen shows them (music position, running timer),
    /// otherwise minutes: the resting clock only needs a frame when the minute turns.
    fn clock_stamp(&self) -> u32 {
        let n = model::now();
        let page = PAGES[self.page];
        let seconds = self.shown == Mode::Expanded && ((page == Page::Media && self.media.has_session) || page == Page::Pomodoro || (page == Page::Home && (self.pomo.running || self.media.playing)));
        if seconds {
            n.minutes * 60 + n.second
        } else {
            n.minutes * 60
        }
    }

    fn live_animation(&self) -> bool {
        match self.shown {
            Mode::Live | Mode::Lyrics | Mode::Mascot | Mode::Alert | Mode::Timer => true,
            // the water page keeps its surface waving while it is open
            Mode::Expanded => self.media.playing || self.pomo.running || self.shelf_drag || PAGES[self.page] == Page::Water,
            _ => false,
        }
    }

    fn slow_checks(&mut self) {
        let caps = caps_on();
        if caps != self.caps {
            self.caps = caps;
            let mut a = Alert::simple(if caps { AlertIcon::CapsOn } else { AlertIcon::CapsOff }, "Caps Lock", if caps { "On" } else { "Off" }, if caps { GREEN } else { GREY });
            a.ttl = Duration::from_millis(1800);
            self.push(a);
        }
        let kb = sys::keyboard_layout();
        if kb != self.keyboard && !kb.is_empty() {
            self.keyboard = kb.clone();
            self.push(Alert::simple(AlertIcon::Glyph(glyph::KEYBOARD, WHITE), "Keyboard", &kb, WHITE));
        }
        let b = sys::battery();
        let (was_plugged, was_pct) = self.battery;
        if b.present {
            if b.plugged != was_plugged {
                let (t, v) = if b.plugged { ("Charger connected", "Charging") } else { ("Charger unplugged", "") };
                let v = if v.is_empty() { format!("{}% remaining", b.percent) } else { format!("{} · {}%", v, b.percent) };
                self.push(Alert::simple(AlertIcon::Battery(b.plugged, b.percent), t, &v, if b.plugged { BATTERY_GREEN } else { GREY }));
            } else if !b.plugged && b.percent != was_pct {
                if b.percent == 20 {
                    self.push(Alert::simple(AlertIcon::Battery(false, 20), "Low battery", "Plug in soon", ORANGE));
                } else if b.percent == 10 {
                    self.push(Alert::simple(AlertIcon::Battery(false, 10), "Battery critical", "Plug in now", RED));
                }
            } else if b.plugged && b.percent == 100 && was_pct < 100 {
                self.push(Alert::simple(AlertIcon::Battery(true, 100), "Fully charged", "You can unplug", BATTERY_GREEN));
            }
        }
        self.battery = (b.plugged, b.percent);

        // hide while a fullscreen app is in front (games, video, slideshows)
        // hide only after a fullscreen app has been in front for a while, and never while the
        // cursor is near the notch
        let (cx, cy) = self.cursor_logical();
        let (sx, sy) = self.shape_pos(self.w.v, self.h.v);
        let near = cx > sx - 60.0 && cx < sx + self.w.v + 60.0 && cy > sy - 40.0 && cy < sy + self.h.v + 40.0;
        let candidate = !self.hover && !near && self.form.is_none() && sys::foreground_fullscreen(self.hwnd);
        let fs = if candidate {
            let since = *self.fs_since.get_or_insert(Instant::now());
            since.elapsed() >= Duration::from_millis(750)
        } else {
            self.fs_since = None;
            false
        };
        if fs != self.fullscreen_hidden {
            self.fullscreen_hidden = fs;
            sys::log(&format!("notch {} (foreground: {})", if fs { "hidden for fullscreen app" } else { "shown" }, sys::foreground_class()));
            unsafe {
                let _ = ShowWindow(self.hwnd, if fs { SW_HIDE } else { SW_SHOWNOACTIVATE });
            }
        }
        if self.reset_confirm.map_or(false, |t| t.elapsed() > Duration::from_secs(3)) {
            self.reset_confirm = None;
            self.dirty = true;
        }
    }

    fn schedule_checks(&mut self) {
        match self.water.check() {
            Some(WaterEvent::Drink) => {
                let each = self.water.each();
                self.push_action_alert(AlertIcon::Glyph(glyph::DROP, CYAN), "Time to Drink", &format!("Stay hydrated! {} ml", each), BLUE, vec![("Drank".into(), AlertAction::Drank), ("Snooze".into(), AlertAction::Snooze)]);
                self.water.save();
            }
            Some(WaterEvent::Refill) => {
                self.push_action_alert(AlertIcon::Glyph(glyph::DROP, CYAN), "Time to Refill", "Your bottle is probably empty", BLUE, vec![("Refilled".into(), AlertAction::Refilled), ("Snooze".into(), AlertAction::Snooze)]);
                self.water.save();
            }
            _ => {}
        }
        for i in model::due_tasks(&mut self.tasks) {
            let title = self.tasks[i].title.clone();
            self.push_action_alert(AlertIcon::Glyph(glyph::CHECKLIST, GREEN), "Task Time!", &title, GREEN, vec![("Start".into(), AlertAction::TaskStart(i)), ("Later".into(), AlertAction::TaskLater(i))]);
        }
    }

    fn update_pomodoro(&mut self) {
        if !self.pomo.running {
            return;
        }
        let el = self.pomo.last.elapsed().as_secs_f64();
        self.pomo.last = Instant::now();
        self.pomo.remaining -= el;
        if self.pomo.remaining <= 0.0 {
            let (title, value, next) = match self.pomo.mode {
                PMode::Focus => {
                    self.pomo.sessions += 1;
                    if self.pomo.sessions % 4 == 0 {
                        ("Long Break!", "15 min, you earned it", PMode::Long)
                    } else {
                        ("Break Time!", "5 minute short break", PMode::Short)
                    }
                }
                _ => ("Focus Time!", "Back to work, 25 min", PMode::Focus),
            };
            let mut a = Alert::simple(AlertIcon::Glyph(glyph::STOPWATCH, AMBER), title, value, AMBER);
            a.sound = true;
            a.ttl = Duration::from_secs(5);
            self.push(a);
            self.pomo.mode = next;
            self.pomo.remaining = next.secs();
            self.pomo.running = next != PMode::Focus;
        }
    }

    fn target_mode(&self) -> Mode {
        let lyric = self.settings.lyrics && self.media.playing && self.lyric_idx.is_some();
        if self.form.is_some() || self.hover || self.shelf_drag {
            Mode::Expanded
        } else if self.alert.is_some() {
            Mode::Alert
        } else if lyric {
            Mode::Lyrics
        } else if self.media.playing {
            Mode::Live
        } else if self.pomo.running {
            Mode::Timer
        } else if self.mascot != MascotState::Off {
            Mode::Mascot
        } else if self.settings.minimal {
            Mode::Minimal
        } else {
            Mode::Collapsed
        }
    }

    fn shape_for(&self, m: Mode) -> (f32, f32, f32) {
        if self.vertical() {
            return self.shape_for_v(m);
        }
        match m {
            Mode::Expanded => (420.0, 150.0, 28.0),
            Mode::Alert => {
                if self.alert.as_ref().map_or(false, |a| !a.actions.is_empty()) {
                    (380.0, 80.0, 24.0)
                } else if self.alert.as_ref().map_or(false, |a| a.card) {
                    (380.0, 60.0, 22.0)
                } else {
                    (320.0, 35.0, 13.0)
                }
            }
            Mode::Lyrics => {
                // the pill grows/shrinks with the current line (like Dynamic Island)
                let w = self.gfx.as_ref().map_or(360.0, |g| (g.measure(&self.lyric_text(), 13.0, 600) + 120.0).clamp(230.0, 600.0));
                (w, 30.0, 11.0)
            }
            // without a lyric line the pill only needs the cover and the equalizer
            Mode::Live => (118.0, 30.0, 11.0),
            Mode::Timer => (200.0, 30.0, 11.0),
            Mode::Minimal => (110.0, 22.0, 8.0),
            // the notch becomes the cat's head
            Mode::Mascot => (96.0, 38.0, 18.0),
            Mode::Collapsed => (150.0, 30.0, 10.0),
        }
    }

    fn update_region(&mut self) {
        let s = self.scale;
        let (w, h) = (self.w.v, self.h.v);
        let (sx, sy) = self.shape_pos(w, h);
        // the cat's whiskers, paws and tail reach out of the notch shape
        let pad = if self.shown == Mode::Mascot { 32.0 } else { SHOULDER + 6.0 };
        let r = (
            ((sx - pad).max(0.0) * s) as i32,
            ((sy - pad).max(0.0) * s) as i32,
            ((sx + w + pad).min(self.win.0) * s).ceil() as i32,
            ((sy + h + pad).min(self.win.1) * s).ceil() as i32,
        );
        if r != self.region {
            self.region = r;
            unsafe {
                let rgn = CreateRectRgn(r.0, r.1, r.2, r.3);
                SetWindowRgn(self.hwnd, rgn, false);
            }
        }
    }
}
