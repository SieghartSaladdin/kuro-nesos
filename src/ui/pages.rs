//! Expanded panel (top dock): paging and the Pomodoro, Water, Shelf and To-do pages, plus
//! their building blocks (timer, bottle, chart, shelf cards, task rows, task form) that the
//! side-dock layouts in vertical.rs reuse. Home, Settings and the header are in home.rs,
//! the music page in music.rs.

use super::*;

impl App {
    pub(super) fn paint_expanded(&self, x0: f32) {
        let g = self.g();
        let page = PAGES[self.page];
        let slide = self.slide.v;
        let base_a = g.alpha.get();
        // header (does not slide)
        self.origin(x0, self.top.get());
        self.grab_strip(420.0);
        self.header(420.0, 17.0, 16.0);
        g.alpha.set(base_a * (1.0 - (slide.abs() / 36.0) * 0.7));
        self.origin(x0 + slide, self.top.get());
        match page {
            Page::Home => self.page_home(),
            Page::Settings => self.page_settings(),
            Page::Media => self.page_media(),
            Page::Pomodoro => self.page_pomodoro(),
            Page::Water if self.water_settings => self.page_water_settings(),
            Page::Water => self.page_water(),
            Page::Shelf => self.page_shelf(),
            Page::Todo => self.page_todo(),
        }
        g.alpha.set(base_a);
        self.origin(x0, self.top.get());
        self.page_nav(210.0, 139.0, !(page == Page::Water && self.water_settings));
    }

    /// Page dots with the arrows right beside them (they nudge outwards on hover).
    pub(super) fn page_nav(&self, cx: f32, cy: f32, arrows: bool) {
        self.page_dots(cx, cy);
        if arrows {
            let half = 13.0 * (PAGES.len() as f32 - 1.0) / 2.0 + 20.0;
            for (dir, act, gl) in [(-1.0, Act::PagePrev, glyph::CHEV_L), (1.0, Act::PageNext, glyph::CHEV_R)] {
                self.round_button(cx + dir * (half + 2.0 * self.hov(act)), cy, 9.0, 0x262626, gl, 8.5, WHITE, act);
            }
        }
    }

    /// Drag handle along the top of the panel (a small grip shows while it is hovered).
    pub(super) fn grab_strip(&self, w: f32) {
        let k = self.hov(Act::Grab).max(if self.drag.is_some() { 1.0 } else { 0.0 });
        if k > 0.0 {
            let gw = 18.0 + 10.0 * k;
            self.g().fill_rrect(w / 2.0 - gw / 2.0, 4.0, gw, 3.5, 1.75, WHITE, 0.35 * k);
        }
        self.hit(0.0, 0.0, w, 30.0, Act::Grab);
    }

    /// Grey note next to the page title in the header ("2 of 4 done", "3 items").
    /// Returns where it ends.
    pub(super) fn header_note(&self, title: &str, note: &str, left: f32, cy: f32) -> f32 {
        let g = self.g();
        let x = left + 18.0 + g.measure(title, 13.5, 600) + 8.0;
        g.text(note, x, cy, 11.0, 400, Align::Left, GREY, 1.0, 120.0);
        x + g.measure(note, 11.0, 400)
    }

    // ------------------------------------------------------------ shared controls

    /// Round play/pause disc; the pause bars and play triangle cross-fade.
    pub(super) fn play_disc(&self, cx: f32, cy: f32, r: f32, color: u32, fg: u32, playing: bool, act: Act) {
        let g = self.g();
        let s = self.squeeze(act) * (1.0 + 0.06 * self.hov(act));
        g.fill_circle(cx, cy, r * s, color, 1.0);
        let pl = self.fx.level(act, playing as u8 as f32);
        let k = r / 15.0 * s;
        if pl > 0.0 {
            for dx in [-4.8, 2.0] {
                g.fill_rrect(cx + dx * k, cy - (7.5 - (1.0 - pl) * 2.5) * k, 3.0 * k, (15.0 - (1.0 - pl) * 5.0) * k, 1.2, fg, pl);
            }
        }
        if pl < 1.0 {
            g.polygon(&[(cx - 4.0 * k, cy - 7.0 * k), (cx - 4.0 * k, cy + 7.0 * k), (cx + 7.0 * k, cy)], fg, 1.0 - pl);
        }
        self.hit(cx - r - 3.0, cy - r - 3.0, 2.0 * r + 6.0, 2.0 * r + 6.0, act);
    }

    /// Amber "+" that turns into an "x" while the task form is open.
    pub(super) fn plus_button(&self, cx: f32, cy: f32) {
        let g = self.g();
        let act = Act::TodoAdd;
        let turn = self.fx.level(act, self.form.is_some() as u8 as f32);
        let s = self.squeeze(act) * (1.0 + 0.08 * self.hov(act));
        g.fill_circle(cx, cy, 10.0 * s, AMBER, 1.0);
        for base in [0.0f32, 90.0] {
            let a = (base + 45.0 * turn).to_radians();
            let (dx, dy) = (a.cos() * 4.5 * s, a.sin() * 4.5 * s);
            g.line(cx - dx, cy - dy, cx + dx, cy + dy, ON_AMBER, 1.0, 1.8);
        }
        self.hit(cx - 12.0, cy - 12.0, 24.0, 24.0, act);
    }

    /// To-do check circle: fills and ticks with a little overshoot when a task is done.
    pub(super) fn check_mark(&self, i: usize, done: bool, cx: f32, cy: f32) {
        let g = self.g();
        let d = self.fx.level(Act::TodoCheck(i), done as u8 as f32);
        if d < 1.0 {
            g.stroke_circle(cx, cy, 6.0, 0x6A6A6A, 1.0 - d, 1.4);
        }
        if d > 0.0 {
            let pop = 1.0 + 0.2 * (d * std::f32::consts::PI).sin();
            g.fill_circle(cx, cy, 6.5 * d * pop, AMBER, 1.0);
            g.icon(glyph::CHECK, cx, cy, 7.5 * d, ON_AMBER, d);
        }
        self.hover_ring(Act::TodoCheck(i), cx, cy, 10.0, 0.08);
    }

    // ------------------------------------------------------------ Pomodoro

    /// Focus / Short / Long chips; the active one gets an outline in the mode colour.
    /// `x` is the left edge, or the centre when `center` is set.
    pub(super) fn mode_chips(&self, x: f32, cy: f32, center: bool) {
        let g = self.g();
        let chips = [("Focus", PMode::Focus, Act::PomoFocus), ("Short", PMode::Short, Act::PomoShort), ("Long", PMode::Long, Act::PomoLong)];
        let widths: Vec<f32> = chips.iter().map(|c| g.measure(c.0, 11.0, 400) + 20.0).collect();
        let total = widths.iter().sum::<f32>() + 6.0 * (chips.len() as f32 - 1.0);
        let mut x = if center { x - total / 2.0 } else { x };
        for ((label, mode, act), w) in chips.into_iter().zip(widths) {
            let on = self.fx.level(act, (self.pomo.mode == mode) as u8 as f32);
            let color = if mode == PMode::Focus { AMBER } else { GREEN };
            self.button(x, cy - 10.0, w, 20.0, 10.0, 0x161616, 1.0, act);
            if on > 0.0 {
                g.stroke_rrect(x, cy - 10.0, w, 20.0, 10.0, color, on, 1.2, false);
            }
            g.text(label, x + w / 2.0, cy, 11.0, 400, Align::Center, mix(0x8A8A8A, color, on), 1.0, w);
            x += w + 6.0;
        }
    }

    /// mm:ss in big digits; the seconds slide up a little each time they change.
    pub(super) fn big_timer(&self, x: f32, cy: f32, size: f32, center: bool) {
        let g = self.g();
        let p = &self.pomo;
        let r = p.remaining.max(0.0);
        let shown = r.ceil();
        let (mm, ss) = (format!("{:02}:", shown as u64 / 60), format!("{:02}", shown as u64 % 60));
        let (mw, sw) = (g.measure(&mm, size, 700), g.measure(&ss, size, 700));
        let left = if center { x - (mw + sw) / 2.0 } else { x };
        g.text(&mm, left, cy, size, 700, Align::Left, WHITE, 1.0, mw + 8.0);
        let k = if p.running { ease((shown - r) as f32 / 0.25) } else { 1.0 };
        let a = g.alpha.get();
        g.alpha.set(a * (0.35 + 0.65 * k));
        g.text(&ss, left + mw, cy + (1.0 - k) * size * 0.22, size, 700, Align::Left, WHITE, 1.0, sw + 8.0);
        g.alpha.set(a);
    }

    /// Straight progress bar of the current Pomodoro phase.
    pub(super) fn pomo_bar(&self, x: f32, y: f32, w: f32) {
        let g = self.g();
        let p = &self.pomo;
        let f = (1.0 - p.remaining / p.mode.secs()).clamp(0.0, 1.0) as f32;
        g.fill_rrect(x, y, w, 4.0, 2.0, 0x222222, 1.0);
        if f > 0.0 {
            g.fill_rrect(x, y, (w * f).max(4.0), 4.0, 2.0, self.pomo_color(), 1.0);
        }
    }

    /// Big start/pause disc; a soft ring keeps rippling out of it while the timer runs.
    pub(super) fn pomo_play(&self, cx: f32, cy: f32, r: f32) {
        let color = self.pomo_color();
        if self.pomo.running {
            let t = (self.started.elapsed().as_secs_f32() / 1.6).fract();
            self.g().stroke_circle(cx, cy, r + 3.0 + 10.0 * t, color, 0.45 * (1.0 - t), 2.0);
        }
        let fg = if color == AMBER { ON_AMBER } else { 0x0B2A12 };
        self.play_disc(cx, cy, r, color, fg, self.pomo.running, Act::PomoToggle);
    }

    pub(super) fn pomo_label(&self) -> String {
        format!("{} · session {} of 4", self.pomo.mode.label(), self.pomo.sessions % 4 + 1)
    }

    pub(super) fn page_pomodoro(&self) {
        self.mode_chips(16.0, 40.0, false);
        self.big_timer(16.0, 79.0, 44.0, false);
        self.pomo_bar(16.0, 108.0, 270.0);
        self.g().text(&self.pomo_label(), 16.0, 122.0, 10.5, 400, Align::Left, GREY, 1.0, 126.0);
        self.round_button(332.0, 74.0, 15.0, 0x222222, glyph::REFRESH, 13.0, WHITE, Act::PomoReset);
        self.pomo_play(381.0, 74.0, 22.0);
    }

    // ------------------------------------------------------------ Water

    /// "60% · Next 10:00", "Reminders off", ...
    pub(super) fn water_status(&self) -> String {
        let w = &self.water;
        let tail = if !w.on {
            "Reminders off".to_string()
        } else if w.taken >= w.goal {
            "Goal reached".to_string()
        } else if let Some(n) = w.next_time() {
            format!("Next {}", model::hhmm(n))
        } else {
            "Done for today".to_string()
        };
        format!("{}% · {}", w.percent(), tail)
    }

    /// A bottle that fills to today's progress: the level glides, the surface waves, and
    /// after "Drink" a drop falls in and ripples.
    pub(super) fn water_bottle(&self, x: f32, y: f32, w: f32, h: f32) {
        let g = self.g();
        let wt = &self.water;
        let f = self.fx.level(Act::WaterDrink, (wt.taken as f32 / wt.goal.max(1) as f32).min(1.0));
        let neck = w * 0.42;
        g.fill_rrect(x + (w - neck) / 2.0, y, neck, 7.0, 3.0, 0x2A3A4A, 1.0);
        let (by, bh) = (y + 9.0, h - 9.0);
        g.fill_rrect(x, by, w, bh, 12.0, 0x0B131B, 1.0);
        let (lx, lw) = (x + 3.0, w - 6.0);
        let bottom = by + bh - 3.0;
        let level = bottom - (bh - 6.0) * f;
        if f > 0.002 {
            // the liquid is clipped to the inside of the bottle, so its rounded bottom and the
            // waving surface never poke through the glass
            let t = self.started.elapsed().as_secs_f32();
            g.push_clip_rrect(lx, by + 3.0, lw, bh - 6.0, 9.5);
            g.fill_rrect(lx, level + 2.0, lw, bottom - level, 0.0, 0x2E7FD0, 1.0);
            let mut wave = vec![(lx, level + 4.0)];
            for i in 0..=12 {
                let u = i as f32 / 12.0;
                wave.push((lx + lw * u, level + (t * 3.2 + u * 6.3).sin() * 1.6));
            }
            wave.push((lx + lw, level + 4.0));
            g.polygon(&wave, 0x2E7FD0, 1.0);
            g.fill_rrect(lx + 4.0, level + 6.0, 3.0, (bottom - level - 12.0).max(0.0), 1.5, WHITE, 0.12);
            g.pop_clip();
        }
        g.stroke_rrect(x, by, w, bh, 12.0, 0x2A3A4A, 1.0, 2.0, false);
        g.text(&format!("{}%", wt.percent()), x + w / 2.0, by + bh * 0.55, 11.0, 700, Align::Center, WHITE, 0.9, w);
        if let Some(t0) = self.drink_at {
            let e = t0.elapsed().as_secs_f32();
            if e < 0.45 {
                let k = (e / 0.45).powi(2);
                g.icon(glyph::DROP, x + w / 2.0, (y - 8.0) + (level - y + 8.0) * k, 10.0, CYAN, 1.0);
            } else if e < 0.95 {
                let k = (e - 0.45) / 0.5;
                g.stroke_circle(x + w / 2.0, level, 3.0 + 9.0 * k, WHITE, 0.5 * (1.0 - k), 1.2);
            }
        }
    }

    /// Hourly water bars over the active hours, labelled every three hours.
    pub(super) fn water_chart(&self, left: f32, right: f32, top: f32, bottom: f32) {
        let g = self.g();
        let w = &self.water;
        let first = (w.start / 60) as usize;
        let last = ((w.end + 59) / 60).min(24) as usize;
        let n = (last.saturating_sub(first)).max(1);
        let maxv = w.hours[first..last.max(first)].iter().copied().max().unwrap_or(0).max(w.bottle).max(1) as f32;
        let slot = (right - left) / n as f32;
        let bw = (slot * 0.62).min(10.0);
        let now = model::now().hour;
        for i in 0..n {
            let hour = first + i;
            let x = left + slot * i as f32 + (slot - bw) / 2.0;
            g.fill_rrect(x, top, bw, bottom - top, bw / 2.0, 0x141414, 1.0);
            let v = w.hours.get(hour).copied().unwrap_or(0) as f32 / maxv;
            if v > 0.0 {
                let hh = ((bottom - top) * v).max(bw);
                g.fill_rrect(x, bottom - hh, bw, hh, bw / 2.0, BLUE, 1.0);
            }
            if hour == now {
                g.fill_circle(x + bw / 2.0, top - 5.0, 1.8, WHITE, 0.8);
            }
            if hour % 3 == 0 {
                g.text(&format!("{:02}", hour), x + bw / 2.0, bottom + 10.0, 10.0, 400, Align::Center, GREY, 1.0, 30.0);
            }
        }
    }

    /// Drink button, settings and the reminder switch in a row starting at `x`.
    pub(super) fn water_actions(&self, x: f32, cy: f32) {
        let g = self.g();
        self.button(x, cy - 12.0, 66.0, 24.0, 12.0, 0x123A63, 1.0, Act::WaterDrink);
        g.icon(glyph::DROP, x + 15.0, cy, 11.0, CYAN, 1.0);
        g.text("Drink", x + 25.0, cy, 11.5, 400, Align::Left, WHITE, 1.0, 40.0);
        self.round_button(x + 84.0, cy, 12.0, 0x1F1F1F, glyph::SLIDERS, 10.0, WHITE, Act::WaterSettings);
        self.switch(x + 104.0, cy, self.water.on, Act::WaterOn);
        self.hit(x + 102.0, cy - 10.0, 34.0, 20.0, Act::WaterOn);
    }

    pub(super) fn page_water(&self) {
        let g = self.g();
        let w = &self.water;
        self.water_bottle(22.0, 33.0, 44.0, 93.0);
        let litres = format!("{:.1}", w.taken as f32 / 1000.0);
        g.text(&litres, 84.0, 50.0, 26.0, 700, Align::Left, WHITE, 1.0, 100.0);
        let lw = g.measure(&litres, 26.0, 700);
        g.text(&format!("/ {:.1} L", w.goal as f32 / 1000.0), 84.0 + lw + 5.0, 54.0, 12.0, 400, Align::Left, GREY, 1.0, 70.0);
        g.text(&self.water_status(), 84.0, 75.0, 11.0, 400, Align::Left, GREY, 1.0, 140.0);
        self.water_actions(84.0, 104.0);
        g.line(236.0, 38.0, 236.0, 120.0, HAIR, 1.0, 1.0);
        self.water_chart(252.0, 404.0, 42.0, 110.0);
    }

    pub(super) fn page_water_settings(&self) {
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

    // ------------------------------------------------------------ Shelf

    /// One shelf item: lifts a little on hover; click opens it, dragging takes it out.
    pub(super) fn shelf_card(&self, i: usize, x: f32, y: f32, w: f32, h: f32) {
        let g = self.g();
        let Some(it) = self.shelf.items.get(i) else { return };
        let act = Act::ShelfItem(i);
        let k = self.hov(act);
        let y = y - 3.0 * k;
        g.fill_rrect(x, y, w, h, 12.0, CARD, 1.0);
        g.stroke_rrect(x, y, w, h, 12.0, mix(HAIR, 0x4A4A4A, k), 1.0, 1.0, false);
        let thumb = if it.kind == Kind::Image { self.thumb(&it.path) } else { None };
        if let Some(bmp) = &thumb {
            // images (screenshots) show a picture instead of an icon
            g.bitmap_rounded(bmp, x + 4.0, y + 4.0, w - 8.0, h * 0.56, 9.0);
        } else {
            let (gl, c) = match it.kind {
                Kind::Folder => (glyph::FOLDER, 0xFFC94A),
                Kind::Link => (glyph::LINK, BLUE),
                Kind::Image => (glyph::PICTURE, 0xFF7AB6),
                Kind::File => (glyph::DOC, 0xCFCFCF),
            };
            g.icon(gl, x + w / 2.0, y + h * 0.32, 22.0, c, 1.0);
        }
        let (ny, sy) = if thumb.is_some() { (y + h * 0.72, y + h * 0.87) } else { (y + h * 0.64, y + h * 0.8) };
        g.text(&it.name, x + w / 2.0, ny, 10.0, 400, Align::Center, 0xD8D8D8, 1.0, w - 6.0);
        let sub = if it.kind == Kind::Folder || it.kind == Kind::Link { String::new() } else { shelf::size_label(it.size) };
        g.text(&sub, x + w / 2.0, sy, 9.0, 400, Align::Center, DIM, 1.0, w - 6.0);
        self.hit(x, y, w, h, act);
        // remove button, shown while the card (or the button itself) is hovered
        let rm = Act::ShelfRemove(i);
        let shown = k.max(self.hov(rm));
        if shown > 0.05 {
            let (bx, by) = (x + w - 8.0, y + 8.0);
            let s = self.squeeze(rm) * (1.0 + 0.15 * self.hov(rm));
            g.fill_circle(bx, by, 8.0 * s, mix(0x2A2A2A, RED, self.hov(rm)), shown);
            g.icon(glyph::CLOSE, bx, by, 7.0 * s, WHITE, shown);
            self.hit(bx - 9.0, by - 9.0, 18.0, 18.0, rm);
        }
    }

    /// Thumbnail of an image on the Shelf, decoded once (small) and cached.
    pub(super) fn thumb(&self, path: &std::path::Path) -> Option<ID2D1Bitmap> {
        if let Some((_, b)) = self.thumbs.borrow().iter().find(|(p, _)| p == path) {
            return b.clone();
        }
        let bmp = std::fs::read(path).ok().and_then(|bytes| self.g().bitmap_from_bytes(&bytes));
        let mut cache = self.thumbs.borrow_mut();
        if cache.len() >= 16 {
            cache.remove(0);
        }
        cache.push((path.to_path_buf(), bmp.clone()));
        bmp
    }

    /// Shelf item indices to show, newest first.
    pub(super) fn shelf_order(&self, max: usize) -> Vec<usize> {
        (0..self.shelf.items.len()).rev().take(max).collect()
    }

    /// Dashed drop target; glows amber and bobs while files are dragged over the notch.
    pub(super) fn drop_card(&self, x: f32, y: f32, w: f32, h: f32, more: usize) {
        let g = self.g();
        let drag = self.shelf_drag;
        if drag {
            g.fill_rrect(x, y, w, h, 12.0, AMBER, 0.06);
        }
        g.stroke_rrect(x, y, w, h, 12.0, if drag { AMBER } else { 0x3A3A3A }, 1.0, 1.3, true);
        let bob = if drag { (self.started.elapsed().as_secs_f32() * 6.0).sin() * 2.5 } else { 0.0 };
        let wide = w > 120.0;
        let cy = y + h * if wide { 0.38 } else { 0.4 };
        g.icon(if drag { glyph::DOWNLOAD } else { glyph::ADD }, x + w / 2.0, cy + bob, 18.0, if drag { AMBER } else { 0x8A8A8A }, 1.0);
        let label = if drag {
            "Release to add".to_string()
        } else if more > 0 {
            format!("+{} more", more)
        } else if wide {
            "Drop files here".to_string()
        } else {
            "Drop here".to_string()
        };
        g.text(&label, x + w / 2.0, y + h * 0.68, if wide { 12.5 } else { 10.0 }, 400, Align::Center, if drag { AMBER } else { 0x9A9A9A }, 1.0, w - 6.0);
        if wide && !drag {
            g.text("Drag them back out anytime · click to open", x + w / 2.0, y + h * 0.84, 10.0, 400, Align::Center, DIM, 1.0, w - 10.0);
        }
    }

    /// "N items" and Clear next to the Shelf title.
    pub(super) fn shelf_header(&self, left: f32, cy: f32) {
        let n = self.shelf.items.len();
        if n == 0 {
            return;
        }
        let end = self.header_note("Shelf", &format!("{} item{}", n, if n == 1 { "" } else { "s" }), left, cy);
        self.button(end + 10.0, cy - 8.5, 42.0, 17.0, 8.5, 0x1F1F1F, 1.0, Act::ShelfClear);
        self.g().text("Clear", end + 31.0, cy, 11.0, 400, Align::Center, WHITE, 1.0, 40.0);
    }

    pub(super) fn page_shelf(&self) {
        self.shelf_header(16.0, 17.0);
        let n = self.shelf.items.len();
        let (x0, y, h, cw, gap) = (16.0, 34.0, 90.0, 58.0, 8.0);
        let order = self.shelf_order(5);
        let shown = order.len();
        for (k, &i) in order.iter().enumerate() {
            self.shelf_card(i, x0 + k as f32 * (cw + gap), y, cw, h);
        }
        let dx = x0 + shown as f32 * (cw + gap);
        self.drop_card(dx, y, 404.0 - dx, h, n - shown);
    }

    // ------------------------------------------------------------ To-do

    /// One task row: check circle, title (struck through as it completes), time chip
    /// (amber outline while it is still ahead), and an edit button when selected.
    pub(super) fn todo_row(&self, i: usize, x: f32, y: f32, w: f32, h: f32) {
        let g = self.g();
        let Some(t) = self.tasks.get(i) else { return };
        let cy = y + h / 2.0;
        let sel = self.task_sel == Some(i);
        let k = self.fx.level(Act::TodoRow(i), sel as u8 as f32).max(self.hov(Act::TodoRow(i)));
        if k > 0.0 {
            g.fill_rrect(x, y, w, h, 8.0, 0x1A1A1A, k);
        }
        self.hit(x + 24.0, y, w - 24.0, h, Act::TodoRow(i));
        self.check_mark(i, t.done, x + 12.0, cy);
        self.hit(x, y, 24.0, h, Act::TodoCheck(i));
        let time = match (t.start, t.end) {
            (Some(s), Some(e)) => format!("{}–{}", model::hhmm(s), model::hhmm(e)),
            (Some(s), None) => model::hhmm(s),
            _ => String::new(),
        };
        let mut right = x + w - 6.0;
        if !time.is_empty() {
            let ahead = !t.done && t.start.is_some_and(|s| s >= model::now().minutes);
            let tw = g.measure(&time, 10.0, 400) + 14.0;
            if ahead {
                g.stroke_rrect(right - tw, cy - 9.0, tw, 18.0, 9.0, AMBER, 1.0, 1.0, false);
            }
            g.text(&time, right - tw / 2.0, cy, 10.0, 400, Align::Center, if ahead { AMBER } else { GREY }, 1.0, tw);
            right -= tw + 6.0;
        }
        if sel && self.form.is_none() {
            self.round_button(right - 9.0, cy, 9.0, 0x262626, glyph::EDIT, 9.0, WHITE, Act::TodoEdit);
            right -= 24.0;
        }
        let d = self.fx.level(Act::TodoCheck(i), t.done as u8 as f32);
        let tx = x + 26.0;
        let max_w = right - tx - 6.0;
        g.text(&t.title, tx, cy, 12.5, 400, Align::Left, mix(WHITE, DIM, d), 1.0, max_w);
        if d > 0.0 {
            let lw = g.measure(&t.title, 12.5, 400).min(max_w);
            g.line(tx, cy + 0.5, tx + lw * d, cy + 0.5, DIM, 1.0, 1.0);
        }
    }

    /// Empty list: a short invitation instead of a big box.
    pub(super) fn todo_empty(&self, cx: f32, cy: f32) {
        let g = self.g();
        g.text("No tasks yet", cx, cy, 13.0, 700, Align::Center, 0x8A8A8A, 1.0, 200.0);
        g.text("Press + to add one", cx, cy + 20.0, 11.0, 400, Align::Center, DIM, 1.0, 200.0);
    }

    pub(super) fn todo_scrollbar(&self, x: f32, top: f32, height: f32, rows: usize) {
        let n = self.tasks.len();
        if n > rows {
            let h = height * rows as f32 / n as f32;
            let y = top + (height - h) * self.task_scroll as f32 / (n - rows) as f32;
            self.g().fill_rrect(x, y, 2.0, h, 1.0, 0x4A4A4A, 1.0);
        }
    }

    pub(super) fn page_todo(&self) {
        let done = self.tasks.iter().filter(|t| t.done).count();
        if !self.tasks.is_empty() {
            self.header_note("To-do", &format!("{} of {} done", done, self.tasks.len()), 16.0, 17.0);
        }
        self.plus_button(350.0, 17.0);
        if let Some(f) = &self.form {
            self.todo_form(f, 16.0, 36.0, 388.0, false);
            return;
        }
        if self.tasks.is_empty() {
            self.todo_empty(210.0, 66.0);
            return;
        }
        for (row, i) in (self.task_scroll..self.tasks.len()).take(3).enumerate() {
            self.todo_row(i, 16.0, 34.0 + row as f32 * 28.0, 384.0, 26.0);
        }
        self.todo_scrollbar(404.0, 34.0, 82.0, 3);
    }

    /// Title and time inputs side by side (`stacked`: one above the other), then
    /// Cancel / Save (and Delete when editing).
    pub(super) fn todo_form(&self, f: &Form, x: f32, y: f32, w: f32, stacked: bool) {
        let g = self.g();
        let blink = (self.started.elapsed().as_millis() / 530) % 2 == 0;
        let focus_title = f.field == 0;
        let (tw, tx2, ty2) = if stacked { (w, x, y + 34.0) } else { (w * 0.6, x + w * 0.6 + 8.0, y) };
        let time_w = if stacked { w } else { w - w * 0.6 - 8.0 };
        g.fill_rrect(x, y, tw, 26.0, 8.0, 0x0A0A0A, 1.0);
        g.stroke_rrect(x, y, tw, 26.0, 8.0, if focus_title { AMBER } else { 0x333333 }, 1.0, 1.2, false);
        self.hit(x, y, tw, 26.0, Act::TodoTitle);
        if f.title.is_empty() && !focus_title {
            g.text("Task title", x + 10.0, y + 13.0, 12.5, 400, Align::Left, 0x6A6A6A, 1.0, tw - 16.0);
        } else {
            // show the tail of long titles
            let mut shown = f.title.clone();
            while g.measure(&shown, 12.5, 400) > tw - 24.0 && !shown.is_empty() {
                shown.remove(0);
            }
            let cursor = if focus_title && blink { "|" } else { "" };
            g.text(&format!("{}{}", shown, cursor), x + 10.0, y + 13.0, 12.5, 400, Align::Left, WHITE, 1.0, tw - 14.0);
        }
        g.fill_rrect(tx2, ty2, time_w, 26.0, 8.0, 0x141414, 1.0);
        g.stroke_rrect(tx2, ty2, time_w, 26.0, 8.0, if focus_title { 0x333333 } else { AMBER }, 1.0, 1.2, false);
        self.hit(tx2, ty2, time_w, 26.0, Act::TodoTime);
        g.icon(glyph::CLOCK, tx2 + 14.0, ty2 + 13.0, 11.0, GREY, 1.0);
        let mask = model::time_mask(&f.digits);
        g.text(&mask, tx2 + 26.0, ty2 + 13.0, 12.0, 400, Align::Left, if f.digits.is_empty() { 0x6A6A6A } else { WHITE }, 1.0, time_w - 30.0);

        let by = if stacked { y + 72.0 } else { y + 38.0 };
        let bw = if stacked { (w - 8.0) / 2.0 } else { 80.0 };
        let (cx, sx) = if stacked { (x, x + bw + 8.0) } else { (x + w - 2.0 * bw - 8.0, x + w - bw) };
        self.button(cx, by, bw, 24.0, 12.0, 0x262626, 1.0, Act::TodoCancel);
        g.text("Cancel", cx + bw / 2.0, by + 12.0, 12.0, 400, Align::Center, WHITE, 1.0, bw);
        self.button(sx, by, bw, 24.0, 12.0, AMBER, 1.0, Act::TodoSave);
        g.text("Save", sx + bw / 2.0, by + 12.0, 12.0, 700, Align::Center, ON_AMBER, 1.0, bw);
        if f.edit.is_some() {
            let (dx, dy) = if stacked { (x + w - 12.0, by + 44.0) } else { (x + 12.0, by + 12.0) };
            self.round_button(dx, dy, 11.0, 0x2A1515, glyph::DELETE, 9.5, RED, Act::TodoDelete);
        }
        let hint = "Enter saves · Esc cancels · Tab: time (HHMM HHMM)";
        if stacked {
            g.text_box(hint, x, by + 64.0, w, 40.0, 10.0, 400, DIM, 1.0);
        } else {
            g.text(hint, if f.edit.is_some() { x + 30.0 } else { x }, by + 12.0, 10.0, 400, Align::Left, DIM, 1.0, w - 2.0 * bw - 40.0);
        }
    }
}
