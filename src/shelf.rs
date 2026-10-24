//! File shelf: items are copied into %LOCALAPPDATA%\KuroNesos\Shelf and listed in shelf.txt.
//! Accepts OLE drops of files (CF_HDROP), virtual files (FileGroupDescriptorW/FileContents,
//! e.g. images dragged from a browser) and links (UniformResourceLocatorW -> .url file),
//! and supports dragging items back out (DoDragDrop with the shell's data object).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use windows::core::{implement, w, Interface, Result, HRESULT, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::model::{roaming, shelf_dir};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    File,
    Folder,
    Link,
    Image,
}

#[derive(Clone)]
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub kind: Kind,
}

fn kind_of(p: &Path) -> Kind {
    if p.is_dir() {
        return Kind::Folder;
    }
    match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("url") => Kind::Link,
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "heic") => Kind::Image,
        _ => Kind::File,
    }
}

fn item(p: PathBuf) -> Item {
    let size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let kind = kind_of(&p);
    let name = if kind == Kind::Link { name.trim_end_matches(".url").to_string() } else { name };
    Item { path: p, name, size, kind }
}

pub fn size_label(n: u64) -> String {
    let f = n as f64;
    if f >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB", f / 1024.0 / 1024.0 / 1024.0)
    } else if f >= 1024.0 * 1024.0 {
        format!("{:.1} MB", f / 1024.0 / 1024.0)
    } else {
        format!("{} KB", (n + 1023) / 1024)
    }
}

/// Keeps characters that are valid in a Windows file name.
fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if "\\/:*?\"<>|".contains(c) || (c as u32) < 32 { '_' } else { c }).collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() {
        "item".into()
    } else {
        s.chars().take(80).collect()
    }
}

fn unique(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let stem = Path::new(name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = Path::new(name).extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
    (2..1000).map(|i| dir.join(format!("{} ({}){}", stem, i, ext))).find(|p| !p.exists()).unwrap_or(p)
}

pub struct Shelf {
    pub items: Vec<Item>,
}

impl Shelf {
    pub fn load() -> Self {
        let list = fs::read_to_string(roaming().join("shelf.txt")).unwrap_or_default();
        let items = list.lines().filter(|l| !l.is_empty()).map(PathBuf::from).filter(|p| p.exists()).map(item).collect();
        Shelf { items }
    }

    pub fn save(&self) {
        let t: Vec<String> = self.items.iter().map(|i| i.path.to_string_lossy().to_string()).collect();
        let _ = fs::write(roaming().join("shelf.txt"), t.join("\n"));
    }

    fn push(&mut self, p: PathBuf) {
        if !self.items.iter().any(|i| i.path == p) {
            self.items.push(item(p));
        }
    }

    pub fn add(&mut self, d: Dropped) {
        let dir = shelf_dir();
        match d {
            Dropped::Path(src) => {
                if src.is_dir() || fs::metadata(&src).map(|m| m.len() > 512 * 1024 * 1024).unwrap_or(true) {
                    // folders and very large files are kept as references
                    self.push(src);
                } else {
                    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
                    let dst = unique(&dir, &sanitize(&name));
                    if fs::copy(&src, &dst).is_ok() {
                        self.push(dst);
                    } else {
                        self.push(src);
                    }
                }
            }
            Dropped::Bytes(name, bytes) => {
                let dst = unique(&dir, &sanitize(&name));
                if fs::write(&dst, bytes).is_ok() {
                    self.push(dst);
                }
            }
            Dropped::Url(url) => {
                let host = url.split("://").nth(1).unwrap_or(&url).split(['/', '?', '#']).next().unwrap_or("link");
                let host = host.trim_start_matches("www.");
                let dst = unique(&dir, &format!("{}.url", sanitize(if host.is_empty() { "link" } else { host })));
                if fs::write(&dst, format!("[InternetShortcut]\r\nURL={}\r\n", url)).is_ok() {
                    self.push(dst);
                }
            }
        }
        self.save();
    }

    /// Removes everything; copies that live inside the shelf folder are deleted.
    pub fn clear(&mut self) {
        let dir = shelf_dir();
        for i in &self.items {
            if i.path.starts_with(&dir) {
                let _ = fs::remove_file(&i.path);
            }
        }
        self.items.clear();
        self.save();
    }
}

// ------------------------------------------------------------------ OLE drop target

pub enum Dropped {
    Path(PathBuf),
    Bytes(String, Vec<u8>),
    Url(String),
}

/// Drops are queued here and handled by the window procedure (posted message), so the
/// OLE callback never re-enters the UI state.
pub static QUEUE: Mutex<Vec<Dropped>> = Mutex::new(Vec::new());

pub const WM_SHELF_DRAG: u32 = 0x8000 + 20;
pub const WM_SHELF_DROP: u32 = 0x8000 + 21;

#[implement(IDropTarget)]
pub struct DropTarget {
    pub hwnd: HWND,
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(&self, _data: Option<&IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe {
            *effect = DROPEFFECT_COPY;
            let _ = PostMessageW(self.hwnd, WM_SHELF_DRAG, WPARAM(1), LPARAM(0));
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe { *effect = DROPEFFECT_COPY };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        unsafe {
            let _ = PostMessageW(self.hwnd, WM_SHELF_DRAG, WPARAM(0), LPARAM(0));
        }
        Ok(())
    }

    fn Drop(&self, data: Option<&IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe {
            *effect = DROPEFFECT_COPY;
            if let Some(d) = data {
                let items = extract(d);
                if let Ok(mut q) = QUEUE.lock() {
                    q.extend(items);
                }
            }
            let _ = PostMessageW(self.hwnd, WM_SHELF_DRAG, WPARAM(0), LPARAM(0));
            let _ = PostMessageW(self.hwnd, WM_SHELF_DROP, WPARAM(0), LPARAM(0));
        }
        Ok(())
    }
}

fn fmt(cf: u16, index: i32, tymed: TYMED) -> FORMATETC {
    FORMATETC { cfFormat: cf, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: index, tymed: tymed.0 as u32 }
}

unsafe fn global_bytes(h: HGLOBAL) -> Vec<u8> {
    let size = GlobalSize(h);
    let p = GlobalLock(h) as *const u8;
    if p.is_null() {
        return Vec::new();
    }
    let v = std::slice::from_raw_parts(p, size).to_vec();
    let _ = GlobalUnlock(h);
    v
}

unsafe fn stream_bytes(s: &IStream) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let mut read = 0u32;
        let hr = s.Read(buf.as_mut_ptr() as *mut _, buf.len() as u32, Some(&mut read));
        if hr.is_err() || read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read as usize]);
        if out.len() > 512 * 1024 * 1024 {
            break;
        }
    }
    out
}

unsafe fn extract(d: &IDataObject) -> Vec<Dropped> {
    let mut out = Vec::new();
    // 1. real files
    if let Ok(mut m) = d.GetData(&fmt(CF_HDROP.0, -1, TYMED_HGLOBAL)) {
        let hdrop = HDROP(m.u.hGlobal.0);
        let n = DragQueryFileW(hdrop, 0xFFFF_FFFF, None);
        for i in 0..n {
            let len = DragQueryFileW(hdrop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            DragQueryFileW(hdrop, i, Some(&mut buf));
            out.push(Dropped::Path(PathBuf::from(String::from_utf16_lossy(&buf[..len]))));
        }
        ReleaseStgMedium(&mut m);
        if !out.is_empty() {
            return out;
        }
    }
    // 2. virtual files
    let cf_desc = RegisterClipboardFormatW(w!("FileGroupDescriptorW")) as u16;
    let cf_contents = RegisterClipboardFormatW(w!("FileContents")) as u16;
    if let Ok(mut m) = d.GetData(&fmt(cf_desc, -1, TYMED_HGLOBAL)) {
        let raw = global_bytes(m.u.hGlobal);
        ReleaseStgMedium(&mut m);
        if raw.len() >= 4 {
            let count = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
            let rec = std::mem::size_of::<FILEDESCRIPTORW>();
            for i in 0..count.min(32) {
                let off = 4 + i * rec;
                if off + rec > raw.len() {
                    break;
                }
                let fd = std::ptr::read_unaligned(raw[off..].as_ptr() as *const FILEDESCRIPTORW);
                let file_name = fd.cFileName;
                let name_len = file_name.iter().position(|&c| c == 0).unwrap_or(file_name.len());
                let name = String::from_utf16_lossy(&file_name[..name_len]);
                if let Ok(mut c) = d.GetData(&fmt(cf_contents, i as i32, TYMED(TYMED_HGLOBAL.0 | TYMED_ISTREAM.0))) {
                    let bytes = if c.tymed == TYMED_ISTREAM.0 as u32 {
                        match c.u.pstm.as_ref() {
                            Some(s) => stream_bytes(s),
                            None => Vec::new(),
                        }
                    } else {
                        global_bytes(c.u.hGlobal)
                    };
                    ReleaseStgMedium(&mut c);
                    if !bytes.is_empty() {
                        out.push(Dropped::Bytes(name, bytes));
                    }
                }
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    // 3. links
    let cf_url = RegisterClipboardFormatW(w!("UniformResourceLocatorW")) as u16;
    if let Ok(mut m) = d.GetData(&fmt(cf_url, -1, TYMED_HGLOBAL)) {
        let raw = global_bytes(m.u.hGlobal);
        ReleaseStgMedium(&mut m);
        let wide: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
        let url = String::from_utf16_lossy(&wide);
        if !url.is_empty() {
            out.push(Dropped::Url(url));
        }
    }
    out
}

// ------------------------------------------------------------------ drag out

#[implement(IDropSource)]
struct DropSource;

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if keys.0 & MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Runs the modal OLE drag loop for one shelf item.
pub fn drag_out(path: &Path) {
    unsafe {
        let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
        let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(PCWSTR(wide.as_ptr()), None) else { return };
        let Ok(data) = item.BindToHandler::<_, IDataObject>(None, &BHID_DataObject) else { return };
        let source: IDropSource = DropSource.into();
        let mut effect = DROPEFFECT_NONE;
        let _ = DoDragDrop(&data, &source, DROPEFFECT_COPY | DROPEFFECT_LINK, &mut effect);
        let _ = source.cast::<windows::core::IUnknown>();
    }
}
