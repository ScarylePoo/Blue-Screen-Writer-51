# Handoff: right-to-left (Arabic, Hebrew) and CJK (Chinese, Japanese, Korean)

Status (2026-11): Blue Screen Writer 51 fully supports Latin and Cyrillic scripts. This note
describes what is needed to add RTL and CJK, and where in the code each piece goes.

## What already works
* The document model is Unicode (`El::Ch(char)`), files (.docx/.odt/.rtf/.doc/WP 5.x) carry
  Unicode text, so Arabic/Hebrew/CJK text already loads, saves and round-trips unchanged.
* WP 5.x import maps WP character sets 9 (Hebrew) and 11 (Japanese kana) to Unicode
  (`src/wpchars.rs`, `docs/formats/wp5_charsets.tsv`).
* Printing goes through Windows GDI (`src/print.rs`), which does its own shaping and bidi
  *per run* — so isolated RTL words print correctly today if the font has the glyphs.

## Gaps and plan

### 1. Screen font coverage (small)
`src/render.rs` uses DejaVu Sans Mono (Latin, Greek, Cyrillic, Hebrew partially, no Arabic
shaping, no CJK). Add a fallback chain: try the Windows font for the code point
(e.g. `Segoe UI`, `Arial Unicode MS`, `MS Gothic`, `SimSun`, `Malgun Gothic`, `Arial` for
Arabic) via `fonts::find_face`, rasterised with fontdue. CJK glyphs are double width: give
`LCell` a `width: u8` (1 or 2) and let the grid store a "continuation" cell; East Asian Width
property (Unicode UAX #11) decides — embed a compact table.

### 2. Text-mode layout of CJK (medium)
`src/layout.rs` pass 1: one screen column per char → use the cell width (2 for wide chars);
line breaking: CJK allows a break between any two ideographs (UAX #14 classes ID/CJ), with
kinsoku rules (no line start with 。、」 etc.). Add a small break-class table; keep using real
metrics for positions (`Metrics::width`).

### 3. Bidirectional text (large)
* Model stays logical order. Add paragraph direction: `Code::Dir(Rtl|Ltr)` ("[Dir:RTL]"), and
  map docx `w:bidi` / `w:rtl` both ways (`src/docx.rs`), ODT `style:writing-mode`.
* Layout: after a line's logical content is known, run the Unicode Bidi Algorithm (UAX #9) on
  the line to get the visual order (the pure-Rust `unicode-bidi` crate is suitable), then assign
  screen columns / x positions in visual order and keep `caret` positions mapped from logical
  indices to visual columns (cursor movement stays logical, like Word: Right arrow = next
  logical char; optionally offer visual movement).
* Right-aligned paragraphs by default in RTL; tabs measured from the right margin.
* Reveal Codes: show the logical stream (as WP does) — no change.
* Printing/View Document (`src/page.rs`): build runs in visual order per line; for GDI use
  `ETO_RTLREADING` for RTL runs and let Uniscribe shape Arabic (`ExtTextOutW` with
  `ETO_GLYPH_INDEX` off does shaping automatically for complex scripts in modern Windows).
  For View Document pixels, Arabic needs contextual shaping: use the pure-Rust `rustybuzz`
  (HarfBuzz port) to shape runs, then rasterise glyph ids with fontdue/ab_glyph.

### 4. Input
Windows IME (Japanese/Chinese/Korean) composition: winit delivers `WindowEvent::Ime`
(Preedit/Commit) once `window.set_ime_allowed(true)` is called. Show the preedit string at the
cursor (underlined) and insert on Commit — `src/app.rs` + a small preedit field in `Wp`.
Arabic/Hebrew keyboards already work through `KeyEvent.text`.

### 5. Spell checking / hyphenation / sorting
Add Hunspell dictionaries (he_IL, ar, ja has none — skip), word segmentation for CJK (no spaces;
for counting words use characters), locale collation for Sort.

### Suggested order
1 (fonts) → 4 (IME) → 2 (CJK layout) → 3 (bidi), each behind a `[Lang:..]`-aware setting so
Latin/Cyrillic users see no change.
