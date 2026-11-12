# Blue Screen Writer 51 — architecture & conventions

A native, clean-room word processor with the WordPerfect 5.1 for DOS experience
(for long-time WP 5.1 users who know it by muscle memory, e.g. translators),
plus sensible modern features. Rust, Windows first (x86_64-pc-windows-gnu), own
window (winit) rendering an 80×N character grid.

**User-visible text must call the program "Blue Screen Writer 51" — never "WP" or
"WordPerfect"** (old *document formats* may be called "WP 5.x files").
Keep WP 5.1 key sequences, menus, prompts and Reveal Codes names exact otherwise.

## Build / test
* `./tools/cargo.sh build -j 16` / `./tools/cargo.sh test -j 16 [filter]` — runs Windows
  cargo from WSL in the repo/worktree containing the script. No C compiler: pure-Rust crates only.
* Exe: `target/debug/bsw51.exe`. Headless test mode (no window):
  `./target/debug/bsw51.exe --config 'test-out\cfg' --keys '<script>' --dump 'test-out\x.txt' --png 'test-out\x.png'`
  (or `--script file`). Script: `"literal text"`, `{F6}`, `{Shift-F8}`, `{Alt-F4}`, `{Ctrl-Home}`,
  `{Enter}`, `{Left}`, `{Bksp}`, `{Del}`, `{Esc}`, `{PgDn}`, mouse `{Click c,r}` `{DClick c,r}`
  `{Drag c,r}` `{Up c,r}` `{Right c,r}` `{Wheel n}`. NOTE the default keyboard is the
  *Alternate* layout: F1 = Help, Esc = Cancel, F3 = repeat. `--dump` writes the screen text +
  cursor + the full code stream; `--png` a screenshot (the Read tool can view PNGs).
  Always pass `--config 'test-out\cfg'` so tests don't touch real settings.
* Paths given to the exe are Windows paths (F: = /mnt/f, I: = /mnt/i).

## Model (src/doc.rs)
`Document { els: Vec<El>, init: InitialFormat, initial_codes, summary, id, track }`.
`El::Ch(char) | El::Code(Code)` — a flat stream exactly like Reveal Codes. Units WPU = 1/1200".
Paired attributes `AttrOn(Attr)/AttrOff(Attr)`; nested text lives inside codes
(`Note{kind,els}`, `HeadFoot{which,occ,els}`); tables `TblDef{cols}/Row/Cell/TblOff`;
tracked changes `RevOn{del,author,date}/RevOff{del}`; `Unknown{name,data}` keeps anything not modelled.

**Adding a Code variant — touch all of these:** `doc.rs` (variant + doc comment),
`reveal.rs::code_text` (exact WP Reveal name), `stream.rs` (`code_to_str`/`str_to_code`,
lossless!), `editor.rs::is_invisible` (if it occupies screen space), `layout.rs`
(effect on layout), `page.rs` (printing), `docx.rs` (write + read, as faithfully as Word allows),
and if relevant `wp5.rs` (import from WP 5.x; it currently emits `Unknown` with the WP Reveal name
and raw bytes for many groups — convert those to your real code).

## Pipeline
* `layout.rs`: pass 1 builds `Line`s (cells = one screen column per char, caret positions in
  screen col + WPU), tables via visual rows (`VRow`), tabs/centre/flush right; pass 2
  `paginate()` assigns pages/y per visual row (headers/footers/footnotes reserve space) and
  builds display rows (`DRow`). `Metrics` = real TTF widths (`fonts::TtfMetrics`).
* `page.rs`: physical pages (`Run`s of positioned text + `rules`) from the layout → used by
  `pagerender.rs` (View Document pixels) and `print.rs` (Windows GDI printing, PDF via
  "Microsoft Print to PDF").
* `editor.rs`: one document + cursor/block/undo; `ui.rs`: `Wp` (whole program state), the edit
  screen, status line, Reveal Codes drawing, `edit_func` (all WP function keys), mouse.
* Screens: `trait Screen { input, draw, full_screen, draw_pixels, mouse_select }`, stack in
  `Wp.screens`; outcomes Keep/Close/CloseAll/Replace/Push. Helpers in `screens.rs`: `Prompt`,
  `YesNo`, `MenuLine` (bottom-line numbered menus, `~` marks mnemonic). Full-screen forms:
  `menus::Form` with `Item`/`Edit` (Fields, YesNo, Choice, Sub, Action). Register mouse targets
  while drawing with `wp.hit(row, c0, c1, HitAction::Input(..)|Select(n)|Cursor(i))`.
* Nested text editing (notes, headers, comments, initial codes): `subedit::open(wp, title, els, reveal, on_done)`.
* Keys: `keys.rs` (`Func` = every WP function; `default_map`, `alternate_map`), `.WPK` → `wpk.rs`,
  macros `wpm.rs` (full WP 5.1 macro language), `macros.rs` (Alt-F10, Ctrl-F10),
  `macroedit.rs` (macro editor, also used for key actions), `kbedit.rs` (Setup → Keyboard
  Layout list / Edit / Map, writes .WPK via `wpk::encode`).
* `menubar.rs` (Alt-= pull-down menus: items feed the equivalent key inputs; visible bar
  shifts the document down — use `wp.doc_top()` / `wp.status_row()`), `help.rs` (F3/F1 help),
  `shell.rs` (Ctrl-F1).  `Screen::kind()` tells macros ({STATE}) what is on top.
* Files: `fileio.rs` (sniff + load/save), `docx.rs` (native format; private part `wp51/stream.txt`
  keeps the exact code stream; custom property WP51NativeId = document id), `wp5.rs`, `msdoc.rs`,
  `rtf.rs`, `stream.rs`. Safety: `history.rs` (recovery copies, autosave, revisions), `revisions.rs`.
* Text columns (Alt-F7, 1): `columns.rs` — `[Col Def]`/`[Col On]`/`[Col Off]`; pass 1 wraps lines for
  a column, `paginate()` snakes newspaper lines through the columns of each page (`Layout::line_col`,
  `col_blocks`) and `columns::regroup` puts them side by side; parallel groups are laid out like table
  rows. .docx: newspaper = continuous section breaks with `<w:cols>`, parallel = borderless one-row
  tables (read back as tables).
* Merge/Sort/Math/Spreadsheet: `merge.rs` (Shift-F9 / F9 / Ctrl-F9; merge codes are
  `Code::Merge(name)` + parameter text ending in `~`; engine `merge::Engine`; secondary data =
  WP records, CSV/TSV (`;` `,` or tab, first row = field names) or .xlsx; Word-style `«Field»`
  works too), `sort.rs` (Sort screen + engine, German collation), `math.rs` (Table Edit 5 Math,
  `Code::Formula` at the start of a cell; Alt-F7 3 Math columns: `MathDef`/`Math`/`MathOp`;
  WP evaluates strictly left to right), `sheet.rs` (Ctrl-F5 5: CSV/xlsx import, `SsLink` links).
  In .docx, merge codes are written as plain-text placeholders `{FIELD}name~` (Ctrl-F9, 3
  "Convert Old Merge Codes" turns such text — and WP 5.0 `^F` codes — back into codes); formula
  cells and math results export their computed values (ordinary text); math operators,
  formulas and link codes are not exported (the private stream part keeps them).
* More formats/IO: `odt.rs` (.odt/.ott/.fodt), `export.rs` (RTF, e-mail HTML, text in UTF-8/1252/850/1251),
  `agile.rs` (password-protected .docx, ECMA-376 Agile Encryption + CFB writer; `Document.password`),
  `clipboard.rs` (Win32 clipboard; Shift-Ins/Ctrl-Ins/Shift-Del = `Func::Other(CLIP_*)`), `recent.rs`
  (last 9 files, ↑ at Shift-F10), `links.rs` (`[Link:url]…[Link End]`, `[Bookmark:name]`), `docinfo.rs`
  (Ctrl-F5 7 Document Information, Save As, links menu), `aids.rs` (smart quotes, spell-as-you-type,
  autocorrect; `Settings.ext`), `password.rs` (Ctrl-F5 2).
* Styles / outline / paragraph numbering: `styles.rs` (model: `Document::styles`, numbering,
  `virtual_stream` = the document with style codes expanded + `[Par Num]` numbers as text, which
  `layout()` lays out and maps back to document indices; printing uses `flat_document`; style
  libraries), `styles_ui.rs` (Alt-F8 screens, Shift-F5 Outline/Para Num/Define, Enter/Tab hooks),
  `docx_styles.rs` (styles.xml pStyle/rStyle, numbering.xml numPr ↔ `[Par Num]`).
* Graphics (Alt-F9): `gfx.rs` (box/line model `Code::GBox/BoxOpt/BoxNewNum/BoxNum/GLine`, Reveal text,
  stream form with base64 image bytes, numbering, wrap zones → `Layout.boxes` + `VRow::Gap`, page drawing,
  edit-screen outlines), `gfxui.rs` (menus, Definition/Options/Line screens, image editor), `raster.rs`
  (decode PNG/JPEG/BMP/GIF via `image`, `Placed` images on `Page.images`), `wpg.rs` (WPG 1.0 rasteriser),
  `equation.rs` (equation text → runs/rules), `gfxdocx.rs` (docx drawings/text boxes both ways).
  `rtf.rs`, `stream.rs`. Import filters `fmt_*.rs` (WP 6+, WP 4.2, WP Mac, Write/Word DOS, Word 2,
  Works, Word 2003 XML, AbiWord, Ami Pro, WordStar, HTML, Markdown (also export), PDF): `fileio::detect`
  picks the reader by content signature first, file extension second; readers return
  `Result<Document, String>`, never panic, and build their stream with `fmt_build::Builder`;
  `fmt_text.rs` = code pages + plain-text encoding detection. Safety: `history.rs` (recovery copies, autosave, revisions), `revisions.rs`.
* Other: `spell.rs` (Hunspell via spellbook; dictionaries in assets/dict, thesaurus assets/thes),
  `tables.rs`, `track.rs` (track changes + compare), `setup.rs` (Shift-F1), `settings.rs`,
  `filelist.rs` (F5), `fonts.rs`, `print.rs`, `linedraw.rs`, `tabset.rs`.
* Mark Text (Alt-F5) in `marktext.rs`: `Code::Mark(Mark::…)` codes (ToC/list/index/ToA marks,
  `[Def Mark]`/`[End Def]`, `[Ref]`/`[Target]`, `[Subdoc…]`), Define/Generate screens and the
  generation engine (`generate_with`, page numbers from the layout, repeated until stable).
  Comments (Ctrl-F5, 4) in `comments.rs`; their edit-screen boxes are `DRow::Comment` display rows
  (no paper space; Setup → Display → Comments Display).
* Shift-F8 Format codes beyond the core ones live in `Code::Fmt(fmt::Fmt)`: `fmt.rs` (model, Reveal
  names, stream, WP5 import, page numbering), `fmtmenu.rs` (all Format screens, hyphenation prompt,
  Home-key codes), `fmtlayout.rs` (keep-together, paper per page), `fmtpage.rs` (centre page, line
  numbers, labels on sheets), `fmtdocx.rs` (Word mapping), `hyph.rs` (Liang hyphenation, assets/hyph).

## Specs
`docs/spec/` (keys.md, screens.md incl. appendices with every WP 5.1 menu text, reveal_codes.md,
display.md, extracted/ help text), `docs/formats/` (WP5 document format, charsets, WPM/WPK/SET).
Her real WP 5.1 install (read-only!): `/mnt/i/Claude Space/wp51` (I:\Claude Space\wp51).

## Rules for parallel work
* Work only on your feature; keep changes to shared files (doc.rs, ui.rs, layout.rs, …) small,
  additive and well-placed (new match arms, new functions) to keep merges easy. Put new logic in
  new modules (`mod` line in main.rs).
* `./tools/cargo.sh test` must pass; add tests for your feature.
* Commit on your branch with the trailer
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
