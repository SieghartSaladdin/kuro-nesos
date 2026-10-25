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
        if let Some(p) = s.file {
            // a new screenshot: straight onto the Shelf (newest first)
            if self.settings.screenshots {
                self.shelf.add_path(p);
                self.push(Alert::simple(AlertIcon::Glyph(glyph::PICTURE, AMBER), "Screenshot", "Saved to Shelf", AMBER));
            }
            return;
        }
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
                2 => self.sys_alert(SysAlert { icon: glyph::BLUETOOTH, title: "Connected".into(), value: "Galaxy Buds2 Pro".into(), color: BLUE, file: None }),
                3 => self.sys_alert(SysAlert { icon: glyph::WIFI, title: "Connected".into(), value: "Rumah_5G".into(), color: GREEN, file: None }),
                4 => self.push(Alert::simple(AlertIcon::Glyph(glyph::KEYBOARD, WHITE), "Keyboard", "ID-ID", WHITE)),
                5 => self.sys_alert(SysAlert { icon: glyph::MESSAGE, title: "WhatsApp".into(), value: "Dina: Nanti malam jadi ngopi?".into(), color: sys::app_color("WhatsApp"), file: None }),
                6 => self.push_action_alert(AlertIcon::Glyph(glyph::DROP, CYAN), "Time to Drink", &format!("Stay hydrated! {} ml", self.water.each()), BLUE, vec![("Drank".into(), AlertAction::Drank), ("Snooze".into(), AlertAction::Snooze)]),
                7 => {
                    if self.tasks.is_empty() {
                        self.tasks.push(Task { done: false, start: Some(model::now().minutes), end: None, title: "Rapat tim mingguan".into(), alerted: true, later_until: None });
                    }
                    let title = self.tasks[0].title.clone();
                    self.push_action_alert(AlertIcon::Glyph(glyph::CHECKLIST, GREEN), "Task Time!", &title, GREEN, vec![("Start".into(), AlertAction::TaskStart(0)), ("Later".into(), AlertAction::TaskLater(0))]);
                }
                8 => {
                    let mut a = Alert::simple(AlertIcon::Glyph(glyph::STOPWATCH, AMBER), "Focus Time!", "Back to work, 25 min", AMBER);
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
            self.alert_badge(a, 22.0, 17.5, 11.0);
            g.text(&a.title, 42.0, 17.5, 13.0, 600, Align::Left, WHITE, 1.0, w * 0.5);
            g.text(&a.value, w - 20.0, 17.5, 13.0, 400, Align::Right, a.color, 1.0, w * 0.42);
            return;
        }
        // card: badge, title + body, buttons on the right, time line along the bottom
        let h = if a.actions.is_empty() { 60.0 } else { 80.0 };
        self.alert_badge(a, 34.0, 30.0, 18.0);
        g.text(&a.title, 62.0, 22.0, 13.5, 700, Align::Left, WHITE, 1.0, w - 120.0);
        g.text("now", w - 22.0, 22.0, 10.5, 400, Align::Right, DIM, 1.0, 40.0);
        g.text(&a.value, 62.0, 40.0, 12.0, 400, Align::Left, 0xB8B8B8, 1.0, w - 84.0);
        let mut right = w - 18.0;
        for (i, (label, _)) in a.actions.iter().enumerate().rev() {
            let bw = g.measure(label, 11.5, 600) + 26.0;
            self.alert_button(a, i, label, right - bw, 51.0, bw);
            right -= bw + 6.0;
        }
        self.alert_timer(a, 24.0, h - 4.0, w - 48.0);
    }

    /// Rounded square in the alert's colour with its icon; it pops in when the alert arrives.
    pub(super) fn alert_badge(&self, a: &Alert, cx: f32, cy: f32, half: f32) {
        let g = self.g();
        let e = a.born.map_or(1.0, |b| b.elapsed().as_secs_f32());
        let pop = 1.0 + 0.22 * (e.min(0.35) / 0.35 * std::f32::consts::PI).sin();
        let s = half * pop;
        g.fill_rrect(cx - s, cy - s, 2.0 * s, 2.0 * s, s * 0.55, a.color, 0.16);
        g.stroke_rrect(cx - s, cy - s, 2.0 * s, 2.0 * s, s * 0.55, a.color, 0.35, 1.0, false);
        self.alert_icon(a.icon, cx, cy, s * 0.62);
    }

    /// Primary action filled with the alert colour, the others dark with a hairline.
    pub(super) fn alert_button(&self, a: &Alert, i: usize, label: &str, x: f32, y: f32, w: f32) {
        let g = self.g();
        let primary = i == 0;
        let act = Act::Alert(i);
        self.button(x, y, w, 22.0, 11.0, if primary { a.color } else { CARD }, 1.0, act);
        if !primary {
            g.stroke_rrect(x, y, w, 22.0, 11.0, HAIR, 1.0, 1.0, false);
        }
        let fg = if primary { mix(a.color, 0x000000, 0.82) } else { WHITE };
        g.text(label, x + w / 2.0, y + 11.0, 11.5, 600, Align::Center, fg, 1.0, w);
    }

    /// Thin line in the alert colour that shrinks until the alert goes away.
    pub(super) fn alert_timer(&self, a: &Alert, x: f32, y: f32, w: f32) {
        if let Some(b) = a.born {
            let f = 1.0 - (b.elapsed().as_secs_f32() / a.ttl.as_secs_f32()).min(1.0);
            let g = self.g();
            g.fill_rrect(x, y, w, 2.0, 1.0, 0x1C1C1C, 1.0);
            g.fill_rrect(x, y, w * f, 2.0, 1.0, a.color, 0.7);
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
