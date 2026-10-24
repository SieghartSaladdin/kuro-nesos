//! Vertical layouts used when the notch is docked to the left or right screen edge:
//! a narrow tab at rest, tall cards for live content and a 240 x 440 panel on hover.

use super::*;

const PW: f32 = 240.0; // expanded panel width
const PAD: f32 = 18.0;
const CW: f32 = PW - 2.0 * PAD; // content width

impl App {
    pub(super) fn vertical(&self) -> bool {
        self.settings.dock != Dock::Top
    }

    /// Visible rows in the to-do list.
    pub(super) fn todo_rows(&self) -> usize {
        if self.vertical() {
            7
        } else {
            3
        }
    }

    fn text_h(&self, s: &str, w: f32, size: f32, weight: u32) -> f32 {
        self.gfx.as_ref().map_or(size * 1.4, |g| g.text_height(s, w, size, weight))
    }

    pub(super) fn shape_for_v(&self, m: Mode) -> (f32, f32, f32) {
        match m {
            Mode::Expanded => (PW, 440.0, 28.0),
            Mode::Alert => match &self.alert {
                Some(a) if !a.actions.is_empty() => (176.0, 150.0, 22.0),
                Some(a) if a.card => (176.0, 52.0 + self.text_h(&a.value, 152.0, 11.5, 400).min(64.0) + 16.0, 20.0),
                _ => (150.0, 78.0, 18.0),
            },
            // the lyric card grows with the wrapped line
            Mode::Lyrics => (140.0, (44.0 + self.text_h(&self.lyric_text(), 120.0, 12.5, 600).min(180.0) + 12.0).max(70.0), 18.0),
            Mode::Live => (46.0, 120.0, 16.0),
            Mode::Timer => (56.0, 124.0, 18.0),
            Mode::Minimal => (28.0, 72.0, 10.0),
            Mode::Collapsed | Mode::Mascot => (40.0, 110.0, 14.0),
        }
    }

    // ------------------------------------------------------------ compact states

    pub(super) fn paint_collapsed_v(&self, x0: f32, w: f32, h: f32) {
        self.origin(x0, self.top.get());
        let g = self.g();
        let (_, time) = date_strings();
        let (hh, mm) = time.split_once(':').unwrap_or((time.as_str(), ""));
        g.text(hh, w / 2.0, h / 2.0 - 13.0, 15.0, 700, Align::Center, WHITE, 1.0, w);
        g.fill_circle(w / 2.0, h / 2.0, 1.6, GREY, 1.0);
        g.text(mm, w / 2.0, h / 2.0 + 13.0, 15.0, 700, Align::Center, WHITE, 1.0, w);
    }

    pub(super) fn paint_live_v(&self, x0: f32, w: f32, h: f32) {
        self.origin(x0, self.top.get());
        self.art(w / 2.0 - 15.0, 14.0, 30.0, 8.0);
        self.eq_bars(w / 2.0 - 7.6, h - 26.0, GREEN);
    }

    pub(super) fn paint_timer_v(&self, x0: f32, w: f32) {
        self.origin(x0, self.top.get());
        let g = self.g();
        let frac = (self.pomo.remaining / self.pomo.mode.secs()) as f32;
        let focus = self.pomo.mode == PMode::Focus;
        let color = if focus { RED } else { GREEN };
        let c = w / 2.0;
        g.stroke_circle(c, 32.0, 14.0, 0x3A1A1A, 1.0, 3.0);
        g.arc(c, 32.0, 14.0, -90.0, 360.0 * frac, color, 1.0, 3.0);
        g.text(if focus { "Focus" } else { "Break" }, c, 62.0, 10.0, 600, Align::Center, GREY, 1.0, w);
        let s = self.pomo.remaining.max(0.0) as u64;
        g.text(&format!("{:02}", s / 60), c, 84.0, 15.0, 700, Align::Center, color, 1.0, w);
        g.text(&format!("{:02}", s % 60), c, 104.0, 15.0, 700, Align::Center, color, 0.75, w);
    }

    pub(super) fn paint_mascot_v(&self, x0: f32, y0: f32, w: f32, h: f32) {
        self.origin(0.0, 0.0);
        let g = self.g();
        let (_, open, happy) = self.mascot_phase();
        let t = self.started.elapsed().as_secs_f32();
        let c = x0 + w / 2.0;
        let cy = y0 + h / 2.0 - 10.0;
        let (_, my) = self.cursor_logical();
        let look = if open > 0.0 || happy > 0.0 { ((my - cy) / 80.0).clamp(-1.0, 1.0) * 2.0 } else { 0.0 };
        for ex in [c - 8.0, c + 8.0] {
            let ey = cy + look;
            if happy > 0.01 {
                g.glow(ex, ey, 7.0, 6.5, 0xBFD8FF, 0.35 * happy);
                let s = 3.4;
                let pts: Vec<(f32, f32)> = (0..=8)
                    .map(|i| {
                        let a = std::f32::consts::PI * (1.0 + i as f32 / 8.0);
                        (ex + a.cos() * s, ey + 1.2 + a.sin() * s * 0.85)
                    })
                    .collect();
                g.polyline(&pts, WHITE, happy, 1.7);
            } else {
                let ry = 0.9 + 3.8 * open;
                let pulse = if open < 0.01 { 0.75 + 0.25 * (t * 2.0).sin() } else { 1.0 };
                g.glow(ex, ey, 6.5, 4.0 + ry, 0xBFD8FF, 0.45 * pulse);
                g.fill_ellipse(ex, ey, 2.9 - 0.5 * open, ry, WHITE, pulse);
            }
        }
        if happy > 0.01 {
            // waving paw under the eyes
            let (px, py) = (c, cy + 30.0);
            g.set_transform(rotation((t * 9.0).sin() * 18.0, px, py + 4.0));
            g.glow(px, py, 8.0, 8.0, 0xBFD8FF, 0.3 * happy);
            g.fill_ellipse(px, py + 1.0, 3.2, 2.9, WHITE, happy);
            for (dx, dy) in [(-2.7, -2.4), (0.0, -3.6), (2.7, -2.4)] {
                g.fill_circle(px + dx, py + dy, 1.35, WHITE, happy);
            }
            self.origin(0.0, 0.0);
        }
    }

    pub(super) fn paint_lyrics_v(&self, x0: f32, w: f32, h: f32) {
        self.origin(x0, self.top.get());
        let g = self.g();
        self.art(10.0, 10.0, 22.0, 6.0);
        self.eq_bars(w - 26.0, 21.0, CYAN);
        let text = self.lyric_text();
        let shown = if text.is_empty() { "♪".to_string() } else { text };
        let t = ease(self.lyric_change.elapsed().as_secs_f32() / 0.25);
        let a = g.alpha.get();
        g.alpha.set(a * t);
        g.text_box(&shown, 10.0, 42.0 + (1.0 - t) * 5.0, w - 20.0, h - 50.0, 12.5, 600, WHITE, 1.0);
        g.alpha.set(a);
    }

    pub(super) fn paint_alert_v(&self, x0: f32, w: f32, h: f32) {
        let Some(a) = &self.alert else { return };
        self.origin(x0, self.top.get());
        let g = self.g();
        if !a.big() {
            self.alert_icon(a.icon, w / 2.0, 22.0, 10.0);
            g.text(&a.title, w / 2.0, 45.0, 12.5, 600, Align::Center, WHITE, 1.0, w - 16.0);
            g.text(&a.value, w / 2.0, 62.0, 11.5, 400, Align::Center, a.color, 1.0, w - 16.0);
            return;
        }
        g.glow(24.0, 24.0, 20.0, 20.0, a.color, 0.18);
        g.fill_circle(24.0, 24.0, 13.0, a.color, 0.16);
        self.alert_icon(a.icon, 24.0, 24.0, 10.0);
        g.text(&a.title, 44.0, 24.0, 13.0, 700, Align::Left, WHITE, 1.0, w - 54.0);
        let body_h = if a.actions.is_empty() { h - 62.0 } else { h - 92.0 };
        g.text_box(&a.value, 12.0, 44.0, w - 24.0, body_h.max(14.0), 11.5, 400, GREY, 1.0);
        if !a.actions.is_empty() {
            let n = a.actions.len() as f32;
            let bw = (w - 24.0 - (n - 1.0) * 8.0) / n;
            for (i, (label, _)) in a.actions.iter().enumerate() {
                let bx = 12.0 + i as f32 * (bw + 8.0);
                let primary = i == 0;
                self.button(bx, h - 40.0, bw, 24.0, 12.0, if primary { a.color } else { 0x262626 }, if primary { 0.95 } else { 1.0 }, Act::Alert(i));
                g.text(label, bx + bw / 2.0, h - 28.0, 12.0, 600, Align::Center, if primary { 0x08121C } else { WHITE }, 1.0, bw);
            }
        }
        if let Some(b) = a.born {
            let f = 1.0 - (b.elapsed().as_secs_f32() / a.ttl.as_secs_f32()).min(1.0);
            g.fill_rrect(12.0, h - 10.0, (w - 24.0) * f, 2.0, 1.0, if a.color == WHITE { GREY } else { a.color }, 0.5);
        }
    }

    // ------------------------------------------------------------ expanded panel

    /// Page dots with the gliding indicator, centred on (cx, cy).
    pub(super) fn page_dots(&self, cx: f32, cy: f32) {
        let g = self.g();
        let gap = 13.0;
        let first = cx - gap * (PAGES.len() as f32 - 1.0) / 2.0;
        for i in 0..PAGES.len() {
            let x = first + gap * i as f32;
            let color = if self.hot(Act::Dot(i)) { 0xC8CDD5 } else { 0x5B6270 };
            g.fill_circle(x, cy, 2.0, color, 1.0);
            self.hit(x - gap / 2.0, cy - 10.0, gap, 20.0, Act::Dot(i));
        }
        // the indicator stretches a little while it travels
        let pos = self.dot.v.clamp(-0.3, PAGES.len() as f32 - 0.7);
        let travel = (pos - pos.round()).abs();
        let iw = 12.0 + travel * 10.0;
        let ix = first + gap * pos;
        g.glow(ix, cy, 13.0, 6.0, WHITE, 0.12);
        g.fill_rrect(ix - iw / 2.0, cy - 2.5, iw, 5.0, 2.5, WHITE, 1.0);
    }

    pub(super) fn paint_expanded_v(&self, x0: f32) {
        let g = self.g();
        let page = PAGES[self.page];
        let slide = self.slide.v;
        let base_a = g.alpha.get();
        let top = self.top.get();
        self.origin(x0, top);
        self.grab_strip(PW);
        self.header_v();
        g.alpha.set(base_a * (1.0 - (slide.abs() / 36.0) * 0.7));
        self.origin(x0 + slide * 0.6, top);
        match page {
            Page::Media => self.page_media_v(),
            Page::Pomodoro => self.page_pomodoro_v(),
            Page::Water if self.water_settings => self.page_water_settings_v(),
            Page::Water => self.page_water_v(),
            Page::Shelf => self.page_shelf_v(),
            Page::Todo => self.page_todo_v(),
        }
        g.alpha.set(base_a);
        self.origin(x0, top);
        let ny = 418.0;
        if !(page == Page::Water && self.water_settings) {
            self.round_button(26.0, ny, 10.0, 0x3C3C3C, glyph::CHEV_L, 10.0, WHITE, Act::PagePrev);
            self.round_button(PW - 26.0, ny, 10.0, 0x3C3C3C, glyph::CHEV_R, 10.0, WHITE, Act::PageNext);
        }
        self.page_dots(PW / 2.0, ny);
    }

    fn header_v(&self) {
        let g = self.g();
        let (date, time) = date_strings();
        let dw = g.measure(&date, 12.0, 400);
        let tw = g.measure(&time, 12.5, 700);
        let left = PW / 2.0 - (dw + 7.0 + tw) / 2.0;
        g.text(&date, left, 20.0, 12.0, 400, Align::Left, 0xD0D0D0, 1.0, dw + 4.0);
        g.text(&time, left + dw + 7.0, 20.0, 12.5, 700, Align::Left, WHITE, 1.0, tw + 4.0);
        let page = PAGES[self.page];
        let title: &[(&str, u32)] = match page {
            Page::Media => &[],
            Page::Pomodoro => &[("Pomodoro", WHITE)],
            Page::Water if self.water_settings => &[],
            Page::Water => &[("Water ", WHITE), ("Reminder", BLUE)],
            Page::Shelf => &[("Shelf", WHITE)],
            Page::Todo => &[("Todo", WHITE), ("List", GREEN)],
        };
        let mut x = PAD;
        for (s, c) in title {
            g.text(s, x, 46.0, 14.5, 600, Align::Left, *c, 1.0, 140.0);
            x += g.measure(s, 14.5, 600);
        }
        if self.pomo.running && page != Page::Pomodoro {
            let frac = (self.pomo.remaining / self.pomo.mode.secs()) as f32;
            g.fill_circle(PW - 26.0, 46.0, 8.0, 0x3A1414, 1.0);
            g.pie(PW - 26.0, 46.0, 8.0, -90.0, 360.0 * frac, RED, 0.9);
        }
    }

    /// (main line, secondary line) for the current lyric: romaji + original script when
    /// romaji is on and available, otherwise the line and the next one.
    fn lyric_pair(&self) -> Option<(String, String)> {
        let i = self.lyric_idx?;
        let l = self.lyrics.lock().unwrap();
        let line = l.lines.get(i)?;
        let text = line.text.clone();
        Some(match line.roman.clone().filter(|_| self.settings.romaji) {
            Some(r) => (r, text),
            None => {
                let next = l.lines.get(i + 1).map(|x| if self.settings.romaji { x.roman.clone().unwrap_or_else(|| x.text.clone()) } else { x.text.clone() });
                (text, next.unwrap_or_default())
            }
        })
    }

    fn page_media_v(&self) {
        let g = self.g();
        let m = &self.media;
        self.art(64.0, 56.0, 112.0, 14.0);
        let has = m.has_session && !m.title.is_empty();
        let lyric_on = self.settings.lyrics && has;
        let status = self.lyrics.lock().unwrap().status.clone();
        // title and artist always stay visible under the art; the lyric gets its own block
        if has {
            g.text(&m.title, PAD, 186.0, 13.5, 700, Align::Left, WHITE, 1.0, CW);
            g.text(&m.artist, PAD, 204.0, 11.5, 400, Align::Left, GREY, 1.0, CW);
        }
        match self.lyric_pair().filter(|_| lyric_on) {
            Some((cur, sub)) => {
                let t = ease(self.lyric_change.elapsed().as_secs_f32() / 0.3);
                let a = g.alpha.get();
                g.alpha.set(a * t);
                let cur = if cur.is_empty() { "♪".to_string() } else { cur };
                g.text_box(&cur, PAD, 222.0 + (1.0 - t) * 5.0, CW, 48.0, 14.5, 700, WHITE, 1.0);
                g.text_box(&sub, PAD, 272.0, CW, 28.0, 11.5, 400, DIM, 1.0);
                g.alpha.set(a);
            }
            None if has => {
                let note = if lyric_on && status == LyricsStatus::Loading {
                    "Loading lyrics"
                } else if lyric_on && status == LyricsStatus::NotFound {
                    "Lyrics not available"
                } else {
                    ""
                };
                g.text(note, PAD, 240.0, 11.5, 400, Align::Left, DIM, 1.0, CW);
            }
            None => g.text("Tidak ada media", PW / 2.0, 214.0, 15.0, 700, Align::Center, WHITE, 1.0, CW),
        }

        // timeline
        let pos = m.pos_now();
        g.fill_rrect(PAD, 314.0, CW, 3.0, 1.5, 0x1F1F1F, 1.0);
        if m.duration > 0.0 {
            let f = (pos / m.duration).clamp(0.0, 1.0) as f32;
            g.fill_rrect(PAD, 314.0, (CW * f).max(3.0), 3.0, 1.5, 0xD8D8D8, 1.0);
            if self.hot(Act::MediaSeek) {
                g.fill_circle(PAD + CW * f, 315.5, 4.5, WHITE, 1.0);
            }
        }
        self.hit(PAD, 307.0, CW, 16.0, Act::MediaSeek);
        g.text(&mmss(pos), PAD, 331.0, 11.5, 400, Align::Left, 0x7A7A7A, 1.0, 60.0);
        g.text(&format!("-{}", mmss(m.duration - pos)), PAD + CW, 331.0, 11.5, 400, Align::Right, 0x7A7A7A, 1.0, 70.0);

        // controls: lyrics, romaji, previous, play/pause, next
        let cy = 372.0;
        let fg = if has { WHITE } else { 0x7A7A7A };
        let lyr = self.settings.lyrics && has;
        g.icon(glyph::MESSAGE, 30.0, cy, 15.0, if lyr { WHITE } else { 0x7A7A7A }, 1.0);
        if lyr {
            g.fill_circle(40.0, cy - 7.5, 2.4, GREEN, 1.0);
        }
        self.hit(17.0, cy - 12.0, 26.0, 24.0, Act::Lyrics);
        let ron = self.settings.romaji;
        if ron || self.hot(Act::Romaji) {
            g.fill_rrect(51.0, cy - 9.0, 26.0, 18.0, 9.0, 0x262626, 1.0);
        }
        g.text("Aa", 64.0, cy, 10.5, 700, Align::Center, if ron { WHITE } else { 0x7A7A7A }, 1.0, 26.0);
        if ron {
            g.fill_circle(75.0, cy - 7.5, 2.4, GREEN, 1.0);
        }
        self.hit(49.0, cy - 12.0, 30.0, 24.0, Act::Romaji);
        let tri = |cx: f32, dir: f32, s: f32| vec![(cx - dir * s * 0.55, cy - s * 0.6), (cx - dir * s * 0.55, cy + s * 0.6), (cx + dir * s * 0.65, cy)];
        for (cx, dir, act) in [(118.0, -1.0, Act::MediaPrev), (202.0, 1.0, Act::MediaNext)] {
            if self.hot(act) {
                g.fill_circle(cx, cy, 14.0, WHITE, 0.08);
            }
            g.polygon(&tri(cx - dir * 5.0, dir, 9.0), fg, 0.8);
            g.polygon(&tri(cx + dir * 4.0, dir, 9.0), fg, 0.8);
            self.hit(cx - 14.0, cy - 13.0, 28.0, 26.0, act);
        }
        let pc = 160.0;
        if self.hot(Act::MediaPlay) {
            g.fill_circle(pc, cy, 15.0, WHITE, 0.08);
        }
        if m.playing {
            g.fill_rrect(pc - 7.0, cy - 8.5, 4.6, 17.0, 1.5, fg, 1.0);
            g.fill_rrect(pc + 2.4, cy - 8.5, 4.6, 17.0, 1.5, fg, 1.0);
        } else {
            g.polygon(&[(pc - 6.0, cy - 9.5), (pc - 6.0, cy + 9.5), (pc + 9.5, cy)], fg, 1.0);
        }
        self.hit(pc - 15.0, cy - 14.0, 30.0, 28.0, Act::MediaPlay);
    }

    fn page_pomodoro_v(&self) {
        let g = self.g();
        let p = &self.pomo;
        let frac = (p.remaining / p.mode.secs()).clamp(0.0, 1.0) as f32;
        let focus = p.mode == PMode::Focus;
        let color = if focus { RED } else { GREEN };
        let (cx, cy) = (PW / 2.0, 140.0);
        g.glow(cx, cy, 78.0, 78.0, color, 0.26);
        g.stroke_circle(cx, cy, 54.0, color, 0.22, 8.0);
        g.arc(cx, cy, 54.0, -90.0, 360.0 * frac, color, 1.0, 8.0);
        g.icon(glyph::STOPWATCH, cx, cy, 44.0, WHITE, 1.0);
        g.text(&mmss(p.remaining.ceil()), cx, 238.0, 34.0, 700, Align::Center, WHITE, 1.0, 170.0);
        self.round_button(PW - 30.0, 238.0, 12.5, 0x262626, glyph::REFRESH, 12.0, WHITE, Act::PomoReset);
        g.text(p.mode.label(), cx, 268.0, 12.0, 400, Align::Center, GREY, 1.0, 170.0);
        self.button(cx - 40.0, 288.0, 80.0, 30.0, 15.0, color, 1.0, Act::PomoToggle);
        if p.running {
            g.fill_rrect(cx - 6.0, 296.0, 3.4, 14.0, 1.2, WHITE, 1.0);
            g.fill_rrect(cx + 2.6, 296.0, 3.4, 14.0, 1.2, WHITE, 1.0);
        } else {
            g.polygon(&[(cx - 4.5, 296.0), (cx - 4.5, 310.0), (cx + 7.5, 303.0)], WHITE, 1.0);
        }
        let half = (CW - 8.0) / 2.0;
        self.button(PAD, 330.0, half, 28.0, 14.0, if focus { 0x171717 } else { 0x333333 }, 1.0, Act::PomoShort);
        g.icon(glyph::CAFE, PAD + 16.0, 344.0, 11.0, WHITE, 1.0);
        g.text("Short Break", PAD + 26.0, 344.0, 10.5, 400, Align::Left, WHITE, 1.0, half - 30.0);
        let fx = PAD + half + 8.0;
        self.button(fx, 330.0, half, 28.0, 14.0, if focus { 0x333333 } else { 0x171717 }, 1.0, Act::PomoFocus);
        g.icon(glyph::TARGET, fx + 16.0, 344.0, 11.0, WHITE, 1.0);
        g.text("Focus Time", fx + 26.0, 344.0, 10.5, 400, Align::Left, WHITE, 1.0, half - 30.0);
    }

    fn page_water_v(&self) {
        let g = self.g();
        let w = &self.water;
        let litres = format!("{:.1}", w.taken as f32 / 1000.0);
        g.text(&litres, PAD, 84.0, 30.0, 700, Align::Left, WHITE, 1.0, 120.0);
        let lw = g.measure(&litres, 30.0, 700);
        g.text(&format!("/ {:.1} L", w.goal as f32 / 1000.0), PAD + lw + 6.0, 88.0, 13.5, 400, Align::Left, GREY, 1.0, 80.0);
        let on = w.on;
        self.button(PW - PAD - 34.0, 75.0, 34.0, 18.0, 9.0, if on { 0x2699FF } else { 0x3A3A3A }, 1.0, Act::WaterOn);
        let kx = if on { PW - PAD - 9.0 } else { PW - PAD - 25.0 };
        g.fill_circle(kx, 84.0, 7.2, WHITE, 1.0);
        g.icon(glyph::BELL, kx, 84.0, 8.5, if on { 0x2699FF } else { GREY }, 1.0);

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
        let mut x = PAD;
        for (s, c, wt) in [("Today · ", GREY, 400), (pct.as_str(), BLUE, 700), (tail.as_str(), GREY, 400)] {
            g.text(s, x, 112.0, 12.0, wt, Align::Left, c, 1.0, CW);
            x += g.measure(s, 12.0, wt);
        }
        g.fill_rrect(PAD, 126.0, CW, 4.0, 2.0, 0x212121, 1.0);
        let f = (w.taken as f32 / w.goal.max(1) as f32).min(1.0);
        if f > 0.0 {
            g.fill_rrect(PAD, 126.0, (CW * f).max(4.0), 4.0, 2.0, BLUE, 1.0);
        }

        for (i, x) in [(0u8, PW / 2.0 - 5.0), (1u8, PW / 2.0 + 5.0)] {
            g.fill_circle(x, 150.0, 2.0, if self.water_sub == i { WHITE } else { DIM }, 1.0);
            self.hit(x - 4.0, 144.0, 8.0, 12.0, Act::WaterSub(i));
        }
        if self.water_sub == 0 {
            let (cx, cy, r) = (PW / 2.0, 230.0, 56.0);
            g.arc(cx, cy, r, 135.0, 270.0, 0x0F2640, 1.0, 8.0);
            if f > 0.0 {
                g.arc(cx, cy, r, 135.0, 270.0 * f, BLUE, 1.0, 8.0);
            }
            let a = (135.0 + 270.0 * f).to_radians();
            g.fill_circle(cx + r * a.cos(), cy + r * a.sin(), 5.0, BLUE, 1.0);
            g.text(&w.percent().to_string(), cx, cy - 8.0, 30.0, 700, Align::Center, WHITE, 1.0, 90.0);
            g.icon(glyph::DROP, cx - 7.0, cy + 18.0, 11.0, BLUE, 1.0);
            g.text("%", cx + 2.0, cy + 18.0, 12.5, 600, Align::Left, GREY, 1.0, 20.0);
            self.hit(cx - r, cy - r, r * 2.0, r * 2.0, Act::WaterSub(1));
        } else {
            self.water_chart_v();
        }
        let half = (CW - 8.0) / 2.0;
        self.button(PAD, 330.0, half, 28.0, 14.0, 0x0D4580, 1.0, Act::WaterDrink);
        g.icon(glyph::DROP, PAD + 22.0, 344.0, 12.0, CYAN, 1.0);
        g.text("Drink", PAD + 34.0, 344.0, 12.0, 400, Align::Left, WHITE, 1.0, 60.0);
        let sx = PAD + half + 8.0;
        self.button(sx, 330.0, half, 28.0, 14.0, 0x1F1F1F, 1.0, Act::WaterSettings);
        g.icon(glyph::SLIDERS, sx + 18.0, 344.0, 11.0, WHITE, 1.0);
        g.text("Settings", sx + 30.0, 344.0, 11.0, 400, Align::Left, WHITE, 1.0, 60.0);
    }

    fn water_chart_v(&self) {
        let g = self.g();
        let w = &self.water;
        let first = (w.start / 60) as usize;
        let last = ((w.end + 59) / 60).min(24) as usize;
        let n = last.saturating_sub(first).max(1);
        let (left, right, top, bottom) = (PAD, PW - 46.0, 170.0, 290.0);
        let maxv = w.hours[first..last.max(first)].iter().copied().max().unwrap_or(0).max(w.bottle).max(1) as f32;
        for (frac, label) in [(1.0, format!("{}", maxv as u32)), (0.5, format!("{}", (maxv / 2.0) as u32)), (0.0, "0 ml".to_string())] {
            let y = bottom - (bottom - top) * frac;
            g.dashed_line(left, y, right, y, 0x2A2A2A, 1.0, 1.0);
            g.text(&label, right + 5.0, y, 9.5, 400, Align::Left, GREY, 1.0, 40.0);
        }
        let slot = (right - left) / n as f32;
        let bw = (slot * 0.6).min(9.0);
        for i in 0..n {
            let x = left + slot * i as f32 + (slot - bw) / 2.0;
            g.fill_rrect(x, top, bw, bottom - top, bw / 2.0, 0x161616, 1.0);
            let v = w.hours[(first + i).min(23)] as f32 / maxv;
            if v > 0.0 {
                let hh = ((bottom - top) * v).max(bw);
                g.fill_rrect(x, bottom - hh, bw, hh, bw / 2.0, BLUE, 1.0);
            }
            if (first + i) % 3 == 0 {
                g.text(&format!("{:02}", first + i), x + bw / 2.0, 304.0, 10.0, 400, Align::Center, GREY, 1.0, 30.0);
            }
        }
        self.hit(left, top, right - left, bottom - top, Act::WaterSub(0));
    }

    fn page_water_settings_v(&self) {
        let g = self.g();
        let w = &self.water;
        self.round_button(28.0, 46.0, 10.0, 0x3C3C3C, glyph::CHEV_L, 9.5, WHITE, Act::WaterBack);
        g.text("Settings", 44.0, 46.0, 14.5, 600, Align::Left, WHITE, 1.0, 120.0);
        let label = |s: &str, y: f32| g.text(s, PAD, y, 13.0, 400, Align::Left, 0xD8D8D8, 1.0, 70.0);
        let stepper = |y: f32, value: String, minus: Act, plus: Act| {
            self.round_button(106.0, y, 10.5, 0x262626, glyph::REMOVE, 9.5, WHITE, minus);
            g.text(&value, 154.0, y, 13.5, 700, Align::Center, WHITE, 1.0, 70.0);
            self.round_button(PW - PAD - 10.5, y, 10.5, 0x262626, glyph::ADD, 9.5, WHITE, plus);
        };
        label("Bottle", 82.0);
        stepper(82.0, format!("{} ml", w.bottle), Act::Bottle(-1), Act::Bottle(1));
        g.text(&format!("{} ml each · {}/bottle", w.each(), w.per_bottle()), PAD, 104.0, 10.5, 400, Align::Left, GREY, 1.0, CW);

        label("Goal", 132.0);
        let half = (CW - 8.0) / 2.0;
        for (i, goal) in GOALS.iter().enumerate() {
            let x = PAD + (i % 2) as f32 * (half + 8.0);
            let y = 146.0 + (i / 2) as f32 * 30.0;
            if w.goal == *goal {
                if let Some(b) = g.linear(x, 0.0, x + half, 0.0, &[(0.0, 0x56CCFF, 1.0), (1.0, 0x39A9FF, 1.0)]) {
                    g.fill_rrect_brush(x, y, half, 24.0, 12.0, &b);
                }
                self.hit(x, y, half, 24.0, Act::Goal(i));
            } else {
                self.button(x, y, half, 24.0, 12.0, 0x191919, 1.0, Act::Goal(i));
            }
            g.text(&format!("{:.1} L", *goal as f32 / 1000.0), x + half / 2.0, y + 12.0, 12.0, 400, Align::Center, WHITE, 1.0, half);
        }

        label("Start", 228.0);
        stepper(228.0, model::hhmm(w.start), Act::Start(-1), Act::Start(1));
        label("End", 260.0);
        stepper(260.0, model::hhmm(w.end), Act::End(-1), Act::End(1));

        g.fill_rrect(PAD, 284.0, CW, 52.0, 12.0, 0x0F1926, 1.0);
        g.text(&w.reminders().to_string(), PAD + CW * 0.27, 302.0, 20.0, 700, Align::Center, CYAN, 1.0, 80.0);
        g.text("reminders", PAD + CW * 0.27, 320.0, 10.0, 400, Align::Center, GREY, 1.0, 90.0);
        g.line(PAD + CW / 2.0, 294.0, PAD + CW / 2.0, 326.0, 0x23344A, 1.0, 1.0);
        g.text(&w.refills().to_string(), PAD + CW * 0.73, 302.0, 20.0, 700, Align::Center, CYAN, 1.0, 80.0);
        g.text("refills", PAD + CW * 0.73, 320.0, 10.0, 400, Align::Center, GREY, 1.0, 90.0);

        let confirm = self.reset_confirm.is_some();
        self.button(PAD, 346.0, CW, 24.0, 12.0, if confirm { 0x3A1212 } else { 0x171717 }, 1.0, Act::WaterReset);
        g.icon(glyph::DELETE, PW / 2.0 - 22.0, 358.0, 10.0, if confirm { RED } else { WHITE }, 1.0);
        g.text(if confirm { "Sure?" } else { "Reset" }, PW / 2.0 - 12.0, 358.0, 11.5, 400, Align::Left, if confirm { RED } else { WHITE }, 1.0, 60.0);
    }

    fn page_shelf_v(&self) {
        let g = self.g();
        let items = &self.shelf.items;
        if items.is_empty() || self.shelf_drag {
            let accent = if self.shelf_drag { BLUE } else { 0x5A5A5A };
            if self.shelf_drag {
                g.fill_rrect(PAD, 66.0, CW, 324.0, 14.0, BLUE, 0.08);
            }
            g.stroke_rrect(PAD, 66.0, CW, 324.0, 14.0, accent, 1.0, 1.3, true);
            let bob = if self.shelf_drag { (self.started.elapsed().as_secs_f32() * 6.0).sin() * 2.0 } else { 0.0 };
            let ic = if self.shelf_drag { BLUE } else { WHITE };
            g.icon(glyph::DOWNLOAD, PW / 2.0, 200.0 + bob, 26.0, ic, 1.0);
            g.fill_rrect(PW / 2.0 - 15.0, 215.0, 30.0, 3.0, 1.5, ic, 0.9);
            g.text(if self.shelf_drag { "Release to add" } else { "Drop files here" }, PW / 2.0, 246.0, 13.5, 400, Align::Center, WHITE, 1.0, CW);
            g.text_box("Drag out anytime · click to open", PAD + 20.0, 262.0, CW - 40.0, 30.0, 10.0, 400, GREY, 1.0);
            return;
        }
        self.button(PW - PAD - 44.0, 37.0, 44.0, 18.0, 9.0, 0x1F1F1F, 1.0, Act::ShelfClear);
        g.text("Clear", PW - PAD - 22.0, 46.0, 11.0, 400, Align::Center, WHITE, 1.0, 44.0);
        let (tw, th, gap) = (64.0, 86.0, 6.0);
        for (i, it) in items.iter().enumerate().take(9) {
            let x = PAD + (i % 3) as f32 * (tw + gap);
            let y = 68.0 + (i / 3) as f32 * (th + 8.0);
            let act = Act::ShelfItem(i);
            g.fill_rrect(x, y, tw, th, 10.0, 0x161616, 1.0);
            if self.hot(act) {
                g.fill_rrect(x, y, tw, th, 10.0, WHITE, 0.07);
            }
            let (gl, c) = match it.kind {
                Kind::Folder => (glyph::FOLDER, 0xFFC94A),
                Kind::Link => (glyph::LINK, BLUE),
                Kind::Image => (glyph::PICTURE, 0xFF7AB6),
                Kind::File => (glyph::DOC, 0xCFCFCF),
            };
            g.icon(gl, x + tw / 2.0, y + 28.0, 22.0, c, 1.0);
            g.text(&it.name, x + tw / 2.0, y + 58.0, 10.0, 400, Align::Center, 0xD8D8D8, 1.0, tw - 6.0);
            let sub = if it.kind == Kind::Folder || it.kind == Kind::Link { String::new() } else { shelf::size_label(it.size) };
            g.text(&sub, x + tw / 2.0, y + 72.0, 9.0, 400, Align::Center, DIM, 1.0, tw - 6.0);
            self.hit(x, y, tw, th, act);
        }
        if items.len() > 9 {
            g.text(&format!("+{}", items.len() - 9), PW - PAD, 392.0, 10.0, 400, Align::Right, GREY, 1.0, 40.0);
        }
    }

    fn page_todo_v(&self) {
        let g = self.g();
        let done = self.tasks.iter().filter(|t| t.done).count();
        g.text(&format!("{}/{}", done, self.tasks.len()), PAD, 84.0, 32.0, 700, Align::Left, WHITE, 1.0, 150.0);
        g.text("Task Done Today", PAD, 112.0, 12.0, 400, Align::Left, GREY, 1.0, 150.0);

        let half = (CW - 8.0) / 2.0;
        let adding = self.form.as_ref().map_or(false, |f| f.edit.is_none());
        self.button(PAD, 128.0, half, 26.0, 13.0, if adding { 0x0F2B17 } else { 0x262626 }, 1.0, Act::TodoAdd);
        g.icon(glyph::ADD_CIRCLE, PAD + 15.0, 141.0, 12.0, if adding { GREEN } else { WHITE }, 1.0);
        g.text("Add Task", PAD + 26.0, 141.0, 10.5, 400, Align::Left, if adding { GREEN } else { WHITE }, 1.0, half - 28.0);
        let ex = PAD + half + 8.0;
        let can_edit = self.task_sel.is_some() && self.form.is_none();
        if can_edit {
            self.button(ex, 128.0, half, 26.0, 13.0, 0x262626, 1.0, Act::TodoEdit);
        } else {
            g.fill_rrect(ex, 128.0, half, 26.0, 13.0, 0x121212, 1.0);
        }
        let ec = if can_edit { WHITE } else { 0x4A4A4A };
        g.icon(glyph::EDIT, ex + 15.0, 141.0, 11.0, ec, 1.0);
        g.text("Edit Task", ex + 26.0, 141.0, 10.5, 400, Align::Left, ec, 1.0, half - 28.0);

        let (cx0, cy0, cw, ch) = (PAD, 166.0, CW, 232.0);
        g.fill_rrect(cx0, cy0, cw, ch, 14.0, 0x131313, 1.0);
        if let Some(f) = &self.form {
            self.todo_form_v(f, cx0, cy0, cw);
            return;
        }
        if self.tasks.is_empty() {
            g.fill_rrect(PW / 2.0 - 18.0, 230.0, 36.0, 36.0, 9.0, 0x414141, 1.0);
            g.icon(glyph::CHECKLIST, PW / 2.0, 248.0, 18.0, 0x1A1A1A, 1.0);
            g.text("No tasks.", PW / 2.0, 290.0, 13.0, 400, Align::Center, 0x7A7A7A, 1.0, CW);
            return;
        }
        let rows = self.todo_rows();
        for (row, i) in (self.task_scroll..self.tasks.len()).take(rows).enumerate() {
            let t = &self.tasks[i];
            let y = cy0 + 8.0 + row as f32 * 31.0;
            let (rx, rw) = (cx0 + 6.0, cw - 12.0);
            if self.task_sel == Some(i) || self.hot(Act::TodoRow(i)) {
                g.fill_rrect(rx, y, rw, 28.0, 8.0, 0x1F1F1F, 1.0);
            }
            self.hit(rx + 22.0, y, rw - 22.0, 28.0, Act::TodoRow(i));
            if t.done {
                g.fill_circle(rx + 12.0, y + 14.0, 6.5, GREEN, 1.0);
                g.icon(glyph::CHECK, rx + 12.0, y + 14.0, 7.5, 0x0B2A12, 1.0);
            } else {
                g.stroke_circle(rx + 12.0, y + 14.0, 6.0, 0x6A6A6A, 1.0, 1.4);
            }
            self.hit(rx, y, 22.0, 28.0, Act::TodoCheck(i));
            let title_w = rw - 30.0;
            g.text(&t.title, rx + 25.0, y + 10.0, 12.0, 400, Align::Left, if t.done { DIM } else { WHITE }, 1.0, title_w);
            let time = match (t.start, t.end) {
                (Some(s), Some(e)) => format!("{}–{}", model::hhmm(s), model::hhmm(e)),
                (Some(s), None) => model::hhmm(s),
                _ => String::new(),
            };
            if !time.is_empty() {
                g.text(&time, rx + 25.0, y + 22.0, 9.5, 400, Align::Left, GREY, 1.0, title_w);
            }
        }
        if self.tasks.len() > rows {
            let n = self.tasks.len() as f32;
            let h = (ch - 16.0) * rows as f32 / n;
            let y = cy0 + 8.0 + (ch - 16.0 - h) * self.task_scroll as f32 / (n - rows as f32);
            g.fill_rrect(cx0 + cw - 4.0, y, 2.0, h, 1.0, 0x4A4A4A, 1.0);
        }
    }

    fn todo_form_v(&self, f: &Form, x0: f32, y0: f32, w: f32) {
        let g = self.g();
        let (x, iw) = (x0 + 8.0, w - 16.0);
        let blink = (self.started.elapsed().as_millis() / 530) % 2 == 0;
        let focus_title = f.field == 0;
        let ty = y0 + 10.0;
        g.fill_rrect(x, ty, iw, 28.0, 8.0, 0x080808, 1.0);
        g.stroke_rrect(x, ty, iw, 28.0, 8.0, if focus_title { 0xD9D9D9 } else { 0x333333 }, 1.0, 1.4, false);
        self.hit(x, ty, iw, 28.0, Act::TodoTitle);
        if f.title.is_empty() && !focus_title {
            g.text("Task title", x + 9.0, ty + 14.0, 12.0, 400, Align::Left, 0x6A6A6A, 1.0, iw - 16.0);
        } else {
            let mut shown = f.title.clone();
            while g.measure(&shown, 12.0, 400) > iw - 22.0 && !shown.is_empty() {
                shown.remove(0);
            }
            let cursor = if focus_title && blink { "|" } else { "" };
            let text = if shown.is_empty() && !cursor.is_empty() { cursor.to_string() } else { format!("{}{}", shown, cursor) };
            g.text(&text, x + 9.0, ty + 14.0, 12.0, 400, Align::Left, WHITE, 1.0, iw - 14.0);
        }
        let ry = ty + 36.0;
        g.fill_rrect(x, ry, iw, 28.0, 8.0, 0x212124, 1.0);
        if !focus_title {
            g.stroke_rrect(x, ry, iw, 28.0, 8.0, 0xD9D9D9, 1.0, 1.2, false);
        }
        self.hit(x, ry, iw, 28.0, Act::TodoTime);
        g.icon(glyph::CLOCK, x + 14.0, ry + 14.0, 12.0, WHITE, 1.0);
        g.text(&model::time_mask(&f.digits), x + 27.0, ry + 14.0, 12.0, 400, Align::Left, if f.digits.is_empty() { 0x6A6A6A } else { WHITE }, 1.0, iw - 50.0);
        if f.edit.is_some() {
            self.round_button(x + iw - 13.0, ry + 14.0, 9.0, 0x2A1515, glyph::DELETE, 9.0, RED, Act::TodoDelete);
        }
        let by = ry + 38.0;
        let half = (iw - 8.0) / 2.0;
        self.button(x, by, half, 24.0, 12.0, 0x262626, 1.0, Act::TodoCancel);
        g.text("Cancel", x + half / 2.0, by + 12.0, 12.0, 400, Align::Center, WHITE, 1.0, half);
        self.button(x + half + 8.0, by, half, 24.0, 12.0, GREEN, 1.0, Act::TodoSave);
        g.text("Save", x + half + 8.0 + half / 2.0, by + 12.0, 12.0, 700, Align::Center, 0x0B2A12, 1.0, half);
        g.text_box("Enter saves · Esc cancels · Tab switches to the time (HHMM HHMM)", x, by + 34.0, iw, 40.0, 10.0, 400, DIM, 1.0);
    }
}
