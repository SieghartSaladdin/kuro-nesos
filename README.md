<p align="center">
  <img src="assets/logo.png" width="160" alt="Kuro Nesos logo: a pencil-sketched black cat whose head is the notch">
</p>

<h1 align="center">Kuro Nesos</h1>

<p align="center">
  <b>A living notch for Windows.</b><br>
  Music with synced lyrics &amp; romaji, a focus timer, water reminder, to-do list, file shelf,
  system alerts and an idle cat mascot, all in a small black island at the edge of your screen.
</p>

<p align="center"><i>kuro</i> (黒, Japanese) = black &nbsp;·&nbsp; <i>nesos</i> (νῆσος, Greek) = island</p>

---

<p align="center">
  <img src="docs/screenshots/music.png" width="560" alt="Music page with synced lyrics">
</p>

<p align="center">
  <img src="docs/screenshots/lyrics-pill.png" width="330" alt="Live lyrics in the collapsed notch">
</p>

<p align="center">
  <img src="docs/screenshots/pomodoro.png" width="420" alt="Pomodoro page">
  <img src="docs/screenshots/water.png" width="420" alt="Water reminder page">
</p>

<p align="center">
  <img src="docs/screenshots/todo.png" width="420" alt="To-do page">
  <img src="docs/screenshots/alert-drink.png" width="400" alt="Actionable water reminder">
</p>

<p align="center">
  <img src="docs/screenshots/alert-message.png" width="400" alt="Mirrored Windows notification">
  <img src="docs/screenshots/alert-caps.png" width="340" alt="Caps Lock alert">
</p>

## Features

- **Notch that lives on your screen.** A black pill hangs from the top edge and expands into a
  panel when you rest the cursor on it. Drag the panel's header to slide it along the edge or
  over to the left/right edge, where it turns vertical: a slim tab that opens into a tall panel.
- **Never in the way.** Clicks go straight through the closed notch to the window below, and
  it fades out while the cursor passes over it, so tabs and buttons underneath stay usable.
  It only opens when the cursor rests on it for a moment (or right away with `Ctrl`). Smooth spring animations,
  synced to your display's refresh rate (60/120/144 Hz).
- **Music.** Works with any player that shows up in the Windows media flyout (Spotify,
  browsers, Apple Music, ...): title, artist, album art, progress with seeking, play/pause,
  next/previous.
- **Equalizer that follows the beat.** While music plays, 11 thin amber bars mirrored around
  a centre line (bass in the middle, treble at both ends) move with six frequency bands of
  what you hear, and jump on every hit. With lyrics off the pill shrinks to just the cover
  and the equalizer.
- **Synced lyrics** from [LRCLIB](https://lrclib.net). The collapsed notch grows and shrinks with
  each line, long lines wrap on the music page; instrumental breaks show a note icon.
- **Romaji / romanization** (toggle with the `Aa` chip or the tray menu), offline:
  Japanese → romaji (kanji included, e.g. あの水平線が遠ざかっていく → *ano suiheisen ga
  toozakatte iku*), Chinese → pinyin, Korean → Revised Romanization, Russian → BGN Latin,
  plus Greek, Thai, Arabic, ... through ICU (built into Windows).
- **Pomodoro** focus timer with short/long breaks and a progress ring.
- **Water reminder**: daily goal, bottle size and active hours, reminders spread over the day,
  hourly chart, "Drank / Refilled / Snooze" right from the notification.
- **To-do list** with optional time ranges; a "Task Time!" alert when a task starts.
- **File shelf**: drop files, browser images or links onto the notch, click to open, drag them
  back out to any app. New screenshots (Win+PrtScn, Snipping Tool) land on the shelf by
  themselves, with a thumbnail; remove one with its × or everything with *Clear* (the original
  in Pictures\Screenshots is kept). Turn it off in Settings → *Screenshots to Shelf*.
- **Alerts**: Caps Lock, charger plugged/unplugged, low battery, Wi-Fi/Ethernet changes,
  Bluetooth devices, keyboard layout, and mirrored Windows notifications.
- **Kuro the cat**: after 15 s without input the notch turns into a little black neko that
  naps (ears, whiskers, paws over the edge, a swishing tail and drifting "z"s), opens its
  amber eyes and follows your cursor when you come back, then waves hello.
- **Kuro amber design**: pure black panels with 1px hairlines and a single amber accent (the
  cat's eyes) for whatever is active; a **Home** page of bento tiles shows music, focus timer,
  water, to-do and shelf at a glance, and a **Settings** page lives right in the panel.
- **Smooth everywhere**: spring-animated shape, cross-fading views, and every control eases
  its hover highlight, squeezes a little when clicked, and animates switches and checks.
- **Polite**: hides while a fullscreen app (game, video) is in front, rebuilds its renderer
  after a GPU reset, uses about 10-20 MB of RAM and ~2 % of one CPU core when idle.

## Install

From the [Releases](https://github.com/SieghartSaladdin/kuro-nesos/releases) page pick one:

- **`KuroNesos-<version>-Setup.exe`** – installer. Installs for your user only (no admin
  rights) into `%LOCALAPPDATA%\Programs\Kuro Nesos`, adds a Start Menu entry, can start Kuro
  Nesos with Windows, and uninstalls cleanly from *Settings → Apps*.
- **`KuroNesos-<version>-portable.zip`** – portable. Unzip anywhere (even a USB stick) and run
  `kuro-nesos.exe`; while `portable.txt` sits next to it, all data stays in a `KuroNesosData`
  folder beside the exe.
- **`kuro-nesos.exe`** – the plain single file; data goes to `%APPDATA%\KuroNesos`.

Windows SmartScreen may warn about an unrecognised app because the exe is not code-signed yet;
choose *More info → Run anyway*.

### Requirements

- Windows 10 1903+ or Windows 11 (64-bit)
- **Kanji → romaji** works out of the box: if Windows has Japanese reading data (the
  Japanese language feature) it is used; otherwise Kuro Nesos downloads a 16 MB Japanese
  dictionary (IPADIC) **once**, the first time a song has kanji, and reads kanji offline
  from then on. Without either, kanji lines stay in their original script.

### Build from source

1. Install [Rust](https://rustup.rs) (stable, MSVC toolchain) and the
   *Visual Studio Build Tools* with the "Desktop development with C++" components
   (MSVC + Windows SDK).
2. Build:

   ```bash
   cargo build --release
   ```

   The binary is `target/release/kuro-nesos.exe`.
3. Optional, the release packages: with [Inno Setup 6](https://jrsoftware.org/isinfo.php)
   installed, `installer/build.ps1` writes the installer, the portable zip and the exe to
   `dist/`.

## Usage

| Where | What |
|---|---|
| Rest the cursor on the notch (or hold `Ctrl`) | expand it; move away to collapse |
| Drag the panel header | move the notch along the edge or to the left/right/top edge |
| `‹` `›`, the dots, or the mouse wheel | switch page (Home, Music, Pomodoro, Water, Shelf, To-do, Settings) |
| Home tiles | open the music, Pomodoro, water, to-do or shelf page; play/pause right from Home |
| Settings page | Start with Windows, Kuro the cat, Romaji lyrics, Lyrics, Minimal notch, Screenshots to Shelf, Reset position |
| Lyrics bubble / `Aa` chip on the music page | lyrics on/off, romaji on/off |
| Tray icon (right-click) | Start with Windows, Mascot, Minimal notch, Romaji lyrics, Reset position, About, Exit |
| To-do form | type the title, `Tab` to the time field (`HHMM HHMM`), `Enter` saves, `Esc` cancels |

### Command-line flags (testing)

```
--demo alerts|lyrics|mascot|all   fake notifications / fake song with lyrics / fast mascot
--pin                              keep the notch expanded
--page N                           start on page N (0 = Home, 1 = Music ... 5 = To-do, 6 = Settings)
--test-lyrics "Title|Artist"       fetch lyrics once, write the result to the log, exit
--test-roman "line|line"           romanize lines, write the result to the log, exit
--test-media                       log the current media session, exit
```

Set `KURO_DEBUG_FPS=1` to log the render frame rate.

## Privacy

- **Online:** the lyrics search (track title and artist are sent to `lrclib.net`), and,
  only on PCs without Japanese reading data, a one-time download of the Japanese dictionary
  from this project's GitHub releases. Lyrics themselves never leave your PC; romanization
  is offline.
- **Notifications:** mirroring Windows notifications needs your consent (Windows asks once).
  Their text is only shown in the notch, never stored.
- **Audio:** while music plays, the equalizer reads what your speakers play (WASAPI
  loopback) and measures only its loudness per frequency band, in memory. Nothing is
  recorded, stored or sent.
- **Screenshots:** new files in Pictures\Screenshots are only linked on the shelf, not copied
  or uploaded.
- **Local data:** settings, water log and to-do list in `%APPDATA%\KuroNesos`, shelf items and a
  small log in `%LOCALAPPDATA%\KuroNesos`.

## How it is built

Rust with the [`windows`](https://github.com/microsoft/windows-rs) crate, no UI framework:

| File | Role |
|---|---|
| `src/main.rs` | window, frame loop paced by the compositor, tray, command line |
| `src/gfx.rs` | Direct3D 11 → DXGI swap chain → DirectComposition, drawn with Direct2D + DirectWrite |
| `src/ui/` | notch state machine (`mod.rs`), animations (`anim.rs`), input, painting, pages, vertical layouts, music, alerts, mascot, window helpers |
| `src/media.rs` | Global System Media Transport Controls (title, art, timeline, commands) |
| `src/audio.rs` | equalizer levels: WASAPI loopback split into six bands, beat detection |
| `src/lyrics.rs`, `src/json.rs` | LRCLIB client, LRC parser, minimal JSON reader |
| `src/roman.rs`, `src/jdict.rs` | romanization (ICU transliterator, Japanese reading analyzer, IPADIC kanji reader, Hangul RR) |
| `src/sys.rs` | battery, network, Bluetooth, notifications, screenshots folder, keyboard layout, fullscreen detection, chime |
| `src/model.rs`, `src/shelf.rs` | settings, water, to-do, shelf storage, OLE drag & drop, portable mode |
| `installer/` | Inno Setup script, portable marker and `build.ps1` for the release packages |

The logo is drawn by code: `python assets/make_logo.py` (needs Pillow) regenerates
`assets/logo.png` and the app icon `assets/kuro-nesos.ico`.

Contributor notes (module map, conventions, frame pacing and memory budget) are in
[AGENTS.md](AGENTS.md).

## Credits

- Synced lyrics: [LRCLIB](https://lrclib.net).
- Kanji readings: [IPADIC](https://github.com/taku910/mecab) (mecab-ipadic-2.7.0),
  © 2000–2003 Nara Institute of Science and Technology; the dictionary file is built with
  `tools/make_ja_dict.py` and published as the `ja-dict-1` release.
- Icons: Segoe Fluent Icons (shipped with Windows).

## License

[MIT](LICENSE)
