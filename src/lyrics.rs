//! Synced lyrics from LRCLIB (https://lrclib.net):
//! GET /api/search?track_name=..&artist_name=.. with an identifying User-Agent,
//! falling back to /api/search?q=.. and picking the result whose duration is closest.

use std::sync::{Arc, Mutex};

use windows::core::HSTRING;
use windows::Foundation::Uri;
use windows::Web::Http::HttpClient;
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use crate::json::{self, Json};

/// Identifies the app to LRCLIB and GitHub, as LRCLIB asks clients to.
pub const USER_AGENT: &str = "Kuro-Nesos/1.1 (https://github.com/SieghartSaladdin/kuro-nesos)";

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Loading,
    Ready,
    NotFound,
}

#[derive(Clone, Debug)]
pub struct Line {
    pub t: f64, // seconds
    pub text: String,
    /// Latin transliteration (romaji, pinyin, ...) when the line uses another script.
    pub roman: Option<String>,
}

#[derive(Clone)]
pub struct Lyrics {
    pub key: String,
    pub status: Status,
    pub lines: Vec<Line>,
}

impl Lyrics {
    pub fn empty() -> Self {
        Lyrics { key: String::new(), status: Status::Idle, lines: Vec::new() }
    }

    /// Index of the line active at `pos` seconds.
    pub fn index_at(&self, pos: f64) -> Option<usize> {
        if self.lines.is_empty() || pos < self.lines[0].t - 0.05 {
            return None;
        }
        let i = self.lines.partition_point(|l| l.t <= pos + 0.05);
        Some(i.saturating_sub(1))
    }
}

pub type Shared = Arc<Mutex<Lyrics>>;

pub fn shared() -> Shared {
    Arc::new(Mutex::new(Lyrics::empty()))
}

/// Strips decorations such as " (Official Video)", " [Lyrics]", " - Remastered".
pub fn clean_title(t: &str) -> String {
    let mut s = t.to_string();
    for sep in [" (", " [", " - "] {
        if let Some(i) = s.find(sep) {
            if i > 0 {
                s.truncate(i);
            }
        }
    }
    s.trim().to_string()
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn http_get(url: &str) -> Option<String> {
    let client = HttpClient::new().ok()?;
    if let Ok(h) = client.DefaultRequestHeaders() {
        if let Ok(ua) = h.UserAgent() {
            let _ = ua.TryParseAdd(&HSTRING::from(USER_AGENT));
        }
    }
    let uri = Uri::CreateUri(&HSTRING::from(url)).ok()?;
    let body = client.GetStringAsync(&uri).ok()?.get().ok()?;
    Some(body.to_string())
}

/// Parses "[mm:ss.xx] text" lines (multiple stamps per line allowed) plus "[offset:+/-ms]".
pub fn parse_lrc(src: &str) -> Vec<Line> {
    let mut offset = 0.0f64;
    let mut out = Vec::new();
    for raw in src.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let tag = &stripped[..end];
            rest = &stripped[end + 1..];
            if let Some(v) = tag.strip_prefix("offset:") {
                offset = v.trim().parse::<f64>().unwrap_or(0.0) / 1000.0;
                continue;
            }
            let mut parts = tag.splitn(2, ':');
            let (Some(m), Some(sec)) = (parts.next(), parts.next()) else { continue };
            if let (Ok(m), Ok(sec)) = (m.trim().parse::<f64>(), sec.trim().parse::<f64>()) {
                stamps.push(m * 60.0 + sec);
            }
        }
        let text = rest.trim().to_string();
        for t in stamps {
            out.push(Line { t: (t - offset).max(0.0), text: text.clone(), roman: None });
        }
    }
    out.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn best(results: &Json, duration: f64) -> Option<Vec<Line>> {
    let mut best: Option<(f64, &str)> = None;
    for r in results.as_arr() {
        let Some(synced) = r.get("syncedLyrics").and_then(Json::as_str) else { continue };
        if synced.trim().is_empty() {
            continue;
        }
        let d = r.get("duration").and_then(Json::as_f64).unwrap_or(0.0);
        let diff = if duration > 0.0 && d > 0.0 { (d - duration).abs() } else { 0.0 };
        if best.map_or(true, |(bd, _)| diff < bd) {
            best = Some((diff, synced));
        }
    }
    best.map(|(_, s)| parse_lrc(s)).filter(|l| !l.is_empty())
}

pub fn fetch(title: &str, artist: &str, duration: f64) -> Option<Vec<Line>> {
    let track = clean_title(title);
    let url = format!("https://lrclib.net/api/search?track_name={}&artist_name={}", enc(&track), enc(artist));
    if let Some(body) = http_get(&url) {
        if let Some(lines) = json::parse(&body).and_then(|j| best(&j, duration)) {
            return Some(lines);
        }
    }
    let q = format!("https://lrclib.net/api/search?q={}", enc(&format!("{} {}", track, artist)));
    let body = http_get(&q)?;
    json::parse(&body).and_then(|j| best(&j, duration))
}

/// Fills `roman` for every line written in a non-Latin script.
pub fn romanize(lines: &mut [Line]) {
    let sample: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    let r = crate::roman::Romanizer::new(&sample);
    for l in lines.iter_mut() {
        l.roman = r.romanize(&l.text);
    }
}

/// Starts a background fetch for `key` unless it is already current.
pub fn request(shared: &Shared, key: String, title: String, artist: String, duration: f64) {
    {
        let mut g = shared.lock().unwrap();
        if g.key == key {
            return;
        }
        *g = Lyrics { key: key.clone(), status: Status::Loading, lines: Vec::new() };
    }
    let shared = shared.clone();
    std::thread::spawn(move || {
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let res = fetch(&title, &artist, duration).map(|mut lines| {
            romanize(&mut lines);
            lines
        });
        let mut g = shared.lock().unwrap();
        if g.key != key {
            return;
        }
        match res {
            Some(lines) => {
                g.lines = lines;
                g.status = Status::Ready;
            }
            None => g.status = Status::NotFound,
        }
    });
}
