# Building from Source

This page is for people who want to build Blue Screen Writer 51 themselves or work on it. You do not need any of this to use the program; download it from the Releases page instead (see [Installation and Starting](Installation-and-Starting)).

The program is written in Rust. Its source code is public domain ([The Unlicense](https://unlicense.org)).

## Requirements

- **Rust** (stable), installed with [rustup](https://rustup.rs).
- The **`x86_64-pc-windows-gnu`** toolchain. It uses the toolchain's own bundled linker, so **no Visual Studio and no MSYS2** are needed. The program uses pure-Rust libraries only; no C compiler is required.

```
rustup toolchain install stable-x86_64-pc-windows-gnu
```

## Building

From the repository folder:

```
cargo +stable-x86_64-pc-windows-gnu build --release
```

The program is then `target\release\bsw51.exe`. When run from the repository, it finds the dictionaries, thesauri and hyphenation patterns in `assets\`.

`build.rs` embeds the program icon (`assets\bsw51.ico`, made by `tools\make_icon.py`) and the version information into the executable.

A debug build (`cargo build`, without `--release`) is faster to compile and is written to `target\debug\bsw51.exe`.

### Building from WSL

The program is a Windows program. Under WSL (Windows Subsystem for Linux), run the **Windows** `cargo.exe` rather than a Linux cargo. The helper script `tools/cargo.sh` does this for the repository it lives in:

```
./tools/cargo.sh build -j 16
./tools/cargo.sh test -j 16
```

(Edit the path to `cargo.exe` in the script to match your Windows user folder.)

## Running the tests

```
cargo +stable-x86_64-pc-windows-gnu test
```

Test documents are in `tests/data`. They come from the test suites of open-source document-import projects (LibreOffice, libwpd, libmwaw and others) or were made for this project.

## Headless test mode

For automated testing, the program can run without a window: it plays a list of keys, then writes the screen as text and/or as a picture.

```
bsw51.exe --config TESTDIR some.docx --keys "{Ctrl-F5}\"6\"" --dump screen.txt --png screen.png
```

| Option | Meaning |
|---|---|
| `--config DIR` | Use `DIR` for settings, history and recovery instead of `%APPDATA%\Blue Screen Writer 51`. **Always use this for tests**, so your real settings are not touched. |
| `--keys "…"` | The keys to play (see below). |
| `--script FILE` | Read the keys from a file. |
| `--dump FILE` | Write the final screen as text, plus the cursor position and the document's full code stream. |
| `--png FILE` | Write a screenshot of the final screen. |
| a file name | Open this document first. |

Key syntax: text in double quotes is typed as it is (`"Hello"`); keys are written in braces, for example `{F6}`, `{Shift-F8}`, `{Alt-F4}`, `{Ctrl-Home}`, `{Enter}`, `{Left}`, `{Bksp}`, `{Del}`, `{Esc}`, `{PgDn}`. Mouse actions: `{Click c,r}`, `{DClick c,r}`, `{Drag c,r}`, `{Up c,r}`, `{Right c,r}`, `{Wheel n}` (column and row on the screen).

Remember that the standard keyboard is the **Alternate** keyboard: in scripts, `{F1}` is Help, `{Esc}` is Cancel and `{F3}` is Repeat.

## Other command-line options

| Option | Meaning |
|---|---|
| `bsw51.exe FILE` | Start and open `FILE`. If it does not exist, the new document gets that name. |
| `--config DIR` | Keep settings in `DIR` (also useful for a portable installation on a USB stick). |
| `--register` | Register the program for document file types (like Shift-F1, 7, 1, but without opening the Default apps window). |
| `--unregister` | Remove the registration. |
| `--icon-png FILE [SIZE]` | Write the program icon as a PNG image (standard size 1024). |

## Making a release

Releases are built automatically by the GitHub Actions workflow `.github/workflows/release.yml`. It starts when:

- a release is created on GitHub (Releases, *Draft a new release*, with a new tag such as `v1.0.1`), or
- a tag starting with `v` is pushed:

```
git tag v1.0.1
git push origin v1.0.1
```

- or it is started by hand from the **Actions** tab (you type the version number).

The workflow, on a Windows machine:

1. puts the version number into `Cargo.toml` (so it appears in the program's Windows properties);
2. installs the `stable-x86_64-pc-windows-gnu` toolchain;
3. runs the tests (`cargo test --release`);
4. builds `bsw51.exe` (`cargo build --release`);
5. packages the program, the `dict`, `thes` and `hyph` folders, an empty `WPM` folder, the licences and `READ ME.txt`;
6. converts `docs/GUIDE.md` into `Guide - What Is New.docx` using the program itself;
7. zips everything as `BlueScreenWriter51-<version>-windows-x64.zip` and attaches it to the release (creating the release if it does not exist yet).

## Developer documentation

In the repository:

- `docs/ARCHITECTURE.md` — architecture and conventions (the document model, the layout pipeline, screens, keys, and the checklist for adding a new code);
- `docs/formats/` — notes on the file formats;
- `docs/spec/` — the screen, key, display and Reveal Codes specifications;
- `docs/TODO.md` — open items;
- `docs/HANDOFF-RTL-CJK.md` — the plan for right-to-left and Chinese/Japanese/Korean scripts.

One rule for contributors: text the user sees must call the program **Blue Screen Writer 51**, never by the name of the program it is modelled on. Old document formats may be called by their names (for example "WP 5.x files").

## Licences of bundled data

The program's own code is public domain. Bundled third-party data keeps its own licence; the licence texts are in the respective folders:

- spelling dictionaries (`assets/dict`) and hyphenation patterns (`assets/hyph`) from the LibreOffice dictionaries project — GPL, LGPL, MPL or BSD-style, depending on the language;
- thesauri (`assets/thes`) — LGPL, and the WordNet licence for English;
- DejaVu Sans Mono font (`assets/fonts`) — Bitstream Vera / DejaVu licence.
