# Contributing to Kuro Nesos

Thanks for wanting to help. Bug reports, ideas and pull requests are all welcome.

## Reporting a bug

Open an [issue](https://github.com/SieghartSaladdin/kuro-nesos/issues) with:

- your Windows version and display scale (100 %, 150 %, ...),
- what you did, what you expected and what happened,
- a screenshot or a short clip of the notch if it is visual,
- the last lines of `%LOCALAPPDATA%\KuroNesos\log.txt` (or `KuroNesosData\log.txt` in the
  portable version).

## Suggesting a feature

Open an issue describing the problem it solves first. The notch is small and should stay calm,
so new things need to earn their place.

## Development setup

1. Install [Rust](https://rustup.rs) (stable, MSVC toolchain) and the *Visual Studio Build Tools*
   with "Desktop development with C++".
2. Build and run:

   ```bash
   cargo build --release
   ```

3. Useful flags while working on the UI:

   ```
   --pin --page N      keep the panel open on page N (0 Home ... 6 Settings)
   --demo all          fake alerts, a fake song with lyrics and a fast-sleeping cat
   ```

4. Before you test, back up `%APPDATA%\KuroNesos` (settings, water, to-do, shelf) if you care about
   that data, or use the portable build (`portable.txt` next to the exe) to keep test data apart.

## Code conventions

[AGENTS.md](AGENTS.md) has the module map, how drawing and animation work, frame pacing and the
memory budget. In short:

- Rust 2021 with the `windows` crate, no UI framework and no new dependencies without a good reason.
- Keep to the style of the file you touch: naming, comment density and layout.
- Coordinates are logical pixels; `Gfx` scales them for the display.
- Every control registers a hit area and an action (`Act`) while it is painted; hover and press
  animations come from there.
- Anything that stores files goes through `model::data_dir`, so portable mode keeps working.
- Do not log or store notification text, audio or file contents.

Run `cargo build --release` without warnings before opening a pull request.

## Pull requests

- One change per pull request, with a short description of what and why.
- Add screenshots (before / after) for visual changes; update `docs/screenshots/` and the README
  when a page changes.
- Update `README.md` and `AGENTS.md` when you add a feature, a setting or a module.
- Commit messages: a short imperative subject line ("Add ..."), details in the body if needed.

## Releases

Maintainers bump the version in `Cargo.toml` and `kuro-nesos.rc`, run `installer/build.ps1`
(needs Inno Setup 6) and attach the installer, the portable zip and the exe to a GitHub release.

## Code of Conduct

This project follows the [Code of Conduct](CODE_OF_CONDUCT.md). By taking part you agree to it.
