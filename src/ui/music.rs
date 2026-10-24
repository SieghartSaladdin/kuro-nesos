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

    pub(super) fn page_media(&self) {
        let g = self.g();
        let m = &self.media;
        self.art(44.0, 12.0, 62.0, 10.0);
        let has = m.has_session && !m.title.is_empty();
        let lyric_on = self.settings.lyrics && has;
        let status = self.lyrics.lock().unwrap().status.clone();
        if lyric_on && self.lyric_idx.is_some() {
            // with romaji on: romanized line large, the original script small underneath;
            // otherwise the current line and the next one
            let (cur, next) = {
                let l = self.lyrics.lock().unwrap();
                let i = self.lyric_idx.unwrap();
                let line = l.lines.get(i);
                let text = line.map(|x| x.text.clone()).unwrap_or_default();
                match line.and_then(|x| x.roman.clone()).filter(|_| self.settings.romaji) {
                    Some(roman) => (roman, text),
                    None => {
                        let next = l.lines.get(i + 1).map(|x| if self.settings.romaji { x.roman.clone().unwrap_or_else(|| x.text.clone()) } else { x.text.clone() });
                        (text, next.unwrap_or_default())
                    }
                }
            };
            let t = ease(self.lyric_change.elapsed().as_secs_f32() / 0.3);
            let a = g.alpha.get();
            g.alpha.set(a * t);
            let cur = if cur.is_empty() { "♪".to_string() } else { cur };
            let width = 262.0;
            let rise = (1.0 - t) * 5.0;
            if g.measure(&cur, 13.0, 700) <= width {
                // fits on one line: biggest size that fits, then next line and credit
                let cs = fit_size(g, &cur, width, 15.0, 13.0, 700);
                let ns = fit_size(g, &next, width, 12.0, 10.0, 400);
                g.text(&cur, 122.0, 33.0 + rise, cs, 700, Align::Left, WHITE, 1.0, width);
                g.text(&next, 122.0, 52.0, ns, 400, Align::Left, DIM, 1.0, width);
                g.alpha.set(a);
                let credit = if m.artist.is_empty() { m.title.clone() } else { format!("{} · {}", m.title, m.artist) };
                g.text(&credit, 122.0, 68.0, 10.5, 400, Align::Left, GREY, 1.0, width);
            } else {
                // long line: wrap onto two lines, next line underneath
                g.text_box(&cur, 122.0, 25.0 + rise, width, 38.0, 13.5, 700, WHITE, 1.0);
                let ns = fit_size(g, &next, width, 11.5, 10.0, 400);
                g.text(&next, 122.0, 70.0, ns, 400, Align::Left, DIM, 1.0, width);
                g.alpha.set(a);
            }
        } else if has {
            g.text(&m.title, 122.0, 37.0, 14.5, 700, Align::Left, WHITE, 1.0, 258.0);
            let sub = if lyric_on && status == LyricsStatus::Loading {
                "Loading lyrics".to_string()
            } else if lyric_on && status == LyricsStatus::NotFound {
                format!("{} · Lyrics not available", m.artist)
            } else {
                m.artist.clone()
            };
            g.text(&sub, 122.0, 57.0, 12.0, 400, Align::Left, GREY, 1.0, 258.0);
        } else {
            g.text("Tidak ada media", 122.0, 46.0, 14.5, 700, Align::Left, WHITE, 1.0, 258.0);
        }

        // timeline
        let pos = m.pos_now();
        g.text(&mmss(pos), 44.0, 91.0, 13.0, 400, Align::Left, 0x7A7A7A, 1.0, 60.0);
        g.text(&format!("-{}", mmss(m.duration - pos)), 376.0, 91.0, 13.0, 400, Align::Right, 0x7A7A7A, 1.0, 70.0);
        let (bx, bw) = (96.0, 222.0);
        g.fill_rrect(bx, 89.5, bw, 3.0, 1.5, 0x1F1F1F, 1.0);
        if m.duration > 0.0 {
            let f = (pos / m.duration).clamp(0.0, 1.0) as f32;
            g.fill_rrect(bx, 89.5, (bw * f).max(3.0), 3.0, 1.5, 0xD8D8D8, 1.0);
            let k = self.hov(Act::MediaSeek);
            if k > 0.0 {
                g.fill_circle(bx + bw * f, 91.0, 4.5 * k, WHITE, k);
            }
        }
        self.hit(bx, 83.0, bw, 16.0, Act::MediaSeek);

        // controls
        let fg = if has { WHITE } else { 0x7A7A7A };
        let lyr_color = if self.settings.lyrics && has { WHITE } else { 0x7A7A7A };
        g.icon(glyph::MESSAGE, 75.0, 117.0, 15.0, lyr_color, 1.0);
        if self.settings.lyrics && has {
            g.fill_circle(85.0, 109.5, 2.4, GREEN, 1.0);
        }
        self.hit(62.0, 105.0, 26.0, 24.0, Act::Lyrics);
        // romaji toggle chip
        let ron = self.settings.romaji;
        let rk = self.fx.level(Act::Romaji, ron as u8 as f32).max(self.hov(Act::Romaji));
        if rk > 0.0 {
            g.fill_rrect(93.0, 108.0, 26.0, 18.0, 9.0, 0x262626, rk);
        }
        g.text("Aa", 106.0, 117.0, 10.5, 700, Align::Center, if ron { WHITE } else { 0x7A7A7A }, 1.0, 26.0);
        if ron {
            g.fill_circle(117.0, 109.5, 2.4, GREEN, 1.0);
        }
        self.hit(91.0, 105.0, 30.0, 24.0, Act::Romaji);
        let tri = |cx: f32, dir: f32, s: f32| vec![(cx - dir * s * 0.55, 117.0 - s * 0.6), (cx - dir * s * 0.55, 117.0 + s * 0.6), (cx + dir * s * 0.65, 117.0)];
        // previous / next: double triangles
        for (cx, dir, act) in [(163.0, -1.0, Act::MediaPrev), (257.0, 1.0, Act::MediaNext)] {
            self.hover_ring(act, cx, 117.0, 14.0, 0.08);
            g.polygon(&tri(cx - dir * 5.0, dir, 9.0), fg, 0.8);
            g.polygon(&tri(cx + dir * 4.0, dir, 9.0), fg, 0.8);
            self.hit(cx - 14.0, 104.0, 28.0, 26.0, act);
        }
        self.hover_ring(Act::MediaPlay, 211.0, 117.0, 15.0, 0.08);
        // pause bars and play triangle cross-fade
        let pl = self.fx.level(Act::MediaPlay, m.playing as u8 as f32);
        if pl > 0.0 {
            g.fill_rrect(204.0, 108.5 + (1.0 - pl) * 3.0, 4.6, 17.0 - (1.0 - pl) * 6.0, 1.5, fg, pl);
            g.fill_rrect(213.4, 108.5 + (1.0 - pl) * 3.0, 4.6, 17.0 - (1.0 - pl) * 6.0, 1.5, fg, pl);
        }
        if pl < 1.0 {
            g.polygon(&[(205.0, 107.5), (205.0, 126.5), (220.5, 117.0)], fg, 1.0 - pl);
        }
        self.hit(196.0, 103.0, 30.0, 28.0, Act::MediaPlay);
    }
}
