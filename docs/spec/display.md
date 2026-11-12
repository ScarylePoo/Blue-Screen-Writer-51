# WordPerfect 5.1 (DOS) — Display Specification (text mode)

## 1. Grid and windows
* 80 columns × 25 rows, CP437 glyphs, 16-colour CGA/EGA/VGA text palette.
  (Her `.SET` also lists an 80×43 16-colour text driver; 80×25 is primary.)
* Rows 0–23: document window (24 text lines). Row 24: status line / prompts / one-line menus.
* No menu bar unless Setup→Menu Bar Remains Visible = Yes (then row 0 is the menu bar and the
  document window is 23 lines [?]).
* Background of the whole screen = Normal background colour (blue by default).
* Cursor: blinking hardware-style underline cursor at the insertion point (Setup→Display
  Options→Cursor Height for graphics mode only). In the Reveal Codes lower pane: reverse block.

## 2. Colours (screen attributes)

WP keeps a table of 20 screen attributes (order as in `Setup: Colors`). Each text run is drawn
with the entry matching its attribute set; a run with several attributes uses
"Bold & Underline" if exactly bold+underline, else "Other Combinations" [? — WP actually uses
the single matching entry when one attribute, "Other Combinations" for any other mix].
Attribute byte = `bg<<4 | fg` (standard PC text attribute).

### 2.1 WP 5.1 factory defaults (decoded from WP.EXE @0x17BD8, colour; @0x17B73 monochrome)
| # | Attribute | Colour default | fg on bg | Mono default |
|---|---|---|---|---|
| 1 | Normal | 0x17 | light grey on blue | 07 |
| 2 | Blocked | 0x47 | light grey on red | 70 (reverse) |
| 3 | Underline | 0x71 | blue on light grey | 01 (underline) |
| 4 | Strikeout | 0x30 | black on cyan | 07 |
| 5 | Bold | 0x13 | cyan on blue | 0F (bright) |
| 6 | Double Underline | 0x31 | blue on cyan | 07 |
| 7 | Redline | 0x74 | red on light grey | 0F |
| 8 | Shadow | 0x71 | blue on light grey | 07 |
| 9 | Italics | 0x12 | green on blue | 07 |
| 10 | Small Caps | 0x34 | red on cyan | 07 |
| 11 | Outline | 0x35 | magenta on cyan | 07 |
| 12 | Subscript | 0x74 | red on light grey | 07 |
| 13 | Superscript | 0x75 | magenta on light grey | 07 |
| 14 | Fine Print | 0x50 | black on magenta | 07 |
| 15 | Small Print | 0x57 | light grey on magenta | 07 |
| 16 | Large Print | 0x04 | red on black | 07 |
| 17 | Very Large Print | 0x03 | cyan on black | 07 |
| 18 | Extra Large Print | 0x07 | light grey on black | 07 |
| 19 | Bold & Underline | 0x70 | black on light grey | 09 |
| 20 | Other Combinations | 0x60 | black on brown | 70 |

Attribute-mask values (bit per attribute, used to match): Blocked 0x20000, Underline 0x4000,
Strikeout 0x2000, Bold 0x1000, Dbl Und 0x800, Redline 0x400, Shadow 0x200, Italics 0x100,
Small Caps 0x8000, Outline 0x80, Subscript 0x40, Superscript 0x20, Fine 0x10, Small 0x08,
Large 0x04, Very Large 0x02, Extra Large 0x01, Bold&Und 0x5000.

### 2.2 **The user's own colours** (decoded from her `WP{WP}.SET`, four tables)
Table @0x6A5 (customised — probably Doc 1, colour text mode [?]):

| Attribute | Byte | Colours |
|---|---|---|
| Normal | 0x1F | **white on blue** |
| Blocked | 0x47 | light grey on red |
| Underline | 0x70 | black on light grey |
| Strikeout | 0x30 | black on cyan |
| Bold | 0x1B | **light cyan on blue** |
| Double Underline | 0x31 | blue on cyan |
| Redline | 0x74 | red on light grey |
| Shadow | 0x71 | blue on light grey |
| Italics | 0x1E | **yellow on blue** |
| Small Caps | 0x34 | red on cyan |
| Outline | 0x35 | magenta on cyan |
| Subscript | 0x74 | red on light grey |
| Superscript | 0x75 | magenta on light grey |
| Fine | 0x50 | black on magenta |
| Small | 0x57 | light grey on magenta |
| Large | 0x04 | red on black |
| Very Large | 0x03 | cyan on black |
| Extra Large | 0x07 | light grey on black |
| Bold & Underline | 0x7F | white on light grey |
| Other Combinations | 0x70 | black on light grey |

Table @0x740 (probably Doc 2 [?]): Normal 0x3F white on cyan, Underline 0x71, Strikeout 0x2E,
Bold 0x1F white on blue, Dbl Und 0x71, Italics 0x1E, Bold&Und 0x20, Other 0x6F; rest as above.
Tables @0x7DC and @0x877 are default-like (Normal 0x17, Bold 0x13, Italics 0x12; Underline 0x24
resp. 0x1F) — likely the second display mode (e.g. 43-line/font mode) [?].
**Recommendation:** ship her table @0x6A5 as the initial palette, make all 20 entries editable
in a Setup: Colors clone, and keep the factory table for "reset".

### 2.3 Other elements
| Element | Rendering |
|---|---|
| Status line | Normal colour; `Pos` value in current text attribute colour |
| Prompts / menus on row 24 | Normal; numbers/mnemonics in Bold colour |
| Full-screen menus | Normal background; titles and numbers Bold; headers Underline attribute |
| `Block on` | Normal colour, blinking (needs blink, not intensity bg) |
| Block highlight | Blocked attribute (her: light grey on red) |
| Reveal Codes codes | Bold attribute [?]; cursor = reverse of Normal (fg/bg swapped) [?] |
| Reveal Codes ruler | Bold/“Blocked”? — [?] use Bold on Normal |
| Search-confirm highlight / Undelete preview | Blocked attribute |
| Spell/Thesaurus separators | double line `═` in Normal |

16-colour palette (VGA text defaults): 0 black #000000, 1 blue #0000AA, 2 green #00AA00,
3 cyan #00AAAA, 4 red #AA0000, 5 magenta #AA00AA, 6 brown #AA5500, 7 light grey #AAAAAA,
8 dark grey #555555, 9 light blue #5555FF, 10 light green #55FF55, 11 light cyan #55FFFF,
12 light red #FF5555, 13 light magenta #FF55FF, 14 yellow #FFFF55, 15 white #FFFFFF.
Background colours 8–15 are allowed by WP's colour editor (blink disabled) [?].

Font-mode cards (EGA/VGA "Italics font", "Underline font", "Small caps font", "512 characters"
modes, files `VGAITAL.FRS`, `VGAUND.FRS`, `VGASMC.FRS`, `VGA512.FRS`) can show real italic /
underlined glyphs; optional enhancement — a native renderer may simply draw real italics
and underlines in addition to colours (make it a setting; default = colour only, authentic).

## 3. Text rendering in the document window
* **Soft return**: line simply ends; next line begins at left margin. Nothing shown.
* **Hard return**: line ends; optional Hard Return Display Character (default blank) drawn
  at the end position.
* Horizontal position: column = (position in inches − 0) × 10 for 10-pitch display
  (Format: Document → Display Pitch: Automatic, Width 0.1" per column). Left margin 1" →
  text starts in screen column 10 (`Pos 1"`). Line width 6.5" = 65 columns. Proportional fonts
  are displayed one character per cell regardless.
* **Horizontal scrolling**: lines wider than 80 columns scroll the whole window horizontally
  when the cursor moves past the edge.
* **Tabs / indents**: spaces up to the tab stop (filled with the attribute in effect; dot
  leaders drawn as `.`).
* **Centred text**: placed centred between margins (or at the centre tab); **flush right**
  ends at the right margin.
* **Full justification**: NOT shown on screen in text mode — lines are ragged-right on screen
  (only View Document shows justification). This is authentic and expected.
* **Hard page `[HPg]`**: a full-width row of `=` characters (help: "A hard page is indicated by
  a double line"; drawn with `=` [? or `═`]).
* **Soft page `[SPg]`/`[HRt-SPg]`**: full-width row of `-` (`single dashed line`) [? `-` vs `─`].
  These separator rows occupy a screen row but are not document lines.
* **Columns** (Setup Side-by-side Display = Yes): columns drawn side by side at their positions;
  column break (Ctrl-Enter inside columns) moves to next column; with No, each column is shown
  sequentially separated by page-break lines.
* **Tables**: drawn with box-drawing characters according to the cell line styles: default
  outer double, inner single (`╔═╤═╗ ║ │ ║ ╟─┼─╢ ╚═╧═╝`); cell text inside. Line styles None /
  Single / Double / Dashed / Dotted / Thick / Extra Thick map to blank / `─│` / `═║` / and the
  closest CP437 glyphs [?].
* **Comments**: double-line box (`╔═…╗ ║ text ║ ╚═…╝`) spanning margins, text inside.
* **Headers, footers, footnotes, page numbers**: NOT displayed in the editing screen (only in
  View Document / print).
* **Graphics boxes**: in text mode shown as an outline box of the box size with the box
  label `FIG 1`, `TBL 1`, `TXT 1`, `USR 1`, `EQU 1` (WP.EXE strings `FIG `,`TBL `,…) [?].
* **Line draw** characters are ordinary characters.
* Characters not in the screen font: drawn as `■` placeholder [?].
* Attribute colours (section 2) apply to the text cells (and to tab-fill spaces inside an
  attribute pair, e.g. underlined tabs if Underline Tabs = Yes).

## 4. Rewrite / reformat
* Setup→Edit-Screen→Automatically Format and Rewrite = Yes: paragraphs reflow as you type.
  When No, reflow happens when the cursor moves down through the text or on Ctrl-F3,3
  (**Rewrite**: `1 Window; 2 Line Draw; 3 Rewrite`). Ctrl-F3 then Ctrl-F3 again also rewrites [?].

## 5. Ctrl-F3 Screen
* `1 Window; 2 Line Draw; 3 Rewrite: 0` (mnemonics W L R).
* **Window**: `Number of lines in this window: 24` — enter e.g. 12 → screen split: Doc 1 on
  top, a ruler/tab bar line [?] between, Doc 2 below, each with its own status line. Shift-F3
  moves between windows. Enter 24 to close the split. In Reveal Codes, adjusts the upper pane.
* **Line Draw**: `1 │; 2 ║; 3 *; 4 Change; 5 Erase; 6 Move: 1` — arrow keys draw with the
  selected character, joining corners/tees automatically (┌ ┬ ┐ ├ ┼ ┤ └ ┴ ┘ and double-line
  equivalents). Change: `1 ░; 2 ▒; 3 ▓; 4 █; 5 ▄; 6 ▌; 7 ▐; 8 ▀; 9 Other: 0`
  (`Solid character: `). Erase draws spaces; Move moves without drawing. Esc,n,arrow draws n.
  Exit/Cancel ends.
* **Rewrite**: reformat the screen/document now.

## 6. View Document (Shift-F7,6)
Graphics-mode page preview (her driver: VGA 640×480 16 colour). Bottom row:
`1 100%  2 200%  3 Full Page  4 Facing Pages: 3`. Text in black on white unless Setup→View
Document Options. A native implementation should render a page image (out of scope here).

## Sources
* Colour tables: `/mnt/i/Claude Space/wp51/WP.EXE` @0x17B73/0x17BD8, `WP{WP}.SET` @0x6A5,
  0x740, 0x7DC, 0x877 (decoder: `extracted/tools/`; list order from WP.FIL string
  "Normal / Blocked / Underline / …").
* WPHELP.FIL: "Hard Page", "Screen", "Setup: Colors/Fonts/Attributes", "Setup: Automatically
  Format and Rewrite", "Setup: Side-by-side Columns Display".
* VGA palette: standard IBM VGA text-mode DAC defaults.
* WPDOS: https://mendelson.org/wpdos/
