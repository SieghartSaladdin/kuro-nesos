//! Alerts: queueing system and app notifications, the demo script, and the alert views.

use super::*;

impl App {
    pub(super) fn push(&mut self, a: Alert) {
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

    pub(super) fn sys_alert(&mut self, s: SysAlert) {
        let mut a = Alert::simple(AlertIcon::Glyph(s.icon, s.color), &s.title, &s.value, s.color);
        if s.icon == glyph::MESSAGE {
            a.ttl = Duration::from_millis(5000);
            a.sound = true;
            a.card = true;
        }
        self.push(a);
    }

    pub(super) fn push_action_alert(&mut self, icon: AlertIcon, title: &str, value: &str, color: u32, actions: Vec<(String, AlertAction)>) {
        let mut a = Alert::simple(icon, title, value, color);
        a.actions = actions;
        a.ttl = Duration::from_secs(10);
        a.sound = true;
        self.push(a);
    }

    pub(super) fn update_alerts(&mut self, now: Instant) {
        if let Some(a) = &self.alert {
            let born = a.born.unwrap_or(now);
            let held = (self.hover || self.inside) && a.big();
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

    /// An alert with buttons takes clicks (and does not turn into the panel on hover).
    pub(super) fn alert_solid(&self) -> bool {
        self.shown == Mode::Alert && self.alert.as_ref().map_or(false, |a| !a.actions.is_empty())
    }

    pub(super) fn demo_script(&mut self) {
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

    pub(super) fn paint_alert(&self, x0: f32, w: f32) {
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

    pub(super) fn alert_icon(&self, icon: AlertIcon, cx: f32, cy: f32, size: f32) {
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
}
