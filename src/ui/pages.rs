//! Expanded panel (top dock): header, paging and the Pomodoro, Water, Shelf and To-do pages.

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

    /// To-do check circle: fills and ticks with a little overshoot when a task is done.
    pub(super) fn check_mark(&self, i: usize, done: bool, cx: f32, cy: f32) {
        let g = self.g();
        let d = self.fx.level(Act::TodoCheck(i), done as u8 as f32);
        if d < 1.0 {
            g.stroke_circle(cx, cy, 6.0, 0x6A6A6A, 1.0 - d, 1.4);
        }
        if d > 0.0 {
            let pop = 1.0 + 0.2 * (d * std::f32::consts::PI).sin();
            g.fill_circle(cx, cy, 6.5 * d * pop, GREEN, 1.0);
            g.icon(glyph::CHECK, cx, cy, 7.5 * d, 0x0B2A12, d);
        }
        self.hover_ring(Act::TodoCheck(i), cx, cy, 10.0, 0.08);
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

    pub(super) fn header(&self) {
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

    pub(super) fn page_pomodoro(&self) {
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

    pub(super) fn page_water(&self) {
        let g = self.g();
        let w = &self.water;
        let litres = format!("{:.1}", w.taken as f32 / 1000.0);
        g.text(&litres, 44.0, 49.0, 30.0, 700, Align::Left, WHITE, 1.0, 120.0);
        let lw = g.measure(&litres, 30.0, 700);
        g.text(&format!("/ {:.1} L", w.goal as f32 / 1000.0), 44.0 + lw + 6.0, 53.0, 14.0, 400, Align::Left, GREY, 1.0, 80.0);

        // reminder toggle
        let on = w.on;
        let k = self.fx.level(Act::WaterOn, on as u8 as f32);
        self.button(162.0, 37.0, 34.0, 18.0, 9.0, 0x3A3A3A, 1.0, Act::WaterOn);
        g.fill_rrect(162.0, 37.0, 34.0, 18.0, 9.0, 0x2699FF, k);
        let kx = 171.0 + 16.0 * k;
        g.fill_circle(kx, 46.0, 7.2, WHITE, 1.0);
        g.icon(glyph::BELL, kx, 46.0, 8.5, if k > 0.5 { 0x2699FF } else { GREY }, 1.0);

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

    pub(super) fn water_gauge(&self) {
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

    pub(super) fn water_chart(&self) {
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

    pub(super) fn page_shelf(&self) {
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
            let k = self.hov(act);
            if k > 0.0 {
                g.fill_rrect(x, y, 51.0, 88.0, 10.0, WHITE, 0.07 * k);
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

    pub(super) fn page_todo(&self) {
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
            let k = self.fx.level(Act::TodoRow(i), sel as u8 as f32).max(self.hov(Act::TodoRow(i)));
            if k > 0.0 {
                g.fill_rrect(202.0, y, 169.0, 25.0, 8.0, 0x1F1F1F, k);
            }
            self.hit(222.0, y, 149.0, 25.0, Act::TodoRow(i));
            self.check_mark(i, t.done, 212.0, y + 12.5);
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

    pub(super) fn todo_form(&self, f: &Form) {
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
