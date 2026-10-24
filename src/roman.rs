//! Offline romanization of lyrics (romaji, pinyin, Cyrillic -> Latin, Hangul -> Latin, ...).
//! Japanese text is first read into kana with Windows' JapanesePhoneticAnalyzer, so kanji get
//! Japanese readings instead of Chinese pinyin; everything then goes through the ICU
//! transliterator that ships with Windows (icu.dll, "Any-Latin").

use windows::core::HSTRING;
use windows::Globalization::JapanesePhoneticAnalyzer;
use windows::Win32::Globalization::*;

/// True when the text contains letters outside the Latin script.
pub fn needs_romanization(s: &str) -> bool {
    s.chars().any(|c| c.is_alphabetic() && (c as u32) > 0x24F && !(0x1E00..=0x1EFF).contains(&(c as u32)))
}

fn has_kana(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9D))
}

fn has_cjk(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF))
}

/// An ICU transliterator handle (icu.dll, available since Windows 10 1903).
struct Translit(*mut *mut core::ffi::c_void);

impl Translit {
    fn open(id: &str) -> Option<Self> {
        let w: Vec<u16> = id.encode_utf16().collect();
        let mut status = U_ZERO_ERROR;
        let t = unsafe { utrans_openU(w.as_ptr(), w.len() as i32, UTRANS_FORWARD, std::ptr::null(), 0, std::ptr::null_mut(), &mut status) };
        (status.0 <= 0 && !t.is_null()).then_some(Translit(t))
    }

    fn apply(&self, s: &str) -> String {
        let src: Vec<u16> = s.encode_utf16().collect();
        let cap = src.len() * 4 + 64;
        let mut buf = vec![0u16; cap];
        buf[..src.len()].copy_from_slice(&src);
        let mut len = src.len() as i32;
        let mut limit = len;
        let mut status = U_ZERO_ERROR;
        unsafe { utrans_transUChars(self.0 as *const *const _, buf.as_mut_ptr(), &mut len, cap as i32, 0, &mut limit, &mut status) };
        if status.0 > 0 {
            return s.to_string();
        }
        String::from_utf16_lossy(&buf[..len.max(0) as usize])
    }
}

impl Drop for Translit {
    fn drop(&mut self) {
        unsafe { utrans_close(self.0) }
    }
}

/// Kana reading of Japanese text, one space between words. None when Windows has no
/// Japanese reading data (the Japanese language feature is not installed).
fn japanese_reading(s: &str) -> Option<String> {
    let words = JapanesePhoneticAnalyzer::GetWords(&HSTRING::from(s)).ok()?;
    let mut out = String::new();
    let mut any = false;
    for w in words {
        any = true;
        let display = w.DisplayText().map(|h| h.to_string()).unwrap_or_default();
        let yomi = w.YomiText().map(|h| h.to_string()).unwrap_or_default();
        if yomi.trim().is_empty() && has_han(&display) {
            return None;
        }
        let piece = if yomi.trim().is_empty() { display } else { yomi };
        let punct = piece.chars().all(|c| !c.is_alphanumeric());
        if !out.is_empty() && !punct && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push_str(&piece);
    }
    any.then_some(out)
}

fn has_han(s: &str) -> bool {
    s.chars().any(|c| matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF))
}

fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let c = match c {
            '　' => ' ',
            '、' => ',',
            '。' => '.',
            '「' | '」' | '『' | '』' => '"',
            '！' => '!',
            '？' => '?',
            c => c,
        };
        if c == ' ' && (out.ends_with(' ') || out.is_empty()) {
            continue;
        }
        if matches!(c, ',' | '.' | '!' | '?') && out.ends_with(' ') {
            out.pop();
        }
        out.push(c);
    }
    out.trim().to_string()
}

pub struct Romanizer {
    any: Option<Translit>,
    /// Common BGN/PCGN romanizations read more naturally than ICU's default
    /// ("Myagkoye kreslo" instead of "Mâgkoe kreslo", "maeumeun" instead of "ma-eum-eun").
    russian: Option<Translit>,
    japanese: bool,
}

impl Romanizer {
    /// `japanese` should be true when the song contains kana anywhere, so kanji-only lines
    /// of that song are read as Japanese too.
    pub fn new(sample: &[&str]) -> Self {
        Romanizer {
            any: Translit::open("Any-Latin"),
            russian: Translit::open("Russian-Latin/BGN"),
            japanese: sample.iter().any(|l| has_kana(l)),
        }
    }

    pub fn romanize(&self, s: &str) -> Option<String> {
        if !needs_romanization(s) {
            return None;
        }
        let any = self.any.as_ref()?;
        let src = if self.japanese && has_cjk(s) {
            match japanese_reading(s) {
                Some(r) => r,
                // without Japanese reading data, kanji would come out as Chinese pinyin:
                // keep such lines in their original script instead
                None if has_han(s) => return None,
                None => s.to_string(),
            }
        } else {
            s.to_string()
        };
        let mut src = src;
        if let Some(t) = self.russian.as_ref().filter(|_| src.chars().any(|c| matches!(c as u32, 0x0400..=0x04FF))) {
            src = t.apply(&src);
        }
        if src.chars().any(is_hangul) {
            src = hangul_rr(&src);
        }
        let r = tidy(&any.apply(&src));
        (!r.is_empty() && r != s).then_some(r)
    }
}

fn is_hangul(c: char) -> bool {
    matches!(c as u32, 0xAC00..=0xD7A3)
}

/// Revised Romanization of Korean with the usual liaison rules: a final consonant moves to
/// a following silent ㅇ (마음은 -> maeumeun), ㄹ is "r" between vowels and "l" next to
/// another ㄹ (사랑해요 -> saranghaeyo, 달려 -> dallyeo).
fn hangul_rr(s: &str) -> String {
    const INITIAL: [&str; 19] = ["g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p", "h"];
    const VOWEL: [&str; 21] = ["a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we", "wi", "yu", "eu", "ui", "i"];
    const FINAL: [&str; 28] = ["", "k", "k", "k", "n", "n", "n", "t", "l", "k", "m", "l", "l", "l", "p", "l", "m", "p", "p", "t", "t", "ng", "t", "t", "k", "t", "p", "t"];
    // final consonant before a silent ㅇ: (what stays as coda, what becomes the next onset)
    const LIAISON: [(&str, &str); 28] = [
        ("", ""), ("", "g"), ("", "kk"), ("k", "s"), ("", "n"), ("n", "j"), ("", "n"), ("", "d"), ("", "r"), ("l", "g"),
        ("l", "m"), ("l", "b"), ("l", "s"), ("l", "t"), ("l", "p"), ("", "r"), ("", "m"), ("", "b"), ("p", "s"), ("", "s"),
        ("", "ss"), ("ng", ""), ("", "j"), ("", "ch"), ("", "k"), ("", "t"), ("", "p"), ("", ""),
    ];
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() * 2);
    let mut carry: Option<&str> = None;
    let mut prev_final = 0usize;
    for (k, &c) in chars.iter().enumerate() {
        if !is_hangul(c) {
            out.push(c);
            carry = None;
            prev_final = 0;
            continue;
        }
        let code = c as usize - 0xAC00;
        let (i, v, f) = (code / 588, (code % 588) / 28, code % 28);
        let onset = match carry.take() {
            Some(o) if i == 11 => o,
            _ if i == 5 && prev_final == 8 => "l",
            _ => INITIAL[i],
        };
        let next_silent = chars.get(k + 1).is_some_and(|&n| is_hangul(n) && (n as usize - 0xAC00) / 588 == 11);
        let coda = if f > 0 && next_silent {
            let (coda, next_onset) = LIAISON[f];
            carry = Some(next_onset);
            coda
        } else {
            FINAL[f]
        };
        out.push_str(onset);
        out.push_str(VOWEL[v]);
        out.push_str(coda);
        prev_final = f;
    }
    out
}
