//! Notch state machine, layout and painting. Sizes are 96-DPI logical units.

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

#[path = "ui_vertical.rs"]
mod vertical;

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

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Media,
    Pomodoro,
    Water,
    Shelf,
    Todo,
}

const PAGES: [Page; 5] = [Page::Media, Page::Pomodoro, Page::Water, Page::Shelf, Page::Todo];

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
    WaterOn,
    WaterDrink,
    WaterSettings,
    WaterSub(u8),
    WaterBack,
    Bottle(i32),
    Goal(usize),
    Start(i32),
    End(i32),
    WaterReset,
    ShelfItem(usize),
    ShelfClear,
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

struct Spring {
    v: f32,
    vel: f32,
}

impl Spring {
    fn new(v: f32) -> Self {
        Spring { v, vel: 0.0 }
    }
    /// Slightly under-damped spring; returns true while still moving.
    fn step(&mut self, target: f32, dt: f32) -> bool {
        let (k, c) = (230.0, 24.0);
        let mut t = 0.0;
        while t < dt {
            let h = (dt - t).min(1.0 / 240.0);
            let a = k * (target - self.v) - c * self.vel;
            self.vel += a * h;
            self.v += self.vel * h;
            t += h;
        }
        if (target - self.v).abs() < 0.05 && self.vel.abs() < 0.05 {
            self.v = target;
            self.vel = 0.0;
            return false;
        }
        true
    }
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
    hover_enter: Option<Instant>,
    hover_leave: Option<Instant>,
    page: usize,
    slide: Spring,
    /// Animated pagination indicator position (page index).
    dot: Spring,
    water_settings: bool,
    water_sub: u8,
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
    press: Option<(f32, f32, Act)>,
    settings: Settings,
    queue: VecDeque<Alert>,
    alert: Option<Alert>,
    mascot: MascotState,
    caps: bool,
    keyboard: String,
    battery: (bool, u8),
    fullscreen_hidden: bool,
    hits: RefCell<Vec<Hit>>,
    off: Cell<(f32, f32)>,
    /// Top of the notch shape in window coordinates (0 when docked at the top edge).
    top: Cell<f32>,
    hot: Option<Act>,
    mouse: (f32, f32),
    sys_rx: Receiver<SysAlert>,
    last_tick: Instant,
    last_slow: Instant,
    last_check: Instant,
    last_second: u32,
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

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
            hover_enter: None,
            hover_leave: None,
            page,
            slide: Spring::new(0.0),
            dot: Spring::new(page as f32),
            water_settings: false,
            water_sub: 0,
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
            press: None,
            settings,
            queue: VecDeque::new(),
            alert: None,
            mascot: MascotState::Off,
            caps: caps_on(),
            keyboard: sys::keyboard_layout(),
            battery: (b.plugged, b.percent),
            fullscreen_hidden: false,
            hits: RefCell::new(Vec::new()),
            off: Cell::new((0.0, 0.0)),
            top: Cell::new(0.0),
            hot: None,
            mouse: (-1.0, -1.0),
            sys_rx,
            last_tick: Instant::now(),
            last_slow: Instant::now(),
            last_check: Instant::now(),
            last_second: 99,
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

    fn push(&mut self, a: Alert) {
        // replace an identical pending alert (e.g. Caps Lock pressed repeatedly)
        if let Some(cur) = &mut self.alert {
            if cur.title == a.title && !cur.big() && !a.big() {
                cur.value = a.value;
                cur.color = a.color;
                cur.icon = a.icon;
                cur.born = Some(Instant::now());
                self.dirty = true;
                return;
            }
        }
        self.queue.retain(|q| q.title != a.title || q.big());
        self.queue.push_back(a);
    }

    fn sys_alert(&mut self, s: SysAlert) {
        let mut a = Alert::simple(AlertIcon::Glyph(s.icon, s.color), &s.title, &s.value, s.color);
        if s.icon == glyph::MESSAGE {
            a.ttl = Duration::from_millis(5000);
            a.sound = true;
            a.card = true;
        }
        self.push(a);
    }

    // ------------------------------------------------------------ periodic logic

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
        self.update_region();

        let sec = model::now().second;
        let live = self.live_animation();
        let hovering_soon = self.hover_enter.is_some() || self.hover_leave.is_some();
        if self.fullscreen_hidden {
            // nothing is visible while a fullscreen app (e.g. a game) is in front
            self.interval = 250;
            return;
        }
        if moving || live || sec != self.last_second || self.dirty {
            self.last_second = sec;
            self.dirty = false;
            if !self.render() && self.gfx_lost.is_none() {
                self.gfx_lost = Some(Instant::now());
            }
        }
        let sleeping = matches!(self.mascot, MascotState::Sleeping(t) if t.elapsed() > Duration::from_secs(1));
        // 60 fps only while something moves; idle clock and the slowly "breathing" sleeping
        // mascot are fine at 10 fps
        self.interval = if moving || hovering_soon || (live && !sleeping) { 16 } else { 100 };

        // give unused memory back: after 3 s of calm (at most once a minute), and every
        // 2 minutes anyway (e.g. while music keeps the notch animating)
        let calm = self.interval == 100;
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

    fn live_animation(&self) -> bool {
        match self.shown {
            Mode::Live | Mode::Lyrics | Mode::Mascot | Mode::Alert | Mode::Timer => true,
            Mode::Expanded => self.media.playing || self.pomo.running || self.shelf_drag,
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

    fn push_action_alert(&mut self, icon: AlertIcon, title: &str, value: &str, color: u32, actions: Vec<(String, AlertAction)>) {
        let mut a = Alert::simple(icon, title, value, color);
        a.actions = actions;
        a.ttl = Duration::from_secs(10);
        a.sound = true;
        self.push(a);
    }

    fn update_media(&mut self) {
        let m = media::snapshot();
        if m.art_id != self.media.art_id || (m.art.is_some() != self.art.is_some()) {
            self.art = m.art.as_ref().and_then(|bytes| self.gfx.as_ref().and_then(|g| g.bitmap_from_bytes(bytes))).map(|b| (m.art_id, b));
            self.dirty = true;
        }
        if m.key() != self.media.key() || m.playing != self.media.playing {
            self.dirty = true;
        }
        self.media = m;
        if !self.media.title.is_empty() && self.settings.lyrics {
            if self.demo.lyrics && self.media.title == media::DEMO_TITLE {
                self.demo_lyrics();
            } else {
                let artist = self.media.artist.clone();
                lyrics::request(&self.lyrics, self.media.key(), self.media.title.clone(), artist, self.media.duration);
            }
        }
        let idx = {
            let l = self.lyrics.lock().unwrap();
            if l.status == LyricsStatus::Ready && l.key == self.media.key() {
                l.index_at(self.media.pos_now())
            } else {
                None
            }
        };
        if idx != self.lyric_idx {
            self.lyric_idx = idx;
            self.lyric_change = Instant::now();
        }
    }

    /// Current lyric line as displayed (romanized when romaji is on and available).
    fn lyric_text(&self) -> String {
        let l = self.lyrics.lock().unwrap();
        self.lyric_idx
            .and_then(|i| l.lines.get(i))
            .map(|x| if self.settings.romaji { x.roman.clone().unwrap_or_else(|| x.text.clone()) } else { x.text.clone() })
            .unwrap_or_default()
    }

    fn demo_lyrics(&mut self) {
        let key = self.media.key();
        let mut l = self.lyrics.lock().unwrap();
        if l.key == key {
            return;
        }
        const LINES: [&str; 12] = [
            "Langit jingga menyapa pelan",
            "Di ujung kota kita bertemu",
            "Lampu jalan mulai menyala",
            "Menghangatkan cerita lama",
            "Senja di ujung kota",
            "Takkan pernah sama",
            "Bila kau tak di sini",
            "Menemani langkahku",
            "Biar angin yang bercerita",
            "Tentang rindu yang tersisa",
            "Senja di ujung kota",
            "Selalu tentang kita",
        ];
        l.key = key;
        l.status = LyricsStatus::Ready;
        l.lines = (0..24).map(|i| lyrics::Line { t: 2.0 + i as f64 * 3.9, text: LINES[i % LINES.len()].to_string(), roman: None }).collect();
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
            let mut a = Alert::simple(AlertIcon::Glyph(glyph::STOPWATCH, RED), title, value, RED);
            a.sound = true;
            a.ttl = Duration::from_secs(5);
            self.push(a);
            self.pomo.mode = next;
            self.pomo.remaining = next.secs();
            self.pomo.running = next != PMode::Focus;
        }
    }

    fn update_alerts(&mut self, now: Instant) {
        if let Some(a) = &self.alert {
            let born = a.born.unwrap_or(now);
            let held = self.hover && a.big();
            if !held && now.duration_since(born) > a.ttl {
                self.alert = None;
                self.dirty = true;
            }
        }
        if self.alert.is_none() {
            if let Some(mut a) = self.queue.pop_front() {
                a.born = Some(now);
                if a.sound {
                    sys::chime();
                }
                self.alert = Some(a);
                self.dirty = true;
            }
        }
    }

    fn update_mascot(&mut self, now: Instant) {
        let eligible = self.settings.pet
            && !self.hover
            && self.form.is_none()
            && self.alert.is_none()
            && !self.media.playing
            && !self.pomo.running
            && !self.shelf_drag
            && !self.settings.minimal;
        if !eligible {
            if self.mascot != MascotState::Off {
                self.mascot = MascotState::Off;
            }
            return;
        }
        let threshold = if self.demo.mascot { 4000 } else { 15000 };
        let idle = idle_ms();
        self.mascot = match self.mascot {
            MascotState::Off if idle >= threshold => MascotState::Sleeping(now),
            MascotState::Sleeping(_) if idle < 400 => MascotState::Waking(now),
            MascotState::Waking(t) if now.duration_since(t) > Duration::from_millis(2600) => MascotState::Off,
            s => s,
        };
    }

    fn cursor_logical(&self) -> (f32, f32) {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
            let mut r = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut r);
            ((p.x - r.left) as f32 / self.scale, (p.y - r.top) as f32 / self.scale)
        }
    }

    fn update_hover(&mut self, now: Instant) {
        if self.demo.pin {
            self.hover = true;
            return;
        }
        let (x, y) = self.cursor_logical();
        let (w, h) = (self.w.v, self.h.v);
        let (sx, sy) = self.shape_pos(w, h);
        let inside = x >= sx - 4.0 && x <= sx + w + 4.0 && y >= sy - 4.0 && y <= sy + h + 6.0;
        if inside {
            self.hover_leave = None;
            if !self.hover {
                let t = *self.hover_enter.get_or_insert(now);
                if now.duration_since(t) >= Duration::from_millis(70) {
                    self.hover = true;
                    self.hover_enter = None;
                    self.dirty = true;
                }
            }
        } else {
            self.hover_enter = None;
            if self.hover && self.form.is_none() && self.press.is_none() {
                let t = *self.hover_leave.get_or_insert(now);
                if now.duration_since(t) >= Duration::from_millis(320) {
                    self.hover = false;
                    self.hover_leave = None;
                    self.hot = None;
                    self.water_settings = false;
                    self.dirty = true;
                }
            }
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
                let w = self.gfx.as_ref().map_or(360.0, |g| (g.measure(&self.lyric_text(), 13.0, 600) + 96.0).clamp(230.0, 600.0));
                (w, 30.0, 11.0)
            }
            Mode::Live => (232.0, 30.0, 11.0),
            Mode::Timer => (200.0, 30.0, 11.0),
            Mode::Minimal if self.settings.dock != Dock::Top => (70.0, 22.0, 8.0),
            Mode::Minimal => (110.0, 22.0, 8.0),
            // against a side edge the resting pill stays small so it hides little content
            Mode::Collapsed | Mode::Mascot if self.settings.dock != Dock::Top => (92.0, 34.0, 12.0),
            Mode::Collapsed | Mode::Mascot => (150.0, 30.0, 10.0),
        }
    }

    fn update_region(&mut self) {
        let s = self.scale;
        let (w, h) = (self.w.v, self.h.v);
        let (sx, sy) = self.shape_pos(w, h);
        let pad = SHOULDER + 6.0;
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

    // ------------------------------------------------------------ demo

    fn demo_script(&mut self) {
        let t = self.demo_start.elapsed().as_secs_f32();
        let steps: [f32; 11] = [1.0, 4.0, 7.0, 10.0, 13.0, 16.0, 19.0, 27.0, 35.0, 40.0, 45.0];
        while self.demo_step < steps.len() && t >= steps[self.demo_step] {
            let step = self.demo_step;
            self.demo_step += 1;
            match step {
                0 => self.push(Alert::simple(AlertIcon::CapsOn, "Caps Lock", "On", GREEN)),
                1 => self.push(Alert::simple(AlertIcon::Battery(true, 82), "Charger connected", "Charging · 82%", BATTERY_GREEN)),
                2 => self.sys_alert(SysAlert { icon: glyph::BLUETOOTH, title: "Connected".into(), value: "Galaxy Buds2 Pro".into(), color: BLUE }),
                3 => self.sys_alert(SysAlert { icon: glyph::WIFI, title: "Connected".into(), value: "Rumah_5G".into(), color: GREEN }),
                4 => self.push(Alert::simple(AlertIcon::Glyph(glyph::KEYBOARD, WHITE), "Keyboard", "ID-ID", WHITE)),
                5 => self.sys_alert(SysAlert { icon: glyph::MESSAGE, title: "WhatsApp".into(), value: "Dina: Nanti malam jadi ngopi?".into(), color: WHITE }),
                6 => self.push_action_alert(AlertIcon::Glyph(glyph::DROP, CYAN), "Time to Drink", &format!("Stay hydrated! {} ml", self.water.each()), BLUE, vec![("Drank".into(), AlertAction::Drank), ("Snooze".into(), AlertAction::Snooze)]),
                7 => {
                    if self.tasks.is_empty() {
                        self.tasks.push(Task { done: false, start: Some(model::now().minutes), end: None, title: "Rapat tim mingguan".into(), alerted: true, later_until: None });
                    }
                    let title = self.tasks[0].title.clone();
                    self.push_action_alert(AlertIcon::Glyph(glyph::CHECKLIST, GREEN), "Task Time!", &title, GREEN, vec![("Start".into(), AlertAction::TaskStart(0)), ("Later".into(), AlertAction::TaskLater(0))]);
                }
                8 => {
                    let mut a = Alert::simple(AlertIcon::Glyph(glyph::STOPWATCH, RED), "Focus Time!", "Back to work, 25 min", RED);
                    a.sound = true;
                    self.push(a);
                }
                9 => self.push(Alert::simple(AlertIcon::Glyph(glyph::DROP, CYAN), "Goal reached", "2.0 L today", CYAN)),
                _ => self.push(Alert::simple(AlertIcon::Battery(false, 20), "Low battery", "Plug in soon", ORANGE)),
            }
        }
    }

    // ------------------------------------------------------------ input

    fn hit_at(&self, x: f32, y: f32) -> Option<Act> {
        self.hits.borrow().iter().rev().find(|h| x >= h.x && x < h.x + h.w && y >= h.y && y < h.y + h.h).map(|h| h.act)
    }

    fn hit_rect(&self, act: Act) -> Option<(f32, f32, f32, f32)> {
        self.hits.borrow().iter().find(|h| h.act == act).map(|h| (h.x, h.y, h.w, h.h))
    }

    pub fn mouse_move(&mut self, px: i32, py: i32) {
        let (x, y) = (px as f32 / self.scale, py as f32 / self.scale);
        self.mouse = (x, y);
        let hot = self.hit_at(x, y);
        if hot != self.hot {
            self.hot = hot;
            self.dirty = true;
        }
        if let Some((sx, sy, Act::ShelfItem(i))) = self.press {
            if (x - sx).abs() + (y - sy).abs() > 6.0 {
                self.press = None;
                if let Some(item) = self.shelf.items.get(i) {
                    self.effect = Some(Effect::DragOut(item.path.clone()));
                }
            }
        }
    }

    pub fn mouse_down(&mut self, px: i32, py: i32) {
        let (x, y) = (px as f32 / self.scale, py as f32 / self.scale);
        let Some(act) = self.hit_at(x, y) else { return };
        if matches!(act, Act::ShelfItem(_)) {
            self.press = Some((x, y, act));
            return;
        }
        self.act(act, x);
        self.dirty = true;
    }

    pub fn mouse_up(&mut self, px: i32, py: i32) {
        let (x, y) = (px as f32 / self.scale, py as f32 / self.scale);
        if let Some((_, _, Act::ShelfItem(i))) = self.press.take() {
            if self.hit_at(x, y) == Some(Act::ShelfItem(i)) {
                if let Some(item) = self.shelf.items.get(i) {
                    open_path(&item.path);
                }
            }
        }
    }

    pub fn wheel(&mut self, delta: i16) {
        let rows = self.todo_rows();
        if PAGES[self.page] == Page::Todo && self.form.is_none() && self.tasks.len() > rows {
            let max = self.tasks.len() - rows;
            self.task_scroll = if delta < 0 { (self.task_scroll + 1).min(max) } else { self.task_scroll.saturating_sub(1) };
        } else if self.form.is_none() {
            self.go_page(if delta < 0 { 1 } else { -1 });
        }
        self.dirty = true;
    }

    pub fn shelf_drag(&mut self, on: bool) {
        self.shelf_drag = on;
        if on {
            self.set_page(3);
        }
        self.dirty = true;
    }

    pub fn shelf_dropped(&mut self) {
        let items: Vec<shelf::Dropped> = shelf::QUEUE.lock().map(|mut q| q.drain(..).collect()).unwrap_or_default();
        for d in items {
            self.shelf.add(d);
        }
        self.shelf_drag = false;
        self.set_page(3);
        self.hover = true;
        self.dirty = true;
    }

    pub fn key_char(&mut self, ch: u32) {
        let Some(f) = self.form.as_mut() else { return };
        match ch {
            8 => {
                if f.field == 0 {
                    f.title.pop();
                } else {
                    f.digits.pop();
                }
            }
            9 => f.field = 1 - f.field,
            13 => {
                self.save_form();
                return;
            }
            27 => {
                self.close_form();
                return;
            }
            c if c >= 32 => {
                if let Some(c) = char::from_u32(c) {
                    if f.field == 0 {
                        if f.title.chars().count() < 60 {
                            f.title.push(c);
                        }
                    } else if c.is_ascii_digit() {
                        // typing into a complete time range starts it over
                        if f.digits.len() >= 8 {
                            f.digits.clear();
                        }
                        f.digits.push(c);
                    }
                }
            }
            _ => {}
        }
        self.dirty = true;
    }

    pub fn focus_lost(&mut self) {
        if self.form.is_some() {
            self.close_form();
        }
    }

    fn go_page(&mut self, dir: i32) {
        let n = PAGES.len() as i32;
        let next = ((self.page as i32 + dir) % n + n) % n;
        self.slide = Spring::new(if dir > 0 { 36.0 } else { -36.0 });
        self.page = next as usize;
        self.water_settings = false;
    }

    fn set_page(&mut self, p: usize) {
        if p != self.page {
            self.slide = Spring::new(if p > self.page { 36.0 } else { -36.0 });
            self.page = p;
        }
    }

    fn open_form(&mut self, edit: Option<usize>) {
        let (title, digits) = match edit.and_then(|i| self.tasks.get(i)) {
            Some(t) => {
                let mut d = String::new();
                if let Some(s) = t.start {
                    d.push_str(&format!("{:02}{:02}", s / 60, s % 60));
                    if let Some(e) = t.end {
                        d.push_str(&format!("{:02}{:02}", e / 60, e % 60));
                    }
                }
                (t.title.clone(), d)
            }
            None => (String::new(), String::new()),
        };
        self.form = Some(Form { title, digits, field: 0, edit });
        self.effect = Some(Effect::Activate);
    }

    fn close_form(&mut self) {
        if self.form.take().is_some() {
            self.effect = Some(Effect::Deactivate);
        }
        self.dirty = true;
    }

    fn save_form(&mut self) {
        let Some(f) = self.form.as_ref() else { return };
        let title = f.title.trim().to_string();
        if title.is_empty() {
            return;
        }
        let (start, end) = model::parse_time_digits(&f.digits);
        let m = model::now().minutes;
        match f.edit {
            Some(i) if i < self.tasks.len() => {
                let t = &mut self.tasks[i];
                t.title = title;
                t.start = start;
                t.end = end;
                t.alerted = start.map_or(false, |s| s <= m);
            }
            _ => self.tasks.push(Task { done: false, start, end, title, alerted: start.map_or(false, |s| s <= m), later_until: None }),
        }
        model::save_tasks(&self.tasks);
        self.close_form();
    }

    fn act(&mut self, act: Act, x: f32) {
        match act {
            Act::PagePrev => self.go_page(-1),
            Act::PageNext => self.go_page(1),
            Act::Dot(i) => self.set_page(i),
            Act::MediaPrev => {
                let _ = self.media_tx.send(MediaCmd::Prev);
            }
            Act::MediaPlay => {
                let _ = self.media_tx.send(MediaCmd::PlayPause);
            }
            Act::MediaNext => {
                let _ = self.media_tx.send(MediaCmd::Next);
            }
            Act::MediaSeek => {
                if let Some((bx, _, bw, _)) = self.hit_rect(Act::MediaSeek) {
                    if self.media.duration > 0.0 {
                        let frac = ((x - bx) / bw).clamp(0.0, 1.0) as f64;
                        let _ = self.media_tx.send(MediaCmd::Seek(frac * self.media.duration));
                    }
                }
            }
            Act::Lyrics => {
                self.settings.lyrics = !self.settings.lyrics;
                self.settings.save();
            }
            Act::Romaji => self.toggle_romaji(),
            Act::PomoToggle => {
                self.pomo.running = !self.pomo.running;
                self.pomo.last = Instant::now();
            }
            Act::PomoReset => {
                self.pomo.running = false;
                self.pomo.remaining = self.pomo.mode.secs();
            }
            Act::PomoShort => {
                self.pomo.mode = PMode::Short;
                self.pomo.remaining = PMode::Short.secs();
                self.pomo.running = false;
            }
            Act::PomoFocus => {
                self.pomo.mode = PMode::Focus;
                self.pomo.remaining = PMode::Focus.secs();
                self.pomo.running = false;
            }
            Act::WaterOn => {
                self.water.on = !self.water.on;
                if self.water.on {
                    self.water.resync_fired();
                }
                self.water.save();
            }
            Act::WaterDrink => self.drink(),
            Act::WaterSettings => self.water_settings = true,
            Act::WaterBack => self.water_settings = false,
            Act::WaterSub(s) => {
                self.water_sub = s;
                self.water.vis = s == 0;
            }
            Act::Bottle(d) => {
                self.water.set_bottle((self.water.bottle as i32 + d * 50).max(100) as u32);
                self.water.save();
            }
            Act::Goal(i) => {
                self.water.goal = GOALS[i];
                self.water.resync_fired();
                self.water.save();
            }
            Act::Start(d) => {
                let s = (self.water.start as i32 + d * 30).clamp(0, 1380) as u32;
                self.water.set_hours(s, self.water.end);
                self.water.save();
            }
            Act::End(d) => {
                let e = (self.water.end as i32 + d * 30).clamp(60, 1439) as u32;
                self.water.set_hours(self.water.start, e);
                self.water.save();
            }
            Act::WaterReset => {
                if self.reset_confirm.is_some() {
                    self.water.reset();
                    self.water.save();
                    self.reset_confirm = None;
                } else {
                    self.reset_confirm = Some(Instant::now());
                }
            }
            Act::ShelfItem(_) => {}
            Act::ShelfClear => self.shelf.clear(),
            Act::TodoAdd => {
                if self.form.is_some() {
                    self.close_form();
                } else {
                    self.open_form(None);
                }
            }
            Act::TodoEdit => {
                if let Some(i) = self.task_sel {
                    self.open_form(Some(i));
                }
            }
            Act::TodoRow(i) => self.task_sel = if self.task_sel == Some(i) { None } else { Some(i) },
            Act::TodoCheck(i) => {
                if let Some(t) = self.tasks.get_mut(i) {
                    t.done = !t.done;
                    model::save_tasks(&self.tasks);
                }
            }
            Act::TodoDelete => {
                if let Some(i) = self.form.as_ref().and_then(|f| f.edit) {
                    if i < self.tasks.len() {
                        self.tasks.remove(i);
                        model::save_tasks(&self.tasks);
                        self.task_sel = None;
                    }
                }
                self.close_form();
            }
            Act::TodoCancel => self.close_form(),
            Act::TodoSave => self.save_form(),
            Act::TodoTitle => {
                if let Some(f) = self.form.as_mut() {
                    f.field = 0;
                }
            }
            Act::TodoTime => {
                if let Some(f) = self.form.as_mut() {
                    f.field = 1;
                }
            }
            Act::Alert(i) => {
                let action = self.alert.as_ref().and_then(|a| a.actions.get(i)).map(|a| a.1);
                self.alert = None;
                match action {
                    Some(AlertAction::Drank) => self.drink(),
                    Some(AlertAction::Refilled) => {
                        self.water.refill();
                        self.water.save();
                    }
                    Some(AlertAction::Snooze) => {
                        self.water.snooze();
                        self.water.save();
                    }
                    Some(AlertAction::TaskStart(_)) => {
                        self.pomo.mode = PMode::Focus;
                        self.pomo.remaining = PMode::Focus.secs();
                        self.pomo.running = true;
                        self.pomo.last = Instant::now();
                    }
                    Some(AlertAction::TaskLater(i)) => {
                        if let Some(t) = self.tasks.get_mut(i) {
                            t.later_until = Some(model::now().minutes + 10);
                        }
                    }
                    None => {}
                }
            }
        }
    }

    fn drink(&mut self) {
        if let Some(WaterEvent::GoalReached) = self.water.drink() {
            let mut a = Alert::simple(AlertIcon::Glyph(glyph::DROP, CYAN), "Goal reached", &format!("{:.1} L today", self.water.taken as f32 / 1000.0), CYAN);
            a.sound = true;
            self.push(a);
        }
        self.water.save();
    }

    // ------------------------------------------------------------ painting helpers

    /// Renderer; painting only happens while it exists (see `render`).
    fn g(&self) -> &Gfx {
        self.gfx.as_ref().expect("renderer available while painting")
    }

    /// Rebuilds the D3D/D2D/DComp stack after a device loss (retries every 0.5 s).
    fn recover_renderer(&mut self) {
        let Some(t) = self.gfx_lost else { return };
        // the old DirectComposition target must be released before a new one can bind the window
        self.gfx = None;
        if t.elapsed() < Duration::from_millis(500) {
            return;
        }
        match Gfx::new(self.hwnd, self.px.0, self.px.1, self.scale) {
            Ok(g) => {
                self.gfx = Some(g);
                self.gfx_lost = None;
                self.art = None;
                self.region = (0, 0, 0, 0);
                self.dirty = true;
                sys::log("render: device recreated");
            }
            Err(e) => {
                self.gfx_lost = Some(Instant::now());
                sys::log(&format!("render: recreate failed: {:?}", e));
            }
        }
    }

    fn hit(&self, x: f32, y: f32, w: f32, h: f32, act: Act) {
        let (ox, oy) = self.off.get();
        self.hits.borrow_mut().push(Hit { x: x + ox, y: y + oy, w, h, act });
    }

    fn origin(&self, x: f32, y: f32) {
        self.off.set((x, y));
        self.g().set_offset(x, y);
    }

    fn hot(&self, act: Act) -> bool {
        self.hot == Some(act)
    }

    fn button(&self, x: f32, y: f32, w: f32, h: f32, r: f32, bg: u32, a: f32, act: Act) {
        let g = self.g();
        g.fill_rrect(x, y, w, h, r, bg, a);
        if self.hot(act) {
            g.fill_rrect(x, y, w, h, r, WHITE, 0.08);
        }
        self.hit(x, y, w, h, act);
    }

    fn round_button(&self, cx: f32, cy: f32, r: f32, bg: u32, glyph: u16, size: f32, fg: u32, act: Act) {
        let g = self.g();
        g.fill_circle(cx, cy, r, bg, 1.0);
        if self.hot(act) {
            g.fill_circle(cx, cy, r, WHITE, 0.1);
        }
        g.icon(glyph, cx, cy, size, fg, 1.0);
        self.hit(cx - r, cy - r, r * 2.0, r * 2.0, act);
    }

    fn eq_bars(&self, x: f32, cy: f32, color: u32) {
        let t = self.started.elapsed().as_secs_f32();
        for i in 0..4 {
            let phase = t * (5.0 + i as f32 * 1.3) + i as f32 * 1.7;
            let h = if self.media.playing { 4.0 + (phase.sin() * 0.5 + 0.5) * 9.0 } else { 3.0 };
            self.g().fill_rrect(x + i as f32 * 4.2, cy - h / 2.0, 2.6, h, 1.3, color, 1.0);
        }
    }

    fn art(&self, x: f32, y: f32, s: f32, r: f32) {
        let g = self.g();
        if let Some((_, bmp)) = &self.art {
            g.bitmap_rounded(bmp, x, y, s, s, r);
        } else {
            g.fill_rrect(x, y, s, s, r, 0x262626, 1.0);
            g.icon(glyph::MUSIC, x + s / 2.0, y + s / 2.0, s * 0.42, 0x7A7A7A, 1.0);
        }
    }

    // ------------------------------------------------------------ render

    /// Draws one frame; returns false if the GPU device was lost.
    fn render(&self) -> bool {
        if std::env::var_os("KURO_DEBUG_FPS").is_some() {
            thread_local!(static FPS: Cell<(u32, Option<Instant>)> = const { Cell::new((0, None)) });
            FPS.with(|f| {
                let (n, start) = f.get();
                let start = start.unwrap_or_else(Instant::now);
                if start.elapsed() >= Duration::from_secs(1) {
                    sys::log(&format!("fps {} ({:?})", n + 1, self.shown));
                    f.set((0, Some(Instant::now())));
                } else {
                    f.set((n + 1, Some(start)));
                }
            });
        }
        let Some(g) = self.gfx.as_ref() else { return true };
        self.hits.borrow_mut().clear();
        g.begin();
        g.alpha.set(1.0);
        let (w, h, r) = (self.w.v, self.h.v, self.r.v);
        let (x0, y0) = self.shape_pos(w, h);
        self.top.set(y0);
        let geom = match self.settings.dock {
            Dock::Top => g.notch_geometry(self.win.0 / 2.0, w, h, r, SHOULDER),
            Dock::Left => g.side_geometry(0.0, y0, w, h, r, SHOULDER, true),
            Dock::Right => g.side_geometry(self.win.0, y0, w, h, r, SHOULDER, false),
        };
        if let Some(geom) = geom {
            g.fill_geometry(&geom, 0x000000, 1.0);
            if self.shown == Mode::Mascot {
                self.mascot_background(&geom, x0, y0, w, h);
            }
            g.push_clip(&geom);
            g.alpha.set(ease(self.content_a));
            if self.vertical() {
                match self.shown {
                    Mode::Expanded => self.paint_expanded_v(x0),
                    Mode::Alert => self.paint_alert_v(x0, w, h),
                    Mode::Live => self.paint_live_v(x0, w, h),
                    Mode::Lyrics => self.paint_lyrics_v(x0, w, h),
                    Mode::Timer => self.paint_timer_v(x0, w),
                    Mode::Mascot => self.paint_mascot_v(x0, y0, w, h),
                    Mode::Collapsed => self.paint_collapsed_v(x0, w, h),
                    Mode::Minimal => {}
                }
            } else {
                match self.shown {
                    Mode::Expanded => self.paint_expanded(x0),
                    Mode::Alert => self.paint_alert(x0, w),
                    Mode::Live => self.paint_live(x0, w, false),
                    Mode::Lyrics => self.paint_live(x0, w, true),
                    Mode::Timer => self.paint_timer_live(x0, w),
                    Mode::Mascot => self.paint_mascot(x0, y0, w, h),
                    Mode::Collapsed => self.paint_collapsed(x0, w, h),
                    Mode::Minimal => {}
                }
            }
            g.alpha.set(1.0);
            self.origin(0.0, 0.0);
            g.pop_clip();
        }
        g.end()
    }

    fn paint_collapsed(&self, x0: f32, w: f32, h: f32) {
        self.origin(x0, self.top.get());
        let (_, time) = date_strings();
        self.g().text(&time, w / 2.0, h / 2.0, 14.5, 700, Align::Center, WHITE, 1.0, w);
    }

    fn paint_live(&self, x0: f32, w: f32, lyric: bool) {
        self.origin(x0, self.top.get());
        let g = self.g();
        self.art(9.0, 5.0, 20.0, 6.0);
        self.eq_bars(w - 26.0, 15.0, if lyric { CYAN } else { GREEN });
        if lyric {
            let text = self.lyric_text();
            let t = (self.lyric_change.elapsed().as_secs_f32() / 0.25).min(1.0);
            let shown = if text.is_empty() { "♪".to_string() } else { text };
            let size = fit_size(g, &shown, w - 88.0, 13.0, 10.5, 600);
            g.alpha.set(g.alpha.get() * ease(t));
            g.text(&shown, w / 2.0, 15.0 + (1.0 - ease(t)) * 6.0, size, 600, Align::Center, WHITE, 1.0, w - 88.0);
            g.alpha.set(ease(self.content_a));
        }
    }

    fn paint_timer_live(&self, x0: f32, w: f32) {
        self.origin(x0, self.top.get());
        let g = self.g();
        let total = self.pomo.mode.secs();
        let frac = (self.pomo.remaining / total) as f32;
        let color = if self.pomo.mode == PMode::Focus { RED } else { GREEN };
        g.stroke_circle(17.0, 15.0, 7.5, 0x3A1A1A, 1.0, 2.4);
        g.arc(17.0, 15.0, 7.5, -90.0, 360.0 * frac, color, 1.0, 2.4);
        g.text(if self.pomo.mode == PMode::Focus { "Focus" } else { "Break" }, 32.0, 15.0, 12.0, 600, Align::Left, GREY, 1.0, 80.0);
        g.text(&mmss(self.pomo.remaining), w - 14.0, 15.0, 14.5, 700, Align::Right, color, 1.0, 80.0);
    }

    fn paint_alert(&self, x0: f32, w: f32) {
        let Some(a) = &self.alert else { return };
        self.origin(x0, self.top.get());
        let g = self.g();
        if !a.big() {
            self.alert_icon(a.icon, 24.0, 17.5, 9.0);
            g.text(&a.title, 46.0, 17.5, 13.5, 400, Align::Left, WHITE, 1.0, w * 0.48);
            g.text(&a.value, w - 22.0, 17.5, 13.5, 400, Align::Right, a.color, 1.0, w * 0.42);
            return;
        }
        // actionable card
        g.glow(30.0, 30.0, 26.0, 26.0, a.color, 0.18);
        g.fill_circle(30.0, 30.0, 15.0, a.color, 0.16);
        self.alert_icon(a.icon, 30.0, 30.0, 12.0);
        g.text(&a.title, 54.0, 22.0, 14.5, 700, Align::Left, WHITE, 1.0, w - 70.0);
        g.text(&a.value, 54.0, 40.0, 12.0, 400, Align::Left, GREY, 1.0, w - 70.0);
        let mut right = w - 16.0;
        for (i, (label, _)) in a.actions.iter().enumerate().rev() {
            let bw = g.measure(label, 12.0, 600) + 26.0;
            let primary = i == 0;
            self.button(right - bw, 50.0, bw, 22.0, 11.0, if primary { a.color } else { 0x262626 }, if primary { 0.95 } else { 1.0 }, Act::Alert(i));
            g.text(label, right - bw / 2.0, 61.0, 12.0, 600, Align::Center, if primary { 0x08121C } else { WHITE }, 1.0, bw);
            right -= bw + 8.0;
        }
        // remaining time bar
        if let Some(b) = a.born {
            let f = 1.0 - (b.elapsed().as_secs_f32() / a.ttl.as_secs_f32()).min(1.0);
            let (y, full) = if a.actions.is_empty() { (52.0, w - 70.0) } else { (72.0, (right - 62.0).max(0.0)) };
            g.fill_rrect(54.0, y, full * f, 2.0, 1.0, if a.color == WHITE { GREY } else { a.color }, 0.5);
        }
    }

    fn alert_icon(&self, icon: AlertIcon, cx: f32, cy: f32, size: f32) {
        let g = self.g();
        match icon {
            AlertIcon::Glyph(gl, c) => g.icon(gl, cx, cy, size * 1.45, c, 1.0),
            AlertIcon::CapsOn | AlertIcon::CapsOff => {
                let s = size / 9.0;
                let pts = [(cx, cy - 8.0 * s), (cx + 7.5 * s, cy - 0.5 * s), (cx + 3.5 * s, cy - 0.5 * s), (cx + 3.5 * s, cy + 3.0 * s), (cx - 3.5 * s, cy + 3.0 * s), (cx - 3.5 * s, cy - 0.5 * s), (cx - 7.5 * s, cy - 0.5 * s)];
                if icon == AlertIcon::CapsOn {
                    g.polygon(&pts, WHITE, 1.0);
                    g.fill_rrect(cx - 3.5 * s, cy + 5.0 * s, 7.0 * s, 2.2 * s, 0.8, WHITE, 1.0);
                } else {
                    let mut closed = pts.to_vec();
                    closed.push(pts[0]);
                    g.polyline(&closed, WHITE, 1.0, 1.5);
                    g.line(cx - 3.5 * s, cy + 6.0 * s, cx + 3.5 * s, cy + 6.0 * s, WHITE, 1.0, 1.5);
                }
            }
            AlertIcon::Battery(plugged, pct) => {
                let (w, h) = (size * 2.3, size * 1.2);
                let x = cx - w / 2.0;
                g.stroke_rrect(x, cy - h / 2.0, w, h, 3.0, WHITE, 0.9, 1.4, false);
                g.fill_rrect(x + w + 0.8, cy - h / 5.0, 1.8, h / 2.5, 0.8, WHITE, 0.9);
                let color = if plugged { BATTERY_GREEN } else if pct <= 20 { RED } else { WHITE };
                g.fill_rrect(x + 2.0, cy - h / 2.0 + 2.0, (w - 4.0) * pct as f32 / 100.0, h - 4.0, 1.5, color, 1.0);
            }
        }
    }

    // ------------------------------------------------------------ mascot

    fn mascot_phase(&self) -> (f32, f32, f32) {
        // (brightness 0..1, open 0..1, happy 0..1)
        match self.mascot {
            MascotState::Sleeping(t) => {
                let k = (t.elapsed().as_secs_f32() / 0.8).min(1.0);
                (0.55 * ease(k), 0.0, 0.0)
            }
            MascotState::Waking(t) => {
                let e = t.elapsed().as_secs_f32();
                let open = ease(e / 0.25);
                let happy = ease((e - 0.45) / 0.2);
                let fade = 1.0 - ease((e - 2.2) / 0.4);
                ((0.55 + 0.45 * open) * fade, open * (1.0 - happy), happy * fade)
            }
            MascotState::Off => (0.0, 0.0, 0.0),
        }
    }

    fn mascot_background(&self, geom: &windows::Win32::Graphics::Direct2D::ID2D1PathGeometry1, x0: f32, y0: f32, w: f32, h: f32) {
        let g = self.g();
        let (bright, _, _) = self.mascot_phase();
        if bright <= 0.01 {
            return;
        }
        let breath = 0.92 + 0.08 * (self.started.elapsed().as_secs_f32() * 2.1).sin();
        let (l, m, r) = if bright > 0.6 { (0x19A0E5, 0x256DED, 0x5443CD) } else { (0x0C5A79, 0x123D79, 0x27256D) };
        let a = (bright / 0.55).min(1.0) * breath;
        if let Some(b) = g.linear(x0, 0.0, x0 + w, 0.0, &[(0.0, l, a), (0.5, m, a), (1.0, r, a)]) {
            g.fill_geometry_brush(geom, &b);
        }
        if let Some(b) = g.linear(0.0, y0, 0.0, y0 + h, &[(0.0, 0x000000, 1.0), (0.38, 0x000000, 0.92), (1.0, 0x000000, 0.0)]) {
            g.fill_geometry_brush(geom, &b);
        }
    }

    fn paint_mascot(&self, x0: f32, y0: f32, w: f32, h: f32) {
        self.origin(0.0, 0.0);
        let g = self.g();
        let (_, open, happy) = self.mascot_phase();
        let t = self.started.elapsed().as_secs_f32();
        let (mx, _) = self.cursor_logical();
        let c = x0 + w / 2.0;
        let look = if open > 0.0 || happy > 0.0 { ((mx - c) / 60.0).clamp(-1.0, 1.0) * 2.5 } else { 0.0 };
        let cy = y0 + h / 2.0 + 1.0;
        for ex in [c - 11.5 + look, c + 11.5 + look] {
            if happy > 0.01 {
                g.glow(ex, cy, 9.0, 8.0, 0xBFD8FF, 0.35 * happy);
                let s = 4.2;
                let pts: Vec<(f32, f32)> = (0..=8).map(|i| {
                    let a = std::f32::consts::PI * (1.0 + i as f32 / 8.0);
                    (ex + a.cos() * s, cy + 1.5 + a.sin() * s * 0.85)
                }).collect();
                g.polyline(&pts, WHITE, happy, 1.9);
            } else {
                let ry = 1.1 + 4.6 * open;
                let pulse = if open < 0.01 { 0.75 + 0.25 * (t * 2.0).sin() } else { 1.0 };
                g.glow(ex, cy, 8.0, 5.0 + ry, 0xBFD8FF, 0.45 * pulse);
                g.fill_ellipse(ex, cy, 3.6 - 0.6 * open, ry, WHITE, pulse);
            }
        }
        if happy > 0.01 {
            // waving paw
            let (px, py) = (c + 30.0 + look, cy + 2.0);
            let angle = (t * 9.0).sin() * 18.0;
            g.set_transform(rotation(angle, px, py + 4.0));
            g.glow(px, py, 9.0, 9.0, 0xBFD8FF, 0.3 * happy);
            g.fill_ellipse(px, py + 1.0, 3.6, 3.2, WHITE, happy);
            for (dx, dy) in [(-3.0, -2.6), (0.0, -4.0), (3.0, -2.6)] {
                g.fill_circle(px + dx, py + dy, 1.5, WHITE, happy);
            }
            self.origin(0.0, 0.0);
        }
    }

    // ------------------------------------------------------------ expanded pages

    fn paint_expanded(&self, x0: f32) {
        let g = self.g();
        let page = PAGES[self.page];
        let slide = self.slide.v;
        let base_a = g.alpha.get();
        // header (does not slide)
        self.origin(x0, self.top.get());
        self.header();
        g.alpha.set(base_a * (1.0 - (slide.abs() / 36.0) * 0.7));
        self.origin(x0 + slide, self.top.get());
        match page {
            Page::Media => self.page_media(),
            Page::Pomodoro => self.page_pomodoro(),
            Page::Water => {
                if self.water_settings {
                    self.page_water_settings()
                } else {
                    self.page_water()
                }
            }
            Page::Shelf => self.page_shelf(),
            Page::Todo => self.page_todo(),
        }
        g.alpha.set(base_a);
        self.origin(x0, self.top.get());
        if !(page == Page::Water && self.water_settings) {
            self.round_button(22.0, 69.0, 9.5, 0x3C3C3C, glyph::CHEV_L, 10.0, WHITE, Act::PagePrev);
            self.round_button(398.0, 69.0, 9.5, 0x3C3C3C, glyph::CHEV_R, 10.0, WHITE, Act::PageNext);
        }
        self.page_dots(210.0, 139.0);
    }

    fn header(&self) {
        let g = self.g();
        let (date, time) = date_strings();
        let dw = g.measure(&date, 13.0, 400);
        let tw = g.measure(&time, 13.5, 700);
        let total = dw + 8.0 + tw;
        let left = 210.0 - total / 2.0;
        g.text(&date, left, 18.0, 13.0, 400, Align::Left, 0xD0D0D0, 1.0, dw + 4.0);
        g.text(&time, left + dw + 8.0, 18.0, 13.5, 700, Align::Left, WHITE, 1.0, tw + 4.0);
        let page = PAGES[self.page];
        let title: &[(&str, u32)] = match page {
            Page::Media => &[],
            Page::Pomodoro => &[("Pomodoro", WHITE)],
            Page::Water if self.water_settings => &[],
            Page::Water => &[("Water ", WHITE), ("Reminder", BLUE)],
            Page::Shelf => &[("Shelf", WHITE)],
            Page::Todo => &[("Todo", WHITE), ("List", GREEN)],
        };
        let mut x = 15.0;
        for (s, c) in title {
            g.text(s, x, 20.0, 14.5, 600, Align::Left, *c, 1.0, 120.0);
            x += g.measure(s, 14.5, 600);
        }
        if self.pomo.running && page != Page::Pomodoro {
            let frac = (self.pomo.remaining / self.pomo.mode.secs()) as f32;
            g.fill_circle(395.0, 18.0, 8.0, 0x3A1414, 1.0);
            g.pie(395.0, 18.0, 8.0, -90.0, 360.0 * frac, RED, 0.9);
        }
    }

    fn page_media(&self) {
        let g = self.g();
        let m = &self.media;
        self.art(44.0, 12.0, 62.0, 10.0);
        let has = m.has_session && !m.title.is_empty();
        let lyric_on = self.settings.lyrics && has;
        let status = self.lyrics.lock().unwrap().status.clone();
        if lyric_on && self.lyric_idx.is_some() {
            // with romaji on: romanized line large, the original script small underneath;
            // otherwise the current line and the next one
            let (cur, next) = {
                let l = self.lyrics.lock().unwrap();
                let i = self.lyric_idx.unwrap();
                let line = l.lines.get(i);
                let text = line.map(|x| x.text.clone()).unwrap_or_default();
                match line.and_then(|x| x.roman.clone()).filter(|_| self.settings.romaji) {
                    Some(roman) => (roman, text),
                    None => {
                        let next = l.lines.get(i + 1).map(|x| if self.settings.romaji { x.roman.clone().unwrap_or_else(|| x.text.clone()) } else { x.text.clone() });
                        (text, next.unwrap_or_default())
                    }
                }
            };
            let t = ease(self.lyric_change.elapsed().as_secs_f32() / 0.3);
            let a = g.alpha.get();
            g.alpha.set(a * t);
            let cur = if cur.is_empty() { "♪".to_string() } else { cur };
            let width = 262.0;
            let rise = (1.0 - t) * 5.0;
            if g.measure(&cur, 13.0, 700) <= width {
                // fits on one line: biggest size that fits, then next line and credit
                let cs = fit_size(g, &cur, width, 15.0, 13.0, 700);
                let ns = fit_size(g, &next, width, 12.0, 10.0, 400);
                g.text(&cur, 122.0, 33.0 + rise, cs, 700, Align::Left, WHITE, 1.0, width);
                g.text(&next, 122.0, 52.0, ns, 400, Align::Left, DIM, 1.0, width);
                g.alpha.set(a);
                let credit = if m.artist.is_empty() { m.title.clone() } else { format!("{} · {}", m.title, m.artist) };
                g.text(&credit, 122.0, 68.0, 10.5, 400, Align::Left, GREY, 1.0, width);
            } else {
                // long line: wrap onto two lines, next line underneath
                g.text_box(&cur, 122.0, 25.0 + rise, width, 38.0, 13.5, 700, WHITE, 1.0);
                let ns = fit_size(g, &next, width, 11.5, 10.0, 400);
                g.text(&next, 122.0, 70.0, ns, 400, Align::Left, DIM, 1.0, width);
                g.alpha.set(a);
            }
        } else if has {
            g.text(&m.title, 122.0, 37.0, 14.5, 700, Align::Left, WHITE, 1.0, 258.0);
            let sub = if lyric_on && status == LyricsStatus::Loading {
                "Loading lyrics".to_string()
            } else if lyric_on && status == LyricsStatus::NotFound {
                format!("{} · Lyrics not available", m.artist)
            } else {
                m.artist.clone()
            };
            g.text(&sub, 122.0, 57.0, 12.0, 400, Align::Left, GREY, 1.0, 258.0);
        } else {
            g.text("Tidak ada media", 122.0, 46.0, 14.5, 700, Align::Left, WHITE, 1.0, 258.0);
        }

        // timeline
        let pos = m.pos_now();
        g.text(&mmss(pos), 44.0, 91.0, 13.0, 400, Align::Left, 0x7A7A7A, 1.0, 60.0);
        g.text(&format!("-{}", mmss(m.duration - pos)), 376.0, 91.0, 13.0, 400, Align::Right, 0x7A7A7A, 1.0, 70.0);
        let (bx, bw) = (96.0, 222.0);
        g.fill_rrect(bx, 89.5, bw, 3.0, 1.5, 0x1F1F1F, 1.0);
        if m.duration > 0.0 {
            let f = (pos / m.duration).clamp(0.0, 1.0) as f32;
            g.fill_rrect(bx, 89.5, (bw * f).max(3.0), 3.0, 1.5, 0xD8D8D8, 1.0);
            if self.hot(Act::MediaSeek) {
                g.fill_circle(bx + bw * f, 91.0, 4.5, WHITE, 1.0);
            }
        }
        self.hit(bx, 83.0, bw, 16.0, Act::MediaSeek);

        // controls
        let fg = if has { WHITE } else { 0x7A7A7A };
        let lyr_color = if self.settings.lyrics && has { WHITE } else { 0x7A7A7A };
        g.icon(glyph::MESSAGE, 75.0, 117.0, 15.0, lyr_color, 1.0);
        if self.settings.lyrics && has {
            g.fill_circle(85.0, 109.5, 2.4, GREEN, 1.0);
        }
        self.hit(62.0, 105.0, 26.0, 24.0, Act::Lyrics);
        // romaji toggle chip
        let ron = self.settings.romaji;
        if ron || self.hot(Act::Romaji) {
            g.fill_rrect(93.0, 108.0, 26.0, 18.0, 9.0, 0x262626, 1.0);
        }
        g.text("Aa", 106.0, 117.0, 10.5, 700, Align::Center, if ron { WHITE } else { 0x7A7A7A }, 1.0, 26.0);
        if ron {
            g.fill_circle(117.0, 109.5, 2.4, GREEN, 1.0);
        }
        self.hit(91.0, 105.0, 30.0, 24.0, Act::Romaji);
        let tri = |cx: f32, dir: f32, s: f32| vec![(cx - dir * s * 0.55, 117.0 - s * 0.6), (cx - dir * s * 0.55, 117.0 + s * 0.6), (cx + dir * s * 0.65, 117.0)];
        // previous / next: double triangles
        for (cx, dir, act) in [(163.0, -1.0, Act::MediaPrev), (257.0, 1.0, Act::MediaNext)] {
            if self.hot(act) {
                g.fill_circle(cx, 117.0, 14.0, WHITE, 0.08);
            }
            g.polygon(&tri(cx - dir * 5.0, dir, 9.0), fg, 0.8);
            g.polygon(&tri(cx + dir * 4.0, dir, 9.0), fg, 0.8);
            self.hit(cx - 14.0, 104.0, 28.0, 26.0, act);
        }
        if self.hot(Act::MediaPlay) {
            g.fill_circle(211.0, 117.0, 15.0, WHITE, 0.08);
        }
        if m.playing {
            g.fill_rrect(204.0, 108.5, 4.6, 17.0, 1.5, fg, 1.0);
            g.fill_rrect(213.4, 108.5, 4.6, 17.0, 1.5, fg, 1.0);
        } else {
            g.polygon(&[(205.0, 107.5), (205.0, 126.5), (220.5, 117.0)], fg, 1.0);
        }
        self.hit(196.0, 103.0, 30.0, 28.0, Act::MediaPlay);
    }

    fn page_pomodoro(&self) {
        let g = self.g();
        let p = &self.pomo;
        let frac = (p.remaining / p.mode.secs()).clamp(0.0, 1.0) as f32;
        let color = if p.mode == PMode::Focus { RED } else { GREEN };
        g.glow(80.0, 72.0, 46.0, 46.0, color, 0.28);
        g.stroke_circle(80.0, 72.0, 30.0, color, 0.22, 6.5);
        g.arc(80.0, 72.0, 30.0, -90.0, 360.0 * frac, color, 1.0, 6.5);
        g.icon(glyph::STOPWATCH, 80.0, 72.0, 30.0, WHITE, 1.0);

        g.text(&mmss(p.remaining.ceil()), 265.0, 50.0, 31.0, 700, Align::Center, WHITE, 1.0, 150.0);
        self.round_button(331.0, 50.0, 12.5, 0x262626, glyph::REFRESH, 12.0, WHITE, Act::PomoReset);
        g.text(p.mode.label(), 265.0, 77.0, 12.0, 400, Align::Center, GREY, 1.0, 150.0);

        let short_active = p.mode != PMode::Focus;
        self.button(145.0, 92.0, 85.0, 27.0, 13.5, if short_active { 0x333333 } else { 0x171717 }, 1.0, Act::PomoShort);
        g.icon(glyph::CAFE, 159.0, 105.5, 11.0, WHITE, 1.0);
        g.text("Short Break", 168.0, 105.5, 10.0, 400, Align::Left, WHITE, 1.0, 60.0);
        self.button(239.0, 92.0, 53.0, 27.0, 13.5, color, 1.0, Act::PomoToggle);
        if p.running {
            g.fill_rrect(259.5, 99.5, 3.2, 12.0, 1.2, WHITE, 1.0);
            g.fill_rrect(266.3, 99.5, 3.2, 12.0, 1.2, WHITE, 1.0);
        } else {
            g.polygon(&[(261.0, 99.5), (261.0, 111.5), (271.5, 105.5)], WHITE, 1.0);
        }
        self.button(301.0, 92.0, 85.0, 27.0, 13.5, if short_active { 0x171717 } else { 0x333333 }, 1.0, Act::PomoFocus);
        g.icon(glyph::TARGET, 315.0, 105.5, 11.0, WHITE, 1.0);
        g.text("Focus Time", 324.0, 105.5, 10.0, 400, Align::Left, WHITE, 1.0, 60.0);
    }

    fn page_water(&self) {
        let g = self.g();
        let w = &self.water;
        let litres = format!("{:.1}", w.taken as f32 / 1000.0);
        g.text(&litres, 44.0, 49.0, 30.0, 700, Align::Left, WHITE, 1.0, 120.0);
        let lw = g.measure(&litres, 30.0, 700);
        g.text(&format!("/ {:.1} L", w.goal as f32 / 1000.0), 44.0 + lw + 6.0, 53.0, 14.0, 400, Align::Left, GREY, 1.0, 80.0);

        // reminder toggle
        let on = w.on;
        self.button(162.0, 37.0, 34.0, 18.0, 9.0, if on { 0x2699FF } else { 0x3A3A3A }, 1.0, Act::WaterOn);
        let kx = if on { 187.0 } else { 171.0 };
        g.fill_circle(kx, 46.0, 7.2, WHITE, 1.0);
        g.icon(glyph::BELL, kx, 46.0, 8.5, if on { 0x2699FF } else { GREY }, 1.0);

        // status line
        let pct = format!("{}%", w.percent());
        let tail = if !w.on {
            " · Reminders off".to_string()
        } else if w.taken >= w.goal {
            " · Goal reached".to_string()
        } else if let Some(n) = w.next_time() {
            format!(" · Next {}", model::hhmm(n))
        } else {
            " · Done for today".to_string()
        };
        let mut x = 44.0;
        for (s, c, wt) in [("Today · ", GREY, 400), (pct.as_str(), BLUE, 700), (tail.as_str(), GREY, 400)] {
            g.text(s, x, 71.0, 13.0, wt, Align::Left, c, 1.0, 160.0);
            x += g.measure(s, 13.0, wt);
        }
        g.fill_rrect(44.0, 83.5, 152.0, 4.0, 2.0, 0x212121, 1.0);
        let f = (w.taken as f32 / w.goal.max(1) as f32).min(1.0);
        if f > 0.0 {
            g.fill_rrect(44.0, 83.5, (152.0 * f).max(4.0), 4.0, 2.0, BLUE, 1.0);
        }
        self.button(44.0, 99.0, 58.0, 25.0, 12.5, 0x0D4580, 1.0, Act::WaterDrink);
        g.icon(glyph::DROP, 58.0, 111.5, 12.0, CYAN, 1.0);
        g.text("Drink", 68.0, 111.5, 12.0, 400, Align::Left, WHITE, 1.0, 40.0);
        self.button(109.0, 99.0, 71.0, 25.0, 12.5, 0x1F1F1F, 1.0, Act::WaterSettings);
        g.icon(glyph::SLIDERS, 122.0, 111.5, 11.0, WHITE, 1.0);
        g.text("Settings", 132.0, 111.5, 11.0, 400, Align::Left, WHITE, 1.0, 50.0);

        g.line(207.0, 37.0, 207.0, 120.0, 0x262626, 1.0, 1.0);
        // sub-page dots
        for (i, x) in [(0u8, 356.0), (1u8, 365.0)] {
            g.fill_circle(x, 41.0, 2.0, if self.water_sub == i { WHITE } else { DIM }, 1.0);
            self.hit(x - 4.0, 36.0, 8.0, 10.0, Act::WaterSub(i));
        }
        if self.water_sub == 0 {
            self.water_gauge();
        } else {
            self.water_chart();
        }
    }

    fn water_gauge(&self) {
        let g = self.g();
        let (cx, cy, r) = (293.0, 75.0, 33.0);
        g.arc(cx, cy, r, 135.0, 270.0, 0x0F2640, 1.0, 6.5);
        let f = (self.water.taken as f32 / self.water.goal.max(1) as f32).min(1.0);
        if f > 0.0 {
            g.arc(cx, cy, r, 135.0, 270.0 * f, BLUE, 1.0, 6.5);
        }
        let a = (135.0 + 270.0 * f).to_radians();
        g.fill_circle(cx + r * a.cos(), cy + r * a.sin(), 4.2, BLUE, 1.0);
        g.text(&self.water.percent().to_string(), cx, cy - 6.0, 24.0, 700, Align::Center, WHITE, 1.0, 70.0);
        g.icon(glyph::DROP, cx - 6.0, cy + 15.0, 10.0, BLUE, 1.0);
        g.text("%", cx + 2.0, cy + 15.0, 12.0, 600, Align::Left, GREY, 1.0, 20.0);
        self.hit(cx - r, cy - r, r * 2.0, r * 2.0, Act::WaterSub(1));
    }

    fn water_chart(&self) {
        let g = self.g();
        let w = &self.water;
        let first = (w.start / 60) as usize;
        let last = ((w.end + 59) / 60).min(24) as usize;
        let n = (last - first).max(1);
        let (left, right, top, bottom) = (218.0, 344.0, 52.0, 104.0);
        let maxv = w.hours[first..last].iter().copied().max().unwrap_or(0).max(w.bottle).max(1) as f32;
        for (frac, label) in [(1.0, format!("{}", maxv as u32)), (0.5, format!("{}", (maxv / 2.0) as u32)), (0.0, "0 ml".to_string())] {
            let y = bottom - (bottom - top) * frac;
            g.dashed_line(left, y, right, y, 0x2A2A2A, 1.0, 1.0);
            g.text(&label, right + 6.0, y, 10.0, 400, Align::Left, GREY, 1.0, 40.0);
        }
        let slot = (right - left) / n as f32;
        let bw = (slot * 0.6).min(9.0);
        for i in 0..n {
            let x = left + slot * i as f32 + (slot - bw) / 2.0;
            g.fill_rrect(x, top, bw, bottom - top, bw / 2.0, 0x161616, 1.0);
            let v = w.hours[first + i] as f32 / maxv;
            if v > 0.0 {
                let hh = ((bottom - top) * v).max(bw);
                g.fill_rrect(x, bottom - hh, bw, hh, bw / 2.0, BLUE, 1.0);
            }
            let hour = first + i;
            if hour % 3 == 0 {
                g.text(&format!("{:02}", hour), x + bw / 2.0, 116.0, 10.5, 400, Align::Center, GREY, 1.0, 30.0);
            }
        }
        self.hit(left, top, right - left, bottom - top, Act::WaterSub(0));
    }

    fn page_water_settings(&self) {
        let g = self.g();
        let w = &self.water;
        self.round_button(30.0, 19.0, 9.5, 0x3C3C3C, glyph::CHEV_L, 9.0, WHITE, Act::WaterBack);
        g.text("Settings", 45.0, 20.0, 14.5, 600, Align::Left, WHITE, 1.0, 100.0);

        let label = |s: &str, y: f32| g.text(s, 22.0, y, 13.5, 400, Align::Left, 0xD8D8D8, 1.0, 60.0);
        label("Bottle", 50.0);
        self.round_button(87.0, 50.0, 10.5, 0x262626, glyph::REMOVE, 9.5, WHITE, Act::Bottle(-1));
        g.text(&format!("{} ml", w.bottle), 125.0, 50.0, 14.0, 700, Align::Center, WHITE, 1.0, 60.0);
        self.round_button(163.0, 50.0, 10.5, 0x262626, glyph::ADD, 9.5, WHITE, Act::Bottle(1));
        g.text(&format!("{} ml each · {}/bottle", w.each(), w.per_bottle()), 180.0, 50.0, 10.0, 400, Align::Left, GREY, 1.0, 120.0);

        label("Goal", 81.0);
        for (i, goal) in GOALS.iter().enumerate() {
            let x = 75.0 + i as f32 * 57.0;
            let sel = w.goal == *goal;
            if sel {
                if let Some(b) = g.linear(x, 0.0, x + 52.0, 0.0, &[(0.0, 0x56CCFF, 1.0), (1.0, 0x39A9FF, 1.0)]) {
                    g.fill_rrect_brush(x, 69.5, 52.0, 23.0, 11.5, &b);
                }
                self.hit(x, 69.5, 52.0, 23.0, Act::Goal(i));
            } else {
                self.button(x, 69.5, 52.0, 23.0, 11.5, 0x191919, 1.0, Act::Goal(i));
            }
            g.text(&format!("{:.1} L", *goal as f32 / 1000.0), x + 26.0, 81.0, 12.5, 400, Align::Center, WHITE, 1.0, 50.0);
        }

        label("Hours", 111.0);
        self.round_button(87.0, 111.0, 10.5, 0x262626, glyph::REMOVE, 9.5, WHITE, Act::Start(-1));
        g.text(&model::hhmm(w.start), 125.0, 111.0, 14.0, 700, Align::Center, WHITE, 1.0, 60.0);
        self.round_button(163.0, 111.0, 10.5, 0x262626, glyph::ADD, 9.5, WHITE, Act::Start(1));
        g.text("to", 184.0, 111.0, 12.0, 400, Align::Center, GREY, 1.0, 20.0);
        self.round_button(205.0, 111.0, 10.5, 0x262626, glyph::REMOVE, 9.5, WHITE, Act::End(-1));
        g.text(&model::hhmm(w.end), 243.5, 111.0, 14.0, 700, Align::Center, WHITE, 1.0, 60.0);
        self.round_button(282.0, 111.0, 10.5, 0x262626, glyph::ADD, 9.5, WHITE, Act::End(1));

        // summary card
        g.fill_rrect(307.0, 37.0, 91.0, 60.0, 10.0, 0x0F1926, 1.0);
        g.text(&w.reminders().to_string(), 352.5, 52.0, 21.0, 700, Align::Center, CYAN, 1.0, 80.0);
        g.text("reminders", 352.5, 67.0, 10.5, 400, Align::Center, GREY, 1.0, 80.0);
        g.line(322.0, 75.0, 383.0, 75.0, 0x23344A, 1.0, 1.0);
        let rf = w.refills().to_string();
        let rw = g.measure(&rf, 14.0, 700);
        let total = rw + 4.0 + g.measure("refills", 10.5, 400);
        g.text(&rf, 352.5 - total / 2.0, 86.5, 14.0, 700, Align::Left, CYAN, 1.0, 40.0);
        g.text("refills", 352.5 - total / 2.0 + rw + 4.0, 87.0, 10.5, 400, Align::Left, GREY, 1.0, 50.0);

        let confirm = self.reset_confirm.is_some();
        self.button(307.0, 104.0, 91.0, 18.0, 9.0, if confirm { 0x3A1212 } else { 0x171717 }, 1.0, Act::WaterReset);
        g.icon(glyph::DELETE, 323.0, 113.0, 10.0, if confirm { RED } else { WHITE }, 1.0);
        g.text(if confirm { "Sure?" } else { "Reset" }, 333.0, 113.0, 11.5, 400, Align::Left, if confirm { RED } else { WHITE }, 1.0, 60.0);
    }

    fn page_shelf(&self) {
        let g = self.g();
        let items = &self.shelf.items;
        if items.is_empty() || self.shelf_drag {
            let accent = if self.shelf_drag { BLUE } else { 0x5A5A5A };
            if self.shelf_drag {
                g.fill_rrect(44.0, 33.0, 336.0, 90.0, 13.0, BLUE, 0.08);
            }
            g.stroke_rrect(44.0, 33.0, 336.0, 90.0, 13.0, accent, 1.0, 1.3, true);
            let bob = if self.shelf_drag { (self.started.elapsed().as_secs_f32() * 6.0).sin() * 2.0 } else { 0.0 };
            g.icon(glyph::DOWNLOAD, 212.0, 62.0 + bob, 20.0, if self.shelf_drag { BLUE } else { WHITE }, 1.0);
            g.fill_rrect(201.0, 73.0, 22.0, 3.0, 1.5, if self.shelf_drag { BLUE } else { WHITE }, 0.9);
            g.text(if self.shelf_drag { "Release to add" } else { "Drop files here" }, 212.0, 98.0, 13.0, 400, Align::Center, WHITE, 1.0, 300.0);
            g.text("Drag out anytime · click to open", 212.0, 112.0, 10.0, 400, Align::Center, GREY, 1.0, 300.0);
            return;
        }
        self.button(338.0, 11.0, 42.0, 17.0, 8.5, 0x1F1F1F, 1.0, Act::ShelfClear);
        g.text("Clear", 359.0, 19.5, 11.0, 400, Align::Center, WHITE, 1.0, 40.0);
        let max = 6;
        for (i, it) in items.iter().enumerate().take(max) {
            let x = 44.0 + i as f32 * 57.0;
            let y = 32.0;
            let act = Act::ShelfItem(i);
            g.fill_rrect(x, y, 51.0, 88.0, 10.0, 0x161616, 1.0);
            if self.hot(act) {
                g.fill_rrect(x, y, 51.0, 88.0, 10.0, WHITE, 0.07);
            }
            let (gl, c) = match it.kind {
                Kind::Folder => (glyph::FOLDER, 0xFFC94A),
                Kind::Link => (glyph::LINK, BLUE),
                Kind::Image => (glyph::PICTURE, 0xFF7AB6),
                Kind::File => (glyph::DOC, 0xCFCFCF),
            };
            g.icon(gl, x + 25.5, y + 28.0, 22.0, c, 1.0);
            g.text(&it.name, x + 25.5, y + 58.0, 10.0, 400, Align::Center, 0xD8D8D8, 1.0, 46.0);
            let sub = if it.kind == Kind::Folder || it.kind == Kind::Link { "".to_string() } else { shelf::size_label(it.size) };
            g.text(&sub, x + 25.5, y + 72.0, 9.0, 400, Align::Center, DIM, 1.0, 46.0);
            self.hit(x, y, 51.0, 88.0, act);
        }
        if items.len() > max {
            g.text(&format!("+{}", items.len() - max), 380.0, 128.0, 10.0, 400, Align::Right, GREY, 1.0, 40.0);
        }
    }

    fn page_todo(&self) {
        let g = self.g();
        let done = self.tasks.iter().filter(|t| t.done).count();
        g.text(&format!("{}/{}", done, self.tasks.len()), 44.0, 50.0, 34.0, 700, Align::Left, WHITE, 1.0, 150.0);
        g.text("Task Done Today", 44.0, 81.0, 12.0, 400, Align::Left, GREY, 1.0, 150.0);

        let adding = self.form.as_ref().map_or(false, |f| f.edit.is_none());
        self.button(42.0, 95.0, 73.0, 27.0, 13.5, if adding { 0x0F2B17 } else { 0x262626 }, 1.0, Act::TodoAdd);
        g.icon(glyph::ADD_CIRCLE, 56.0, 108.5, 13.0, if adding { GREEN } else { WHITE }, 1.0);
        g.text("Add Task", 66.0, 108.5, 10.5, 400, Align::Left, if adding { GREEN } else { WHITE }, 1.0, 50.0);
        let can_edit = self.task_sel.is_some() && self.form.is_none();
        if can_edit {
            self.button(120.0, 95.0, 71.0, 27.0, 13.5, 0x262626, 1.0, Act::TodoEdit);
        } else {
            g.fill_rrect(120.0, 95.0, 71.0, 27.0, 13.5, 0x121212, 1.0);
        }
        let ec = if can_edit { WHITE } else { 0x4A4A4A };
        g.icon(glyph::EDIT, 133.0, 108.5, 12.0, ec, 1.0);
        g.text("Edit Task", 143.0, 108.5, 10.5, 400, Align::Left, ec, 1.0, 50.0);

        g.fill_rrect(197.0, 32.0, 179.0, 91.0, 14.0, 0x131313, 1.0);
        if let Some(f) = &self.form {
            self.todo_form(f);
            return;
        }
        if self.tasks.is_empty() {
            g.fill_rrect(270.0, 45.0, 32.0, 32.0, 8.0, 0x414141, 1.0);
            g.icon(glyph::CHECKLIST, 286.0, 61.0, 17.0, 0x1A1A1A, 1.0);
            g.text("No tasks.", 286.5, 98.0, 13.0, 400, Align::Center, 0x7A7A7A, 1.0, 160.0);
            return;
        }
        for (row, i) in (self.task_scroll..self.tasks.len()).take(3).enumerate() {
            let t = &self.tasks[i];
            let y = 38.0 + row as f32 * 27.0;
            let sel = self.task_sel == Some(i);
            if sel || self.hot(Act::TodoRow(i)) {
                g.fill_rrect(202.0, y, 169.0, 25.0, 8.0, 0x1F1F1F, 1.0);
            }
            self.hit(222.0, y, 149.0, 25.0, Act::TodoRow(i));
            if t.done {
                g.fill_circle(212.0, y + 12.5, 6.5, GREEN, 1.0);
                g.icon(glyph::CHECK, 212.0, y + 12.5, 7.5, 0x0B2A12, 1.0);
            } else {
                g.stroke_circle(212.0, y + 12.5, 6.0, 0x6A6A6A, 1.0, 1.4);
            }
            self.hit(202.0, y, 20.0, 25.0, Act::TodoCheck(i));
            let time = match (t.start, t.end) {
                (Some(s), Some(e)) => format!("{}–{}", model::hhmm(s), model::hhmm(e)),
                (Some(s), None) => model::hhmm(s),
                _ => String::new(),
            };
            let tw = if time.is_empty() { 0.0 } else { g.measure(&time, 10.0, 400) + 8.0 };
            g.text(&t.title, 224.0, y + 12.5, 12.5, 400, Align::Left, if t.done { DIM } else { WHITE }, 1.0, 143.0 - tw);
            if t.done {
                let lw = g.measure(&t.title, 12.5, 400).min(143.0 - tw);
                g.line(224.0, y + 13.0, 224.0 + lw, y + 13.0, DIM, 1.0, 1.0);
            }
            if !time.is_empty() {
                g.text(&time, 366.0, y + 12.5, 10.0, 400, Align::Right, GREY, 1.0, 80.0);
            }
        }
        if self.tasks.len() > 3 {
            let n = self.tasks.len() as f32;
            let h = 80.0 * 3.0 / n;
            let y = 38.0 + (80.0 - h) * self.task_scroll as f32 / (n - 3.0);
            g.fill_rrect(372.0, y, 2.0, h, 1.0, 0x4A4A4A, 1.0);
        }
    }

    fn todo_form(&self, f: &Form) {
        let g = self.g();
        let blink = (self.started.elapsed().as_millis() / 530) % 2 == 0;
        let focus_title = f.field == 0;
        g.fill_rrect(202.0, 41.0, 168.0, 25.0, 8.0, 0x080808, 1.0);
        g.stroke_rrect(202.0, 41.0, 168.0, 25.0, 8.0, if focus_title { 0xD9D9D9 } else { 0x333333 }, 1.0, 1.4, false);
        self.hit(202.0, 41.0, 168.0, 25.0, Act::TodoTitle);
        if f.title.is_empty() {
            g.text("Task title", 211.0, 53.5, 12.5, 400, Align::Left, 0x6A6A6A, 1.0, 150.0);
        }
        if !f.title.is_empty() || focus_title {
            let cursor = if focus_title && blink { "|" } else { "" };
            // show the tail of long titles
            let mut shown = f.title.clone();
            while g.measure(&shown, 12.5, 400) > 148.0 && !shown.is_empty() {
                shown.remove(0);
            }
            g.text(&format!("{}{}", shown, cursor), 211.0, 53.5, 12.5, 400, Align::Left, WHITE, 1.0, 155.0);
        }
        g.fill_rrect(202.0, 69.0, 168.0, 24.0, 8.0, 0x212124, 1.0);
        if !focus_title {
            g.stroke_rrect(202.0, 69.0, 168.0, 24.0, 8.0, 0xD9D9D9, 1.0, 1.2, false);
        }
        self.hit(202.0, 69.0, 168.0, 24.0, Act::TodoTime);
        g.icon(glyph::CLOCK, 215.0, 81.0, 12.0, WHITE, 1.0);
        let mask = model::time_mask(&f.digits);
        g.text(&mask, 228.0, 81.0, 12.5, 400, Align::Left, if f.digits.is_empty() { 0x6A6A6A } else { WHITE }, 1.0, 130.0);
        if f.edit.is_some() {
            self.round_button(358.0, 81.0, 8.0, 0x2A1515, glyph::DELETE, 8.5, RED, Act::TodoDelete);
        }
        self.button(203.0, 99.0, 80.0, 20.0, 10.0, 0x262626, 1.0, Act::TodoCancel);
        g.text("Cancel", 243.0, 109.0, 12.0, 400, Align::Center, WHITE, 1.0, 70.0);
        self.button(290.0, 99.0, 80.0, 20.0, 10.0, GREEN, 1.0, Act::TodoSave);
        g.text("Save", 330.0, 109.0, 12.0, 700, Align::Center, 0x0B2A12, 1.0, 70.0);
    }
}

pub fn open_path(p: &std::path::Path) {
    let wide: Vec<u16> = p.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        ShellExecuteW(None, windows::core::w!("open"), windows::core::PCWSTR(wide.as_ptr()), None, None, SW_SHOWNORMAL);
    }
}

pub fn activate(hwnd: HWND, on: bool) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if on {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !(WS_EX_NOACTIVATE.0 as isize));
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(hwnd);
        } else {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE.0 as isize);
        }
    }
}
