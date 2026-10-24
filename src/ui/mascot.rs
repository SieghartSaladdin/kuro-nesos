//! Mascot: the black cat that naps in the notch while you are away and waves when you return.

use super::*;
use windows::Foundation::Numerics::Matrix3x2;

impl App {
    pub(super) fn update_mascot(&mut self, now: Instant) {
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

    pub(super) fn mascot_phase(&self) -> (f32, f32, f32) {
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

    /// Local frame of the cat: centre, length along the edge, depth away from it, and the
    /// rotation that makes the cat's "up" point at the screen edge the notch hangs from.
    fn cat_frame(&self, x0: f32, y0: f32, w: f32, h: f32) -> (f32, f32, f32, f32, f32) {
        let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0);
        match self.settings.dock {
            Dock::Top => (cx, cy, w, h, 0.0),
            Dock::Left => (cx, cy, h, w, -90.0),
            Dock::Right => (cx, cy, h, w, 90.0),
        }
    }

    /// Whiskers, front paws hanging over the lower edge, the swishing tail and sleepy "z"s.
    /// Drawn after the notch shape is filled but outside its clip, so they stick out of it.
    pub(super) fn paint_cat_outside(&self, x0: f32, y0: f32, w: f32, h: f32) {
        let g = self.g();
        let a = ease(self.content_a);
        if a <= 0.01 {
            return;
        }
        let (_, open, happy) = self.mascot_phase();
        let awake = open.max(happy);
        let (cx, cy, fw, fh, deg) = self.cat_frame(x0, y0, w, h);
        let frame = rotation(deg, cx, cy);
        let t = self.started.elapsed().as_secs_f32();
        let bottom = cy + fh / 2.0;
        g.set_transform(frame);

        // tail: leaves the notch near its far end, hangs down and swishes (faster when awake)
        let sway = (t * (1.1 + 2.2 * awake)).sin();
        let (bx, by) = (cx + fw / 2.0 - 12.0, bottom - 5.0);
        let tail: Vec<(f32, f32)> = (0..=14)
            .map(|i| {
                let u = i as f32 / 14.0;
                (bx + 9.0 * u + sway * 10.0 * u * u, by + 21.0 * u - 7.0 * u * u * u)
            })
            .collect();
        g.polyline(&tail, RIM, 0.9 * a, 6.2);
        g.polyline(&tail, FUR, a, 4.4);

        // whiskers, twitching a little when awake
        let twitch = awake * (t * 7.0).sin() * 1.2;
        for s in [-1.0f32, 1.0] {
            for k in 0..3 {
                let k = k as f32;
                let y = cy + 3.5 + k * 2.6;
                let tip = (cx + s * (fw / 2.0 + 12.0 - k * 2.0), y - 3.0 + k * 3.4 + twitch * s * (1.0 - k * 0.5));
                g.line(cx + s * 23.0, y, tip.0, tip.1, 0xD6D6D6, 0.75 * a, 1.0);
            }
        }

        // front paws over the edge; the right one waves while the cat is happy
        for (i, px) in [cx - 15.0, cx + 15.0].into_iter().enumerate() {
            let waving = i == 1 && happy > 0.01;
            let lift = if waving { 4.0 * happy } else { 0.0 };
            if waving {
                g.set_transform(mul(rotation((t * 9.0).sin() * 24.0 * happy, px, bottom - 2.0), frame));
            }
            let py = bottom + 1.5 - lift;
            g.fill_ellipse(px, py, 7.6, 5.6, RIM, 0.9 * a);
            g.fill_ellipse(px, py, 6.6, 4.7, FUR, a);
            for dx in [-2.2, 2.2] {
                g.line(px + dx, py + 1.6, px + dx, py + 3.8, 0x4A4A4A, a, 1.0);
            }
            if waving {
                g.set_transform(frame);
            }
        }

        // sleeping: little "z"s drift away from the notch
        if awake < 0.05 {
            for i in 0..3 {
                let p = (t * 0.45 + i as f32 / 3.0).fract();
                let fade = (p * 4.0).min(1.0) * (1.0 - p);
                g.text("z", cx + fw / 2.0 + 2.0 + p * 16.0, bottom - 2.0 + p * 12.0, 7.5 + p * 5.0, 700, Align::Center, 0xC8C8C8, a * fade * 0.85, 20.0);
            }
        }
        self.origin(0.0, 0.0);
    }

    /// The face inside the notch: ears tucked under the screen edge, amber eyes that follow
    /// the cursor and blink (closed while asleep, happy arcs when it greets you), nose and mouth.
    pub(super) fn paint_cat_face(&self, x0: f32, y0: f32, w: f32, h: f32) {
        let g = self.g();
        let (_, open, happy) = self.mascot_phase();
        let (cx, cy, fw, fh, deg) = self.cat_frame(x0, y0, w, h);
        let t = self.started.elapsed().as_secs_f32();
        let top = cy - fh / 2.0;
        g.set_transform(rotation(deg, cx, cy));

        // ears (their tips disappear under the edge), breathing slowly while asleep
        let breath = if open.max(happy) < 0.05 { (t * 1.8).sin() * 0.8 } else { 0.0 };
        for s in [-1.0f32, 1.0] {
            let ex = cx + s * (fw / 2.0 - 15.0);
            let base = top + 12.0 + breath;
            g.polygon(&[(ex - 10.0, base), (ex + 10.0, base), (ex + s * 2.0, top - 6.0)], 0x1A1A1A, 1.0);
            g.polygon(&[(ex - 5.5, base - 1.5), (ex + 5.5, base - 1.5), (ex + s * 1.2, top - 1.0)], 0x40252C, 1.0);
        }

        // where the cursor is, in the cat's own frame
        let (mx, my) = self.cursor_logical();
        let r = (-deg).to_radians();
        let (dx, dy) = (mx - cx, my - cy);
        let (lx, ly) = (dx * r.cos() - dy * r.sin(), dx * r.sin() + dy * r.cos());
        let look = ((lx / 70.0).clamp(-1.0, 1.0) * 2.2, (ly / 50.0).clamp(-1.0, 1.0) * 1.3);

        // a quick blink every few seconds while awake
        let p = t % 3.7;
        let blink = if p < 0.14 { 1.0 - 0.9 * (p / 0.14 * std::f32::consts::PI).sin() } else { 1.0 };
        let ey = cy - 1.5;
        for s in [-1.0f32, 1.0] {
            let ex = cx + s * 17.0;
            if happy > 0.01 {
                let arc: Vec<(f32, f32)> = (0..=8)
                    .map(|i| {
                        let u = i as f32 / 8.0;
                        (ex - 5.0 + 10.0 * u, ey + 2.0 - 4.5 * (u * std::f32::consts::PI).sin())
                    })
                    .collect();
                g.polyline(&arc, AMBER, happy, 2.2);
            }
            if open > 0.01 {
                let ry = 6.2 * open * blink;
                g.glow(ex, ey, 11.0, 9.0, AMBER, 0.2 * open);
                g.fill_ellipse(ex, ey, 6.4, ry, AMBER, open);
                g.fill_ellipse(ex + look.0, ey + look.1 * open, 1.7, ry * 0.86, 0x101010, open);
                g.fill_circle(ex + look.0 + 2.0, ey - 2.4 * open, 1.1, WHITE, 0.9 * open * blink);
            }
            let closed = 1.0 - open - happy;
            if closed > 0.01 {
                let arc: Vec<(f32, f32)> = (0..=8)
                    .map(|i| {
                        let u = i as f32 / 8.0;
                        (ex - 5.0 + 10.0 * u, ey + 2.6 * (u * std::f32::consts::PI).sin())
                    })
                    .collect();
                g.polyline(&arc, 0x8E8E8E, closed, 1.8);
            }
        }

        // nose, "ω" mouth and a blush when it is happy to see you
        g.polygon(&[(cx - 2.6, cy + 4.6), (cx + 2.6, cy + 4.6), (cx, cy + 7.2)], NOSE, 1.0);
        for s in [-1.0f32, 1.0] {
            g.polyline(&[(cx, cy + 7.6), (cx + s * 2.4, cy + 9.8), (cx + s * 4.8, cy + 7.8)], 0x9A9A9A, 1.0, 1.2);
            g.fill_ellipse(cx + s * 27.0, cy + 5.0, 4.2, 2.2, NOSE, 0.35 * happy);
        }
        self.origin(0.0, 0.0);
    }
}

const FUR: u32 = 0x050505;
/// Faint rim light so the black cat stays visible on dark wallpapers.
const RIM: u32 = 0x3C3C3C;
const AMBER: u32 = 0xF5C542;
const NOSE: u32 = 0xE58FA0;

/// `a` then `b` (row-vector convention, as Direct2D uses).
fn mul(a: Matrix3x2, b: Matrix3x2) -> Matrix3x2 {
    Matrix3x2 {
        M11: a.M11 * b.M11 + a.M12 * b.M21,
        M12: a.M11 * b.M12 + a.M12 * b.M22,
        M21: a.M21 * b.M11 + a.M22 * b.M21,
        M22: a.M21 * b.M12 + a.M22 * b.M22,
        M31: a.M31 * b.M11 + a.M32 * b.M21 + b.M31,
        M32: a.M31 * b.M12 + a.M32 * b.M22 + b.M32,
    }
}
