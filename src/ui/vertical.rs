//! Vertical layouts used when the notch is docked to the left or right screen edge:
//! a narrow tab at rest, tall cards for live content and a 240 x 440 panel on hover.

use super::*;
use super::paint::EQ_W;

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
            9
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
            Mode::Mascot => (40.0, 100.0, 18.0),
            Mode::Collapsed => (40.0, 110.0, 14.0),
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
        self.eq_bars((w - EQ_W) / 2.0, h - 26.0, AMBER);
    }

    pub(super) fn paint_timer_v(&self, x0: f32, w: f32) {
        self.origin(x0, self.top.get());
        let g = self.g();
        let frac = (self.pomo.remaining / self.pomo.mode.secs()) as f32;
        let focus = self.pomo.mode == PMode::Focus;
        let color = self.pomo_color();
        let c = w / 2.0;
        g.stroke_circle(c, 32.0, 14.0, 0x2A2416, 1.0, 3.0);
        g.arc(c, 32.0, 14.0, -90.0, 360.0 * frac, color, 1.0, 3.0);
        g.text(if focus { "Focus" } else { "Break" }, c, 62.0, 10.0, 600, Align::Center, GREY, 1.0, w);
        let s = self.pomo.remaining.max(0.0) as u64;
        g.text(&format!("{:02}", s / 60), c, 84.0, 15.0, 700, Align::Center, color, 1.0, w);
        g.text(&format!("{:02}", s % 60), c, 104.0, 15.0, 700, Align::Center, color, 0.75, w);
    }

    pub(super) fn paint_lyrics_v(&self, x0: f32, w: f32, h: f32) {
        self.origin(x0, self.top.get());
        let g = self.g();
        self.art(10.0, 10.0, 22.0, 6.0);
        self.eq_bars(w - 12.0 - EQ_W, 21.0, AMBER);
        let text = self.lyric_text();
        let t = ease(self.lyric_change.elapsed().as_secs_f32() / 0.25);
        let a = g.alpha.get();
        g.alpha.set(a * t);
        let y = 42.0 + (1.0 - t) * 5.0;
        if text.is_empty() {
            g.icon(glyph::MUSIC, 18.0, y + 9.0, 15.0, WHITE, 1.0);
        } else {
            g.text_box(&text, 10.0, y, w - 20.0, h - 50.0, 12.5, 600, WHITE, 1.0);
        }
        g.alpha.set(a);
    }

    pub(super) fn paint_alert_v(&self, x0: f32, w: f32, h: f32) {
        let Some(a) = &self.alert else { return };
        self.origin(x0, self.top.get());
        let g = self.g();
        if !a.big() {
            self.alert_badge(a, w / 2.0, 24.0, 13.0);
            g.text(&a.title, w / 2.0, 48.0, 12.5, 600, Align::Center, WHITE, 1.0, w - 16.0);
            g.text(&a.value, w / 2.0, 64.0, 11.5, 400, Align::Center, a.color, 1.0, w - 16.0);
            return;
        }
        self.alert_badge(a, 28.0, 28.0, 15.0);
        g.text(&a.title, 50.0, 24.0, 13.0, 700, Align::Left, WHITE, 1.0, w - 60.0);
        g.text("now", 50.0, 38.0, 10.0, 400, Align::Left, DIM, 1.0, 40.0);
        let body_h = if a.actions.is_empty() { h - 66.0 } else { h - 96.0 };
        g.text_box(&a.value, 12.0, 50.0, w - 24.0, body_h.max(14.0), 11.5, 400, 0xB8B8B8, 1.0);
        if !a.actions.is_empty() {
            let n = a.actions.len() as f32;
            let bw = (w - 24.0 - (n - 1.0) * 6.0) / n;
            for (i, (label, _)) in a.actions.iter().enumerate() {
                self.alert_button(a, i, label, 12.0 + i as f32 * (bw + 6.0), h - 40.0, bw);
            }
        }
        self.alert_timer(a, 14.0, h - 9.0, w - 28.0);
    }

    // ------------------------------------------------------------ expanded panel

    /// Page dots with the gliding indicator, centred on (cx, cy).
    pub(super) fn page_dots(&self, cx: f32, cy: f32) {
        let g = self.g();
        let gap = 13.0;
        let first = cx - gap * (PAGES.len() as f32 - 1.0) / 2.0;
        for i in 0..PAGES.len() {
            let x = first + gap * i as f32;
            let k = self.hov(Act::Dot(i));
            g.fill_circle(x, cy, 2.0, 0x5B6270, 1.0);
            if k > 0.0 {
                g.fill_circle(x, cy, 2.0 + 0.8 * k, 0xC8CDD5, k);
            }
            self.hit(x - gap / 2.0, cy - 10.0, gap, 20.0, Act::Dot(i));
        }
        // the indicator stretches a little while it travels
        let pos = self.dot.v.clamp(-0.3, PAGES.len() as f32 - 0.7);
        let travel = (pos - pos.round()).abs();
        let iw = 12.0 + travel * 10.0;
        let ix = first + gap * pos;
        g.glow(ix, cy, 13.0, 6.0, AMBER, 0.14);
        g.fill_rrect(ix - iw / 2.0, cy - 2.5, iw, 5.0, 2.5, AMBER, 1.0);
    }

    pub(super) fn paint_expanded_v(&self, x0: f32) {
        let g = self.g();
        let page = PAGES[self.page];
        let slide = self.slide.v;
        let base_a = g.alpha.get();
        let top = self.top.get();
        self.origin(x0, top);
        self.grab_strip(PW);
        self.header(PW, 22.0, 12.0);
        g.alpha.set(base_a * (1.0 - (slide.abs() / 36.0) * 0.7));
        self.origin(x0 + slide * 0.6, top);
        match page {
            Page::Home => self.page_home_v(),
            Page::Settings => self.page_settings_v(),
            Page::Media => self.page_media_v(),
            Page::Pomodoro => self.page_pomodoro_v(),
            Page::Water if self.water_settings => self.page_water_settings_v(),
            Page::Water => self.page_water_v(),
            Page::Shelf => self.page_shelf_v(),
            Page::Todo => self.page_todo_v(),
        }
        g.alpha.set(base_a);
        self.origin(x0, top);
        self.page_nav(PW / 2.0, 418.0, !(page == Page::Water && self.water_settings));
    }

    /// (main line, secondary line) for the current lyric: romaji + original script when
    /// romaji is on and available, otherwise the line and the next one.
    pub(super) fn lyric_pair(&self) -> Option<(String, String)> {
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
        let s = 112.0 * (1.0 + self.cover_bump());
        self.art(120.0 - s / 2.0, 112.0 - s / 2.0, s, 14.0);
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
                let y = 222.0 + (1.0 - t) * 5.0;
                if cur.is_empty() {
                    g.icon(glyph::MUSIC, PAD + 9.0, y + 11.0, 18.0, WHITE, 1.0);
                } else {
                    g.text_box(&cur, PAD, y, CW, 48.0, 14.5, 700, WHITE, 1.0);
                }
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
            g.fill_rrect(PAD, 314.0, (CW * f).max(3.0), 3.0, 1.5, AMBER, 1.0);
            let k = self.hov(Act::MediaSeek);
            if k > 0.0 {
                g.fill_circle(PAD + CW * f, 315.5, 4.5 * k, WHITE, k);
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
            g.fill_circle(40.0, cy - 7.5, 2.4, AMBER, 1.0);
        }
        self.hit(17.0, cy - 12.0, 26.0, 24.0, Act::Lyrics);
        let ron = self.settings.romaji;
        let rk = self.fx.level(Act::Romaji, ron as u8 as f32).max(self.hov(Act::Romaji));
        if rk > 0.0 {
            g.fill_rrect(51.0, cy - 9.0, 26.0, 18.0, 9.0, 0x262626, rk);
        }
        g.text("Aa", 64.0, cy, 10.5, 700, Align::Center, if ron { AMBER } else { 0x7A7A7A }, 1.0, 26.0);
        if ron {
            g.fill_circle(75.0, cy - 7.5, 2.4, AMBER, 1.0);
        }
        self.hit(49.0, cy - 12.0, 30.0, 24.0, Act::Romaji);
        let tri = |cx: f32, dir: f32, s: f32| vec![(cx - dir * s * 0.55, cy - s * 0.6), (cx - dir * s * 0.55, cy + s * 0.6), (cx + dir * s * 0.65, cy)];
        for (cx, dir, act) in [(118.0, -1.0, Act::MediaPrev), (202.0, 1.0, Act::MediaNext)] {
            self.hover_ring(act, cx, cy, 14.0, 0.08);
            g.polygon(&tri(cx - dir * 5.0, dir, 9.0), fg, 0.8);
            g.polygon(&tri(cx + dir * 4.0, dir, 9.0), fg, 0.8);
            self.hit(cx - 14.0, cy - 13.0, 28.0, 26.0, act);
        }
        let pc = 160.0;
        self.play_button(pc, cy, 15.0, has);
    }

    fn page_pomodoro_v(&self) {
        self.mode_chips(PW / 2.0, 66.0, true);
        self.big_timer(PW / 2.0, 142.0, 54.0, true);
        self.pomo_bar(PAD, 190.0, CW);
        self.g().text(&self.pomo_label(), PW / 2.0, 206.0, 11.0, 400, Align::Center, GREY, 1.0, CW);
        self.round_button(PW / 2.0 - 72.0, 272.0, 17.0, 0x222222, glyph::REFRESH, 14.0, WHITE, Act::PomoReset);
        self.pomo_play(PW / 2.0, 272.0, 30.0);
    }

    fn page_water_v(&self) {
        let g = self.g();
        let w = &self.water;
        self.water_bottle(PAD + 6.0, 52.0, 66.0, 150.0);
        let x = PAD + 92.0;
        let litres = format!("{:.1}", w.taken as f32 / 1000.0);
        g.text(&litres, x, 78.0, 28.0, 700, Align::Left, WHITE, 1.0, CW - 92.0);
        g.text(&format!("of {:.1} L", w.goal as f32 / 1000.0), x, 101.0, 12.0, 400, Align::Left, GREY, 1.0, CW - 92.0);
        g.text_box(&self.water_status(), x, 112.0, CW - 92.0, 30.0, 10.5, 400, GREY, 1.0);
        self.button(x, 146.0, CW - 92.0, 24.0, 12.0, 0x123A63, 1.0, Act::WaterDrink);
        g.icon(glyph::DROP, x + 16.0, 158.0, 11.0, CYAN, 1.0);
        g.text("Drink", x + 27.0, 158.0, 11.5, 400, Align::Left, WHITE, 1.0, 60.0);
        self.round_button(x + 12.0, 190.0, 12.0, 0x1F1F1F, glyph::SLIDERS, 10.0, WHITE, Act::WaterSettings);
        self.switch(x + 32.0, 190.0, w.on, Act::WaterOn);
        self.hit(x + 30.0, 180.0, 34.0, 20.0, Act::WaterOn);
        g.line(PAD, 220.0, PAD + CW, 220.0, HAIR, 1.0, 1.0);
        g.text("Today", PAD, 236.0, 11.0, 400, Align::Left, GREY, 1.0, 100.0);
        self.water_chart(PAD, PAD + CW, 254.0, 340.0);
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
        self.shelf_header(12.0, 22.0);
        let n = self.shelf.items.len();
        if n == 0 {
            self.drop_card(PAD, 52.0, CW, 150.0, 0);
            return;
        }
        let gap = 8.0;
        let (cw, ch) = ((CW - 2.0 * gap) / 3.0, 84.0);
        let at = |i: usize| (PAD + (i % 3) as f32 * (cw + gap), 50.0 + (i / 3) as f32 * (ch + gap));
        let order = self.shelf_order(8);
        let shown = order.len();
        for (k, &i) in order.iter().enumerate() {
            let (x, y) = at(k);
            self.shelf_card(i, x, y, cw, ch);
        }
        let (x, y) = at(shown);
        self.drop_card(x, y, cw, ch, n - shown);
    }

    fn page_todo_v(&self) {
        let done = self.tasks.iter().filter(|t| t.done).count();
        if !self.tasks.is_empty() {
            self.header_note("To-do", &format!("{} of {} done", done, self.tasks.len()), 12.0, 22.0);
        }
        self.plus_button(PW - 64.0, 22.0);
        if let Some(f) = &self.form {
            self.todo_form(f, PAD, 54.0, CW, true);
            return;
        }
        if self.tasks.is_empty() {
            self.todo_empty(PW / 2.0, 190.0);
            return;
        }
        let rows = self.todo_rows();
        for (row, i) in (self.task_scroll..self.tasks.len()).take(rows).enumerate() {
            self.todo_row(i, PAD - 4.0, 48.0 + row as f32 * 34.0, CW + 4.0, 30.0);
        }
        self.todo_scrollbar(PW - 10.0, 48.0, rows as f32 * 34.0 - 4.0, rows);
    }
}
