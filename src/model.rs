//! Persistent models: settings, water reminder, to-do list, autostart.
//! Files live in %APPDATA%\KuroNesos (or, in portable mode, in KuroNesosData next to the exe).
//!   settings.txt  pet=1 / minimal=0 / lyrics=1 / romaji=1 / dock=top|left|right / offset=0.500 / screenshots=1
//!   water.txt     bottle, goal, start, end, day, taken, inb, fired, vis, hours, on
//!   todo.txt      day=YYYYMMDD then one task per line: "done|HH:MM|HH:MM|title"

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use windows::core::w;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::*;
use windows::Win32::System::SystemInformation::GetLocalTime;

/// Portable mode: when a `portable.txt` sits next to the exe, every file (settings, water,
/// to-do, shelf, log, Japanese dictionary) lives in a `KuroNesosData` folder beside it.
pub fn portable_dir() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let exe = std::env::current_exe().ok()?;
        let base = exe.parent()?;
        base.join("portable.txt").is_file().then(|| base.join("KuroNesosData"))
    })
    .clone()
}

pub fn data_dir(var: &str) -> PathBuf {
    let d = portable_dir().unwrap_or_else(|| std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".")).join("KuroNesos"));
    let _ = fs::create_dir_all(&d);
    d
}

pub fn roaming() -> PathBuf {
    data_dir("APPDATA")
}

pub fn shelf_dir() -> PathBuf {
    let d = data_dir("LOCALAPPDATA").join("Shelf");
    let _ = fs::create_dir_all(&d);
    d
}

fn kv(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|l| l.split_once('='))
}

pub struct Now {
    pub day: u32,
    pub minutes: u32,
    pub hour: usize,
    pub second: u32,
}

pub fn now() -> Now {
    let t = unsafe { GetLocalTime() };
    Now {
        day: t.wYear as u32 * 10000 + t.wMonth as u32 * 100 + t.wDay as u32,
        minutes: t.wHour as u32 * 60 + t.wMinute as u32,
        hour: t.wHour as usize,
        second: t.wSecond as u32,
    }
}

pub fn hhmm(m: u32) -> String {
    format!("{:02}:{:02}", (m / 60) % 24, m % 60)
}

fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m) = (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

// ------------------------------------------------------------------ settings

#[derive(Clone)]
pub struct Settings {
    pub pet: bool,
    pub minimal: bool,
    pub lyrics: bool,
    /// Show lyrics in Latin letters (romaji, pinyin, ...) when they use another script.
    pub romaji: bool,
    pub dock: Dock,
    /// Centre of the notch along its edge, as a fraction of the screen (0.5 = middle).
    pub offset: f32,
    /// Put new screenshots on the Shelf.
    pub screenshots: bool,
}

/// Where the notch lives: hanging from the top edge, or against the left/right edge.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dock {
    Top,
    Left,
    Right,
}

impl Dock {
    fn name(self) -> &'static str {
        match self {
            Dock::Top => "top",
            Dock::Left => "left",
            Dock::Right => "right",
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let mut s = Settings { pet: true, minimal: false, lyrics: true, romaji: true, dock: Dock::Top, offset: 0.5, screenshots: true };
        if let Ok(t) = fs::read_to_string(roaming().join("settings.txt")) {
            for (k, v) in kv(&t) {
                let on = v.trim() == "1";
                match k {
                    "pet" => s.pet = on,
                    "minimal" => s.minimal = on,
                    "lyrics" => s.lyrics = on,
                    "romaji" => s.romaji = on,
                    "screenshots" => s.screenshots = on,
                    "dock" => {
                        s.dock = match v.trim() {
                            "left" => Dock::Left,
                            "right" => Dock::Right,
                            _ => Dock::Top,
                        }
                    }
                    "offset" => s.offset = v.trim().parse::<f32>().unwrap_or(0.5).clamp(0.0, 1.0),
                    _ => {}
                }
            }
        }
        s
    }

    pub fn save(&self) {
        let t = format!(
            "pet={}\nminimal={}\nlyrics={}\nromaji={}\ndock={}\noffset={:.3}\nscreenshots={}\n",
            self.pet as u8,
            self.minimal as u8,
            self.lyrics as u8,
            self.romaji as u8,
            self.dock.name(),
            self.offset,
            self.screenshots as u8
        );
        let _ = fs::write(roaming().join("settings.txt"), t);
    }
}

// ------------------------------------------------------------------ water

pub const GOALS: [u32; 4] = [1500, 2000, 2500, 3000];
const SLOT_MINUTES: u32 = 15;

#[derive(Clone)]
pub struct Water {
    pub bottle: u32,
    pub goal: u32,
    pub start: u32,
    pub end: u32,
    pub day: u32,
    pub taken: u32,
    pub inb: u32,
    pub fired: u32,
    pub vis: bool,
    pub hours: [u32; 24],
    pub on: bool,
    pub snooze_until: u32,
    pub goal_announced: bool,
}

pub enum WaterEvent {
    Drink,
    Refill,
    GoalReached,
}

impl Water {
    fn fresh(day: u32) -> Self {
        Water { bottle: 500, goal: 2000, start: 480, end: 1020, day, taken: 0, inb: 500, fired: 0, vis: true, hours: [0; 24], on: true, snooze_until: 0, goal_announced: false }
    }

    pub fn load() -> Self {
        let n = now();
        let mut w = Water::fresh(n.day);
        let mut saved_day = n.day;
        if let Ok(t) = fs::read_to_string(roaming().join("water.txt")) {
            for (k, v) in kv(&t) {
                let x = v.trim().parse::<u32>().unwrap_or(0);
                match k {
                    "bottle" => w.bottle = x.clamp(100, 3000),
                    "goal" => w.goal = x.clamp(500, 5000),
                    "start" => w.start = x.min(1380),
                    "end" => w.end = x.min(1439),
                    "day" => saved_day = x,
                    "taken" => w.taken = x,
                    "inb" => w.inb = x,
                    "fired" => w.fired = x,
                    "vis" => w.vis = x != 0,
                    "on" => w.on = x != 0,
                    "hours" => {
                        for (i, h) in v.split(',').take(24).enumerate() {
                            w.hours[i] = h.trim().parse().unwrap_or(0);
                        }
                    }
                    _ => {}
                }
            }
        }
        if w.end <= w.start + 60 {
            w.end = (w.start + 60).min(1439);
        }
        if saved_day != n.day {
            w.new_day(n.day);
        }
        w.goal_announced = w.taken >= w.goal;
        w
    }

    fn new_day(&mut self, day: u32) {
        self.day = day;
        self.taken = 0;
        self.fired = 0;
        self.hours = [0; 24];
        self.inb = self.bottle;
        self.snooze_until = 0;
        self.goal_announced = false;
    }

    pub fn save(&self) {
        let hours: Vec<String> = self.hours.iter().map(|h| h.to_string()).collect();
        let t = format!(
            "bottle={}\ngoal={}\nstart={}\nend={}\nday={}\ntaken={}\ninb={}\nfired={}\nvis={}\nhours={}\non={}\n",
            self.bottle, self.goal, self.start, self.end, self.day, self.taken, self.inb, self.fired, self.vis as u8, hours.join(","), self.on as u8
        );
        let _ = fs::write(roaming().join("water.txt"), t);
    }

    /// Number of reminders over the active window (one every 15 minutes).
    pub fn reminders(&self) -> u32 {
        ((self.end.saturating_sub(self.start)) / SLOT_MINUTES).max(1)
    }

    /// Millilitres per reminder.
    pub fn each(&self) -> u32 {
        (self.goal as f32 / self.reminders() as f32).round().max(1.0) as u32
    }

    pub fn per_bottle(&self) -> u32 {
        (self.bottle as f32 / self.each() as f32).round().max(1.0) as u32
    }

    pub fn refills(&self) -> u32 {
        self.goal.div_ceil(self.bottle.max(1)).saturating_sub(1)
    }

    pub fn interval(&self) -> f32 {
        (self.end - self.start) as f32 / self.reminders() as f32
    }

    pub fn slot_time(&self, i: u32) -> u32 {
        self.start + ((i as f32 + 0.5) * self.interval()) as u32
    }

    pub fn next_time(&self) -> Option<u32> {
        if !self.on || self.taken >= self.goal || self.fired >= self.reminders() {
            return None;
        }
        Some(self.slot_time(self.fired).max(self.snooze_until))
    }

    pub fn percent(&self) -> u32 {
        ((self.taken as f32 / self.goal.max(1) as f32) * 100.0).round() as u32
    }

    pub fn drink(&mut self) -> Option<WaterEvent> {
        let n = now();
        let amount = self.each();
        self.taken += amount;
        self.inb = self.inb.saturating_sub(amount);
        self.hours[n.hour.min(23)] += amount;
        self.snooze_until = 0;
        if self.taken >= self.goal && !self.goal_announced {
            self.goal_announced = true;
            return Some(WaterEvent::GoalReached);
        }
        None
    }

    pub fn refill(&mut self) {
        self.inb = self.bottle;
    }

    pub fn snooze(&mut self) {
        self.snooze_until = now().minutes + 10;
    }

    pub fn reset(&mut self) {
        self.new_day(now().day);
    }

    pub fn set_bottle(&mut self, b: u32) {
        self.bottle = b.clamp(100, 3000);
        self.inb = self.inb.min(self.bottle);
    }

    pub fn set_hours(&mut self, start: u32, end: u32) {
        if end >= start + 60 && end <= 1439 {
            self.start = start;
            self.end = end;
            self.resync_fired();
        }
    }

    /// After a schedule change, count past slots as already handled.
    pub fn resync_fired(&mut self) {
        let m = now().minutes;
        self.fired = (0..self.reminders()).filter(|&i| self.slot_time(i) <= m).count() as u32;
    }

    /// Called periodically; returns an event when a reminder is due.
    pub fn check(&mut self) -> Option<WaterEvent> {
        let n = now();
        if n.day != self.day {
            self.new_day(n.day);
            self.save();
        }
        let next = self.next_time()?;
        if n.minutes < next || n.minutes > self.end {
            return None;
        }
        // skip slots that were missed while the PC was asleep
        while self.fired + 1 < self.reminders() && self.slot_time(self.fired + 1) <= n.minutes {
            self.fired += 1;
        }
        self.fired += 1;
        self.snooze_until = 0;
        Some(if self.inb < self.each() { WaterEvent::Refill } else { WaterEvent::Drink })
    }
}

// ------------------------------------------------------------------ to-do

#[derive(Clone)]
pub struct Task {
    pub done: bool,
    pub start: Option<u32>,
    pub end: Option<u32>,
    pub title: String,
    pub alerted: bool,
    pub later_until: Option<u32>,
}

pub fn load_tasks() -> Vec<Task> {
    let n = now();
    let text = fs::read_to_string(roaming().join("todo.txt")).unwrap_or_default();
    let mut day = n.day;
    let mut tasks = Vec::new();
    for line in text.lines() {
        if let Some(d) = line.strip_prefix("day=") {
            day = d.trim().parse().unwrap_or(n.day);
            continue;
        }
        let mut p = line.splitn(4, '|');
        let (Some(done), Some(s), Some(e), Some(title)) = (p.next(), p.next(), p.next(), p.next()) else { continue };
        let start = parse_hhmm(s);
        tasks.push(Task {
            done: done == "1",
            start,
            end: parse_hhmm(e),
            title: title.to_string(),
            alerted: start.map_or(false, |s| s <= n.minutes),
            later_until: None,
        });
    }
    if day != n.day {
        tasks.retain(|t| !t.done);
        for t in tasks.iter_mut() {
            t.alerted = false;
        }
    }
    tasks
}

pub fn save_tasks(tasks: &[Task]) {
    let mut out = format!("day={}\n", now().day);
    for t in tasks {
        out.push_str(&format!(
            "{}|{}|{}|{}\n",
            t.done as u8,
            t.start.map(hhmm).unwrap_or_default(),
            t.end.map(hhmm).unwrap_or_default(),
            t.title.replace(['\n', '|'], " ")
        ));
    }
    let _ = fs::write(roaming().join("todo.txt"), out);
}

/// Parses the "HH:MM - HH:MM" digit mask used by the task form (up to 8 digits).
pub fn parse_time_digits(d: &str) -> (Option<u32>, Option<u32>) {
    let digits: Vec<u32> = d.chars().filter_map(|c| c.to_digit(10)).collect();
    let take = |i: usize| -> Option<u32> {
        if digits.len() < i + 4 {
            return None;
        }
        let h = digits[i] * 10 + digits[i + 1];
        let m = digits[i + 2] * 10 + digits[i + 3];
        (h < 24 && m < 60).then_some(h * 60 + m)
    };
    (take(0), take(4))
}

pub fn time_mask(d: &str) -> String {
    let mut chars: Vec<char> = d.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
    while chars.len() < 8 {
        chars.push('_');
    }
    format!("{}{}:{}{} - {}{}:{}{}", chars[0], chars[1], chars[2], chars[3], chars[4], chars[5], chars[6], chars[7])
}

/// Returns indexes of tasks whose start time has just arrived.
pub fn due_tasks(tasks: &mut [Task]) -> Vec<usize> {
    let m = now().minutes;
    let mut out = Vec::new();
    for (i, t) in tasks.iter_mut().enumerate() {
        if t.done {
            continue;
        }
        if let Some(l) = t.later_until {
            if m >= l {
                t.later_until = None;
                out.push(i);
            }
            continue;
        }
        if let Some(s) = t.start {
            if !t.alerted && m >= s {
                t.alerted = true;
                out.push(i);
            }
        }
    }
    out
}

// ------------------------------------------------------------------ autostart (HKCU Run)

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: windows::core::PCWSTR = w!("KuroNesos");

pub fn autostart_enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, None) == ERROR_SUCCESS }
}

pub fn set_autostart(on: bool) {
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let value: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().chain(std::iter::once(0)).collect();
            let _ = RegSetKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, REG_SZ.0, Some(value.as_ptr() as *const _), (value.len() * 2) as u32);
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
        }
    }
}
