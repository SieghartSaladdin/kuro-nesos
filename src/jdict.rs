//! Japanese readings for kanji when Windows has no Japanese reading data: a small
//! MeCab-style analyzer (Viterbi over IPADIC words and connection costs). The dictionary
//! (`ja-reading.knd`, built by tools/make_ja_dict.py) is downloaded once from the project's
//! GitHub release into %LOCALAPPDATA%\KuroNesos and memory-mapped only while a song's
//! lyrics are romanized, so it costs no private memory.

use std::fs;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::{HSTRING, PCWSTR};
use windows::Foundation::Uri;
use windows::Storage::Streams::DataReader;
use windows::Web::Http::HttpClient;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Memory::{CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READONLY};

const URL: &str = "https://github.com/SieghartSaladdin/kuro-nesos/releases/download/ja-dict-1/ja-reading.knd";
const FILE: &str = "ja-reading.knd";

// entry flags (see tools/make_ja_dict.py)
const ATTACH: u8 = 1;
const PARTICLE: u8 = 2;
const KANA_SAME: u8 = 4;
/// Longest dictionary word looked up, in characters.
const MAX_WORD: usize = 16;

// unknown-word classes, same order as the table in the file
const KANJI: u8 = 0;
const HIRAGANA: u8 = 1;
const KATAKANA: u8 = 2;
const ALPHA: u8 = 3;
const NUMERIC: u8 = 4;
const SYMBOL: u8 = 5;
const SPACE: u8 = 6;

/// When the last download failed: retried at most every 10 minutes.
static LAST_FAIL: Mutex<Option<Instant>> = Mutex::new(None);

pub struct Dict {
    mapping: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    len: usize,
    n: usize,
    lsize: usize,
    index: usize,
    data: usize,
    matrix: usize,
    unk: [(u16, u16, i16); 8],
}

// the view is read-only and owned by this value
unsafe impl Send for Dict {}

#[derive(Clone, Copy)]
enum Src {
    Bos,
    Entry(u32),
    Unknown(u8),
}

struct Node {
    start: u32,
    end: u32,
    right: u16,
    total: i32,
    prev: u32,
    src: Src,
}

impl Dict {
    /// Opens the dictionary, downloading it first when it is not there yet.
    pub fn load() -> Option<Dict> {
        let path = crate::model::data_dir("LOCALAPPDATA").join(FILE);
        if let Some(d) = Dict::open(&path) {
            return Some(d);
        }
        if LAST_FAIL.lock().ok()?.is_some_and(|t| t.elapsed() < Duration::from_secs(600)) {
            return None;
        }
        if !download(&path) {
            crate::sys::log("jdict: dictionary download failed");
            *LAST_FAIL.lock().ok()? = Some(Instant::now());
            return None;
        }
        crate::sys::log("jdict: dictionary downloaded");
        Dict::open(&path)
    }

    fn open(path: &Path) -> Option<Dict> {
        let file = fs::File::open(path).ok()?;
        let len = file.metadata().ok()?.len() as usize;
        if len < 64 {
            return None;
        }
        unsafe {
            // the mapping keeps the file open after `file` is dropped
            let mapping = CreateFileMappingW(HANDLE(file.as_raw_handle()), None, PAGE_READONLY, 0, 0, PCWSTR::null()).ok()?;
            let view = MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0);
            if view.Value.is_null() {
                let _ = CloseHandle(mapping);
                return None;
            }
            let mut d = Dict { mapping, view, len, n: 0, lsize: 0, index: 0, data: 0, matrix: 0, unk: [(0, 0, 0); 8] };
            d.read_header().then_some(d)
        }
    }

    fn bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.view.Value as *const u8, self.len) }
    }

    /// Bounds-checked reads: a damaged file must never crash the app.
    fn get(&self, o: usize, n: usize) -> &[u8] {
        self.bytes().get(o..o.saturating_add(n)).unwrap_or(&[])
    }

    fn u16_at(&self, o: usize) -> u16 {
        self.get(o, 2).try_into().map_or(0, u16::from_le_bytes)
    }

    fn i16_at(&self, o: usize) -> i16 {
        self.get(o, 2).try_into().map_or(0, i16::from_le_bytes)
    }

    fn u32_at(&self, o: usize) -> usize {
        self.get(o, 4).try_into().map_or(0, u32::from_le_bytes) as usize
    }

    fn read_header(&mut self) -> bool {
        if self.get(0, 4) != b"KNJD" || self.u32_at(4) != 1 || self.u32_at(32) != self.len {
            return false;
        }
        self.n = self.u32_at(8);
        self.lsize = self.u16_at(12) as usize;
        let rsize = self.u16_at(14) as usize;
        self.index = self.u32_at(16);
        self.data = self.u32_at(20);
        self.matrix = self.u32_at(24);
        self.unk = std::array::from_fn(|i| {
            let o = 36 + 6 * i;
            (self.u16_at(o), self.u16_at(o + 2), self.i16_at(o + 4))
        });
        self.index + 4 * self.n <= self.data && self.matrix + 2 * self.lsize * rsize <= self.len
    }

    fn entry(&self, i: usize) -> usize {
        self.data + self.u32_at(self.index + 4 * i)
    }

    fn surface(&self, i: usize) -> &[u8] {
        let o = self.entry(i);
        self.get(o + 9, *self.get(o + 7, 1).first().unwrap_or(&0) as usize)
    }

    /// (left id, right id, cost)
    fn ids(&self, i: usize) -> (u16, u16, i16) {
        let o = self.entry(i);
        (self.u16_at(o), self.u16_at(o + 2), self.i16_at(o + 4))
    }

    /// (flags, katakana reading; empty when it equals the surface in katakana)
    fn reading_of(&self, i: usize) -> (u8, String) {
        let o = self.entry(i);
        let (flags, sl, rl) = match self.get(o + 6, 3) {
            [f, s, r] => (*f, *s as usize, *r as usize),
            _ => (0, 0, 0),
        };
        (flags, String::from_utf8_lossy(self.get(o + 9 + sl, rl)).into_owned())
    }

    /// First entry whose surface is >= `key` (entries are sorted by surface bytes).
    fn lower_bound(&self, key: &[u8]) -> usize {
        let (mut lo, mut hi) = (0, self.n);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.surface(mid) < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    fn conn(&self, right: u16, left: u16) -> i32 {
        self.i16_at(self.matrix + 2 * (right as usize * self.lsize + left as usize)) as i32
    }

    /// Katakana reading of `text`, words separated by spaces ("アノ スイヘイセン ガ ...").
    /// None when a kanji could not be read (it would come out as Chinese pinyin later).
    pub fn reading(&self, text: &str) -> Option<String> {
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain(std::iter::once(text.len())).collect();
        let chars: Vec<char> = text.chars().collect();
        let n = chars.len();
        if n == 0 || n > 400 {
            return None;
        }
        let bytes = text.as_bytes();
        let mut nodes = vec![Node { start: 0, end: 0, right: 0, total: 0, prev: u32::MAX, src: Src::Bos }];
        let mut ends: Vec<Vec<u32>> = vec![Vec::new(); n + 1];
        ends[0].push(0);
        let mut cands: Vec<(usize, u16, u16, i16, Src)> = Vec::new();
        for p in 0..n {
            if ends[p].is_empty() {
                continue;
            }
            cands.clear();
            for len in 1..=MAX_WORD.min(n - p) {
                let key = &bytes[bounds[p]..bounds[p + len]];
                let first = self.lower_bound(key);
                if first >= self.n || !self.surface(first).starts_with(key) {
                    break;
                }
                let mut i = first;
                while i < self.n && self.surface(i) == key {
                    let (l, r, c) = self.ids(i);
                    cands.push((p + len, l, r, c, Src::Entry(i as u32)));
                    i += 1;
                }
            }
            // unknown words, as MeCab's char.def for IPADIC: kanji and hiragana only when no
            // dictionary word starts here (1-2 characters), other scripts always (whole run)
            let c = class(chars[p]);
            let mut run = 1;
            while c != KANJI && p + run < n && class(chars[p + run]) == c {
                run += 1;
            }
            let (l, r, cost) = self.unk[c as usize];
            if matches!(c, KANJI | HIRAGANA) {
                if cands.is_empty() {
                    for len in 1..=run.min(2) {
                        cands.push((p + len, l, r, cost, Src::Unknown(c)));
                    }
                }
            } else {
                cands.push((p + run, l, r, cost, Src::Unknown(c)));
            }
            for &(end, l, r, cost, src) in &cands {
                let mut best = (i32::MAX, 0u32);
                for &pi in &ends[p] {
                    let prev = &nodes[pi as usize];
                    let t = prev.total + self.conn(prev.right, l) + cost as i32;
                    if t < best.0 {
                        best = (t, pi);
                    }
                }
                nodes.push(Node { start: p as u32, end: end as u32, right: r, total: best.0, prev: best.1, src });
                ends[end].push((nodes.len() - 1) as u32);
            }
        }
        let mut i = *ends[n].iter().min_by_key(|&&i| nodes[i as usize].total + self.conn(nodes[i as usize].right, 0))?;
        let mut path = Vec::new();
        while nodes[i as usize].prev != u32::MAX {
            path.push(i as usize);
            i = nodes[i as usize].prev;
        }

        let mut out = String::new();
        let mut prev_flags = 0u8;
        for &k in path.iter().rev() {
            let nd = &nodes[k];
            let surface = &text[bounds[nd.start as usize]..bounds[nd.end as usize]];
            let (flags, reading) = match nd.src {
                Src::Bos => continue,
                Src::Entry(e) => {
                    let (f, r) = self.reading_of(e as usize);
                    (f, if f & KANA_SAME != 0 || r.is_empty() { katakana(surface) } else { r })
                }
                Src::Unknown(c) => (0, if matches!(c, HIRAGANA | KATAKANA) { katakana(surface) } else { surface.to_string() }),
            };
            // particles are pronounced wa / e / o
            let reading = match (flags & PARTICLE != 0, surface) {
                (true, "は") => "ワ".to_string(),
                (true, "へ") => "エ".to_string(),
                (true, "を") => "オ".to_string(),
                _ => reading,
            };
            // auxiliaries, conjunctive particles and suffixes stay with their word; so do
            // particle chains (かな) and anything after a small ッ
            let glue = flags & ATTACH != 0 || (flags & PARTICLE != 0 && prev_flags & PARTICLE != 0) || out.ends_with('ッ') || !reading.chars().any(char::is_alphanumeric);
            if !out.is_empty() && !glue && !out.ends_with(' ') {
                out.push(' ');
            }
            out.push_str(&reading);
            prev_flags = flags;
        }
        (!out.chars().any(is_han)).then_some(out)
    }
}

impl Drop for Dict {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.mapping);
        }
    }
}

fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn class(c: char) -> u8 {
    match c as u32 {
        _ if is_han(c) || matches!(c, '々' | '〆' | 'ヶ') => KANJI,
        0x3041..=0x309F => HIRAGANA,
        0x30A0..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => KATAKANA,
        _ if c.is_whitespace() => SPACE,
        _ if c.is_numeric() => NUMERIC,
        _ if c.is_alphabetic() => ALPHA,
        _ => SYMBOL,
    }
}

fn katakana(s: &str) -> String {
    s.chars().map(|c| if matches!(c as u32, 0x3041..=0x3096) { char::from_u32(c as u32 + 0x60).unwrap_or(c) } else { c }).collect()
}

/// Fetches the dictionary from the GitHub release and stores it once it checks out.
fn download(path: &Path) -> bool {
    let fetch = || -> windows::core::Result<Vec<u8>> {
        let client = HttpClient::new()?;
        if let Ok(ua) = client.DefaultRequestHeaders().and_then(|h| h.UserAgent()) {
            let _ = ua.TryParseAdd(&HSTRING::from(crate::lyrics::USER_AGENT));
        }
        let buf = client.GetBufferAsync(&Uri::CreateUri(&HSTRING::from(URL))?)?.get()?;
        let mut bytes = vec![0u8; buf.Length()? as usize];
        DataReader::FromBuffer(&buf)?.ReadBytes(&mut bytes)?;
        Ok(bytes)
    };
    let Ok(bytes) = fetch() else { return false };
    let total = bytes.get(32..36).and_then(|b| b.try_into().ok()).map_or(0, u32::from_le_bytes) as usize;
    if bytes.get(..4) != Some(b"KNJD".as_slice()) || total != bytes.len() {
        return false;
    }
    let part = path.with_extension("part");
    fs::write(&part, &bytes).is_ok() && fs::rename(&part, path).is_ok()
}
