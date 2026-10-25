//! Music: media session state, synced lyrics (with romanization) and the music page.

use super::*;

impl App {
    pub(super) fn update_media(&mut self) {
        let m = media::snapshot();
        if m.art_id != self.media.art_id || (m.art.is_some() != self.art.is_some()) {
            self.art = m.art.as_ref().and_then(|bytes| self.gfx.as_ref().and_then(|g| g.bitmap_from_bytes(bytes))).map(|b| (m.art_id, b));
            self.dirty = true;
        }
        if m.key() != self.media.key() || m.playing != self.media.playing {
            self.dirty = true;
        }
        if m.key() != self.media.key() {
            self.track_change = Instant::now();
        }
        self.media = m;
        if !self.media.title.is_empty() && self.settings.lyrics {
            if self.demo.lyrics && self.media.title == media::DEMO_TITLE {
                self.demo_lyrics();
            } else {
                let artist = self.media.artist.clone();
                lyrics::request(&self.lyrics, self.media.key(), self.media.title.clone(), artist, self.media.duration);
            }
        }
        let idx = {
            let l = self.lyrics.lock().unwrap();
            if l.status == LyricsStatus::Ready && l.key == self.media.key() {
                l.index_at(self.media.pos_now())
            } else {
                None
            }
        };
        if idx != self.lyric_idx {
            self.lyric_idx = idx;
            self.lyric_change = Instant::now();
        }
    }

    /// Current lyric line as displayed (romanized when romaji is on and available).
    pub(super) fn lyric_text(&self) -> String {
        let l = self.lyrics.lock().unwrap();
        self.lyric_idx
            .and_then(|i| l.lines.get(i))
            .map(|x| if self.settings.romaji { x.roman.clone().unwrap_or_else(|| x.text.clone()) } else { x.text.clone() })
            .unwrap_or_default()
    }

    pub(super) fn demo_lyrics(&mut self) {
        let key = self.media.key();
        let mut l = self.lyrics.lock().unwrap();
        if l.key == key {
            return;
        }
        const LINES: [&str; 12] = [
            "Langit jingga menyapa pelan",
            "Di ujung kota kita bertemu",
            "Lampu jalan mulai menyala",
            "Menghangatkan cerita lama",
            "Senja di ujung kota",
            "Takkan pernah sama",
            "Bila kau tak di sini",
            "Menemani langkahku",
            "Biar angin yang bercerita",
            "Tentang rindu yang tersisa",
            "Senja di ujung kota",
            "Selalu tentang kita",
        ];
        l.key = key;
        l.status = LyricsStatus::Ready;
        l.lines = (0..24).map(|i| lyrics::Line { t: 2.0 + i as f64 * 3.9, text: LINES[i % LINES.len()].to_string(), roman: None }).collect();
    }

    /// 0..0.06 scale bump of the album art right after the track changes.
    pub(super) fn cover_bump(&self) -> f32 {
        let e = self.track_change.elapsed().as_secs_f32();
        if e < 0.45 {
            (e / 0.45 * std::f32::consts::PI).sin() * 0.06
        } else {
            0.0
        }
    }

    /// Top dock: big cover on the left; title, lyric and controls on the right; the
    /// progress line runs along the bottom.
    pub(super) fn page_media(&self) {
        let g = self.g();
        let m = &self.media;
        let has = m.has_session && !m.title.is_empty();
        let s = 100.0 * (1.0 + self.cover_bump());
        self.art(66.0 - s / 2.0, 64.0 - s / 2.0, s, 16.0);
        let (x, lw) = (130.0, 274.0);
        if has {
            g.text(&m.title, x, 21.0, 14.5, 700, Align::Left, WHITE, 1.0, 228.0);
            g.text(&m.artist, x, 38.0, 11.5, 400, Align::Left, GREY, 1.0, 228.0);
        }
        let lyric_on = self.settings.lyrics && has;
        let status = self.lyrics.lock().unwrap().status.clone();
        match self.lyric_pair().filter(|_| lyric_on) {
            Some((cur, sub)) => {
                let t = ease(self.lyric_change.elapsed().as_secs_f32() / 0.3);
                let a = g.alpha.get();
                g.alpha.set(a * t);
                let rise = (1.0 - t) * 5.0;
                if cur.is_empty() {
                    g.icon(glyph::MUSIC, x + 9.0, 60.0 + rise, 18.0, WHITE, 1.0);
                } else if g.measure(&cur, 13.0, 700) <= lw {
                    let cs = fit_size(g, &cur, lw, 15.5, 13.0, 700);
                    g.text(&cur, x, 60.0 + rise, cs, 700, Align::Left, WHITE, 1.0, lw);
                } else {
                    g.text_box(&cur, x, 47.0 + rise, lw, 30.0, 12.0, 700, WHITE, 1.0);
                }
                g.text(&sub, x, 83.0, 10.5, 400, Align::Left, DIM, 1.0, lw);
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
                g.text(note, x, 62.0, 11.5, 400, Align::Left, DIM, 1.0, lw);
            }
            None => g.text("Nothing playing", x, 56.0, 14.5, 700, Align::Left, 0x8A8A8A, 1.0, lw),
        }

        // previous / play / next, with time, romaji and lyrics on the right
        let cy = 107.0;
        let fg = if has { WHITE } else { 0x7A7A7A };
        let tri = |cx: f32, dir: f32, s: f32| vec![(cx - dir * s * 0.55, cy - s * 0.6), (cx - dir * s * 0.55, cy + s * 0.6), (cx + dir * s * 0.65, cy)];
        for (cx, dir, act) in [(141.0, -1.0, Act::MediaPrev), (205.0, 1.0, Act::MediaNext)] {
            self.hover_ring(act, cx, cy, 12.0, 0.08);
            let cx = cx + dir * 2.0 * self.hov(act);
            g.polygon(&tri(cx - dir * 4.5, dir, 8.0), fg, 0.8);
            g.polygon(&tri(cx + dir * 3.5, dir, 8.0), fg, 0.8);
            self.hit(cx - 13.0, cy - 12.0, 26.0, 24.0, act);
        }
        self.play_button(173.0, cy, 11.5, has);
        let pos = m.pos_now();
        if m.duration > 0.0 {
            g.text(&format!("{} / {}", mmss(pos), mmss(m.duration)), 330.0, cy, 11.0, 400, Align::Right, GREY, 1.0, 110.0);
        }
        let ron = self.settings.romaji;
        let rk = self.fx.level(Act::Romaji, ron as u8 as f32).max(self.hov(Act::Romaji));
        if rk > 0.0 {
            g.fill_rrect(340.0, cy - 9.0, 26.0, 18.0, 9.0, 0x262626, rk);
        }
        g.text("Aa", 353.0, cy, 10.5, 700, Align::Center, if ron { AMBER } else { 0x7A7A7A }, 1.0, 26.0);
        self.hit(338.0, cy - 12.0, 30.0, 24.0, Act::Romaji);
        let lyr = self.settings.lyrics && has;
        g.icon(glyph::MESSAGE, 387.0, cy, 14.0, if lyr { WHITE } else { 0x7A7A7A }, 1.0);
        if lyr {
            g.fill_circle(396.0, cy - 7.0, 2.2, AMBER, 1.0);
        }
        self.hit(374.0, cy - 12.0, 28.0, 24.0, Act::Lyrics);

        // progress along the bottom, edge to edge
        let (bx, bw, by) = (16.0, 388.0, 125.0);
        g.fill_rrect(bx, by, bw, 3.0, 1.5, 0x1F1F1F, 1.0);
        if m.duration > 0.0 {
            let f = (pos / m.duration).clamp(0.0, 1.0) as f32;
            g.fill_rrect(bx, by, (bw * f).max(3.0), 3.0, 1.5, AMBER, 1.0);
            let k = self.hov(Act::MediaSeek);
            if k > 0.0 {
                g.fill_circle(bx + bw * f, by + 1.5, 4.5 * k, WHITE, k);
            }
        }
        self.hit(bx, by - 7.0, bw, 16.0, Act::MediaSeek);
    }
}
