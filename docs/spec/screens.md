# WordPerfect 5.1 (DOS) — Screens, Menus and Prompts

Every string quoted in `code` below was extracted from the user's real WP 5.1 installation
(`WP.EXE` plain strings, `WP.FIL` nibble-compressed menu table at file offset 0x861E0,
`WPHELP.FIL`), unless marked **[?]** (= from memory / secondary source, verify).
Two machine-generated appendices at the end list **all** full-screen menus (with exact row
numbers and columns) and **all** one-line menus (with mnemonic letters) found in WP.FIL.
Raw dumps: `docs/spec/extracted/` (extraction scripts only; the dumps of the original program's text are not published).

---

## 1. General conventions

### 1.1 Screen geometry
* 80×25 text grid. Rows 0–23 = document window (24 lines), row 24 = status/prompt line.
  (WP calls them lines 1–24 and line 25.)
* Full-screen menus clear the whole screen (string starts with control 0x0C), draw the menu,
  and put `Selection: 0` on row 24 col 0 (`Selection:` bold, cursor on the `0`) [the help screen
  version is in WP.EXE: `{Selection: 0}` + cursor-left].
* One-line ("bottom line") menus are drawn on row 24 starting at col 0, replacing the status
  line, and end with `: 0` where the cursor sits on the default choice `0`
  (some strings carry their own default, e.g. `… 4 Options: 0`; List Files defaults to `6`,
  Styles to `1`, Line Draw to `1`). Two-line bottom menus use rows 23–24 (List Files, Table Edit,
  Tab Set, Keyboard Layout).

### 1.2 Text attributes inside menus
* Option number and mnemonic letter are **bold** (exact WP rule: the number and the separator
  `-` region are wrapped in bold; the mnemonic letter is flagged separately).
  Setup→Display→Menu Options→**Menu Letter Display** chooses the attribute used for the
  mnemonic letter (default: Bold); option numbers are always bold.
* Full-screen menu titles are bold, e.g. `Format: Line`.
* Column headers in list screens use the underline attribute (e.g. `Job  Document  Destination
  Print Options`, `Character   Meaning`).
* Menu text uses the Normal screen colour; bold = Bold screen colour; see display.md.

### 1.3 Control-code language used by WP's own strings (useful for a data-driven renderer)
Decoded from WP.FIL/WP.EXE/WPHELP.FIL:

| Byte | Meaning |
|---|---|
| 0x0C | clear screen (full-screen menu) |
| 0x10 *c* *r* | move cursor to column *c*, row *r* (0-based); *c* or *r* = 0xFE/0xFF → keep current |
| 0x0A | newline (col 0, next row) |
| 0x0D | carriage return / start of prompt [?] |
| 0x1D / 0x1C | bold on / off |
| 0x14 / 0x15 | underline (2nd attribute) on / off |
| 0x16 | next character is the **mnemonic letter** |
| 0x19 | move cursor one left (used to park the cursor on the default `0`) |
| 0x0E / 0x0F | blink on / off (used for `Block on`, `Macro Def`) [?] |
| 0x0B | clear to end of line [?] |
| 0x08, 0x11, 0x17 | prompt-line housekeeping (go to prompt line / clear it / row 23) [?] |
| 0x07 | beep |
| 0xFF *x* | display glyph *x* (e.g. 0xFF 0x1A = `→`) |

### 1.4 Menu keys
Number or mnemonic letter selects. `0`/Enter = no choice (exit menu). Cancel backs out one
level, Exit (F7) leaves all menus to the document (changes kept). In full-screen menus whose
items show a value (right column), selecting the item moves the cursor onto the value for
editing (Enter accepts, Cancel restores); Yes/No items take `Y`/`N`.
Value column: values are printed starting at about column 40 for Format/Setup menus [?exact
column; see appendix A for label positions — values start 2+ spaces after the longest label,
WP 5.1 uses a fixed column per menu].

---

## 2. Normal editing screen & status line

```
row 0 ..23: document text
row 24    : C:\WP51\LETTER.WP                      Doc 1 Pg 1 Ln 1" Pos 1"
```
* Left: full path of the document file (Setup→Display→Edit-Screen Options→Filename on the
  Status Line = Yes). Empty for an unnamed document. Replaced by `Typeover` (when typeover is
  on), `Block on` (blinking, while Block active), `Macro Def` (blinking, while recording),
  `Align char = .` (during Tab Align), `Outline` (outline mode on), `* Please wait *`,
  `* Timed backup *`, `* Merging *`, prompts and messages. Other indicators seen in WP.EXE:
  `Math `, `Action required in Printer Control`, `Document Conversion in Progress`.
* Right part, right-aligned so it ends at column 79 [?exact]: `Doc 1 Pg 1 Ln 1" Pos 1"`
  - Built from WP.EXE strings: `Col ` (only inside text columns, placed before `Doc`),
    `Cell ` (inside a table, e.g. `Cell A1 Doc 1 Pg 1 Ln 1.17" Pos 1.12"`) [?order], `Doc 1`,
    ` Pg `, ` Ln `, then ` Pos ` / ` POS `.
  - `Pos` is printed **`POS`** when Caps Lock is on.
  - The number after Pos is drawn in the *current text attribute* at the cursor (bold → Bold
    colour, underline → Underline colour, combination → combination colour). This is how the
    user knows whether bold/underline is on.
  - Units per Setup→Environment→Units of Measure→Status Line Display: default inches `"`:
    `Ln 1" Pos 1"`, `Ln 1.17" Pos 2.5"` (2 decimals, trailing zeros dropped [?]);
    `u` (4.2 units) shows `Ln 1 Pos 10`.
* Doc 2 shows `Doc 2`. In a header/footnote/etc. editing screen the left side shows e.g.
  `Footnote:  `, `Header A:  `, `Initial Codes:  `, `Style:  `, `Box:  `, `Spell Edit:  `,
  `Table Edit:  ` (WP.EXE) and the right side shows `Press Exit when done` [?] — e.g. the
  string `}Press {Exit} when done` exists.

---

## 3. F7 Exit

1. If the document was modified: row 24 `Save document? Yes (No)` (Yes default).
   If not modified, the same prompt with `(Text was not modified)` printed at col 51 [WP.FIL
   string at col 0x33].
2. Yes → `Document to be saved: C:\…\NAME` (pre-filled with current name; editable) → if the
   file exists: `Replace C:\…\NAME? No (Yes)` → `Saving C:\…\NAME` message.
3. Then `Exit WP? No (Yes)` with `(Cancel to return to document)` at col 48. When the other
   document (Doc 2) is active/non-empty the prompt is `Exit doc 1? No (Yes)` / `Exit doc 2? …`.
   No → clears the screen to a new empty document (stays in WP). Yes → quit WP (asks about
   Doc 2 if it holds a modified document).
* F7 in substructures (footnote, header, style, menus) = "done/exit".

## 4. F10 Save / Shift-F10 Retrieve
* `Document to be saved: ` (pre-filled with current filename) → existing: `Replace <name>? No (Yes)`.
  Block on: `Block name: `.  Saving message: `Saving <name>`.
* `Document to be retrieved: ` — empty Enter does nothing. If a document is on screen, the
  file is inserted at the cursor (no prompt from Shift-F10; from List Files: `Retrieve into
  current document? No (Yes)`).
* Password files: `Enter Password: `, `Re-Enter Password: `.
* Errors: `ERROR: File not found` etc. (see `extracted/wpfil_messages.txt` — full list of 150+
  error messages).

## 5. F5 List Files

Step 1 — row 24: `Dir C:\WP51\*.*` with `(Type = to change default Dir)` at col 49.
Edit the pattern and press Enter; `=` → `New directory = C:\WP51\` (changes default dir;
asks `Create C:\X? No (Yes)` if it does not exist).

Step 2 — full screen:
```
 0| 11-11-26 10:30a                Directory C:\WP51\*.*
 1| Document size:        0   Free:  2,555,904   Used:  1,234,567     Files:   123
 2|
 3|  .    Current     <Dir>                     │  ..   Parent      <Dir>
 4|  8514A   .VRS     5,226  06-29-10 12:00a   │  ATI     .VRS    17,491  06-29-10 12:00a
 . |  … two columns, sorted by name, highlight bar on current entry …
22|
23| 1 Retrieve; 2 Delete; 3 Move/Rename; 4 Print; 5 Short/Long Display;
24| 6 Look; 7 Other Directory; 8 Copy; 9 Find; N Name Search: 6
```
* Header label positions from WP.FIL/EXE: `Document size:` row 1; `Free:` col 26 row 1;
  `Used:` col 43 row 1; `Files:` col 64 row 1 (becomes `Marked:` when files are marked).
  Date/time template in WP.EXE: `00/00/00  00:00` plus `a`/`p` — US separators `-` in file
  entries, header date format [?]. `Directory ` label row 0.
* Entries: `FILENAME.EXT  size  date  time` (name padded to 8, `.`, 3-char ext, size with
  thousands commas, `<Dir>` for directories, `.  Current <Dir>` and `..  Parent <Dir>` first).
  Two columns separated by `│` [?]. Marked files show `*` after the name.
* Long display (5): header row 2 ` Descriptive Name               Type      Filename       Size    Revision Date`.
  Bottom: `1 Short Display; 2 Long Display: 0`.
* Mnemonics: R D M P S L O C F N (bold). `*` toggles mark; Home,`*` marks/unmarks all.
  `Delete C:\…\FILE? No (Yes)`; marked: `Delete marked files? No (Yes)` → `Marked files will
  be deleted.  Continue? No (Yes)`. Move: `New name: `; marked: `Move marked files to: `.
  Copy: `Copy this file to: ` / `Copy all marked files to: `. Print: `Page(s): (All)`.
  Find: `Find: 1 Name; 2 Doc Summary; 3 First Pg; 4 Entire Doc; 5 Conditions; 6 Undo: 0`,
  `Word pattern: `. Other Directory: `New directory = `. Name Search: type letters, status
  `(Name Search; Enter or arrows to Exit)`.
* **6 Look** (default, also Enter on a file): read-only view; top lines `File: C:\…\NAME`,
  `Size:` col 65, ` WP5.1`, ` Revised: `/` Created: `; bottom
  `Look: 1 Next Doc; 2 Prev Doc: 0` (or `…; 3 Look at Document Summary: 0`). Up/Down/PgUp/PgDn
  scroll; F2 searches. On a directory, Look/Enter lists that directory.

## 6. Search & Replace
* F2: row 24 `-> Srch: ` + previous search string; type text/codes (pressing function keys
  inserts their codes, e.g. F6 inserts `[BOLD]`); F2 or Esc(std kbd) starts. Up/Down arrow
  switches direction (`<- Srch:`). Not found: `* Not found *` (beep if Setup→Beep on Search
  Failure). Found: cursor placed just after the match.
* Home,F2: `-> Extended srch: ` / `<- Extended srch: `.
* Alt-F2: `w/Confirm? No (Yes)` → `-> Srch: ` → `Replace with: ` (F2 to start). With confirm,
  each hit is highlighted and the prompt `Confirm? No (Yes)` [?exact] appears.
* Case rule: lowercase in the search string matches both cases, uppercase matches only uppercase.
* Ctrl-X in a search string = wildcard (any one character) (shown as `^X`) [?].

## 7. F1 Cancel / Undelete
When nothing to cancel: the most recent deletion is re-inserted highlighted (block colour) at
the cursor and row 24 shows `Undelete: 1 Restore; 2 Previous Deletion: 0`. `2` (or Up/Down)
cycles through the last three deletions. `1` inserts it; Cancel removes the preview.

## 8. Esc / Repeat Value; Ctrl-Home Go To; Ctrl-V Compose
* `Repeat Value = 8`; `Go to `; `Key = ` (WP.EXE strings).
* Ctrl-PgDn: `Delete Remainder of page? No (Yes)`.
* Deleting a code in the normal screen (Reveal Codes off): `Delete [BOLD]? No (Yes)` — see
  reveal_codes.md §5.

## 9. Shift-F1 Setup (full menus)

Main (`Setup`): `1 - Mouse`, `2 - Display`, `3 - Environment`, `4 - Initial Settings`,
`5 - Keyboard Layout`, `6 - Location of Files` (rows 2,4,6,8,10,12; mnemonics M D E I K L).

* **1 Mouse** (`Setup: Mouse`): 1 Type, 2 Port, 3 Double Click Interval (1 = .01 sec),
  4 Submenu Delay Time (1 = .01 sec), 5 Acceleration Factor, 6 Left-Handed Mouse,
  7 Assisted Mouse Pointer Movement. Type = list screen `Setup: Mouse Type`
  (`1 Select; 2 Auto-select; 3 Other Disk; N Name Search: 1`); Port: `Port: 1 COM1; 2 COM2; 3 COM3; 4 COM4`.
* **2 Display** (`Setup: Display`): 1 Colors/Fonts/Attributes, 2 Graphics Screen Type,
  3 Text Screen Type, 4 Menu Options, 5 View Document Options, 6 Edit-Screen Options.
  - Colors/Fonts/Attributes → depends on card. For VGA: `Setup: Colors/Fonts` 1 Screen Colors,
    2 Italics Font, 8 Foreground Colors, 3 Underline Font, 8 Foreground Colors, 4 Small Caps
    Font, 8 Foreground Colors, 5 512 Characters, 8 Foreground Colors, 6 Normal Font Only, 16
    Foreground Colors. Screen Colors = grid editor `Setup: Colors` with columns
    `Attribute  Foreground  Background  Sample` (rows: Normal, Blocked, Underline, Strikeout,
    Bold, Double Underline, Redline, Shadow, Italics, Small Caps, Outline, Subscript,
    Superscript, Fine Print, Small Print, Large Print, Very Large Print, Extra Large Print,
    Bold & Underline, Other Combinations). Colours chosen by letter A–P [?]; Shift-F3 (Switch)
    edits Doc 2 colours; Ctrl-F4 (Move) copies attributes [?]. See display.md §2 for values.
  - Text Screen Type → list `Setup: Text Screen Driver` (her setting: `STANDARD.VRS`
    "80x25 16 Color"; also "80x43 16 Color").
  - Menu Options (`Setup: Menu Options`): 1 Menu Letter Display; *Pull-Down Menu*: 2 Pull-Down
    Letter Display, 3 Pull-Down Text, 4 Alt Key Selects Pull-Down Menu; *Menu Bar*: 5 Menu Bar
    Letter Display, 6 Menu Bar Text, 7 Menu Bar Separator Line, 8 Menu Bar Remains Visible.
    Attribute choice: `1 Size; 2 Appearance; 3 Normal` → appearance list (Bold/Undln/…).
  - View Document Options: 1 Text in Black & White, 2 Graphics in Black & White, 3 Bold
    Displayed with Color.
  - Edit-Screen Options: 1 Automatically Format and Rewrite (Yes), 2 Comments Display (Yes),
    3 Filename on the Status Line (Yes), 4 Hard Return Display Character (space = invisible),
    5 Merge Codes Display (Yes), 6 Reveal Codes Window Size (10 [?default]), 7 Side-by-side
    Columns Display (Yes).
* **3 Environment** (`Setup: Environment`): 1 Backup Options, 2 Beep Options, 3 Cursor Speed,
  4 Document Management/Summary, 5 Fast Save (unformatted), 6 Hyphenation, 7 Prompt for
  Hyphenation, 8 Units of Measure, 9 Alternate Keyboard (with sub-text
  `(F1 - Help, Esc - Cancel, F3 - Repeat)` on next row).
  - Backup (`Setup: Backup`): explanatory text, `Backup Directory`, 1 Timed Document Backup
    (`Minutes Between Backups`), 2 Original Document Backup (.BK!). Her backup dir:
    `C:\WP51\JOBS\AUTOBACK\` (from .SET).
  - Beep: 1 Beep on Error (No), 2 Beep on Hyphenation (Yes), 3 Beep on Search Failure (No).
  - Cursor Speed: `Characters Per Second: 1 15; 2 20; 3 30; 4 40; 5 50; 6 Normal` (default 50).
  - Document Management/Summary: 1 Create Summary on Save/Exit, 2 Subject Search Text (`RE:`),
    3 Long Document Names, 4 Default Document Type.
  - Hyphenation: `Hyphenation: 1 External Dictionary/Rules; 2 Internal Rules`.
  - Prompt for Hyphenation: `1 Never; 2 When Required; 3 Always` (default When Required).
  - Units of Measure (`Setup: Units of Measure`): 1 Display and Entry of Numbers for Margins,
    Tabs, etc.; 2 Status Line Display; legend `" = inches`, `i = inches`, `c = centimeters`,
    `p = points`, `w = 1200ths of an inch`, `u = WordPerfect 4.2 Units (Lines/Columns)`.
* **4 Initial Settings** (`Setup: Initial Settings`): 1 Merge, 2 Date Format, 3 Equations,
  4 Format Retrieved Documents for Default Printer, 5 Initial Codes, 6 Repeat Value,
  7 Table of Authorities, 8 Print Options.
  - Date Format screen: full table (appendix A "Date Format"), prompt `Date format: 3 1, 4`.
  - Initial Codes: an editing screen titled `Initial Codes:` in the status area with Reveal
    Codes on (split screen).
* **5 Keyboard Layout** (`Setup: Keyboard Layout`): list of `*.WPK` files; two-line menu
  `1 Select; 2 Delete; 3 Rename; 4 Create; 5 Copy; 6 Original; / 7 Edit; 8 Map; N Name search`.
  Edit screen `Keyboard: Edit` (columns `Key  Action  Description`), menu
  `1 Action; 2 Dscrptn; 3 Original; 4 Create; 5 Move; Macro: 6 Save; 7 Retrieve`.
  Map screen `Keyboard: Map` (Alt/Ctrl key grids).
* **6 Location of Files** (`Setup: Location of Files`): 1 Backup Files, 2 Keyboard/Macro Files,
  3 Thesaurus/Spell/Hyphenation (Main / Supplementary), 4 Printer Files, 5 Style Files
  (Library Filename), 6 Graphic Files, 7 Documents, 8 Spreadsheet Files.
  Her values (.SET): macros `C:\WP51\WPM\`, styles `C:\WP51\STYLE\` + `LIBRARY.STY`,
  graphics `C:\WP51\WPG\`, documents `C:\WP51\JOBS\`, printer `C:\WP51\`.

## 10. Shift-F8 Format

Main screen `Format` (appendix A, exact layout): 1 - Line (Hyphenation, Line Spacing,
Justification, Margins Left/Right, Line Height, Tab Set, Line Numbering, Widow/Orphan
Protection), 2 - Page (Center Page (top to bottom), Page Numbering, Force Odd/Even Page, Paper
Size/Type, Headers and Footers, Suppress, Margins Top/Bottom), 3 - Document (Display Pitch,
Redline Method, Initial Codes/Font, Summary), 4 - Other (Advance, Overstrike, Conditional End of
Page, Printer Functions, Decimal Characters, Underline Spaces/Tabs, Language, Border Options).

All submenu choices insert a code **at the cursor** (or at the start of the paragraph/page
where WP moves it [?]) — codes listed in reveal_codes.md.

* **Format: Line** — items with (default value):
  1 Hyphenation (No) · 2 Hyphenation Zone - Left (10%) / Right (4%) · 3 Justification (Full)
  → `Justification: 1 Left; 2 Center; 3 Right; 4 Full: 0` · 4 Line Height (Auto) →
  `1 Auto; 2 Fixed: 0` · 5 Line Numbering (No) → `Format: Line Numbering` screen · 6 Line
  Spacing (1) · 7 Margins - Left (1") / Right (1") · 8 Tab Set (Rel: -1", every 0.5") →
  Tab Set screen (§10.1) · 9 Widow/Orphan Protection (No).
* **Format: Page** — 1 Center Page (top to bottom) (No) · 2 Force Odd/Even Page →
  `1 Odd; 2 Even: 0` · 3 Headers → `1 Header A; 2 Header B: 0` → `1 Discontinue; 2 Every
  Page; 3 Odd Pages; 4 Even Pages; 5 Edit: 0` · 4 Footers (same with Footer A/B) · 5 Margins
  - Top (1") / Bottom (1") · 6 Page Numbering → `Format: Page Numbering` (1 New Page Number,
  2 Page Number Style, 3 Insert Page Number, 4 Page Number Position → diagram screen
  `Format: Page Number Position`, appendix A) · 7 Paper Size (8.5" x 11") / Type (Standard)
  → `Format: Paper Size/Type` list (`1 Select; 2 Add; 3 Copy; 4 Delete; 5 Edit; N Name
  Search: 1`) · 8 Suppress (this page only) → `Format: Suppress (this page only)`.
* **Format: Document** — 1 Display Pitch - Automatic (Yes) / Width (0.1") · 2 Initial Codes ·
  3 Initial Base Font · 4 Redline Method (`Redline Method: 1 Printer Dependent; 2 Left;
  3 Alternating`) · 5 Summary → `Document Summary` screen.
* **Format: Other** — 1 Advance (`Advance: 1 Up; 2 Down; 3 Line; 4 Left; 5 Right; 6 Position`)
  · 2 Conditional End of Page (`Number of Lines to Keep Together: `) · 3 Decimal/Align
  Character (.) / Thousands' Separator (,) · 4 Language (US) · 5 Overstrike (`1 Create;
  2 Edit`) · 6 Printer Functions → `Format: Printer Functions` · 7 Underline - Spaces (Yes) /
  Tabs (No) · 8 Border Options → `Format: Border Options`. End Centering/Alignment = item 9 in
  interim releases (inserts `[End C/A]`) [?].
* With Block on, Shift-F8 → `Protect block? No (Yes)`.

### 10.1 Tab Set screen (Shift-F8,1,8)
Upper part keeps the document; the bottom 3 rows [?] show a ruler and the two-line prompt:
```
row 21: ....L....L....L....L....L....L....L....L....L....L....L....L....L....L....L...
row 22: 0"   +1"  +2" … (position scale, relative to margin, in units)        [?]
row 23: Delete EOL (clear tabs); Enter Number (set tab); Del (clear tab);
row 24: Type; Left; Center; Right; Decimal; .= Dot Leader; Press Exit when done.
```
(The two text rows are the exact WP.FIL string; `Type`=T opens `Tab Type: 1 Absolute;
2 Relative to Margin`; letters L C R D set tab type at cursor, `.` toggles dot leader —
leader tabs show highlighted/with a dot [?]; Tab=L, Shift-F6=C, Alt-F6=R, Ctrl-F6=D in
interim release.) Tab stops are shown by letters `L C R D` on the ruler line; the ruler
shows positions with `^`/`.` tick marks [?]. Ctrl-End clears tabs right of cursor;
`n,m` Enter sets evenly spaced tabs (e.g. `1",0.5"`); Ctrl-Left/Right moves a stop; Up/Down
moves between stops. Exit (F7) saves → inserts `[Tab Set:…]`; Cancel aborts.

## 11. Shift-F7 Print
`Print` full screen: 1 Full Document, 2 Page, 3 Document on Disk, 4 Control Printer,
5 Multiple Pages, 6 View Document, 7 Initialize Printer (rows 2–8); `Options` (row 11) then
S - Select Printer, B - Binding Offset, N - Number of Copies, U - Multiple Copies Generated by,
G - Graphics Quality, T - Text Quality (rows 13–18), values to the right (e.g.
`Standard Printer`, `0"`, `1`, `WordPerfect`, `Medium`, `High`). `Selection: 0`.
* 3 → `Document name: ` → `Page(s): (All)`. 5 → `Page(s): `.
* 4 Control Printer → `Print: Control Printer` screen (Current Job / Job List, appendix A) with
  `1 Cancel Job(s); 2 Rush Job; 3 Display Jobs; 4 Go (start printer); 5 Stop: 0`.
* 6 View Document → graphics preview; text-mode fallback [?]; bottom
  `1 100%  2 200%  3 Full Page  4 Facing Pages: 3`. PgUp/PgDn change page. Exit/Cancel returns.
* S → `Print: Select Printer` list (her printers: Apple LaserWriter IINTX, HP LaserJet IIP,
  HP LaserJet Series II, PostScript Level 3 Printers (Color)/(Mono), PowerPak, Standard
  Printer, Star NX-2400) with `1 Select; 2 Additional Printers; 3 Edit; 4 Copy; 5 Delete;
  6 Help; 7 Update: 1`.
* G/T: `Graphics Quality: 1 Do Not Print; 2 Draft; 3 Medium; 4 High: 0`, same for Text.
* Block on: `Print block? No (Yes)`.

## 12. Ctrl-F8 Font
* `1 Size; 2 Appearance; 3 Normal; 4 Base Font; 5 Print Color: 0`
* Size: `1 Suprscpt; 2 Subscpt; 3 Fine; 4 Small; 5 Large; 6 Vry Large; 7 Ext Large: 0`
  (mnemonics P B F S L V E)
* Appearance: `1 Bold 2 Undln 3 Dbl Und 4 Italc 5 Outln 6 Shadw 7 Sm Cap 8 Redln 9 Stkout: 0`
  (no semicolons; mnemonics B U D I O A C R S)
* Normal: turns all attributes off (moves cursor past all open attribute end codes).
* Base Font: list screen of fonts (`1 Select; N Name search: 1`) → `[Font:…]`.
* Print Color: `Print Color` screen (appendix A).
* Block on: `Attribute: 1 Size; 2 Appearance: 0`.

## 13. Other function-key menus (all exact)
| Key | Prompt line |
|---|---|
| Ctrl-F1 | `1 Go to DOS; 2 DOS Command: 0` → DOS: screen cleared, `Type the DOS command 'Exit' to leave DOS.`; `DOS Command: `; after a command `Press any key to continue` (with WP Shell: `1 Go to Shell; Clipboard: 2 Save; 3 Append; 4 Retrieve; 5 DOS Command: 0`) |
| Ctrl-F2 | `Check: 1 Word; 2 Page; 3 Document; 4 New Sup. Dictionary; 5 Look Up; 6 Count: 0` (§14) |
| Ctrl-F3 | `1 Window; 2 Line Draw; 3 Rewrite: 0` → Window: `Number of lines in this window: 24`; Line Draw: `1 │; 2 ║; 3 *; 4 Change; 5 Erase; 6 Move: 1`, Change: `1 ░; 2 ▒; 3 ▓; 4 █; 5 ▄; 6 ▌; 7 ▐; 8 ▀; 9 Other: 0`, `Solid character: ` |
| Shift-F3 | switch doc (status `Doc 2`); Block on: `1 Uppercase; 2 Lowercase: 0` |
| Ctrl-F4 | `Move: 1 Sentence; 2 Paragraph; 3 Page; 4 Retrieve: 0` → `1 Move; 2 Copy; 3 Delete; 4 Append: 0` → `Move cursor; press Enter to retrieve.`; Block: `Move: 1 Block; 2 Tabular Column; 3 Rectangle: 0`; Retrieve: `Retrieve: 1 Block; 2 Tabular Column; 3 Rectangle: 0`; Append: `Append to: ` |
| Shift-F5 | `1 Date Text; 2 Date Code; 3 Date Format; 4 Outline; 5 Para Num; 6 Define: 0`; Outline: `Outline: 1 On; 2 Off; 3 Move Family; 4 Copy Family; 5 Delete Family: 0`; Para Num: `Paragraph Level (Press Enter for Automatic): `; Define → `Paragraph Number Definition` screen |
| Ctrl-F5 | `1 DOS Text; 2 Password; 3 Save As; 4 Comment; 5 Spreadsheet: 0`; DOS Text: `1 Save; 2 Retrieve (CR/LF to [HRt]); 3 Retrieve (CR/LF to [SRt] in HZone): 0`; Save As: `1 Generic; 2 WordPerfect 5.0; 3 WordPerfect 4.2: 0`; Password: `Password: 1 Add/Change; 2 Remove: 0`; Comment: `Comment: 1 Create; 2 Edit; 3 Convert to Text: 0` (editor screen `Document Comment` with double-line box); Spreadsheet: `Spreadsheet: 1 Import; 2 Create Link; 3 Edit Link; 4 Link Options: 0` |
| Alt-F5 | Block off: `1 Cross-Ref; 2 Subdoc; 3 Index; 4 ToA Short Form; 5 Define; 6 Generate: 0`; Block on: `Mark for: 1 ToC; 2 List; 3 Index; 4 ToA` [? which prefix] → `ToC Level: `, `List Number: `, `Index heading: `, `Subheading: `; Define → `Mark Text: Define`; Generate → `Mark Text: Generate` → `Existing tables, lists, and indexes will be replaced.  Continue? Yes (No)`, progress `Generation in progress. Pass: n, Page: n` |
| Ctrl-F6 | `[Dec Tab]`, status `Align char = .` |
| Ctrl-F7 | `1 Footnote; 2 Endnote; 3 Endnote Placement: 0` → `Footnote: 1 Create; 2 Edit; 3 New Number; 4 Options: 0` (`Footnote number? `); Create opens footnote editor (status `Footnote:  `, Exit when done) |
| Alt-F7 | `1 Columns; 2 Tables; 3 Math: 0`; Columns: `Columns: 1 On; 2 Off; 3 Define: 0` → `Text Column Definition` screen (`Column Type: 1 Newspaper; 2 Parallel; 3 Parallel with Block Protect: 0`); Tables: `Table: 1 Create; 2 Edit: 0` → `Number of Columns: 3`, `Number of Rows: 1`; Math: `Math: 1 On; 2 Off; 3 Define; 4 Calculate: 0` |
| Table Edit (Alt-F7 in table) | row 23 `Table Edit:  Press Exit when done` + cell/position; row 23/24: `Ctrl-Arrows Column Widths; Ins Insert; Del Delete; Move Move/Copy;` / `1 Size; 2 Format; 3 Lines; 4 Header; 5 Math; 6 Options; 7 Join; 8 Split: 0`; sub-menus in appendix B (`Lines: 1 Left; 2 Right; …`, `Format: 1 Cell; 2 Column; 3 Row Height`, …) |
| Alt-F8 | `Styles` list screen (`Name  Type  Description`), `1 On; 2 Off; 3 Create; 4 Edit; 5 Delete; 6 Save; 7 Retrieve; 8 Update: 1`; `Styles: Edit` screen |
| Shift-F9 | `1 Field; 2 End Record; 3 Input; 4 Page Off; 5 Next Record; 6 More: 0` (More → command list box) |
| Ctrl-F9 | `1 Merge; 2 Sort; 3 Convert Old Merge Codes: 0`; Merge: `Primary file: ` → `Secondary file: `, status `* Merging *`; Sort: `Input file to sort: (Screen)` → `Output file for sort: (Screen)` → Sort screen (`-------- Sort by Line --------` divider, keys table) `1 Perform Action; 2 View; 3 Keys; 4 Select; 5 Action; 6 Order; 7 Type: 0` |
| Alt-F9 | `1 Figure; 2 Table Box; 3 Text Box; 4 User Box; 5 Line; 6 Equation: 0` → `Figure: 1 Create; 2 Edit; 3 New Number; 4 Options: 0` → `Definition: Figure` screen (1 Filename, 2 Contents, 3 Caption, 4 Anchor Type, 5 Vertical Position, 6 Horizontal Position, 7 Size, 8 Wrap Text Around Box, 9 Edit); Line: `Create Line: 1 Horizontal; 2 Vertical; Edit Line: 3 Horizontal; 4 Vertical: 0` |
| Ctrl-F10 | `Define macro: ` (Alt-letter, name, or Enter) → `Description: ` → status `Macro Def` blinking; existing: `ALTA.WPM Already Exists: 1 Replace; 2 Edit; 3 Description: 0`. Ctrl-PgUp while defining: `1 Pause; 2 Display; 3 Assign; 4 Comment: 0` (`Variable: `, `Value: `, `Comment: `) |
| Alt-F10 | `Macro: ` |
| F3 Help | §15 |

## 14. Spell (Ctrl-F2) and Thesaurus (Alt-F1)
* **Spell**: screen splits: document text in the upper part (rows 0–~11), a double-line
  separator, word list in the lower part as columns `A. word   B. word   C. word …`
  (letters bold), then the prompt line(s):
  `Not Found: 1 Skip Once; 2 Skip; 3 Add; 4 Edit; 5 Look Up; 6 Ignore Numbers: 0`
  (typing a list letter replaces the word; more suggestions: `Press Enter for more words`).
  Double word: `Double Word: 1 2 Skip; 3 Delete 2nd; 4 Edit; 5 Disable Double Word Checking`.
  Irregular case: `Irregular Case: 1 2 Skip; 3 Replace; 4 Edit; 5 Disable Case Checking`.
  `Word count: n` at the end (`Press any key to continue`). Look up: `Word or word pattern: `.
  Supplementary dictionary: `Supplemental dictionary name: `. Status `* Please wait *`.
  Her install has German, French, Spanish and US dictionaries (`WP{WP}DE.LEX`, …) — language
  chosen by `[Lang:XX]` code (Shift-F8,4,4).
* **Thesaurus**: top 4–5 rows show the document; rows below show up to three columns of
  words. Each column has a headword (bold) and groups (n)/(v)/(a)/(ant) with entries; a
  column of bold reference letters A,B,C… is shown next to the active column; entries marked
  with a bullet `•` are headwords themselves. Prompt:
  `1 Replace Word; 2 View Doc; 3 Look Up Word; 4 Clear Column: 0`; `Press letter for word`;
  `Word: `; `Word not found`. Arrows move the letter column between columns.

## 15. F3 Help (F1 in her keyboard)
Opening screen = appendix A "Help" (plus at top-right `WP 5.1  mm/dd/yy` [?] and
`License #: …`). Row 24: `Selection: 0` and `(Press ENTER to exit Help)` at col 50.
A letter → alphabetical feature index ("Features [A]" — columns `Feature`, `WordPerfect Key`
at col 39, `Keystrokes` at col 57, e.g. `Block    Block    Alt-F4`); `More... Press a to continue.`
A function key → that key's help (keys with menus: press menu number for sub-help). F3 again
→ keyboard template (Enhanced layout; `Press 1 to view the PC/XT keyboard template`). Enter or
Space exits. All 413 help screens: `extracted/wphelp_screens.txt` (text + positions; bold
shown as `{}`, second attribute `«»`, mnemonic marker `_`).

## 16. Shift-F3 Switch / Doc 2, Ctrl-F3 Window
* Two independent documents; status shows `Doc 1`/`Doc 2`. Colours of Doc 2 may differ (Setup
  colours, Switch while in the colour editor) — her .SET has a second table (Normal = white on
  cyan) [? which table is Doc 2].
* Window split: each window has its own status line; a ruler/tab line separates them
  (same ruler as Reveal Codes) [?].

## 17. Alt-= Menu Bar (pull-downs)
Row 0: `File  Edit  Search  Layout  Mark  Tools  Font  Graphics  Help` (first letters
highlighted). Pull-down items (with shortcut keys right-aligned) were found verbatim in WP.FIL,
e.g. File: `Retrieve   Shft-F10`, `Save       F10`, `Text In    Ctrl-F5`, `Text Out   Ctrl-F5`,
`Password   Ctrl-F5`, `List Files F5`, `Summary`, `Print      Shft-F7`, `Setup      Shft-F1`,
`Go to DOS  Ctrl-F1`, `Exit       F7`; Edit: `Copy  Ctrl-Ins`, `Paste`, `Append`, `Delete  Del`, `Undelete  F1`, `Block  Alt-F4`, `Select`, `Comment  Ctrl-F5`, `Convert Case  Shft-F3`, `Protect Block  Shft-F8`, `Switch Document  Shft-F3`, `Window  Ctrl-F3`, `Reveal Codes  Alt-F3` (the first Edit item, Move/Cut, was not isolated [?]; note WP.FIL also has `Undelete  Esc` for the alternate keyboard) — full raw list:
`extracted/wpfil_pulldown_raw.txt`. Unavailable items in `[ ]`. Low priority for this user.

---

## Appendix A — all full-screen menus in WP.FIL (exact rows/columns; left number = screen row)

Values (current settings) are drawn by the program at run time and are therefore absent here. Bold/mnemonic markup is omitted in this rendering; see the raw strings in `extracted/wpfil_menus.txt` (`{}` = bold, `_` = mnemonic).

#### Print: Control Printer  (WP.FIL string #0)

```
 0|Print: Control Printer
 1|
 2|Current Job
 3|
 4|Job Number:                                    Page Number:
 5|Status:                                        Current Copy:
 6|Message:
 7|Paper:
 8|Location:
 9|Action:
10|
11|
12|Job List
13|
14|Job  Document             Destination          Print Options
15|
16|
17|
18|
19|                                                            Additional Jobs Not
```

#### Format: Page  (WP.FIL string #1)

```
 0|Format: Page
 1|
 2|     1 - Center Page (top to bottom)
 3|
 4|     2 - Force Odd/Even Page
 5|
 6|     3 - Headers
 7|
 8|     4 - Footers
 9|
10|     5 - Margins - Top
11|                   Bottom
12|
13|     6 - Page Numbering
14|
15|     7 - Paper Size
16|               Type
17|
18|     8 - Suppress (this page only)
```

#### Format: Line  (WP.FIL string #5)

```
 0|Format: Line
 1|
 2|     1 - Hyphenation
 3|
 4|     2 - Hyphenation Zone - Left
 5|                            Right
 6|
 7|     3 - Justification
 8|
 9|     4 - Line Height
10|
11|     5 - Line Numbering
12|
13|     6 - Line Spacing
14|
15|     7 - Margins - Left
16|                   Right
17|
18|     8 - Tab Set
19|
20|     9 - Widow/Orphan Protection
```

#### Format: Printer Functions  (WP.FIL string #7)

```
 0|Format: Printer Functions
 1|
 2|     1 - Kerning
 3|
 4|     2 - Printer Command
 5|
 6|     3 - Word Spacing
 7|         Letter Spacing
 8|
 9|     4 - Word Spacing Justification Limits
10|           Compressed to (0% - 100%)
11|           Expanded to (100% - unlimited)
12|
13|     5 - Baseline Placement for Typesetters
14|           (First baseline at top margin)
15|
16|     6 - Leading Adjustment
17|           Primary   - [SRt]
18|           Secondary - [HRt]
```

#### Format: Page Number Position  (WP.FIL string #9)

```
 0|Format: Page Number Position
 1|
 2|
 3|      Every Page               Alternating Pages
 4|     ┌───────────┐        ┌───────────┐ ┌───────────┐
 5|     │ 1   2   3 │        │ 4         │ │         4 │
 6|     │           │        │           │ │           │
 7|     │           │        │           │ │           │
 8|     │           │        │ Even      │ │       Odd │
 9|     │           │        │           │ │           │
10|     │           │        │           │ │           │
11|     │ 5   6   7 │        │ 8         │ │         8 │
12|     └───────────┘        └───────────┘ └───────────┘
13|
14|
15|     9 - No Page Numbers
```

#### Format: Suppress (this page only)  (WP.FIL string #10)

```
 0|Format: Suppress (this page only)
 1|
 2|     1 - Suppress All Page Numbering, Headers and Footers
 3|
 4|     2 - Suppress Headers and Footers
 5|
 6|     3 - Print Page Number at Bottom Center
 7|
 8|     4 - Suppress Page Numbering
 9|
10|     5 - Suppress Header A
11|
12|     6 - Suppress Header B
13|
14|     7 - Suppress Footer A
15|
16|     8 - Suppress Footer B
```

#### Merge: DOS Text File  (WP.FIL string #14)

```
 0|Merge: DOS Text File
 1|
 2|     1 - Field delimiters  - Begin
 3|                             End
 4|
 5|     2 - Record delimiters - Begin
 6|                             End
```

#### Select Printer: COM Port  (WP.FIL string #21)

```
 0|Select Printer: COM Port
 1|
 2|     1 - Baud
 3|
 4|     2 - Parity
 5|
 6|     3 - Stop Bits (1 or 2)
 7|
 8|     4 - Character Length (7 or 8)
 9|
10|     5 - XON/XOFF
```

#### Format: Other  (WP.FIL string #25)

```
 0|Format: Other
 1|
 2|     1 - Advance
 3|
 4|     2 - Conditional End of Page
 5|
 6|     3 - Decimal/Align Character
 7|         Thousands' Separator
 8|
 9|     4 - Language
10|
11|     5 - Overstrike
12|
13|     6 - Printer Functions
14|
15|     7 - Underline - Spaces
16|                     Tabs
17|
18|     8 - Border Options
```

#### Math Definition           Use arrow keys to position cursor  (WP.FIL string #29)

```
 0|Math Definition           Use arrow keys to position cursor
 1|
 2|Columns                   A B C D E F G H I J K L M N O P Q R S T U V W X
 3|
 4|Type
 5|
 6|Negative Numbers
 7|
 8|Number of Digits to
 9|  the Right (0-4)
10|
11|Calculation   1
12|  Formulas    2
13|              3
14|              4
15|
16|Type of Column:
17|      0 = Calculation    1 = Text     2 = Numeric    3 = Total
18|
19|Negative Numbers
20|      ( = Parentheses (50.00)         - = Minus Sign  -50.00Press Exit when done
```

#### Text Column Definition  (WP.FIL string #33)

```
 0|Text Column Definition
 1|
 2|     1 - Type
 3|
 4|     2 - Number of Columns
 5|
 6|     3 - Distance Between Columns
 7|
 8|     4 - Margins
 9|
10|     Column    Left      Right     Column    Left      Right
11|      1:                           13:
12|      2:                           14:
13|      3:                           15:
14|      4:                           16:
15|      5:                           17:
16|      6:                           18:
17|      7:                           19:
18|      8:                           20:
19|      9:                           21:
20|     10:                           22:
21|     11:                           23:
22|     12:                           24:
```

#### Printer file not found  (WP.FIL string #35)

```
 0|Printer file not found
 1|
 2|     The printer file (.ALL) which was used to create this printer
 3|     selection cannot be found.
 4|
 5|     Please specify the directory where the printer files can be found.
 6|     If you do not know which printer disk contains the printer file, try
 7|     each disk until one succeeds.
```

#### Definition for Table of Authorities  (WP.FIL string #36)

```
 0|Definition for Table of Authorities
 1|
 2|     1 - Dot Leaders
 3|
 4|     2 - Underlining Allowed
 5|
 6|     3 - Blank Line Between Authorities
```

#### Outline Styles  (WP.FIL string #40)

```
 0|Outline Styles
 1|
 2|  Name               Description
```

#### Outline Styles: Edit  (WP.FIL string #43)

```
 0|Outline Styles: Edit
 1|
 2|
 3|
 4|
 5|
 6|
 7|  Level  Type      Enter
```

#### Setup: Beep Options  (WP.FIL string #44)

```
 0|Setup: Beep Options
 1|
 2|     1 - Beep on Error
 3|
 4|     2 - Beep on Hyphenation
 5|
 6|     3 - Beep on Search Failure
```

#### Setup: Initial Settings  (WP.FIL string #45)

```
 0|Setup: Initial Settings
 1|
 2|     1 - Merge
 3|
 4|     2 - Date Format
 5|
 6|     3 - Equations
 7|
 8|     4 - Format Retrieved Documents
 9|           for Default Printer
10|
11|     5 - Initial Codes
12|
13|     6 - Repeat Value
14|
15|     7 - Table of Authorities
16|
17|     8 - Print Options
```

#### Job List  (WP.FIL string #54)

```
 0|Job List
 1|
 2|Job  Document             Destination          Print Options
```

#### Setup: Units of Measure  (WP.FIL string #55)

```
 0|Setup: Units of Measure
 1|
 2|     1 - Display and Entry of Numbers
 3|           for Margins, Tabs, etc.
 4|
 5|     2 - Status Line Display
 6|
 7|
 8|Legend:
 9|
10|     " = inches
11|     i = inches
12|     c = centimeters
13|     p = points
14|     w = 1200ths of an inch
15|     u = WordPerfect 4.2 Units (Lines/Columns)
```

#### Setup: Location of Files  (WP.FIL string #56)

```
 0|Setup: Location of Files
 1|
 2|     1 - Backup Files
 3|
 4|     2 - Keyboard/Macro Files
 5|
 6|     3 - Thesaurus/Spell/Hyphenation
 7|                        Main
 8|                        Supplementary
 9|
10|     4 - Printer Files
11|
12|     5 - Style Files
13|               Library Filename
14|
15|     6 - Graphic Files
16|
17|     7 - Documents
18|
19|     8 - Spreadsheet Files
```

#### Date Format  (WP.FIL string #57)

```
 0|Date Format
 1|
 2|     Character   Meaning
 3|        1        Day of the Month
 4|        2        Month (number)
 5|        3        Month (word)
 6|        4        Year (all four digits)
 7|        5        Year (last two digits)
 8|        6        Day of the Week (word)
 9|        7        Hour (24-hour clock)
10|        8        Hour (12-hour clock)
11|        9        Minute
12|        0        am / pm
13|        %,$      Used before a number, will:
14|                   Pad numbers less than 10 with a leading zero or space
15|                   Abbreviate the month or day of the week
16|
17|     Examples:  3 1, 4       = December 25, 1984
18|                %6 %3 1, 4   = Tue Dec 25, 1984
19|                %2/%1/5 (6)  = 01/01/85 (Tuesday)
20|                $2/$1/5 (%6) =  1/ 1/85 (Tue)
21|                8:90         = 10:55am
22|
23|
24|Date format:
```

#### Setup: Document Summary  (WP.FIL string #58)

```
 0|Setup: Document Summary
 1|
 2|     1 - Create on Save/Exit
 3|
 4|     2 - Subject Search Text
```

#### Footnote Options  (WP.FIL string #67)

```
 0|Footnote Options
 1|
 2|     1 - Spacing Within Footnotes
 3|                 Between Footnotes
 4|
 5|     2 - Amount of Note to Keep Together
 6|
 7|     3 - Style for Number in Text
 8|
 9|     4 - Style for Number in Note
10|
11|     5 - Footnote Numbering Method
12|
13|     6 - Start Footnote Numbers each Page
14|
15|     7 - Line Separating Text and Footnotes
16|
17|     8 - Print Continued Message
18|
19|     9 - Footnotes at Bottom of Page
```

#### Setup: Backup  (WP.FIL string #68)

```
 0|Setup: Backup
 1|
 2|     Timed backup files are deleted when you exit WP normally.  If you
 3|     have a power or machine failure, you will find the backup file in the
 4|     backup directory indicated in Setup: Location of Files.
 5|
 6|         Backup Directory
 7|
 8|     1 - Timed Document Backup
 9|         Minutes Between Backups
10|
11|
12|     Original backup will save the original document with a .BK! extension
13|     whenever you replace it during a Save or Exit.
14|
15|     2 - Original Document Backup
```

#### Print  (WP.FIL string #74)

```
 0|Print
 1|
 2|     1 - Full Document
 3|     2 - Page
 4|     3 - Document on Disk
 5|     4 - Control Printer
 6|     5 - Multiple Pages
 7|     6 - View Document
 8|     7 - Initialize Printer
 9|
10|
11|Options
12|
13|     S - Select Printer
14|     B - Binding Offset
15|     N - Number of Copies
16|     U - Multiple Copies Generated by
17|     G - Graphics Quality
18|     T - Text Quality
```

#### Paragraph Number Definition  (WP.FIL string #79)

```
 0|Paragraph Number Definition
 1|
 2|     1 - Starting Paragraph Number
 3|           (in legal style)
 4|                                                   Levels
 5|                                 1     2     3     4     5     6     7     8
 6|     2 - Paragraph               1.    a.    i.   (1)   (a)   (i)    1)    a)
 7|     3 - Outline                 I.    A.    1.    a.   (1)   (a)    i)    a)
 8|     4 - Legal (1.1.1)           1    .1    .1    .1    .1    .1    .1    .1
 9|     5 - Bullets                             -     ■     *     +     ∙     x
10|     6 - User-defined
11|
12|     Current Definition
13|     Attach Previous Level
14|
15|     7 - Enter Inserts Paragraph Number
16|
17|     8 - Automatically Adjust to Current Level
18|
19|     9 - Outline Style Name
```

#### 1 - No Page Numbers  (WP.FIL string #85)

```
 0|
 1|
 2|     1 - No Page Numbers
 3|
 4|     2 - Page Numbers Follow Entries
 5|
 6|     3 - (Page Numbers) Follow Entries
 7|
 8|     4 - Flush Right Page Numbers
 9|
10|     5 - Flush Right Page Numbers with Leaders
```

#### List .  Definition  (WP.FIL string #86)

```
 0|List .  Definition
```

#### Index Definition  (WP.FIL string #87)

```
 0|Index Definition
```

#### Table of Contents Definition  (WP.FIL string #88)

```
 0|Table of Contents Definition
 1|
 2|     1 - Number of Levels
 3|
 4|     2 - Display Last Level in
 5|           Wrapped Format
 6|
 7|     3 - Page Numbering - Level 1
 8|                          Level 2
 9|                          Level 3
10|                          Level 4
11|                          Level 5
```

#### Printer files not found  (WP.FIL string #98)

```
 0|
 1|
 2|Printer files not found
 3|
 4|     Use the Other Disk option to specify a directory for the printer
 5|     files.  Continue to use this option until you find the disk with the
 6|     printer you want.
```

#### Help  (WP.FIL string #99)

```
 0|Help
 1|
 2|
 3|     Press any letter to get an alphabetical list of features.
 4|
 5|          The list will include the features that start with that letter,
 6|          along with the name of the key where the feature is found.  You
 7|          can then press that key to get a description of how the feature
 8|          works.
 9|
10|
11|     Press any function key to get information about the use of the key.
12|
13|          Some keys may let you choose from a menu to get more information
14|          about various options.  Press HELP again to display the template.
```

#### Type the DOS command 'Exit' to leave DOS.  (WP.FIL string #104)

```
 0|Type the DOS command 'Exit' to leave DOS.
```

#### Setup: Print Options  (WP.FIL string #105)

```
 0|Setup: Print Options
 1|
 2|     1 - Binding Offset
 3|
 4|     2 - Number of Copies
 5|         Multiple Copies Generated by
 6|
 7|     3 - Graphics Quality
 8|
 9|     4 - Text Quality
10|
11|     5 - Redline Method
12|
13|     6 - Size Attribute Ratios - Fine
14|         (% of Normal)          Small
15|                                Large
16|                           Very Large
17|                          Extra Large
18|                      Super/Subscript
```

#### Mark Text: Define  (WP.FIL string #121)

```
 0|Mark Text: Define
 1|
 2|     1 - Define Table of Contents
 3|
 4|     2 - Define List
 5|
 6|     3 - Define Index
 7|
 8|     4 - Define Table of Authorities
 9|
10|     5 - Edit Table of Authorities Full Form
```

#### Format: Line Numbering  (WP.FIL string #122)

```
 0|Format: Line Numbering
 1|
 2|     1 - Count Blank Lines
 3|
 4|     2 - Number Every n Lines, where n is
 5|
 6|     3 - Position of Number from Left Edge
 7|
 8|     4 - Starting Number
 9|
10|     5 - Restart Numbering on Each Page
```

#### Document Summary  (WP.FIL string #125)

```
 0|Document Summary
 1|
 2|         Revision Date
 3|
 4|     1 - Creation Date
 5|
 6|     2 - Document Name
 7|         Document Type
 8|
 9|     3 - Author
10|         Typist
11|
12|     4 - Subject
13|
14|     5 - Account
15|
16|     6 - Keywords
17|
18|     7 - Abstract                 (Retrieve to capture; Del to remove summary)
```

#### Print  (WP.FIL string #126)

```
 0|Print
 1|
 2|     1 - Full Document
 3|     2 - Page
 4|     3 - Document on Disk
 5|     4 - Control Printer
 6|     5 - Multiple Pages
 7|     6 - View Document
 8|     7 - Initialize Printer
 9|
10|
11|Options
12|
13|     S - Select Printer
14|     B - Binding Offset
15|     N - Number of Copies
16|     U - Multiple Copies Generated by
17|     G - Graphics Quality
18|     T - Text Quality
19|     A - Banners
20|     O - Form Number
```

#### Document Comment  (WP.FIL string #127)

```
 0|Document Comment
 1|
 2|╔═════════════════════════════════════════════════════════════════════════════╗
 3|║                                                                             ║
 4|║                                                                             ║
 5|║                                                                             ║
 6|║                                                                             ║
 7|║                                                                             ║
 8|║                                                                             ║
 9|║                                                                             ║
10|║                                                                             ║
11|║                                                                             ║
12|║                                                                             ║
13|║                                                                             ║
14|║                                                                             ║
15|║                                                                             ║
16|║                                                                             ║
17|║                                                                             ║
18|║                                                                             ║
19|╚═════════════════════════════════════════════════════════════════════════════╝
```

#### Select Printer: Edit  (WP.FIL string #134)

```
 0|Select Printer: Edit
 1|
 2|         Filename
 3|
 4|     1 - Name
 5|
 6|     2 - Port
 7|
 8|     3 - Sheet Feeder
 9|
10|     4 - Cartridges and Fonts
11|
12|     5 - Initial Base Font
13|
14|     6 - Path for Downloadable
15|           Fonts and Printer
16|           Command Files
17|
18|     7 - Print to Hardware Port
```

#### Endnote Options  (WP.FIL string #135)

```
 0|Endnote Options
 1|
 2|     1 - Spacing Within Endnotes
 3|                 Between Endnotes
 4|
 5|     2 - Amount of Endnote to Keep Together
 6|
 7|     3 - Style for Numbers in Text
 8|
 9|     4 - Style for Numbers in Note
10|
11|     5 - Endnote Numbering Method
```

#### Definition:  (WP.FIL string #138)

```
 0|Definition:
 1|
 2|     1 - Filename
 3|
 4|     2 - Contents
 5|
 6|     3 - Caption
 7|
 8|     4 - Anchor Type
 9|
10|     5 - Vertical Position
11|
12|     6 - Horizontal Position
13|
14|     7 - Size
15|
16|     8 - Wrap Text Around Box
17|
18|     9 - Edit
```

#### Options:  (WP.FIL string #144)

```
 0|Options:
 1|
 2|     1 - Border Style
 3|              Left
 4|              Right
 5|              Top
 6|              Bottom
 7|     2 - Outside Border Space
 8|              Left
 9|              Right
10|              Top
11|              Bottom
12|     3 - Inside Border Space
13|              Left
14|              Right
15|              Top
16|              Bottom
17|     4 - First Level Numbering Method
18|     5 - Second Level Numbering Method
19|     6 - Caption Number Style
20|     7 - Position of Caption
21|     8 - Minimum Offset from Paragraph
22|     9 - Gray Shading (% of black)
```

#### Print Color  (WP.FIL string #146)

```
 0|Print Color
 1|
 2|                               Primary Color Mixture
 3|                              Red     Green      Blue
 4|
 5|     1 - Black
 6|     2 - White
 7|     3 - Red
 8|     4 - Green
 9|     5 - Blue
10|     6 - Yellow
11|     7 - Magenta
12|     8 - Cyan
13|     9 - Orange
14|     A - Gray
15|     N - Brown
16|     O - Other
17|
18|     Current Color
```

#### Tie Reference to:  (WP.FIL string #154)

```
 0|Tie Reference to:
 1|
 2|     1 - Page Number
 3|
 4|     2 - Paragraph/Outline Number
 5|
 6|     3 - Footnote Number
 7|
 8|     4 - Endnote Number
 9|
10|     5 - Graphics Box Number
```

#### Format  (WP.FIL string #155)

```
 0|Format
 1|
 2|     1 - Line
 3|              Hyphenation                    Line Spacing
 4|              Justification                  Margins Left/Right
 5|              Line Height                    Tab Set
 6|              Line Numbering                 Widow/Orphan Protection
 7|
 8|     2 - Page
 9|              Center Page (top to bottom)    Page Numbering
10|              Force Odd/Even Page            Paper Size/Type
11|              Headers and Footers            Suppress
12|              Margins Top/Bottom
13|
14|     3 - Document
15|              Display Pitch                  Redline Method
16|              Initial Codes/Font             Summary
17|
18|     4 - Other
19|              Advance                        Overstrike
20|              Conditional End of Page        Printer Functions
21|              Decimal Characters             Underline Spaces/Tabs
22|              Language                       Border Options
```

#### Styles  (WP.FIL string #157)

```
 0|Styles
 1|
 2|  Name         Type     Description
```

#### Styles: Edit  (WP.FIL string #158)

```
 0|Styles: Edit
 1|
 2|     1 - Name
 3|
 4|     2 - Type
 5|
 6|     3 - Description
 7|
 8|     4 - Codes
```

#### Mark Text: Cross-Reference  (WP.FIL string #159)

```
 0|Mark Text: Cross-Reference
 1|
 2|     1 - Mark Reference
 3|
 4|     2 - Mark Target
 5|
 6|     3 - Mark Both Reference and Target
```

#### Format: Paper Type  (WP.FIL string #166)

```
 0|Format: Paper Type
 1|
 2|     1 - Standard
 3|
 4|     2 - Bond
 5|
 6|     3 - Letterhead
 7|
 8|     4 - Labels
 9|
10|     5 - Envelope
11|
12|     6 - Transparency
13|
14|     7 - Cardstock
15|
16|     8 - Other
```

#### Setup: Print Options  (WP.FIL string #168)

```
 0|Setup: Print Options
 1|
 2|     1 - Binding Offset
 3|
 4|     2 - Number of Copies
 5|         Multiple Copies Generated by
 6|
 7|     3 - Graphics Quality
 8|
 9|     4 - Text Quality
10|
11|     5 - Redline Method
12|
13|     6 - Size Attribute Ratios - Fine
14|         (% of Normal)          Small
15|                                Large
16|                           Very Large
17|                          Extra Large
18|                      Super/Subscript
19|
20|     7 - Banners
21|
22|     8 - Form Number
```

#### Format: Document  (WP.FIL string #169)

```
 0|Format: Document
 1|
 2|     1 - Display Pitch - Automatic
 3|                         Width
 4|
 5|     2 - Initial Codes
 6|
 7|     3 - Initial Base Font
 8|
 9|     4 - Redline Method
10|
11|     5 - Summary
```

#### Format: Initial Codes  (WP.FIL string #170)

```
 0|Format: Initial Codes
```

#### Setup: Screen Drivers  (WP.FIL string #173)

```
 0|Setup: Screen Drivers
 1|
 2|     1 - Graphic Screen Drivers
 3|
 4|     2 - Text Screen Drivers
 5|
 6|     3 - Display Options
```

#### Mark Text: Generate  (WP.FIL string #176)

```
 0|Mark Text: Generate
 1|
 2|     1 - Remove Redline Markings and Strikeout Text from Document
 3|
 4|     2 - Compare Screen and Disk Documents and Add Redline and Strikeout
 5|
 6|     3 - Expand Master Document
 7|
 8|     4 - Condense Master Document
 9|
10|     5 - Generate Tables, Indexes, Cross-References, etc.
```

#### Setup  (WP.FIL string #177)

```
 0|Setup
 1|
 2|     1 - Mouse
 3|
 4|     2 - Display
 5|
 6|     3 - Environment
 7|
 8|     4 - Initial Settings
 9|
10|     5 - Keyboard Layout
11|
12|     6 - Location of Files
```

#### Setup: Attributes  (WP.FIL string #179)

```
 0|Setup: Attributes
 1|
 2|Attribute                Blink  Bold  Blocked  Underline  Normal  Sample
```

#### Setup: Colors  (WP.FIL string #180)

```
 0|Setup: Colors
 1|
 2|     1 - Screen Colors
 3|
 4|     2 - Fast Text
 5|           (may cause snow on screen)
```

#### Setup: Colors  (WP.FIL string #186)

```
 0|Setup: Colors
 1|
 2|Attribute                Foreground  Background  Sample
```

#### Setup: Colors/Fonts  (WP.FIL string #187)

```
 0|Setup: Colors/Fonts
 1|
 2|     1 - Screen Colors
 3|
 4|     2 - Italics Font, 8 Foreground Colors
 5|
 6|     3 - Underline Font, 8 Foreground Colors
 7|
 8|     4 - Small Caps Font, 8 Foreground Colors
 9|
10|     5 - 512 Characters, 8 Foreground Colors
11|
12|     6 - Normal Font Only, 16 Foreground Colors
```

#### Setup: Colors/Fonts  (WP.FIL string #189)

```
 0|Setup: Colors/Fonts
 1|
 2|Attribute                Font  Foreground  Background  Sample
```

#### Setup: Fonts/Attributes  (WP.FIL string #190)

```
 0|Setup: Fonts/Attributes
 1|
 2|     1 - Screen Attributes
 3|
 4|     2 - 12 Fonts, 256 Characters
 5|
 6|     3 - 6  Fonts, 512 Characters
 7|
 8|     4 - Normal Font Only
```

#### Setup: Fonts/Attributes  (WP.FIL string #192)

```
 0|Setup: Fonts/Attributes
 1|
 2|Attribute                Font  Reverse  Bold  Underline  Strikeout  Sample
```

#### Setup: Fonts/Attributes  (WP.FIL string #206)

```
 0|Setup: Fonts/Attributes
 1|
 2|     1 - Screen Attributes
 3|
 4|     2 - 12 Fonts, 256 Characters
 5|
 6|     3 - 6  Fonts, 512 Characters
 7|
 8|     4 - Normal Font Only
 9|
10|     5 - Foreground/Background Colors
```

#### Setup: Foreground/Background Colors  (WP.FIL string #207)

```
 0|Setup: Foreground/Background Colors
 1|
 2|     1 - Foreground Color
 3|
 4|     2 - Background Color
 5|
 6|     3 - Cursor Color
 7|
 8|     4 - Underline Color                                    Sample
 9|
10|     5 - Strikeout Color                                    Sample
11|           (RamFont only)
12|
13|     6 - Bold Color                                         Sample
14|           (Normal font only)
```

#### Display Options  (WP.FIL string #215)

```
 0|Display Options
 1|
 2|     1 - Method
 3|
 4|     2 - Scale - Horizontal
 5|                 Vertical
 6|
 7|     3 - Justification
 8|
 9|     4 - Graphics
10|
11|     5 - Cursor Height
```

#### Setup: Table of Authorities  (WP.FIL string #220)

```
 0|Setup: Table of Authorities
 1|
 2|     1 - Dot Leaders
 3|
 4|     2 - Underlining Allowed
 5|
 6|     3 - Blank Line between Authorities
```

#### Setup: Display  (WP.FIL string #221)

```
 0|Setup: Display
 1|
 2|     1 - Colors/Fonts/Attributes
 3|
 4|     2 - Graphics Screen Type
 5|
 6|     3 - Text Screen Type
 7|
 8|     4 - Menu Options
 9|
10|     5 - View Document Options
11|
12|     6 - Edit-Screen Options
```

#### Graphics: Horizontal Line  (WP.FIL string #224)

```
 0|Graphics: Horizontal Line
 1|
 2|     1 - Horizontal Position
 3|
 4|     2 - Vertical Position
 5|
 6|     3 - Length of Line
 7|
 8|     4 - Width of Line
 9|
10|     5 - Gray Shading (% of black)
```

#### Graphics: Vertical Line  (WP.FIL string #226)

```
 0|Graphics: Vertical Line
 1|
 2|     1 - Horizontal Position
 3|
 4|     2 - Vertical Position
 5|
 6|     3 - Length of Line
 7|
 8|     4 - Width of Line
 9|
10|     5 - Gray Shading (% of black)
```

#### Format: Paper Type  (WP.FIL string #229)

```
 0|Format: Paper Type
 1|
 2|     1 - Standard
 3|
 4|     2 - Bond
 5|
 6|     3 - Letterhead
 7|
 8|     4 - Labels
 9|
10|     5 - Envelope
11|
12|     6 - Transparency
13|
14|     7 - Cardstock
15|
16|     8 - [ALL OTHERS]
17|
18|     9 - Other
```

#### Format: Paper Size                 Width  Height  (WP.FIL string #230)

```
 0|Format: Paper Size                 Width  Height
 1|
 2|     1 - Standard                  (8.5" x 11")
 3|
 4|     2 - Standard Landscape        (11" x 8.5")
 5|
 6|     3 - Legal                     (8.5" x 14")
 7|
 8|     4 - Legal Landscape           (14" x 8.5")
 9|
10|     5 - Envelope                  (9.5" x 4")
11|
12|     6 - Half Sheet                (5.5" x 8.5")
13|
14|     7 - US Government             (8" x 11")
15|
16|     8 - A4                        (210mm x 297mm)
17|
18|     9 - A4 Landscape              (297mm x 210mm)
19|
20|     o - Other
```

#### Description  (WP.FIL string #237)

```
 0|
 1|
 2|
 3|
 4|     Description
 5|
 6|     ╔═══════════════════════════════════════════════════════════════════════╗
 7|     ║                                                                       ║
 8|     ║                                                                       ║
 9|     ║                                                                       ║
10|     ║                                                                       ║
11|     ║                                                                       ║
12|     ║                                                                       ║
13|     ║                                                                       ║
14|     ║                                                                       ║
15|     ║                                                                       ║
16|     ║                                                                       ║
17|     ║                                                                       ║
18|     ║                                                                       ║
19|     ║                                                                       ║
20|     ║                                                                       ║
21|     ║                                                                       ║
22|     ╚═══════════════════════════════════════════════════════════════════════╝
```

#### Find: Conditions                                     Files Selected:  (WP.FIL string #241)

```
 0|Find: Conditions                                     Files Selected:
 1|
 2|     1 - Perform Search
 3|     2 - Reset Conditions
 4|
 5|     3 - Revision Date - From
 6|                         To
 7|
 8|     4 - Text - Document Summary
 9|                First Page
10|                Entire Document
11|
12|     5 - Document Summary
13|         Creation Date - From
14|                         To
15|         Document Name
16|         Document Type
17|         Author
18|         Typist
19|         Subject
20|         Account
21|         Keywords
22|         Abstract
```

#### ╔═══════════════════════════════╗  (WP.FIL string #245)

```
 0|                                             ╔═══════════════════════════════╗
 1|                                             ║                               ║
 2|                                             ║                               ║
 3|                                             ║                               ║
 4|                                             ║                               ║
 5|                                             ║                               ║
 6|                                             ╩═══════════════════════════════╣
```

#### Setup: Keyboard Layout  (WP.FIL string #247)

```
 0|Setup: Keyboard Layout
```

#### Keyboard: Edit  (WP.FIL string #248)

```
 0|Keyboard: Edit
 1|
 2|  Name:
 3|
 4|  Key            Action            Description
```

#### Spreadsheet:  (WP.FIL string #260)

```
 0|Spreadsheet:
 1|
 2|     1 - Filename
 3|
 4|     2 - Range
 5|
 6|     3 - Type
 7|
 8|     4 - Perform
```

#### ╔═════════════════════════════════════════════╗  (WP.FIL string #262)

```
 0|
 1|
 2|
 3|                                 ╔═════════════════════════════════════════════╗
 4|                                 ║ Name                  Reference             ║
 5|                                 ╟─────────────────────────────────────────────╢
 6|                                 ║                                             ║
 7|                                 ║                                             ║
 8|                                 ║                                             ║
 9|                                 ║                                             ║
10|                                 ║                                             ║
11|                                 ║                                             ║
12|                                 ║                                             ║
13|                                 ║                                             ║
14|                                 ║                                             ║
15|                                 ║                                             ║
16|                                 ║                                             ║
17|                                 ║                                             ║
18|                                 ║                                             ║
19|                                 ║                                             ║
20|                                 ║                                             ║
21|                                 ╟─────────────────────────────────────────────╢
22|                                 ║                               (Name Search) ║
23|                                 ╚═════════════════════════════════════════════╝
```

#### 1 Size; 2 Format; 3 Lines; 4 Header; 5 Math; 6 Options; 7 Join; 8 SplitCtrl-Arro  (WP.FIL string #298)

```
 0|1 Size; 2 Format; 3 Lines; 4 Header; 5 Math; 6 Options; 7 Join; 8 SplitCtrl-Arro
```

#### Keyboard: Map  (WP.FIL string #304)

```
 0|Keyboard: Map
 1|
 2|  Name:
 3|
 4|  Alt  Key       ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890-=\`[];',./
 5|       Action
 6|
 7|  Ctrl Key       ABCDEFGHIJKLMNOPQRSTUVWXYZ[\]_             C = Command
 8|       Action                                               M = Keyboard Macro
 9|
10|       Key        !"#$%&'()*+,-./0123456789:;<=>?@
11|       Action
12|
13|       Key       ABCDEFGHIJKLMNOPQRSTUVWXYZ[\]^_`
14|       Action
15|
16|       Key       abcdefghijklmnopqrstuvwxyz{|}~
17|       Action
18|
19|
20|  Key            Action            Description
```

#### Table Options  (WP.FIL string #313)

```
 0|Table Options
 1|
 2|     1 - Spacing Between Text and Lines
 3|              Left
 4|              Right
 5|              Top
 6|              Bottom
 7|
 8|     2 - Display Negative Results
 9|              1 = with minus signs
10|              2 = with parentheses
11|
12|     3 - Position of Table
13|
14|     4 - Gray Shading (% of black)
```

#### Link Options:  (WP.FIL string #314)

```
 0|Link Options:
 1|
 2|     1 - Update on Retrieve
 3|
 4|     2 - Show Link Codes
 5|
 6|     3 - Update All Links
```

#### Format: Edit Paper Definition  (WP.FIL string #326)

```
 0|Format: Edit Paper Definition
 1|
 2|         Filename
 3|
 4|         Paper Type
 5|
 6|     1 - Maximum Width
 7|
 8|     2 - Prompt to Load
 9|
10|     3 - Location
11|
12|     4 - Text Adjustment - Top
13|                           Side
```

#### Format: Edit Paper Definition  (WP.FIL string #327)

```
 0|Format: Edit Paper Definition
 1|
 2|         Filename
 3|
 4|     1 - Paper Size
 5|
 6|     2 - Paper Type
 7|
 8|     3 - Font Type
 9|
10|     4 - Prompt to Load
11|
12|     5 - Location
13|
14|     6 - Double Sided Printing
15|
16|     7 - Binding Edge
17|
18|     8 - Labels
19|
20|     9 - Text Adjustment - Top
21|                           Side
```

#### Video driver files not found  (WP.FIL string #332)

```
 0|
 1|
 2|Video driver files not found
 3|
 4|     Use the Other Disk option to specify a directory for the driver
 5|     files.  Continue to use this option until you find the disk with the
 6|     driver you want.
```

#### Mouse driver files not found  (WP.FIL string #338)

```
 0|
 1|
 2|Mouse driver files not found
 3|
 4|     Use the Other Disk option to specify a directory for the driver
 5|     files.  Continue to use this option until you find the disk with the
 6|     driver you want.
```

#### Setup: Menu Options  (WP.FIL string #341)

```
 0|Setup: Menu Options
 1|
 2|     1 - Menu Letter Display
 3|
 4|Pull-Down Menu
 5|
 6|     2 - Pull-Down Letter Display
 7|
 8|     3 - Pull-Down Text
 9|
10|     4 - Alt Key Selects Pull-Down Menu
11|
12|Menu Bar
13|
14|     5 - Menu Bar Letter Display
15|
16|     6 - Menu Bar Text
17|
18|     7 - Menu Bar Separator Line
19|
20|     8 - Menu Bar Remains Visible
```

#### Setup: View Document Options  (WP.FIL string #342)

```
 0|Setup: View Document Options
 1|
 2|     1 - Text in Black & White
 3|
 4|     2 - Graphics in Black & White
 5|
 6|     3 - Bold Displayed with Color
```

#### Setup: Edit-Screen Options  (WP.FIL string #343)

```
 0|Setup: Edit-Screen Options
 1|
 2|     1 - Automatically Format and Rewrite
 3|
 4|     2 - Comments Display
 5|
 6|     3 - Filename on the Status Line
 7|
 8|     4 - Hard Return Display Character
 9|
10|     5 - Merge Codes Display
11|
12|     6 - Reveal Codes Window Size
13|
14|     7 - Side-by-side Columns Display
```

#### Setup: Merge DOS Text File  (WP.FIL string #344)

```
 0|Setup: Merge DOS Text File
 1|
 2|     1 - Field Delimiters  - Begin
 3|                             End
 4|
 5|     2 - Record Delimiters - Begin
 6|                             End
```

#### Setup: Equation Options  (WP.FIL string #345)

```
 0|Setup: Equation Options
 1|
 2|     1 - Print as Graphics
 3|
 4|     2 - Graphical Font Size
 5|
 6|     3 - Horizontal Alignment
 7|
 8|     4 - Vertical Alignment
 9|
10|     5 - Keyboard for Editing
```

#### Setup: Mouse  (WP.FIL string #346)

```
 0|Setup: Mouse
 1|
 2|     1 - Type
 3|
 4|     2 - Port
 5|
 6|     3 - Double Click Interval (1 = .01 sec)
 7|
 8|     4 - Submenu Delay Time (1 = .01 sec)
 9|
10|     5 - Acceleration Factor
11|
12|     6 - Left-Handed Mouse
13|
14|     7 - Assisted Mouse Pointer Movement
```

#### Setup: Environment  (WP.FIL string #347)

```
 0|Setup: Environment
 1|
 2|     1 - Backup Options
 3|
 4|     2 - Beep Options
 5|
 6|     3 - Cursor Speed
 7|
 8|     4 - Document Management/Summary
 9|
10|     5 - Fast Save (unformatted)
11|
12|     6 - Hyphenation
13|
14|     7 - Prompt for Hyphenation
15|
16|     8 - Units of Measure
17|
18|     9 - Alternate Keyboard
19|          (F1 - Help, Esc - Cancel, F3 - Repeat)
```

#### Setup: Document Management/Summary  (WP.FIL string #348)

```
 0|Setup: Document Management/Summary
 1|
 2|     1 - Create Summary on Save/Exit
 3|
 4|     2 - Subject Search Text
 5|
 6|     3 - Long Document Names
 7|
 8|     4 - Default Document Type
```

#### Format: Labels  (WP.FIL string #356)

```
 0|Format: Labels
 1|
 2|     1 - Label Size
 3|                    Width
 4|                    Height
 5|
 6|     2 - Number of Labels
 7|                    Columns
 8|                    Rows
 9|
10|     3 - Top Left Corner
11|                    Top
12|                    Left
13|
14|     4 - Distance Between Labels
15|                    Column
16|                    Row
17|
18|     5 - Label Margins
19|                    Left
20|                    Right
21|                    Top
22|                    Bottom
```

#### Format: Page Numbering  (WP.FIL string #368)

```
 0|Format: Page Numbering
 1|
 2|     1 - New Page Number
 3|
 4|     2 - Page Number Style
 5|
 6|     3 - Insert Page Number
 7|
 8|     4 - Page Number Position
```

#### 1 Move; 2 Scale; 3 Rotate; 4 Invert     5 Black & White: 0                (   )A  (WP.FIL string #373)

```
 0|1 Move; 2 Scale; 3 Rotate; 4 Invert     5 Black & White: 0                (   )A
```

#### Equation: Options  (WP.FIL string #381)

```
 0|Equation: Options
 1|
 2|     1 - Print as Graphics
 3|
 4|     2 - Graphical Font Size
 5|
 6|     3 - Horizontal Alignment
 7|
 8|     4 - Vertical Alignment
```

#### Styles: Edit  (WP.FIL string #387)

```
 0|Styles: Edit
 1|
 2|     1 - Name
 3|
 4|     2 - Type
 5|
 6|     3 - Description
 7|
 8|     4 - Codes
 9|
10|     5 - Enter
```

#### 1 Move; 2 Scale; 3 Rotate; 4 Invert     5 B&W; 6 DrawPerfect: 0           (   )A  (WP.FIL string #402)

```
 0|1 Move; 2 Scale; 3 Rotate; 4 Invert     5 B&W; 6 DrawPerfect: 0           (   )A
```

#### Format: Border Options  (WP.FIL string #403)

```
 0|Format: Border Options
 1|
 2|     1 - Single -       Width
 3|                        Shading
 4|
 5|     2 - Double -       Width
 6|                        Shading
 7|                        Spacing Between
 8|
 9|     3 - Dashed -       Width
10|                        Shading
11|                        Dash Spacing
12|                        Dash Length
13|
14|     4 - Dotted -       Width
15|                        Shading
16|                        Dot Spacing
17|
18|     5 - Thick -        Width
19|                        Shading
20|
21|     6 - Extra Thick -  Width
22|                        Shading
```

#### Select Printer: Hardware Setup  (WP.FIL string #410)

```
 0|Select Printer: Hardware Setup
 1|
 2|     1 - Interrupt Request Level (IRQ)
 3|
 4|     2 - Base Address
```


## Appendix B — all one-line (prompt-line) menus and prompts in WP.FIL

Number and mnemonic letter are bold on screen. The program appends `: 0` (cursor on the 0) unless the string already contains a default.

| # | Text (as displayed) | Mnemonic letters |
|---|---|---|
| 2 | `Mark for: 1 ToC; 2 List; 3 Index; 4 ToA` | C L I A |
| 3 | `1 Footnote; 2 Endnote; 3 Endnote Placement` | F E P |
| 4 | `Redline character:` |  |
| 6 | `1 Header A; 2 Header B` | A B |
| 8 | `(All)` |  |
| 11 | `Redline Method: 1 Printer Dependent; 2 Left; 3 Alternating` | P L A |
| 12 | `Delete EOL (clear tabs); Enter Number (set tab); Del (clear tab); / Type; Left; Center; Right; Decimal; .= Dot Leader; Press Exit when done.` | T L C R D . |
| 13 | `1 Note; 2 Number Code; 3 New Number; 4 Options` | N C u O |
| 15 | `Create a comment` |  |
| 16 | `Columns: 1 On; 2 Off; 3 Define` | O f D |
| 17 | `Check: 1 Word; 2 Page; 3 Document; 4 New Sup. Dictionary; 5 Look Up; 6 Count` | W P D N L C |
| 18 | `Not Found: 1 Skip Once; 2 Skip; 3 Add; 4 Edit; 5 Look Up; 6 Ignore Numbers: 0` |  |
| 19 | `Not Found: 1 2 Skip; 3 Ignore Words Containing Numbers; 4 Edit: 0` |  |
| 20 | `1 Auto; 2 Fixed` | A F |
| 22 | `1 Line; 2 Page; 3 Other` | L P O |
| 23 | `Move: 1 Block; 2 Tabular Column; 3 Rectangle` | B C R |
| 24 | `1 Date Text; 2 Date Code; 3 Date Format; 4 Outline; 5 Para Num; 6 Define` | T C F O P D |
| 26 | `1 ToC/List; 2 EndMark; 3 Index; 4 ToA; 5 Defs and Refs; 6 Subdocs` | C E I A D S |
| 27 | `Math: 1 On; 2 Off; 3 Def; 4 +; 5 =; 6 *; 7 t; 8 T; 9 !; A N` | O f D |
| 28 | `Math: 1 On; 2 Off; 3 Define; 4 Calculate` | O f D C |
| 30 | `Move: 1 Sentence; 2 Paragraph; 3 Page; 4 Retrieve` | S P a R |
| 31 | `1 Uppercase; 2 Lowercase` | U L |
| 32 | `1 Move; 2 Copy; 3 Delete; 4 Append` | M C D A |
| 34 | `Labels: 1 Edit; 2 Delete` | E D |
| 37 | `1 Equation Box; 2 New Equation Number; 3 Equation Options; 4 Box Number` | E N O B |
| 38 | `1 Window; 2 Line Draw; 3 Rewrite` | W L R |
| 39 | `1 - Digits, A - Uppercase Letters, a - Lowercase Letters / I - Uppercase Roman, i - Lowercase Roman / X - Uppercase Roman/Digits if Attached, x - Lowercase Roman/Digits if Attached / Other character - Bullet or Punctuation` |  |
| 41 | `Mark:  * Present when print job begins                 Press Exit to save /        + Can be loaded/unloaded during job         Press Cancel to cancel` |  |
| 42 | `1 Select; 2 Change Quantity; N Name search` | S Q N |
| 46 | `Port: 1 LPT 1; 2 LPT 2; 3 LPT 3; 4 COM 1; 5 COM 2; 6 COM 3; 7 COM 4; 8 Other` | O |
| 47 | `1 Select; 2 Help; N Name Search` | S H N |
| 48 | `Mark:  * Present when print job begins                 Press Exit to save /                                                    Press Cancel to cancel` |  |
| 49 | `Undelete: 1 Restore; 2 Previous Deletion` | R P |
| 50 | `+(OR), *(AND), =, <>, >, <, >=, <=;  Press Exit when done` |  |
| 51 | `1 Hyphen; 2 HZone; 3 /; 4 Justification; 5 Line; 6 Margins; 7 Tab Set; 8 W/O` | y Z / J L M T W |
| 52 | `1 Perform Action; 2 View; 3 Keys; 4 Select; 5 Action; 6 Order; 7 Type` | P V K S A O T |
| 53 | `Suppress Top of Form on last page of document` |  |
| 59 | `1 On; 2 Off; 3 Create; 4 Edit; 5 Delete; 6 Save; 7 Retrieve; 8 Update` | O f C E D S R U |
| 60 | `1 DOS Text; 2 Password; 3 Save As; 4 Comment; 5 Spreadsheet` | T P A C S |
| 61 | `Type: a = Alphanumeric; n = Numeric;  Use arrows;  Press Exit when done` | A N |
| 62 | `1 Merge; 2 Sort; 3 Convert Old Merge Codes` | M S C |
| 63 | `Attribute: 1 Size; 2 Appearance` | S A |
| 64 | `not found: 1 Enter Path; 2 Skip Language; 3 Exit Spell` | P S E |
| 65 | `invalid format: 1 Enter Path; 2 Skip Language; 3 Exit Spell` | P S E |
| 66 | `Select Word: 0` |  |
| 69 | `Network Printer` |  |
| 70 | `1 Save; 2 Retrieve (CR/LF to [HRt]); 3 Retrieve (CR/LF to [SRt] in HZone)` | S R e |
| 71 | `Paragraph Level (Press Enter for Automatic):` |  |
| 72 | `Percent of Optimal:` |  |
| 73 | `1 On; 2 Off` | O f |
| 75 | `1 Select; 2 None; 3 Help; N Name search` | S o H N |
| 76 | `1 Select; 2 Other Disk; 3 Help; 4 List Printer Files; N Name Search` | S O H L N |
| 77 | `Double Word: 1 2 Skip; 3 Delete 2nd; 4 Edit; 5 Disable Double Word Checking` |  |
| 78 | `1 Cross-Ref; 2 Subdoc; 3 Index; 4 ToA Short Form; 5 Define; 6 Generate` | R S I A D G |
| 80 | `ToC Level:` |  |
| 81 | `/  /  /  /  /  /  /  /  /  /  /  / -------------------------------- Sort by Line -------------------------------- /  / Key Typ Field Word         Key Typ Field Word         Key Typ Field Word /  /  /  / Select /  /  / Action                     Order                     Type /  /` |  |
| 82 | `List Number:` |  |
| 83 | `Password:` |  |
| 84 | `1 Command; 2 Filename` | C F |
| 89 | `1 None; 2 Pg # Follows; 3 (Pg #) Follows; 4 Flush Rt; 5 Flush Rt with Leader` | N P ( F L |
| 90 | `1 None; 2 Pg # Follows; 3 (Pg #) Follows` | N P ( |
| 91 | `Not enough memory to use entire concordance file.  Continue` |  |
| 92 | `1 Numbers; 2 Letters; 3 Characters` | N L C |
| 93 | `Word Spacing: 1 Normal; 2 Optimal; 3 Percent of Optimal; 4 Set Pitch` | N O P S |
| 94 | `Number of Lines to Keep Together:` |  |
| 95 | `Restart endnote numbering` |  |
| 96 | `/  /  /  /  /  /  /  /  /  /  /  / -------------------------- Sort Secondary Merge File ------------------------- /  / Key Typ Field Line Word    Key Typ Field Line Word    Key Typ Field Line Word /  /  /  / Select /  /  / Action                     Order                     Type /  /` |  |
| 97 | `Delete redline markings and strikeout text` |  |
| 100 | `1 Line Height; 2 Line Numbering; 3 Line Spacing` | H N S |
| 101 | `Prompt for Hyphenation: 1 Never; 2 When Required; 3 Always` | N W A |
| 102 | `1 Go to Shell; Clipboard: 2 Save; 3 Append; 4 Retrieve; 5 DOS Command` | G S A R C |
| 103 | `1 Go to DOS; 2 DOS Command` | G C |
| 106 | `/  /  /  /  /  /  /  /  /  /  /  / ------------------------------ Sort by Paragraph ----------------------------- /  / Key Typ Line Field Word    Key Typ Line Field Word    Key Typ Line Field Word /  /  /  / Select /  /  / Action                     Order                     Type /  /` |  |
| 107 | `Type: 1 Merge; 2 Line; 3 Paragraph` | M L P |
| 108 | `Order: 1 Ascending; 2 Descending` | A D |
| 109 | `Action: 1 Select and Sort; 2 Select Only` | S O |
| 110 | `1 Replace Word; 2 View Doc; 3 Look Up Word; 4 Clear Column: 0` |  |
| 111 | `/  /  /  /  /  /  /  /  /  /  /  / --------------------------------- Sort Table --------------------------------- /  / Key Typ Cell  Line Word    Key Typ Cell  Line Word    Key Typ Cell  Line Word /  /  /  / Select /  /  / Action                     Order                     Type /  /` |  |
| 112 | `Retrieve: 1 Block; 2 Tabular Column; 3 Rectangle` | B C R |
| 113 | `WPHELP.FIL not found.  Insert the diskette and press drive letter:` |  |
| 114 | `Look: 1 Next Doc; 2 Prev Doc: 0` | N P |
| 115 | `Caption: 1 Outside of Border; 2 Inside of Border` | O I |
| 116 | `Short Form:` |  |
| 117 | `1 Footer A; 2 Footer B` | A B |
| 118 | `ToA Section Number (Press Enter for Short Form only):` |  |
| 119 | `WARNING New page num not found between ToA def and first mark (Press any key)` |  |
| 120 | `WARNING Non-unique short form(s) found (Press any key to continue)` |  |
| 123 | `Section Number (1-16):` |  |
| 124 | `Existing tables, lists, and indexes will be replaced.  Continue` |  |
| 128 | `Re-enter Password:` |  |
| 129 | `Concordance file not found.  Continue` |  |
| 130 | `List Number (1-10):` |  |
| 131 | `Concordance Filename (Enter=none):` |  |
| 132 | `1 Select; N Name search` | S N |
| 133 | `1 Select; 2 Additional Printers; 3 Edit; 4 Copy; 5 Delete; 6 Help; 7 Update` | S A E C D H U |
| 136 | `1 Figure; 2 Table Box; 3 Text Box; 4 User Box; 5 Line; 6 Equation` | F T B U L E |
| 137 | `Footnote: 1 Create; 2 Edit; 3 New Number; 4 Options` | C E N O |
| 139 | `Anchor Type: 1 Paragraph; 2 Page; 3 Character` | P a C |
| 140 | `Vertical Position: 1 Full Page; 2 Top; 3 Center; 4 Bottom; 5 Set Position` | F T C B S |
| 141 | `Horizontal Position: 1 Margins; 2 Columns; 3 Set Position` | M C S |
| 142 | `Horizontal Position: 1 Left; 2 Right; 3 Center; 4 Full` | L R C F |
| 143 | `Caption Position: 1 Below Box; 2 Above Box` | B A |
| 145 | `1 None; 2 Single; 3 Double; 4 Dashed; 5 Dotted; 6 Thick; 7 Extra Thick` | N S D a o T E |
| 147 | `1 Size; 2 Appearance; 3 Normal; 4 Base Font; 5 Print Color` | S A N F C |
| 148 | `1 Suprscpt; 2 Subscpt; 3 Fine; 4 Small; 5 Large; 6 Vry Large; 7 Ext Large` | p b F S L V E |
| 149 | `1 Bold 2 Undln 3 Dbl Und 4 Italc 5 Outln 6 Shadw 7 Sm Cap 8 Redln 9 Stkout` | B U D I O a C R S |
| 150 | `Advance: 1 Up; 2 Down; 3 Line; 4 Left; 5 Right; 6 Position` | U D i L R P |
| 151 | `Letter Spacing: 1 Normal; 2 Optimal; 3 Percent of Optimal; 4 Set Pitch` | N O P S |
| 152 | `Character Box Alignment: 1 Top; 2 Center; 3 Bottom; 4 Baseline` | T C B a |
| 153 | `1 100%  2 200%  3 Full Page  4 Facing Pages:` |  |
| 156 | `1 Odd; 2 Even` | O E |
| 160 | `/  / After selecting a reference type, go to the location of the item you want to / reference in your document and press Enter to mark it as the "target". /` |  |
| 161 | `Target Name:` |  |
| 162 | `1 DefMark; 2 EndDef; 3 Ref; 4 Target` | D E R T |
| 163 | `Subdoc Filename:` |  |
| 164 | `1 Create; 2 Edit` | C E |
| 165 | `1 Select; 2 Auto-select; 3 Other Disk; N Name Search` | S A O N |
| 167 | `1 Style On; 2 Style Off; 3 Open Style` | O f S |
| 171 | `Width:` |  |
| 172 | `Height:` |  |
| 174 | `Type: 1 Paired; 2 Open; 3 Outline` | P O t |
| 175 | `1 Include Subdoc; 2 Start of Subdoc; 3 End of Subdoc` | I S E |
| 178 | `/  /  / Normal / Blocked / Underline / Strikeout / Bold / Double Underline / Redline / Shadow / Italics / Small Caps / Outline / Subscript / Superscript / Fine Print / Small Print / Large Print / Very Large Print / Extra Large Print / Bold & Underline / Other Combinations /  / Switch documents; Move to copy settings     Doc` |  |
| 181 | `Characters Per Second: 1 15; 2 20; 3 30; 4 40; 5 50; 6 Normal` | N |
| 183 | `Caption Position: 1 Below Box; 2 Above Box; 3 Left Side; 4 Right Side` | B A L R |
| 184 | `Baud: 1 19200; 2 150; 3 300; 4 600; 5 1200; 6 2400; 7 4800; 8 96000` |  |
| 185 | `Mark:  * Present when print job begins                 Press Exit to save /        + Can be loaded during print job            Press Cancel to cancel` |  |
| 191 | `Parity: 1 None; 2 Odd; 3 Even` | N O E |
| 193 | `1 Cntr; 2 Force; 3 Hdr; 4 Ftr; 5 T/B Mar; 6 PgNum; 7 Sz/Typ; 8 Sup` | C o H F M N S u |
| 194 | `1 Select; 2 Other; N Name search` | S O N |
| 195 | `1 Advance; 2 Block Protect; 3 Conditional EOP; 4 Decimal Char; 5 Language; / 6 Overstrike; 7 Printer Functions; 8 Underline Spaces/Tabs; 9 Border Opt` | A B C D L O P U r |
| 196 | `1 Kerning; 2 Ptr Cmnd; 3 Width Adj; 4 Just Lim; 5 BLine; 6 Leading` | K P W J B L |
| 197 | `1 Date Code; 2 Para Num; 3 Para Num Definition; 4 Outline On; 5 Outline Off` | C P D O f |
| 198 | `1 Figure; 2 New Figure Number; 3 Figure Options; 4 Box Number` | F N O B |
| 199 | `Text Quality: 1 Do Not Print; 2 Draft; 3 Medium; 4 High` | N D M H |
| 200 | `Key:` |  |
| 201 | `Enter: 1 Hrt; 2 Off; 3 Off/On` | H f O |
| 202 | `Graphics Quality: 1 Do Not Print; 2 Draft; 3 Medium; 4 High` | N D M H |
| 203 | `1 Table Box; 2 New Table Box Number; 3 Table Box Options; 4 Box Number` | T N O B |
| 204 | `1 Text Box; 2 New Text Box Number; 3 Text Box Options; 4 Box Number` | T N O B |
| 205 | `1 User-defined Box; 2 New Usr Box Number; 3 Usr Box Options; 4 Box Number` | U N O B |
| 208 | `1 Select; 2 Delete; 3 Rename; 4 Create; 5 Copy; 6 Original; / 7 Edit; 8 Map; N Name search` | S D R C y O E M N |
| 209 | `1 Action; 2 Dscrptn; 3 Original; 4 Create; 5 Move; Macro: 6 Save; 7 Retrieve` | A D O C M S R |
| 210 | `1 Set Width/Auto Height; 2 Set Height/Auto Width; 3 Set Both; 4 Auto Both` | W H B A |
| 211 | `Device or Filename:` |  |
| 212 | `Document is expanded, condense it` |  |
| 213 | `Update Subdocs` |  |
| 214 | `Save Subdocs` |  |
| 216 | `Display Method: 1 Text; 2 Graphics` | T G |
| 217 | `1 Cancel Job(s); 2 Rush Job; 3 Display Jobs; 4 Go (start printer); 5 Stop` | C R D G S |
| 218 | `1 Go to Shell; Clipboard: 2 Save; 3 Retrieve` | G S R |
| 219 | `WARNING:  If you use this option, you will need to initialize your printer / before you can continue printing.  You will also need to make sure all forms / are in their original positions.  Are you sure` |  |
| 222 | `Already Exists: 1 Replace; 2 Edit; 3 Description` | R E D |
| 223 | `1 No Line; 2 2-inch Line; 3 Margin to Margin` | N M |
| 225 | `Rotate Text: 1 0°; 2 90°; 3 180°; 4 270°` |  |
| 227 | `Create Line: 1 Horizontal; 2 Vertical;  Edit Line: 3 Horizontal; 4 Vertical` | H V o e |
| 228 | `Page Number: 1 New; 2 Style; 3 Insert; 4 Position` | N S I P |
| 231 | `1 Size; 2 Appearance; 3 Normal` | S A N |
| 232 | `Find: 1 Name; 2 Doc Summary; 3 First Pg; 4 Entire Doc; 5 Conditions; 6 Undo` | N D P E C U |
| 233 | `Orientation: 1 Portrait; 2 Landscape; 3 Both` | P L B |
| 234 | `Location: 1 Continuous; 2 Bin Number; 3 Manual` | C B M |
| 235 | `Key: ActionKey / Ctrl-PgUp for macro commands;  Press Exit when done` |  |
| 236 | `Macro: ActionFile / Ctrl-PgUp for macro commands;  Press Exit when done` |  |
| 238 | `Description:` |  |
| 239 | `1 Off; 2 Numbers; 3 Letters; 4 Roman Numerals` | f N L R |
| 240 | `1 Discontinue; 2 Every Page; 3 Odd Pages; 4 Even Pages; 5 Edit` | D P O v E |
| 242 | `Horizontal Position: 1 Left; 2 Right; 3 Between Columns; 4 Set Position` | L R B S |
| 243 | `Horizontal Pos: 1 Left; 2 Right; 3 Center; 4 Full; 5 Set Position` | L R C F S |
| 244 | `1 Figure; 2 Table Box; 3 Text Box; 4 User Box; 5 Equation` | F T B U E |
| 246 | `Keyboard Filename:` |  |
| 249 | `Password: 1 Add/Change; 2 Remove` | A R |
| 250 | `Comment: 1 Create; 2 Edit; 3 Convert to Text` | C E T |
| 251 | `? 1 Yes; 2 No; 3 Replace All Remaining` | Y N R |
| 252 | `Multiple Copies Generated by: 1 WordPerfect; 2 Printer` | W P |
| 253 | `1 Pause; 2 Display; 3 Assign; 4 Comment` | P D A C |
| 254 | `1 Pause; 2 Display` | P D |
| 255 | `Column Type: 1 Newspaper; 2 Parallel; 3 Parallel with Block Protect` | N P B |
| 256 | `1 Select; 2 Create; 3 Edit; 4 Delete; 5 Save; 6 Retrieve; 7 Update` | l C E D S R U |
| 257 | `Contents: 1 Graphic; 2 Graphic on Disk; 3 Text; 4 Equation` | G D T E |
| 258 | `Cursor Height: 1 1 Pixel; 2 2 Pixels; 3 Full Character Height` | F |
| 259 | `Spreadsheet: 1 Import; 2 Create Link; 3 Edit Link; 4 Link Options` | I C E L |
| 261 | `Type: 1 Table; 2 Text` | a e |
| 263 | `1 Name; 2 Description; 3 Type; 4 Enter; 5 Codes` | N D T E C |
| 296 | `Table: 1 Create; 2 Edit` | C E |
| 297 | `/ Math: 1 Calculate; 2 Formula; 3 Copy Formula; 4 +; 5 =; 6 *` | C F p + = * |
| 299 | `Lines: 1 Left; 2 Right; 3 Top; 4 Bottom; 5 Inside; 6 Outside; 7 All; 8 Shade` | L R T B I O A S |
| 300 | `Shading: 1 On; 2 Off` | O f |
| 301 | `/ Insert: 1 Rows; 2 Columns` | R C |
| 302 | `/ Split: 1 Rows; 2 Columns` | R C |
| 303 | `1 Key; 2 Macro; 3 Description; 4 Original; 5 Compose; N Key Name Search` | K M D O C N |
| 305 | `1 Retrieve; 2 Delete; 3 Move/Rename; 4 Print; 5 Short/Long Display; / 6 Look; 7 Other Directory; 8 Copy; 9 Find; N Name Search` | R D M P S L O C F N |
| 306 | `Cell: 1 Type; 2 Attributes; 3 Justify; 4 Vertical Alignment; 5 Lock` | T A J V L |
| 307 | `Column: 1 Width; 2 Attributes; 3 Justify; 4 # Digits` | W A J D |
| 308 | `Vertical Alignment: 1 Top; 2 Bottom; 3 Center` | T B C |
| 309 | `/ Delete: 1 Rows; 2 Columns` | R C |
| 310 | `1 Reenter Password; 2 Continue Without Supplementary Dictionary` | R C |
| 311 | `Format: 1 Cell; 2 Column; 3 Row Height` | C l R |
| 312 | `Row Height -- Single line: 1 Fixed; 2 Auto;  Multi-line: 3 Fixed; 4 Auto` | F t x A |
| 315 | `Mode: 1 Manual; 2 Automatic` | M A |
| 316 | `(Retrieve to capture; Del to remove summary)Selection` |  |
| 317 | `Justification: 1 Left; 2 Center; 3 Right; 4 Full; 5 Decimal Align` | L C R F D |
| 318 | `/ Move: 1 Block; 2 Row; 3 Column; 4 Retrieve` | B R C e |
| 319 | `/ Retrieve: 1 Block; 2 Row; 3 Column` | B R C |
| 320 | `/ Delete Block` |  |
| 323 | `Cell Type: 1 Numeric; 2 Text` | N T |
| 324 | `1 Move; 2 Copy; 3 Delete` | M C D |
| 325 | `/ Table Size: 1 Rows; 2 Columns` | R C |
| 328 | `/ 1 Select; 2 Add; 3 Copy; 4 Delete; 5 Edit; N Name Search` | S A C D E N |
| 329 | `Adjust Text: 1 Up; 2 Down; 3 Left; 4 Right` | U D L R |
| 330 | `Orientation: 1 Portrait; 2 Landscape` | P L |
| 331 | `Justification: 1 Left; 2 Center; 3 Right; 4 Full` | L C R F |
| 333 | `Table Position: 1 Left; 2 Right; 3 Center; 4 Full; 5 Set Position` | L R C F S |
| 334 | `Look Doc Summ: 1 Next; 2 Prev; 3 Look at text; 4 Print Summ; 5 Save to File` | N P L r S |
| 335 | `Look: 1 Next Doc; 2 Prev Doc; 3 Look at Document Summary: 0` | N P L |
| 336 | `Line: 1 Horizontal; 2 Vertical` | H V |
| 337 | `/ Undelete` |  |
| 339 | `Type: 1 Paired; 2 Open` | P O |
| 340 | `1 Field; 2 End Record; 3 Input; 4 Page Off; 5 Next Record; 6 More` | F E I P N M |
| 349 | `Hyphenation: 1 External Dictionary/Rules; 2 Internal Rules` | E I |
| 350 | `Outline: 1 On; 2 Off; 3 Move Family; 4 Copy Family; 5 Delete Family` | O f M C D |
| 351 | `1 Size; 2 Appearance; 3 Normal; 4 Reset` | S A N R |
| 352 | `Justification: 1 Left; 2 Center; 3 Right; 4 Full; 5 Decimal Align; 6 Reset` | L C R F D s |
| 353 | `1 Columns; 2 Tables; 3 Math` | C T M |
| 354 | `Column: 1 On; 2 Off; 3 Def` | O f D |
| 355 | `File Already Exists: 1 Replace; 2 Append` | R A |
| 357 | `Create Table from: 1 Tabular Column; 2 Parallel Column` | T P |
| 358 | `/ Tab Type: 1 Absolute; 2 Relative to Margin` | A R |
| 359 | `Binding Edge: 1 Top; 2 Left` | T L |
| 360 | `Irregular Case: 1 2 Skip; 3 Replace; 4 Edit; 5 Disable Case Checking` |  |
| 361 | `Vertical Position: 1 Baseline; 2 Set Position` | B S |
| 362 | `1 │; 2 ║; 3 *; 4 Change; 5 Erase; 6 Move` | 1 2 3 C E M |
| 363 | `Port: 1 COM1; 2 COM2; 3 COM3; 4 COM4` | 1 2 3 4 |
| 364 | `Find: 1 Doc Name; 2 Doc Summ; 3 First Pg; 4 Entire Doc; 5 Conditions; 6 Undo` | N D P E C U |
| 365 | `1 ░; 2 ▒; 3 ▓; 4 █; 5 ▄; 6 ▌; 7 ▐; 8 ▀; 9 Other` | 1 2 3 4 5 6 7 8 O |
| 366 | `Copy Formula To: 1 Cell; 2 Down; 3 Right` | C D R |
| 367 | `Lock: 1 On; 2 Off` | O f |
| 369 | `Multiple Copies Generated by: 1 WordPerfect; 2 Printer; 3 Network` | W P N |
| 370 | `Delete Styles: 1 Leaving Codes; 2 Including Codes; 3 Definition Only` | L I D |
| 371 | `1 Quit; 2 Next Record; 3 Stop` | Q N S |
| 372 | `Delete Row` |  |
| 374 | `Screen Redisplay; List Commands; Switch Window; Setup Options` |  |
| 375 | `Arrow Keys Move; PgUp/PgDn Scale; Goto Reset; Switch Window` |  |
| 379 | `Horizontal Alignment: 1 Left; 2 Center; 3 Right` | L C R |
| 380 | `Vertical Alignment: 1 Top; 2 Center; 3 Bottom` | T C B |
| 382 | `Setup: Mouse Type` |  |
| 383 | `Setup: Graphic Screen Driver` |  |
| 384 | `Setup: Text Screen Driver` |  |
| 385 | `Directory for driver files:` |  |
| 386 | `1 Comment; 2 Spreadsheet Link; 3 Spreadsheet Link End` | C L E |
| 388 | `1 Generic; 2 WordPerfect 5.0; 3 WordPerfect 4.2` | G W P |
| 389 | `(Type = to change default Dir)` |  |
| 390 | `(List Files)` |  |
| 391 | `(List Ranges)` |  |
| 392 | `Endnote: 1 Create; 2 Edit; 3 New Number; 4 Options` | C E N O |
| 393 | `Figure: 1 Create; 2 Edit; 3 New Number; 4 Options: 0` | C E N O |
| 394 | `Table Box: 1 Create; 2 Edit; 3 New Number; 4 Options: 0` | C E N O |
| 395 | `Text Box: 1 Create; 2 Edit; 3 New Number; 4 Options: 0` | C E N O |
| 396 | `User Box: 1 Create; 2 Edit; 3 New Number; 4 Options: 0` | C E N O |
| 397 | `Equation: 1 Create; 2 Edit; 3 New Number; 4 Options: 0` | C E N O |
| 398 | `Clipboard: 1 Save; 2 Append` | S A |
| 399 | `PgUp/PgDn for more` |  |
| 400 | `1 Default Font; 2 Set Point Size` | D S |
| 401 | `1 Short Display; 2 Long Display` | S L |
| 404 | `(Text was not modified)` |  |
| 405 | `(Cancel to return to document)` |  |
| 406 | `Old backup file exists. 1 Rename; 2 Delete` |  |
| 407 | `Cells will be deleted.  Continue` |  |
| 408 | `Table: 1 Tbl Def; 2 Tbl Off` | D O |
| 409 | `Hardware Printing: 1 Yes; 2 No; 3 Advanced Setup` | Y N A |
| 411 | `Enter IRQ level (2-7):` |  |

## Sources
* `/mnt/i/Claude Space/wp51/WP.FIL` (compressed string table @0x861E0, 412 strings; error table @0x92EEE), `WP.EXE` strings, `WPHELP.FIL`, `WP{WP}.SET` — extracted with `extracted/tools/*.py`.
* WP 5.1 interim release notes (prompt wording changes): https://mendelson.org/wpdos/51enhan.txt
* WPDOS site: https://mendelson.org/wpdos/ ; WP 5.1 Workbook https://mendelson.org/wpdos/WP51WKBK.pdf (could not be text-extracted here)
