# WordPerfect 5.1 (DOS) — Reveal Codes Specification

Reveal Codes is the user's most-used feature: this file has priority over all others.
Code mnemonics marked ✔ were verified byte-for-byte in the user's `WP.EXE` (string table at
file offsets 0x194EB–0x19A66, see `extracted/wpexe_strings.txt`). Parameter formats are from
the 5.1 release notes / memory and are marked **[?]** where not verified.

---

## 1. Toggling
* Alt-F3 or F11 toggles. It is a pure view mode: all editing keys and features keep working,
  and text typed appears in both panes.
* Can be on in Doc 1 and Doc 2 independently [?]; it is also used automatically in Initial
  Codes, Style editing, and Document Initial Codes screens (always split there).
* Setup→Display→Edit-Screen Options→6 **Reveal Codes Window Size** = number of lines of the
  bottom (codes) pane. If 1: the bottom pane shows only the code/character under the cursor.
  Ctrl-F3,1 (Window) while in Reveal Codes changes the size of the upper pane.

## 2. Layout (80×25, window size *w*, default *w* = 10 [?])

```
row 0            ┐
 …               │ upper pane: normal editing view, 24 − w − 1 … rows  (11 rows when w = 10 [?])
row 11           ┘
row 12  C:\WP51\JOBS\LETTER.WP                       Doc 1 Pg 1 Ln 1" Pos 1"     ← status line
row 13  [▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲    ▲   ]▲    ← ruler
row 14  ┐
 …      │ lower pane: codes view (w rows)
row 23  ┘
row 24  Press Reveal Codes to restore screen                                       ← prompt line
```
* Status line moves up to sit directly above the ruler (it belongs to the upper pane).
* Row 24 shows `Press Reveal Codes to restore screen` (WP.EXE: `Press {Reveal Codes} to restore
  screen`, where *Reveal Codes* is bold) [? exact: this string may only appear in contexts like
  Initial Codes; the default display may leave row 24 blank — verify]. When a prompt/menu is
  active, it replaces this row.
* With the cursor in the middle of the document the cursor line is kept roughly in the middle
  of the lower pane [?]; both panes scroll together.

### 2.1 Ruler (tab bar)
One full-width row, drawn in the **bold/highlight** attribute [?colour]:
* `▲` (CP437 0x1E) at every tab stop position (absolute screen column of the stop).
* `[` at the left margin, `]` at the right margin.
* `{` instead of `[` (and `}` instead of `]`) when a tab stop coincides with the margin.
  (The four characters `{[}]` appear together in WP.EXE right before the Exit strings — the
  ruler glyph set; the following `Nn` is unexplained [?].)
* Other columns are blank [? or `·`]. Columns beyond the right edge are not shown; when the
  document is scrolled horizontally the ruler scrolls with it.
* Ruler reflects the margins/tabs **in effect at the cursor**.
* Some recall in WP 5.1 tab types being shown on this ruler as `L C R D` (as on the Tab Set
  screen); the triangle form is the WP 5.0/5.1 Reveal Codes convention [?] — keep it
  configurable.

### 2.2 Lower pane contents
* Shows the same text lines as the document, but with every hidden code displayed inline as
  `[Mnemonic]` or `[Mnemonic:params]`, drawn in **bold** (help: "Codes are bolded so they can
  be distinguished from text").
* Text characters are drawn plainly (normal colour); attribute effects (bold, underline)
  are **not** applied to the text in the lower pane — the codes show them [?].
* Line breaking: each document line ends after its `[SRt]`, `[HRt]`, `[HPg]`, `[SPg]`,
  `[HRt-SPg]`, `[DSRt]`, `[ISRt]` code, i.e. one lower-pane row per document line. Because codes
  lengthen the line, a row longer than 80 columns wraps onto the next row (wrap at 80, a code
  is not split across rows [?]).
* Spaces are shown as spaces; tabs/indents only as their codes (no whitespace expansion).
* Codes with long parameters may be shown abbreviated when the cursor is not on them and in full
  when the cursor is on them ("Some codes show more information when the cursor is positioned
  directly on the code" — help screen). Concretely [?]: `[Footnote:1;[Note Num]Text of the
  note…]`, `[Header A:Every page;text…]`, `[Comment]`, `[Style On:Name]`, `[Fig Box:1;FILE.WPG;]`
  show about the first 50 characters of the contained text when the cursor is on them.

### 2.3 Cursor in the lower pane
* Shown as a **reverse-video (highlight) block**:
  - on a text character: one character cell;
  - on a code: the **whole code** `[....]` is highlighted as a unit.
* The upper pane still shows the normal blinking text cursor at the same logical position.
  Moving across a code with Left/Right takes one keystroke and is visible only in the lower pane
  (upper-pane cursor does not move).
* Block: when Block is on, the lower pane shows a `[Block]` marker at the anchor and highlights
  the blocked region in the Blocked attribute [?].

## 3. Editing in Reveal Codes
* Del deletes the code under the cursor; Backspace the code left of the cursor. **No
  confirmation** is asked in Reveal Codes.
* Deleting either half of a **paired code** (attribute on/off, `[Mark]`/`[End Mark]`,
  `[Def Mark]`/`[End Def]` [?], `[Style On]`/`[Style Off]`, `[Hyph On]`? no) removes **both**
  halves; the text between them loses the attribute. Applies to all attribute pairs and to
  `[Mark:ToC,n]…[End Mark:ToC,n]`, `[Style On]…[Style Off]` [?].
* Deleting `[Tbl Def]` deletes the table structure: asks?/table becomes tabbed text [?].
* Codes cannot be typed; they are produced by the features. Codes can be searched (F2 then the
  function key) and replaced.
* Typeover never overwrites codes (codes are pushed right).
* Undelete (Cancel) restores deleted codes too.

## 4. How codes appear in the normal (upper / non-reveal) text window
* All codes are invisible. Their effects are shown:
  - attributes → screen colours/fonts (display.md §2), e.g. bold text in the Bold colour;
  - `[HRt]` → line ends (optionally Hard Return Display Character, Setup default = space);
  - `[HPg]` → full-width line of `=`; `[SPg]`/`[HRt-SPg]` → full-width line of `-` [display.md];
  - `[Center]`, `[Flsh Rgt]`, tabs, indents → horizontal position of text;
  - `[Comment]` → a double-line box with the comment text (Setup: Comments Display = Yes);
  - merge codes `{FIELD}1~` etc. are visible text-like codes (Merge Codes Display = Yes).
* The cursor passes over a code with one keystroke while appearing not to move — this is
  authentic and must be kept.

## 5. Deleting codes in the normal screen — the confirmation prompt
When Reveal Codes is **off** and Backspace/Del hits a hidden code, WP 5.1 asks (WP.EXE strings
`Delete ` + code + `? ` + `No (Yes)`):
```
Delete [BOLD]? No (Yes)
```
* No (default, any key other than Y) → nothing deleted, cursor moves past the code [?].
* Yes → code deleted (pairs: both halves).
* The prompt is issued for codes that are not otherwise visible — at least for attribute
  codes, and (per WP 5.1 behaviour) most format codes [?]. Codes that WP silently skips/deletes
  [?]: `[SRt]` (cannot be deleted at all; deleting just reformats), `[HRt]`, `[Tab]`, `[Indent]`,
  hyphens/hard space (visible effects → deleted without prompt).
* Exact capitalization of the code inside the prompt = the reveal-codes form (`[BOLD]`, `[UND]`,
  `[Tab Set]`?) [?].
* Moving past codes with Backspace when the code is an "end" code (`[bold]`) at the end of
  bolded text: prompt shows `Delete [bold]? No (Yes)` [?].

## 6. Auto-insertion of paired codes
* F6 (Bold), F8 (Underline), Ctrl-F8 Size/Appearance with **Block off**: inserts the ON code
  and the OFF code together, cursor between them: `[BOLD]|[bold]`. Typed text goes between.
  Pressing the same key again (or Ctrl-F8,3 Normal) moves the cursor **past** the OFF code
  (does not insert another code). Right-arrow past `[bold]` does the same.
* With **Block on**: ON code inserted at block start, OFF code at block end.
* Attribute nesting is free: `[BOLD][UND]text[und][bold]`.
* If the cursor is inside an attribute pair and the user presses the attribute key, the cursor
  jumps past the end code (no new code) [?].
* Font size codes behave the same: `[LARGE]…[large]`, `[SUPRSCPT]…[suprscpt]`.
* Old-style single codes from WP 4.2/5.0 are converted on retrieve [?].

## 7. Placement rules (where WP puts format codes) [?]
* Line format codes (margins, justification, line spacing, tab set, hyphenation…) are inserted
  at the cursor; WP 5.1 does NOT auto-move them to the paragraph start (unlike 6.x Auto Code
  Placement). Page format codes (T/B margins, headers, page numbering, paper size, center page,
  suppress) should be at the top of the page to take effect on that page.
* Inserting a format code identical in type to one immediately before it does **not** replace
  it (both remain; later one wins) [?].

---

## 8. Complete code mnemonic list

### 8.1 Line breaks, pages, spacing
| Display | Meaning | Inserted by | Verified |
|---|---|---|---|
| `[SRt]` | soft return (word-wrap) | automatic | ✔ |
| `[HRt]` | hard return | Enter | ✔ |
| `[HPg]` | hard page | Ctrl-Enter | ✔ |
| `[SPg]` | soft page break | automatic | ✔ |
| `[HRt-SPg]` | hard return that falls on a page break | automatic | ✔ |
| `[ISRt]` | invisible soft return | Home,Enter | ✔ |
| `[DSRt]` | deletable soft return (line with no break point) | automatic | ✔ |
| `[Dorm HRt]` | dormant hard return (at top of page) | automatic | ✔ |
| `[ ]` | hard space | Home,Space | [?] |
| `[-]` | hyphen character | `-` | [?] |
| `-` (highlighted) | soft hyphen | Ctrl,`-` / hyphenation | [?] |
| `-` (plain) | hard hyphen | Home,`-` | [?] |
| `[/]` | cancel hyphenation | Home,`/` | [?] |
| `[Hyph On]` / `[Hyph Off]` | hyphenation | Shift-F8,1,1 | ✔ |
| `[HZone:10%,4%]` | hyphenation zone left,right | Shift-F8,1,2 | name ✔, params [?] |
| `[Ln Spacing:1.5]` | line spacing | Shift-F8,1,6 | ✔ |
| `[Ln Height:0.167"]` / `[Ln Height:Auto]` | line height | Shift-F8,1,4 | ✔ name |
| `[Ln Num:On]` / `[Ln Num:Off]` | line numbering | Shift-F8,1,5 | ✔ name |
| `[W/O On]` / `[W/O Off]` | widow/orphan protection | Shift-F8,1,9 | ✔ |
| `[Cndl EOP:3]` | conditional end of page, n lines | Shift-F8,4,2 | ✔ name |
| `[Block Pro:On]` / `[Block Pro:Off]` | block protect | Block,Shift-F8 | ✔ name |
| `[Leading Adj:0",0"]` | leading adjustment | Shift-F8,4,6,6 | ✔ name |

### 8.2 Tabs, alignment, indents
| Display | Meaning | Verified |
|---|---|---|
| `[Tab]` | left tab (soft) | ✔ |
| `[Cntr Tab]` | centre tab | ✔ |
| `[Rgt Tab]` | right tab | ✔ |
| `[Dec Tab]` | decimal tab / Tab Align (Ctrl-F6) | ✔ |
| `[TAB]` `[CNTR TAB]` `[RGT TAB]` `[DEC TAB]` | hard tabs (Home,Tab etc.) — upper case | [?] |
| dot-leader variants | shown as `[Tab]` etc. preceded by `.`? e.g. `[DEC TAB]`… | [?] |
| `[Center]` | centre text on line (Shift-F6) | ✔ |
| `[Flsh Rgt]` | flush right (Alt-F6) | ✔ |
| `[C/A/Flrt]` | end of centring/alignment/flush right (automatic at end of the aligned text when needed) | ✔ (string `C/A/Flrt`) |
| `[End C/A]` | explicit End Centering/Alignment (5.1 interim, Shift-F8,4,9) | [?] (release notes) |
| `[→Indent]` | indent (F4) — arrow is CP437 0x1A | ✔ (`\xFF\x1A` + `Indent`) |
| `[→Indent←]` | left/right indent (Shift-F4) | ✔ |
| `[←Mar Rel]` | margin release (Shift-Tab) | ✔ (`\xFF\x1B` + `Mar Rel`) |
| `[Tab Set:Rel; -1", every 0.5"]` | tab set relative to margin, evenly spaced | ✔ pieces `Tab Set`,`Rel; `,`, every ` |
| `[Tab Set:Abs; 1",1.5",3"]` / `Rel; -1",-0.5",+0",+0.5",…` | individual stops; `No Tabs` when cleared | pieces ✔, exact list syntax [?] |
| `[Decml/Algn Char:.,,]` | decimal/align char and thousands separator | ✔ name |
| `[Just:Left]` `[Just:Center]` `[Just:Right]` `[Just:Full]` | justification | ✔ (`Just` + Left/Center/Right/Full) |
| `[Just On]` / `[Just Off]` | 5.0-era justification (retrieved docs) | ✔ |
| `[Just Lim:60,400]` | word-spacing justification limits % | ✔ name |
| `[Wrd/Ltr Spacing:Optimal,Optimal]` | word/letter spacing | ✔ name |
| `[Kern:On]` / `[Kern:Off]` | kerning | ✔ name |

### 8.3 Margins & page format
| Display | Meaning | Verified |
|---|---|---|
| `[L/R Mar:1",1"]` | left/right margins | ✔ name; format per manual `[L/R Mar:2",2"]` |
| `[T/B Mar:1",1"]` | top/bottom margins | ✔ |
| `[Center Pg]` | center page top-to-bottom | ✔ (`Center Pg`) |
| `[Force:Odd]` / `[Force:Even]` | force odd/even page | ✔ (`Force`,`Odd`,`Even`) |
| `[Header A:Every page;…text…]` / `[Header B:…]` | header; occurrence `Every page`/`Odd pages`/`Even pages`; `Discontinue` → `[Header A:Discontinue]` [?] | ✔ pieces |
| `[Footer A:…]` / `[Footer B:…]` | footers | ✔ |
| `[Suppress:PgNum,HA,HB,FA,FB]` (also `Pg BC`) | suppress this page | ✔ pieces `Suppress`,`PgNum`,`Pg BC` |
| `[Pg Numbering:Bottom Center]` | page number position (9 positions + None: `No page numbering`) | ✔ name, position words [?] |
| `[Pg Num:5]` | new page number (Arabic or roman `Roman`) | ✔ name |
| `[Pg Num Style:Page ^B]` | page number style text | ✔ name |
| `[Insert Pg Num:^B]` | insert page number | ✔ name |
| `[Paper Sz/Typ:8.5" x 11",Standard]` | paper size/type (`Standard`, `Letterhead`, `Bond`, `Envelope`, `Labels`, `Cardstock`, `Transparency`, `[ALL OTHERS]`) | ✔ pieces |
| `[Labels…]` | label definition [?] | |
| `[Brdr Opt]` | border options | ✔ |

### 8.4 Fonts and attributes (paired: UPPER = on, lower = off)
| On / Off | Feature | Verified |
|---|---|---|
| `[BOLD]` / `[bold]` | bold | ✔ lower |
| `[UND]` / `[und]` | underline | ✔ |
| `[DBL UND]` / `[dbl und]` | double underline | ✔ |
| `[ITALC]` / `[italc]` | italics | ✔ |
| `[OUTLN]` / `[outln]` | outline | ✔ |
| `[SHADW]` / `[shadw]` | shadow | ✔ |
| `[SM CAP]` / `[sm cap]` | small caps | ✔ |
| `[REDLN]` / `[redln]` | redline | ✔ |
| `[STKOUT]` / `[stkout]` | strikeout | ✔ |
| `[SUPRSCPT]` / `[suprscpt]` | superscript | ✔ |
| `[SUBSCPT]` / `[subscpt]` | subscript | ✔ |
| `[FINE]` / `[fine]` | fine print | ✔ |
| `[SMALL]` / `[small]` | small print | ✔ |
| `[LARGE]` / `[large]` | large | ✔ |
| `[VRY LARGE]` / `[vry large]` | very large | ✔ |
| `[EXT LARGE]` / `[ext large]` | extra large | ✔ |
| `[Font:Courier 10cpi]` | base font change (Ctrl-F8,4) | ✔ name |
| `[Color:Red]` | print colour (Black, Gray, Red, Green, Blue, Yellow, Magenta, Cyan, Orange, Brown, White; custom → `[Color:R,G,B]`? [?]) | ✔ names |
| `[Undrln:Spaces,Tabs]` | underline spaces/tabs | ✔ `Undrln` |
| `[Ovrstk:xy]` | overstrike | ✔ `Ovrstk` |
| `[Lang:US]` | language (DE, FR, ES, …) | ✔ `Lang` |
| `[Ptr Cmnd:…]` | printer command | ✔ |
| `[BLine:On]` | baseline placement | ✔ |
| `[AdvToLn:2"]` `[AdvToPos:3"]` `[AdvUp:0.1"]` `[AdvDn:0.1"]` `[AdvLft:…]` `[AdvRgt:…]` | advance | ✔ pieces (`Adv` + `ToLn:` …) |

### 8.5 Notes, numbering, outline
| Display | Meaning | Verified |
|---|---|---|
| `[Footnote:1;[Note Num]text…]` | footnote, number; contents shown | ✔ `Footnote`, `Note Num` |
| `[Endnote:1;[Note Num]text…]` | endnote | ✔ |
| `[Note Num]` | number inside note | ✔ |
| `[New Ftn Num:5]` / `[New End Num:5]` | renumber | ✔ |
| `[Ftn Opt]` / `[End Opt]` | note options | ✔ |
| `[Endnote Placement]` | endnote placement | ✔ |
| `[Par Num:Auto]` / `[Par Num:3]` | paragraph number | ✔ `Par Num`, `Auto` |
| `[Par Num Def]` | paragraph number definition | ✔ |
| `[Outline On]` / `[Outline Off]` | outline mode | ✔ |
| `[Outline Lvl 1 …]` | outline style level [?] | ✔ `Outline Lvl ` |
| `[Date:3 1, 4]` | date code (format string) | ✔ `Date` |
| `[Comment]` | document comment | ✔ |

### 8.6 Columns, tables, math
| Display | Meaning | Verified |
|---|---|---|
| `[Col Def:Newspaper;2;1",4";4.5",7.5"]` | column definition (type `Newspaper`/`Parallel`/`Parallel` + `/Block Pro`) | ✔ pieces |
| `[Col On]` / `[Col Off]` | columns on/off | ✔ |
| `[Tbl Def:I;3,2",2",2"]` | table definition (table number, column widths) | ✔ name, params [?] |
| `[Row]` | start of row | ✔ |
| `[Cell]` | start of cell | ✔ |
| `[Hrd Row]` | hard row | ✔ |
| `[Tbl Off]` | end of table | ✔ |
| `[Math Def]` / `[Math On]` / `[Math Off]` | math columns | ✔ |
| `+` `=` `*` `t` `T` `!` `N` | math operators (shown as characters, bold [?]) | |

### 8.7 Graphics
| Display | Verified |
|---|---|
| `[Fig Box:1;FILE.WPG;]` `[Tbl Box:…]` `[Text Box:…]` `[Usr Box:…]` `[Equ Box:…]` | ✔ names |
| `[Fig Opt]` `[Tbl Opt]` `[Txt Opt]` `[Usr Opt]` `[Equ Opt]` | ✔ |
| `[New Fig Num:n]` `[New Tbl Num]` `[New Txt Num]` `[New Usr Num]` `[New Equ Num]` | ✔ |
| `[Box Num]` | ✔ |
| `[HLine:Full,Baseline,6.5",0.013",100%]` / `[VLine:…]` | ✔ names, params [?] |

### 8.8 Mark Text / references
| Display | Verified |
|---|---|
| `[Mark:ToC,1]` … `[End Mark:ToC,1]`, `[Mark:List,1]` … `[End Mark:List,1]` | ✔ pieces (`Mark`,`End Mark`,`ToC,`,`List,`) |
| `[Index:heading;subheading]` | ✔ |
| `[ToA:1;short form;full form]` / `[ToA:;short form;]` | ✔ `ToA`, `Full Form` |
| `[Def Mark:ToC,2:…]` … `[End Def]` | ✔ |
| `[Ref(name):Pg ?]` / `[Target(name)]` (`Par`, `Ftn`, `End`, `Fig`, `Tbl`, `Txt Box`, `Usr Box`, `Equ Box` reference types) | ✔ pieces |
| `[Subdoc:FILE]`, `[Subdoc Start:FILE]`, `[Subdoc End:FILE]` | ✔ |
| `[Link:…]` / `[Link End]` | spreadsheet link ✔ |

### 8.8b Styles
`[Style On:Name]`, `[Style Off:Name]`, `[Open Style:Name]` ✔.

### 8.9 Special
| Display | Meaning |
|---|---|
| `[Block]` | position of the block anchor while Block is on [?] |
| `[Unknown]` | unrecognised code (e.g. 5.1 code read by 5.0) |
| `[Insert Pg Num…]`, `[Pg Num Style…]` | see 8.3 |
| merge codes | displayed as `{FIELD}name~`, `{END RECORD}`, `{INPUT}msg~`, `{PAGE OFF}`, `{NEXT RECORD}` … (5.1 merge command language, visible in both panes) |
| WP characters not in CP437 | shown as a placeholder (small block) in the upper pane; in Reveal Codes as `[■:4,25]`-style char number [?] |

### 8.10 Parameter formatting rules [?]
* Measurements use the Setup "Display and Entry of Numbers" unit: inches as `1"`, `0.5"`,
  `1.25"` (no leading zero suppression? WP shows `0.5"` [?]); cm as `2.54c`; points `72p`;
  w-units `1200w`.
* Multiple parameters separated by `,` (margins) or `;` (groups), no spaces, except the
  tab-set prose `Rel; -1", every 0.5"`.
* Font names exactly as in the printer's .PRS (e.g. `Courier 10cpi`, `Times Roman 12pt`).

## Sources
* `WP.EXE` code-name strings at 0x194F3–0x19A66 (see `extracted/wpexe_strings.txt`);
  `WP.EXE` 0x17EA1 `Press {Reveal Codes} to restore screen`; 0x182AA `{[}]Nn`.
* `WPHELP.FIL` screens "Reveal Codes", "Setup: Reveal Codes Window Size", "Hard Page",
  "Hyphen", "Insert/Typeover" (`extracted/wphelp_screens.txt`).
* WP 5.1 interim release notes (Tab Set syntax, `[End C/A]`, `[Brdr Opt]`):
  https://mendelson.org/wpdos/51enhan.txt
* WP 5.1 workbook (lesson 6 shows `[L/R Mar:2",2"]`): https://mendelson.org/wpdos/WP51WKBK.pdf
