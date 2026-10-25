//! Kuro amber panel chrome: the header (page icon + title, clock with an amber colon), the
//! Home page of bento tiles that summarise every page, and the Settings page that replaces
//! most of the tray menu. Top-dock and side-dock (vertical) variants live side by side.

use super::*;

/// One Home tile: target page, icon, label, value, small suffix, value colour.
type Tile = (Page, u16, &'static str, String, String, u32);

impl App {
    fn page_title(page: Page) -> (u16, &'static str) {
        match page {
            Page::Home => (glyph::HOME, "Home"),
            Page::Media => (glyph::MUSIC, "Music"),
            Page::Pomodoro => (glyph::STOPWATCH, "Pomodoro"),
            Page::Water => (glyph::DROP, "Water"),
            Page::Shelf => (glyph::ATTACH, "Shelf"),
            Page::Todo => (glyph::CHECKLIST, "To-do"),
            Page::Settings => (glyph::GEAR, "Settings"),
        }
    }

    /// Clock (hh:mm), right-aligned at `right`.
    pub(super) fn clock(&self, right: f32, cy: f32, size: f32, color: u32) {
        let (_, time) = date_strings();
        self.g().text(&time, right, cy, size, 700, Align::Right, color, 1.0, 80.0);
    }

    /// Header row: amber page icon and title on the left, the clock (and a running
    /// Pomodoro) on the right. `w` is the panel width.
    pub(super) fn header(&self, w: f32, cy: f32, left: f32) {
        let g = self.g();
        let page = PAGES[self.page];
        // the music page shows its album art instead; water settings has its own back row
        if page != Page::Media && !(page == Page::Water && self.water_settings) {
            let (icon, title) = Self::page_title(page);
            // the icon pops a little while the page slides in
            let pop = 1.0 + 0.35 * (self.slide.v.abs() / 36.0).min(1.0);
            g.icon(icon, left + 6.0, cy, 12.0 * pop, AMBER, 1.0);
            g.text(title, left + 18.0, cy, 13.5, 600, Align::Left, WHITE, 1.0, 120.0);
        }
        let right = w - left;
        self.clock(right, cy, 12.0, GREY);
        if self.pomo.running && page != Page::Pomodoro {
            let frac = (self.pomo.remaining / self.pomo.mode.secs()) as f32;
            let px = right - g.measure("00:00", 12.0, 700) - 14.0;
            g.stroke_circle(px, cy, 6.0, 0x2A2416, 1.0, 2.2);
            g.arc(px, cy, 6.0, -90.0, 360.0 * frac, self.pomo_color(), 1.0, 2.2);
        }
    }

    /// Accent of the Pomodoro: amber while focusing, green on a break.
    pub(super) fn pomo_color(&self) -> u32 {
        if self.pomo.mode == PMode::Focus {
            AMBER
        } else {
            GREEN
        }
    }

    /// A card with a 1px hairline; hover highlight and click squeeze come from `button`.
    fn tile_card(&self, x: f32, y: f32, w: f32, h: f32, act: Act) {
        self.button(x, y, w, h, 12.0, CARD, 1.0, act);
        let s = self.squeeze(act);
        let (cw, ch) = (w * s, h * s);
        self.g().stroke_rrect(x + (w - cw) / 2.0, y + (h - ch) / 2.0, cw, ch, 12.0 * s, HAIR, 1.0, 1.0, false);
    }

    fn tiles(&self) -> [Tile; 4] {
        let p = &self.pomo;
        let (focus, focus_color) = if p.running {
            (mmss(p.remaining.ceil()), self.pomo_color())
        } else {
            (mmss(p.mode.secs()), WHITE)
        };
        let w = &self.water;
        let done = self.tasks.iter().filter(|t| t.done).count();
        let n = self.shelf.items.len();
        [
            (Page::Pomodoro, glyph::STOPWATCH, if p.mode == PMode::Focus { "Focus" } else { "Break" }, focus, String::new(), focus_color),
            (Page::Water, glyph::DROP, "Water", format!("{:.1}", w.taken as f32 / 1000.0), format!(" / {:.1} L", w.goal as f32 / 1000.0), WHITE),
            (Page::Todo, glyph::CHECKLIST, "To-do", done.to_string(), format!(" / {}", self.tasks.len()), WHITE),
            (Page::Shelf, glyph::ATTACH, "Shelf", if n == 0 { "Empty".into() } else { n.to_string() }, if n == 0 { String::new() } else { " files".into() }, WHITE),
        ]
    }

    fn small_tile(&self, x: f32, y: f32, w: f32, h: f32, t: &Tile) {
        let g = self.g();
        let (page, icon, label, value, suffix, color) = t;
        self.tile_card(x, y, w, h, Act::Tile(page_of(*page)));
        g.icon(*icon, x + 15.0, y + 14.0, 10.5, GREY, 1.0);
        g.text(label, x + 24.0, y + 14.0, 10.5, 400, Align::Left, GREY, 1.0, w - 30.0);
        let vy = y + h - 15.0;
        g.text(value, x + 10.0, vy, 15.0, 700, Align::Left, *color, 1.0, w - 14.0);
        if !suffix.is_empty() {
            let vw = g.measure(value, 15.0, 700);
            g.text(suffix, x + 10.0 + vw, vy + 1.0, 10.5, 400, Align::Left, GREY, 1.0, w - 14.0 - vw);
        }
    }

    /// Music tile: art, title, artist (or the current lyric), progress and play/pause.
    fn music_tile(&self, x: f32, y: f32, w: f32, h: f32, art: f32) {
        let g = self.g();
        let m = &self.media;
        self.tile_card(x, y, w, h, Act::Tile(page_of(Page::Media)));
        let has = m.has_session && !m.title.is_empty();
        self.art(x + 10.0, y + 10.0, art, art * 0.25);
        let tx = x + art + 20.0;
        let tw = w - art - 30.0;
        if has {
            let sub = if self.settings.lyrics && self.lyric_idx.is_some() { self.lyric_text() } else { m.artist.clone() };
            g.text(&m.title, tx, y + 10.0 + art * 0.3, 13.0, 700, Align::Left, WHITE, 1.0, tw);
            g.text(&sub, tx, y + 10.0 + art * 0.3 + 18.0, 11.0, 400, Align::Left, GREY, 1.0, tw);
        } else {
            g.text("No media", tx, y + 10.0 + art * 0.45, 13.0, 700, Align::Left, 0x7A7A7A, 1.0, tw);
        }
        // progress + play/pause along the bottom
        let by = y + h - 18.0;
        let (bx, bw) = (x + 12.0, w - 58.0);
        g.fill_rrect(bx, by - 1.5, bw, 3.0, 1.5, 0x262626, 1.0);
        if m.duration > 0.0 {
            let f = (m.pos_now() / m.duration).clamp(0.0, 1.0) as f32;
            g.fill_rrect(bx, by - 1.5, (bw * f).max(3.0), 3.0, 1.5, AMBER, 1.0);
        }
        self.play_button(x + w - 22.0, by, 12.0, has);
    }

    /// Amber play/pause disc; pause bars and play triangle cross-fade.
    pub(super) fn play_button(&self, cx: f32, cy: f32, r: f32, enabled: bool) {
        let (bg, fg) = if enabled { (AMBER, ON_AMBER) } else { (0x3A3A3A, 0x777777) };
        self.play_disc(cx, cy, r, bg, fg, self.media.playing, Act::MediaPlay);
    }

    /// Home, top dock: music on the left, a 2x2 grid of the other pages on the right.
    pub(super) fn page_home(&self) {
        self.music_tile(40.0, 32.0, 176.0, 94.0, 40.0);
        let tiles = self.tiles();
        let (x0, w, h) = (222.0, 76.0, 44.0);
        for (i, t) in tiles.iter().enumerate() {
            self.small_tile(x0 + (i % 2) as f32 * (w + 6.0), 32.0 + (i / 2) as f32 * (h + 6.0), w, h, t);
        }
    }

    /// Home, side dock: music on top, the other pages as a 2x2 grid below.
    pub(super) fn page_home_v(&self) {
        self.music_tile(18.0, 50.0, 204.0, 130.0, 60.0);
        let tiles = self.tiles();
        let (w, h) = (99.0, 72.0);
        for (i, t) in tiles.iter().enumerate() {
            self.small_tile(18.0 + (i % 2) as f32 * (w + 6.0), 188.0 + (i / 2) as f32 * (h + 6.0), w, h, t);
        }
    }

    /// Amber switch; the knob slides and the colour blends (animated by `Fx`).
    pub(super) fn switch(&self, x: f32, cy: f32, on: bool, act: Act) {
        let g = self.g();
        let k = self.fx.level(act, on as u8 as f32);
        g.fill_rrect(x, cy - 8.0, 30.0, 16.0, 8.0, 0x2A2A2A, 1.0);
        g.fill_rrect(x, cy - 8.0, 30.0, 16.0, 8.0, AMBER, k);
        let kx = x + 8.0 + 14.0 * k;
        g.fill_circle(kx, cy, 6.0, if k > 0.5 { ON_AMBER } else { 0x8A8A8A }, 1.0);
    }

    fn setting_rows(&self) -> [(&'static str, Option<bool>, Act); 7] {
        let s = &self.settings;
        [
            ("Start with Windows", Some(self.autostart), Act::Setting(0)),
            ("Kuro the cat", Some(s.pet), Act::Setting(1)),
            ("Romaji lyrics", Some(s.romaji), Act::Setting(2)),
            ("Lyrics", Some(s.lyrics), Act::Setting(3)),
            ("Minimal notch", Some(s.minimal), Act::Setting(4)),
            ("Screenshots to Shelf", Some(s.screenshots), Act::Setting(5)),
            ("Reset position", None, Act::Setting(6)),
        ]
    }

    fn setting_row(&self, x: f32, cy: f32, w: f32, label: &str, on: Option<bool>, act: Act) {
        let g = self.g();
        let k = self.hov(act);
        if k > 0.0 {
            g.fill_rrect(x - 6.0, cy - 11.0, w + 12.0, 22.0, 8.0, WHITE, 0.06 * k);
        }
        g.text(label, x, cy, 12.5, 400, Align::Left, WHITE, 1.0, w - 40.0);
        match on {
            Some(on) => self.switch(x + w - 30.0, cy, on, act),
            None => {
                let moved = self.settings.dock != Dock::Top || (self.settings.offset - 0.5).abs() > 0.001;
                g.fill_rrect(x + w - 44.0, cy - 9.0, 44.0, 18.0, 9.0, 0x262626, 1.0);
                g.text("Reset", x + w - 22.0, cy, 11.0, 400, Align::Center, if moved { WHITE } else { 0x6A6A6A }, 1.0, 44.0);
            }
        }
        self.hit(x - 6.0, cy - 11.0, w + 12.0, 22.0, act);
    }

    /// Settings, top dock: two columns (four rows, then three).
    pub(super) fn page_settings(&self) {
        for (i, (label, on, act)) in self.setting_rows().into_iter().enumerate() {
            let x = if i < 4 { 30.0 } else { 222.0 };
            self.setting_row(x, 42.0 + (i % 4) as f32 * 23.0, 168.0, label, on, act);
        }
        let g = self.g();
        g.line(214.0, 38.0, 214.0, 114.0, HAIR, 1.0, 1.0);
    }

    /// Settings, side dock: one column, version at the bottom.
    pub(super) fn page_settings_v(&self) {
        let g = self.g();
        for (i, (label, on, act)) in self.setting_rows().into_iter().enumerate() {
            let cy = 70.0 + i as f32 * 36.0;
            self.setting_row(PAD + 4.0, cy, CW - 8.0, label, on, act);
            if i < 6 {
                g.line(PAD, cy + 19.0, PAD + CW, cy + 19.0, HAIR, 1.0, 1.0);
            }
        }
        g.text(concat!("Kuro Nesos ", env!("CARGO_PKG_VERSION")), PW / 2.0, 380.0, 10.5, 400, Align::Center, 0x5A5A5A, 1.0, CW);
    }
}

const PW: f32 = 240.0;
const PAD: f32 = 18.0;
const CW: f32 = PW - 2.0 * PAD;

/// Index of `p` in the page order.
pub(super) fn page_of(p: Page) -> usize {
    PAGES.iter().position(|&q| q == p).unwrap_or(0)
}
