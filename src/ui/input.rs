//! Input: hover/dwell detection, click-through, dragging the notch, mouse/keyboard actions.

use super::*;

impl App {
    pub(super) fn cursor_logical(&self) -> (f32, f32) {
        let mut p = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut p);
            let mut r = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut r);
            ((p.x - r.left) as f32 / self.scale, (p.y - r.top) as f32 / self.scale)
        }
    }

    pub(super) fn update_hover(&mut self, now: Instant) {
        if self.demo.pin {
            self.hover = true;
            return;
        }
        if self.drag.is_some() {
            self.hover = true;
            self.hover_leave = None;
            return;
        }
        let (x, y) = self.cursor_logical();
        let (w, h) = (self.w.v, self.h.v);
        let (sx, sy) = self.shape_pos(w, h);
        let inside = x >= sx - 4.0 && x <= sx + w + 4.0 && y >= sy - 4.0 && y <= sy + h + 6.0;
        self.near = x >= sx - 40.0 && x <= sx + w + 40.0 && y >= sy - 40.0 && y <= sy + h + 40.0;
        let (held, pressed) = mouse_buttons();
        if inside != self.inside {
            self.inside = inside;
            self.drag_in = inside && held;
            self.dirty = true;
        }
        if inside {
            self.hover_leave = None;
            if !self.hover {
                // a click that went through to the window below: the user works there
                if pressed && !self.drag_in && !self.solid {
                    self.blocked = true;
                }
                let ctrl = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(windows::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL.0 as i32) } < 0;
                let (delay, radius) = if self.drag_in { (250, 14.0) } else { (400, 6.0) };
                let open = if self.blocked || self.alert_solid() {
                    self.dwell = None;
                    false
                } else if ctrl && !held {
                    true
                } else {
                    // open only once the cursor rests on the notch, not when it passes over it
                    match self.dwell {
                        Some((t, (px, py))) if (x - px).abs() <= radius && (y - py).abs() <= radius => now.duration_since(t) >= Duration::from_millis(delay),
                        _ => {
                            self.dwell = Some((now, (x, y)));
                            false
                        }
                    }
                };
                if open {
                    self.hover = true;
                    self.dwell = None;
                    self.dirty = true;
                }
            }
        } else {
            self.dwell = None;
            self.blocked = false;
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

    /// Whether the window should take mouse input right now.
    pub(super) fn wants_input(&self) -> bool {
        self.hover || self.demo.pin || self.form.is_some() || self.shelf_drag || self.drag.is_some() || self.press.is_some() || self.alert_solid()
    }

    /// Size-all cursor over the header / while moving the notch.
    pub fn grab_cursor(&self) -> bool {
        self.drag.is_some() || self.hot == Some(Act::Grab)
    }

    /// Back to the middle of the top edge (tray menu).
    pub fn reset_position(&mut self) {
        self.settings.offset = 0.5;
        if self.settings.dock != Dock::Top {
            self.set_dock(Dock::Top);
        } else {
            self.settings.save();
        }
        place_window(self.hwnd, Dock::Top, 0.5);
    }

    /// Follows the cursor while the header is dragged: slides along the edge and jumps to
    /// another edge once the cursor is clearly closer to it.
    pub(super) fn drag_to(&mut self, p: POINT) {
        let Some((mut grab, moved, start)) = self.drag else { return };
        let (sw, sh) = unsafe { (GetSystemMetrics(SM_CXSCREEN) as f32, GetSystemMetrics(SM_CYSCREEN) as f32) };
        let (x, y) = (p.x as f32, p.y as f32);
        let cur = self.settings.dock;
        let dist = |d: Dock| match d {
            Dock::Top => y,
            Dock::Left => x,
            Dock::Right => sw - x,
        };
        let best = [Dock::Top, Dock::Left, Dock::Right].into_iter().min_by(|a, b| dist(*a).total_cmp(&dist(*b))).unwrap_or(cur);
        let dock = if best != cur && dist(best) + 48.0 * self.scale < dist(cur) { best } else { cur };
        if dock != cur {
            // the header sits in the middle of the top panel and near the top of a side panel
            grab = if dock == Dock::Top { 0.0 } else { 200.0 * self.scale };
            self.drag = Some((grab, moved, start));
        }
        let (along, len, half) = match dock {
            Dock::Top => (x, sw, 210.0),
            _ => (y, sh, 220.0),
        };
        let lo = (half * self.scale / len).min(0.5);
        self.settings.offset = ((along + grab) / len).clamp(lo, 1.0 - lo);
        if dock != cur {
            self.set_dock(dock);
        }
        place_window(self.hwnd, dock, self.settings.offset);
        self.region = (0, 0, 0, 0);
        self.dirty = true;
    }

    pub(super) fn hit_at(&self, x: f32, y: f32) -> Option<Act> {
        self.hits.borrow().iter().rev().find(|h| x >= h.x && x < h.x + h.w && y >= h.y && y < h.y + h.h).map(|h| h.act)
    }

    pub(super) fn hit_rect(&self, act: Act) -> Option<(f32, f32, f32, f32)> {
        self.hits.borrow().iter().find(|h| h.act == act).map(|h| (h.x, h.y, h.w, h.h))
    }

    pub fn mouse_move(&mut self, px: i32, py: i32) {
        if let Some((grab, moved, start)) = self.drag {
            let p = cursor_screen();
            if moved || (p.x - start.x).abs() + (p.y - start.y).abs() > 6 {
                self.drag = Some((grab, true, start));
                self.drag_to(p);
            }
            return;
        }
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
        self.fx.pulse(act);
        if act == Act::Grab {
            let p = cursor_screen();
            let (sw, sh) = unsafe { (GetSystemMetrics(SM_CXSCREEN) as f32, GetSystemMetrics(SM_CYSCREEN) as f32) };
            let grab = match self.settings.dock {
                Dock::Top => self.settings.offset * sw - p.x as f32,
                _ => self.settings.offset * sh - p.y as f32,
            };
            self.drag = Some((grab, false, p));
            self.dirty = true;
            return;
        }
        if matches!(act, Act::ShelfItem(_)) {
            self.press = Some((x, y, act));
            return;
        }
        self.act(act, x);
        self.dirty = true;
    }

    pub fn mouse_up(&mut self, px: i32, py: i32) {
        let (x, y) = (px as f32 / self.scale, py as f32 / self.scale);
        if let Some((_, moved, _)) = self.drag.take() {
            if moved {
                self.settings.save();
            }
            self.dirty = true;
            return;
        }
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

    pub(super) fn go_page(&mut self, dir: i32) {
        let n = PAGES.len() as i32;
        let next = ((self.page as i32 + dir) % n + n) % n;
        self.slide = Spring::new(if dir > 0 { 36.0 } else { -36.0 });
        self.page = next as usize;
        self.water_settings = false;
    }

    pub(super) fn set_page(&mut self, p: usize) {
        if p != self.page {
            self.slide = Spring::new(if p > self.page { 36.0 } else { -36.0 });
            self.page = p;
        }
    }

    pub(super) fn open_form(&mut self, edit: Option<usize>) {
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

    pub(super) fn close_form(&mut self) {
        if self.form.take().is_some() {
            self.effect = Some(Effect::Deactivate);
        }
        self.dirty = true;
    }

    pub(super) fn save_form(&mut self) {
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

    pub(super) fn act(&mut self, act: Act, x: f32) {
        match act {
            Act::Grab => {}
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

    pub(super) fn drink(&mut self) {
        if let Some(WaterEvent::GoalReached) = self.water.drink() {
            let mut a = Alert::simple(AlertIcon::Glyph(glyph::DROP, CYAN), "Goal reached", &format!("{:.1} L today", self.water.taken as f32 / 1000.0), CYAN);
            a.sound = true;
            self.push(a);
        }
        self.water.save();
    }
}
