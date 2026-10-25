# AGENTS.md

Notes for anyone (human or AI agent) changing Kuro Nesos. Read this before editing.

Kuro Nesos is a "living notch" for Windows: a black island at the top edge (or a vertical tab
on the left/right edge) that shows music with synced lyrics, a Pomodoro timer, a water
reminder, a file shelf, a to-do list, system alerts and an idle black cat. It is a single
portable `.exe` written in Rust on the raw Win32 / Direct2D APIs (no UI framework).

## Build and run

```bash
cargo build --release
```

- Needs Rust stable (MSVC toolchain) and the VS Build Tools (MSVC + Windows SDK).
- Close a running `kuro-nesos.exe` first, or the linker fails with "Access denied".
- `windows-core` must stay a direct dependency: the `#[implement]` macro (OLE drop target)
  needs it.
- Release profile is size-tuned (`opt-level = "s"`, LTO, `panic = "abort"`, stripped); the exe
  is ~600 KB. Keep it small: no new crates without a strong reason.

Test flags (see `main.rs`): `--demo alerts|lyrics|mascot|all`, `--pin` (keep the panel open),
`--page N`, `--test-lyrics "Title|Artist"`, `--test-roman "line|line"`, `--test-media`.
`KURO_DEBUG_FPS=1` logs frames per second and the current mode to the log file.

Files on disk: settings / water / to-do in `%APPDATA%\KuroNesos`, shelf items and `log.txt`
in `%LOCALAPPDATA%\KuroNesos`. `settings.txt` is `key=value` lines:
`pet`, `minimal`, `lyrics`, `romaji`, `screenshots` (0/1), `dock=top|left|right`, `offset=0.500`.

## Module map

| Path | Role |
|---|---|
| `src/main.rs` | window creation, frame loop, tray menu, command line |
| `src/gfx.rs` | D3D11 → DXGI composition swap chain → DirectComposition; Direct2D/DirectWrite drawing helpers |
| `src/ui/mod.rs` | `App` state, enums (`Act`, `Mode`, `Page`, ...), `tick()` (periodic logic, frame pacing), notch shape per mode, window region |
| `src/ui/anim.rs` | `Spring`, `ease`, and `Fx` (per-control hover / press / on-off animations) |
| `src/ui/input.rs` | hover dwell, click-through state, dragging the notch between edges, mouse/keyboard actions (`act`) |
| `src/ui/paint.rs` | `render()`, renderer recovery, hit areas, shared controls (`button`, `round_button`, `hover_ring`), compact views |
| `src/ui/home.rs` | Kuro amber chrome: header (icon + title, amber-colon clock), Home bento tiles, Settings page, `play_button`, `switch`; both docks |
| `src/ui/pages.rs` | top-dock panel: paging and the Pomodoro, Water, Shelf, To-do pages, plus the building blocks both docks share (`play_disc`, `mode_chips`, `big_timer`, `water_bottle`, `water_chart`, `shelf_card`, `drop_card`, `todo_row`, `todo_form`, `plus_button`) |
| `src/ui/vertical.rs` | everything for the left/right docks (vertical shapes, compact views, 240×440 panel pages) |
| `src/ui/music.rs` | media state → lyrics, the music page |
| `src/ui/alerts.rs` | alert queue, system alerts, demo script, alert views |
| `src/ui/mascot.rs` | the black neko: sleep/wake state and drawing (works for every dock via rotation) |
| `src/ui/window.rs` | Win32 helpers: `place_window`, click-through, cursor, focus, `open_path` |
| `src/media.rs` | Global System Media Transport Controls (title, art, timeline, commands) |
| `src/audio.rs` | equalizer levels: a thread captures the default output (WASAPI loopback) only while music plays, splits it into six bands with one-pole filters and hands RMS peaks to the UI through atomics; `Beat` turns them into bar heights relative to each band's half-second average, plus a bass beat pulse |
| `src/lyrics.rs`, `src/json.rs` | LRCLIB client, LRC parser, tiny JSON reader |
| `src/roman.rs` | offline romanization (ICU `icu.dll` transliterators, Windows Japanese reading analyzer, Hangul RR) |
| `src/jdict.rs` | kanji reader for PCs without Japanese data: Viterbi over the memory-mapped IPADIC dictionary `ja-reading.knd`, downloaded once from the `ja-dict-1` release |
| `tools/make_ja_dict.py` | builds `ja-reading.knd` from the IPADIC sources (format documented in the script) |
| `src/sys.rs` | battery, network, Bluetooth, notifications, keyboard layout, fullscreen detection, chime, memory trim |
| `src/model.rs` | settings, water reminder, to-do list, autostart |
| `src/shelf.rs` | shelf storage and OLE drag & drop in/out; screenshots are added by reference (`add_path`) from `ShotWatch` in `sys.rs`, which polls the Screenshots known folder |

Child modules of `ui` are `impl App` blocks. Methods another `ui` module calls are
`pub(super)`; keep private what only one module uses. Add a new page or view as a new
module instead of growing `mod.rs`.

## Conventions

- Match the surrounding code: short doc comments that say *why*, same naming, no comment
  noise. Formatting follows `rustfmt` defaults with long lines allowed.
- Layout is in 96-DPI logical units; `Gfx` applies the DPI scale. Never use physical pixels
  in painting code (only `window.rs`/`input.rs` deal with screen pixels).
- Every interactive element registers a hit area with `self.hit(...)` / `button` /
  `round_button` and an `Act` variant. Visual state comes from `Fx`, not from `self.hot`:
  - `self.hov(act)` – eased hover 0..1 (fade highlights with it),
  - `self.squeeze(act)` – scale for the click "pop",
  - `self.fx.level(act, target)` – smooth on/off (switches, check marks, play/pause).
  No control should jump between two looks without an animation.
  Small reactive touches live next to the control they belong to: the album art bumps on a
  track change (`cover_bump`), the Pomodoro seconds slide and its play button ripples while
  running, the water bottle level glides and waves and a drop falls in on "Drink"
  (`drink_at`), the to-do "+" turns into an "x" and done tasks strike through, shelf cards
  lift on hover, and the header icon pops while a page slides in.
- Anything that animates must make `tick()` see `moving = true` (springs, `Fx::step`,
  `content_a`, ghost fade), otherwise frames stop and the animation freezes halfway.
- The vertical docks have their own layouts in `vertical.rs`; when you change a page in
  `pages.rs`/`music.rs`, change its vertical twin too.
- UI text is English; user-facing strings are short.
- Design language ("Kuro amber"): black `#050505` surfaces, cards `CARD` with a 1px `HAIR`
  outline, grey secondary text, and one accent, `AMBER`, only for what is active (progress,
  play button, switches, Pomodoro focus, page indicator). Text on amber uses
  `ON_AMBER`. Water keeps its blue and alerts keep their semantic colours.

## Window behaviour (do not break)

- The window is `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST`
  with DirectComposition content. While closed it is **click-through**: clicks reach the
  app underneath. `App::wants_input()` decides when it takes input (panel open, form,
  shelf drag, dragging the notch, alerts with buttons).
- It opens only after the cursor **rests** on it (~400 ms, 250 ms while dragging a file in)
  or immediately with `Ctrl`. A click that went through blocks opening until the cursor
  leaves. The closed notch fades to 16% while the cursor is over it.
- The panel header (`Act::Grab`) drags the notch along its edge or to another edge;
  position is `dock` + `offset` in settings. Tray "Reset position" returns to top centre.
- The window region is the shape's bounding box (+ padding; more for the cat's whiskers,
  paws and tail). Keep it tight so it never steals more screen than it shows.
- It hides itself while a fullscreen app is in front, and rebuilds the renderer after a GPU
  device loss or a dock change (the swap chain has the window size).

## Performance budget

Targets: **private memory < 50 MB** (typically 15–25 MB), **idle CPU ≈ 1–3 % of one core**.

- Frame pacing lives in `App::tick()`:
  - display refresh (`DwmFlush`, 60/120/144 Hz) only while something moves or a hover
    is pending;
  - 30 fps for looping decorations (equalizer, alert timer bar) and while the cursor is
    near the closed notch;
  - 10 fps for the sleeping cat and an open panel with nothing moving;
  - 4 wake-ups per second for the resting notch, which redraws only when the minute turns
    (`clock_stamp`: seconds only while the music page or the Pomodoro page shows them).
- Do not add per-frame allocations in hot paths without need; cache text formats/brushes
  in `Gfx`. Album art is downscaled to 160 px on load.
- Memory is handed back with `sys::trim_memory()` when things are calm; keep big buffers
  out of long-lived state.
- Measure after changes: run the release build, wait ~10 s, then sample
  `TotalProcessorTime` over 20 s and `PrivateMemorySize64` (PowerShell `Get-Process`).
  With `pet=0` you measure the resting notch; with the PC idle you measure the cat.

## Privacy

Online traffic is the lyrics lookup on `lrclib.net` (title + artist + duration) and, on PCs
without Japanese reading data, the one-time download of `ja-reading.knd` from the
`ja-dict-1` GitHub release (lyrics are never sent anywhere). Romanization is offline. If the
dictionary format changes, bump its version, publish a new `ja-dict-N` release and update
`URL` in `jdict.rs`; the IPADIC copyright notice must stay embedded in the file. Do not add telemetry or other network calls without saying so in
the README's Privacy section.

## Testing changes visually

There is no unit-test suite for the UI; verify in the real app:

- `--pin --page N` (0 Home ... 6 Settings) to screenshot each page; `--demo all` for fake alerts, a fake song with
  lyrics and a fast-sleeping mascot.
- Change `dock=` in `settings.txt` to check the left/right layouts, and restore the file.
- When automating input, move the real cursor only and post mouse messages to the notch
  window itself; never click into other applications.

## Releasing

Bump the version in `Cargo.toml` and `kuro-nesos.rc`, then run `installer/build.ps1` (needs
Inno Setup 6). It builds the release exe and writes to `dist/`: the installer
`KuroNesos-<version>-Setup.exe` (script `installer/kuro-nesos.iss`, per-user install), the
portable zip (exe + `installer/portable.txt`) and the plain exe. Attach all three to the
GitHub release with a short changelog.

Portable mode (`model::portable_dir`): a `portable.txt` next to the exe moves all data into
`KuroNesosData` beside it. Anything that stores files must go through `model::data_dir`.
