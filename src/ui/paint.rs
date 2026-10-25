//! Painting: renderer access and recovery, hit areas, shared controls and the compact views.

use super::*;

/// The music equalizer: one bar per band each side of the bass bar in the middle.
const EQ_BARS: usize = crate::audio::BANDS * 2 - 1;
const EQ_STEP: f32 = 2.6;
/// Width of the equalizer strip.
pub(super) const EQ_W: f32 = (EQ_BARS - 1) as f32 * EQ_STEP + 1.8;

impl App {
    /// Renderer; painting only happens while it exists (see `render`).
    pub(super) fn g(&self) -> &Gfx {
        self.gfx.as_ref().expect("renderer available while painting")
    }

    /// Rebuilds the D3D/D2D/DComp stack after a device loss (retries every 0.5 s).
    pub(super) fn recover_renderer(&mut self) {
        let Some(t) = self.gfx_lost else { return };
        // the old DirectComposition target must be released before a new one can bind the window
        self.gfx = None;
        self.thumbs.borrow_mut().clear();
        if t.elapsed() < Duration::from_millis(500) {
            return;
        }
        match Gfx::new(self.hwnd, self.px.0, self.px.1, self.scale) {
            Ok(g) => {
                self.gfx = Some(g);
                self.gfx_lost = None;
                self.art = None;
                self.region = (0, 0, 0, 0);
                self.dirty = true;
                sys::log("render: device recreated");
            }
            Err(e) => {
                self.gfx_lost = Some(Instant::now());
                sys::log(&format!("render: recreate failed: {:?}", e));
            }
        }
    }

    pub(super) fn hit(&self, x: f32, y: f32, w: f32, h: f32, act: Act) {
        let (ox, oy) = self.off.get();
        self.hits.borrow_mut().push(Hit { x: x + ox, y: y + oy, w, h, act });
    }

    pub(super) fn origin(&self, x: f32, y: f32) {
        self.off.set((x, y));
        self.g().set_offset(x, y);
    }

    /// Eased hover amount (0..1) of a control: highlights fade in and out instead of popping.
    pub(super) fn hov(&self, act: Act) -> f32 {
        self.fx.hover(act)
    }

    /// Scale of a control right after it was clicked (a short squeeze and release).
    pub(super) fn squeeze(&self, act: Act) -> f32 {
        1.0 - 0.07 * self.fx.press(act)
    }

    /// Faint highlight circle that grows in while a control is hovered.
    pub(super) fn hover_ring(&self, act: Act, cx: f32, cy: f32, r: f32, a: f32) {
        let h = self.hov(act);
        if h > 0.0 {
            self.g().fill_circle(cx, cy, r * (0.7 + 0.3 * h) * self.squeeze(act), WHITE, a * h);
        }
    }

    pub(super) fn button(&self, x: f32, y: f32, w: f32, h: f32, r: f32, bg: u32, a: f32, act: Act) {
        let g = self.g();
        let s = self.squeeze(act);
        let (bw, bh) = (w * s, h * s);
        let (bx, by) = (x + (w - bw) / 2.0, y + (h - bh) / 2.0);
        g.fill_rrect(bx, by, bw, bh, r * s, bg, a);
        let k = self.hov(act);
        if k > 0.0 {
            g.fill_rrect(bx, by, bw, bh, r * s, WHITE, 0.08 * k);
        }
        self.hit(x, y, w, h, act);
    }

    pub(super) fn round_button(&self, cx: f32, cy: f32, r: f32, bg: u32, glyph: u16, size: f32, fg: u32, act: Act) {
        let g = self.g();
        let s = self.squeeze(act) * (1.0 + 0.06 * self.hov(act));
        g.fill_circle(cx, cy, r * s, bg, 1.0);
        let k = self.hov(act);
        if k > 0.0 {
            g.fill_circle(cx, cy, r * s, WHITE, 0.1 * k);
        }
        g.icon(glyph, cx, cy, size * s, fg, 1.0);
        self.hit(cx - r, cy - r, r * 2.0, r * 2.0, act);
    }

    /// Music indicator: thin bars mirrored around a centre line, bass in the middle and treble
    /// at both ends, each following its band of what is playing (or a steady loop when the
    /// output cannot be read). `x` is the left edge of the `EQ_W`-wide strip.
    pub(super) fn eq_bars(&self, x: f32, cy: f32, color: u32) {
        let t = self.started.elapsed().as_secs_f32();
        let live = self.beat.live();
        let mid = EQ_BARS / 2;
        for i in 0..EQ_BARS {
            let band = i.abs_diff(mid);
            let h = if !self.media.playing {
                2.0
            } else if live {
                let kick = if band == 0 { self.beat.pulse * 0.15 } else { 0.0 };
                // treble at the ends stays a little shorter, so the shape reads from the middle
                2.0 + (self.beat.bands[band] + kick).min(1.0) * 18.0 * (1.0 - band as f32 * 0.08)
            } else {
                let phase = t * (5.0 + band as f32 * 1.3) + band as f32 * 1.7;
                3.0 + (phase.sin() * 0.5 + 0.5) * 12.0 * (1.0 - band as f32 * 0.12)
            };
            self.g().fill_rrect(x + i as f32 * EQ_STEP, cy - h / 2.0, 1.8, h, 0.9, color, 1.0);
        }
    }

    pub(super) fn art(&self, x: f32, y: f32, s: f32, r: f32) {
        let g = self.g();
        if let Some((_, bmp)) = &self.art {
            g.bitmap_rounded(bmp, x, y, s, s, r);
        } else {
            g.fill_rrect(x, y, s, s, r, 0x262626, 1.0);
            g.icon(glyph::MUSIC, x + s / 2.0, y + s / 2.0, s * 0.42, 0x7A7A7A, 1.0);
        }
    }

    /// Draws one frame; returns false if the GPU device was lost.
    pub(super) fn render(&self) -> bool {
        if std::env::var_os("KURO_DEBUG_FPS").is_some() {
            thread_local!(static FPS: Cell<(u32, Option<Instant>)> = const { Cell::new((0, None)) });
            FPS.with(|f| {
                let (n, start) = f.get();
                let start = start.unwrap_or_else(Instant::now);
                if start.elapsed() >= Duration::from_secs(1) {
                    sys::log(&format!("fps {} ({:?})", n + 1, self.shown));
                    f.set((0, Some(Instant::now())));
                } else {
                    f.set((n + 1, Some(start)));
                }
            });
        }
        let Some(g) = self.gfx.as_ref() else { return true };
        self.hits.borrow_mut().clear();
        g.begin();
        g.alpha.set(1.0);
        let fade = self.ghost < 0.999;
        if fade {
            g.push_fade(self.ghost);
        }
        let (w, h, r) = (self.w.v, self.h.v, self.r.v);
        let (x0, y0) = self.shape_pos(w, h);
        self.top.set(y0);
        let geom = match self.settings.dock {
            Dock::Top => g.notch_geometry(self.win.0 / 2.0, w, h, r, SHOULDER),
            Dock::Left => g.side_geometry(0.0, y0, w, h, r, SHOULDER, true),
            Dock::Right => g.side_geometry(self.win.0, y0, w, h, r, SHOULDER, false),
        };
        if let Some(geom) = geom {
            g.fill_geometry(&geom, 0x000000, 1.0);
            if self.shown == Mode::Mascot {
                self.paint_cat_outside(x0, y0, w, h);
            }
            g.push_clip(&geom);
            g.alpha.set(ease(self.content_a));
            if self.vertical() {
                match self.shown {
                    Mode::Expanded => self.paint_expanded_v(x0),
                    Mode::Alert => self.paint_alert_v(x0, w, h),
                    Mode::Live => self.paint_live_v(x0, w, h),
                    Mode::Lyrics => self.paint_lyrics_v(x0, w, h),
                    Mode::Timer => self.paint_timer_v(x0, w),
                    Mode::Mascot => self.paint_cat_face(x0, y0, w, h),
                    Mode::Collapsed => self.paint_collapsed_v(x0, w, h),
                    Mode::Minimal => {}
                }
            } else {
                match self.shown {
                    Mode::Expanded => self.paint_expanded(x0),
                    Mode::Alert => self.paint_alert(x0, w),
                    Mode::Live => self.paint_live(x0, w, false),
                    Mode::Lyrics => self.paint_live(x0, w, true),
                    Mode::Timer => self.paint_timer_live(x0, w),
                    Mode::Mascot => self.paint_cat_face(x0, y0, w, h),
                    Mode::Collapsed => self.paint_collapsed(x0, w, h),
                    Mode::Minimal => {}
                }
            }
            g.alpha.set(1.0);
            self.origin(0.0, 0.0);
            g.pop_clip();
        }
        if fade {
            g.pop_clip();
        }
        g.end()
    }

    pub(super) fn paint_collapsed(&self, x0: f32, w: f32, h: f32) {
        self.origin(x0, self.top.get());
        let (_, time) = date_strings();
        let tw = self.g().measure(&time, 14.5, 700);
        self.clock(w / 2.0 + tw / 2.0, h / 2.0, 14.5, WHITE);
    }

    pub(super) fn paint_live(&self, x0: f32, w: f32, lyric: bool) {
        self.origin(x0, self.top.get());
        let g = self.g();
        self.art(9.0, 5.0, 20.0, 6.0);
        self.eq_bars(w - 12.0 - EQ_W, 15.0, AMBER);
        if lyric {
            let text = self.lyric_text();
            let t = (self.lyric_change.elapsed().as_secs_f32() / 0.25).min(1.0);
            let y = 15.0 + (1.0 - ease(t)) * 6.0;
            g.alpha.set(g.alpha.get() * ease(t));
            if text.is_empty() {
                // instrumental break: a note icon as big as the lyric line
                g.icon(glyph::MUSIC, w / 2.0, y, 15.0, WHITE, 1.0);
            } else {
                let size = fit_size(g, &text, w - 112.0, 13.0, 10.5, 600);
                g.text(&text, w / 2.0, y, size, 600, Align::Center, WHITE, 1.0, w - 112.0);
            }
            g.alpha.set(ease(self.content_a));
        }
    }

    pub(super) fn paint_timer_live(&self, x0: f32, w: f32) {
        self.origin(x0, self.top.get());
        let g = self.g();
        let total = self.pomo.mode.secs();
        let frac = (self.pomo.remaining / total) as f32;
        let color = self.pomo_color();
        g.stroke_circle(17.0, 15.0, 7.5, 0x2A2416, 1.0, 2.4);
        g.arc(17.0, 15.0, 7.5, -90.0, 360.0 * frac, color, 1.0, 2.4);
        g.text(if self.pomo.mode == PMode::Focus { "Focus" } else { "Break" }, 32.0, 15.0, 12.0, 600, Align::Left, GREY, 1.0, 80.0);
        g.text(&mmss(self.pomo.remaining), w - 14.0, 15.0, 14.5, 700, Align::Right, color, 1.0, 80.0);
    }
}
