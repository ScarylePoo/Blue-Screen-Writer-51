# WordPerfect 5.1 (DOS) — Keyboard Specification

Status: implementation spec for the clean-room re-implementation.
Uncertain items are marked **[?]**. "Source" tags refer to the list at the end.

Notation: `Shift-F8,1,3` = press Shift-F8, then `1`, then `3` (WP's own notation, used in its
help index). `Home,Home,Up` = press Home, release, Home, release, Up.

---

## 0. IMPORTANT — the user's own installation uses the *Alternate keyboard*

The user's `WP{WP}.SET` (in `/mnt/i/Claude Space/wp51`) names `ALTRNAT.WPK` as the active
keyboard layout (bytes at 0x448 of the .SET file). `WPM/ALTRNAT.WPK` remaps exactly three keys
(decoded from the file; its key descriptions read "Help", "Esc", "Cancel"):

| Physical key | Standard WP 5.1 | **ALTRNAT.WPK (her setup)** |
|---|---|---|
| F1  | Cancel / Undelete | **Help** |
| F3  | Help              | **Esc (Repeat Value)** |
| Esc | Repeat Value (Esc) | **Cancel / Undelete** |

This is the same remapping as `Shift-F1,3,9 Alternate Keyboard = Yes` ("F1 - Help, Esc - Cancel,
F3 - Repeat"; Setup help: "F1 is changed from Cancel to Help, Esc is changed from Repeat Value to
Cancel, and F3 is changed from Help to Repeat Value"). The implementation MUST support both
layouts, selectable in Setup, and ship with the alternate layout active for this user.
Everything else in ALTRNAT.WPK is identical to the built-in layout (decoded table below, §4).

`Ctrl-6` toggles back to the original (built-in) keyboard; pressing `Ctrl-6` twice in succession
restores the previous layout (Source E).

Note also `DE_USA.WPK` / `DE_DOS.WPK` (German layouts with dead keys for `"`, `^`, `'`, `` ` ``)
exist in the install — the user is a translator; dead-key/compose support matters (see §3.6).

---

## 1. Function keys F1–F12 × {plain, Shift, Ctrl, Alt}

Built-in ("original") layout, from the on-screen keyboard template in WPHELP.FIL (Help→F3 twice)
and the key-name table in WP.MRS. Feature names are exactly as WP prints them on the template.

| Key | Plain | Shift | Ctrl | Alt |
|---|---|---|---|---|
| F1  | **Cancel** (Undelete when nothing to cancel) | **Setup** | **Shell** | **Thesaurus** |
| F2  | **->Search** (forward) | **<-Search** (backward) | **Spell** | **Replace** |
| F3  | **Help** | **Switch** (Doc 1/Doc 2; case conversion with Block) | **Screen** | **Reveal Codes** |
| F4  | **->Indent** | **->Indent<-** (left+right indent) | **Move** | **Block** |
| F5  | **List** (List Files) | **Date/Outline** | **Text In/Out** | **Mark Text** |
| F6  | **Bold** | **Center** | **Tab Align** | **Flush Right** |
| F7  | **Exit** | **Print** | **Footnote** | **Columns/Table** |
| F8  | **Underline** | **Format** | **Font** | **Style** |
| F9  | **End Field** (merge) | **Merge Codes** | **Merge/Sort** | **Graphics** |
| F10 | **Save** | **Retrieve** | **Macro Define** | **Macro** (run) |
| F11 | **Reveal Codes** | (unassigned, "Shft F11") | (unassigned, "Ctrl F11") | (unassigned, "Alt F11") |
| F12 | **Block** | (unassigned, "Shft F12") | (unassigned, "Ctrl F12") | (unassigned, "ALT F12") |

* F11 and F12 exist only on the 101-key "Enhanced" keyboard; they duplicate Alt-F3 and Alt-F4.
  The unassigned Shift/Ctrl/Alt-F11/F12 names come from WP.MRS (they can be bound in a
  keyboard layout via Setup→Keyboard Layout→Edit). They do nothing by default (beep [?]).
* Template legend (from help screen): **Ctrl + Function Key** (shown bold), `SHIFT + FUNCTION KEY`
  (shown UPPERCASE), *Alt + Function Key* (shown with the "underline/second" attribute),
  Function key alone (plain text).

### 1.1 What each function key does (summary; menus are in `screens.md`)

| Key | Block OFF | Block ON |
|---|---|---|
| F1 Cancel | Cancels current prompt/menu (backs out one level); stops macro/merge; when idle → Undelete menu | turns Block off |
| Shift-F1 Setup | Setup full-screen menu | same |
| Ctrl-F1 Shell | `1 Go to DOS; 2 DOS Command: 0` (or Shell/Clipboard menu if WP Shell present) | Clipboard save/append (Shell only) |
| Alt-F1 Thesaurus | Thesaurus on word at cursor | — |
| F2 ->Search | `-> Srch:` prompt | extends block to found text |
| Shift-F2 <-Search | `<- Srch:` prompt | extends block |
| Home,F2 / Home,Shift-F2 | `-> Extended srch:` / `<- Extended srch:` (also headers, footers, notes, box captions, text boxes) | |
| Ctrl-F2 Spell | `Check: 1 Word; 2 Page; 3 Document; 4 New Sup. Dictionary; 5 Look Up; 6 Count: 0` | spell-checks the block |
| Alt-F2 Replace | `w/Confirm? No (Yes)` then `-> Srch:` then `Replace with:` | replace within block |
| Home,Alt-F2 | Extended replace | |
| F3 Help | Help screen | |
| Shift-F3 Switch | switch Doc 1 ⇄ Doc 2 | `1 Uppercase; 2 Lowercase: 0` |
| Ctrl-F3 Screen | `1 Window; 2 Line Draw; 3 Rewrite: 0` | |
| Alt-F3 / F11 Reveal Codes | toggle Reveal Codes | same (block highlight also shown in codes window) |
| F4 ->Indent | inserts `[→Indent]` | |
| Shift-F4 ->Indent<- | inserts `[→Indent←]` | |
| Ctrl-F4 Move | `Move: 1 Sentence; 2 Paragraph; 3 Page; 4 Retrieve: 0` | `Move: 1 Block; 2 Tabular Column; 3 Rectangle: 0` |
| Alt-F4 / F12 Block | Block on (`Block on` blinks at bottom-left) | Block off |
| F5 List | `Dir C:\PATH\*.*` prompt then List Files | |
| Shift-F5 Date/Outline | `1 Date Text; 2 Date Code; 3 Date Format; 4 Outline; 5 Para Num; 6 Define: 0` | |
| Ctrl-F5 Text In/Out | `1 DOS Text; 2 Password; 3 Save As; 4 Comment; 5 Spreadsheet: 0` | `Create a comment? No (Yes)` |
| Alt-F5 Mark Text | `1 Cross-Ref; 2 Subdoc; 3 Index; 4 ToA Short Form; 5 Define; 6 Generate: 0` | `1 ToC; 2 List; 3 Index; 4 ToA: 0` [?exact] |
| F6 Bold | inserts `[BOLD]`…`[bold]` pair, cursor between; pressing F6 again moves cursor past `[bold]` | wraps block in `[BOLD]`…`[bold]` |
| Shift-F6 Center | `[Center]` (line centred; dot leader if pressed twice / at tab) | `[Just:Center]? No (Yes)` [?] centre block |
| Ctrl-F6 Tab Align | `[Dec Tab]`; status prompt `Align char = .` | |
| Alt-F6 Flush Right | `[Flsh Rgt]` (twice = with dot leaders) | `[Just:Right]? No (Yes)` [?] |
| F7 Exit | `Save document? Yes (No)` … (see screens) | |
| Shift-F7 Print | Print full-screen menu | `Print block? No (Yes)` |
| Ctrl-F7 Footnote | `1 Footnote; 2 Endnote; 3 Endnote Placement: 0` | |
| Alt-F7 Columns/Table | `1 Columns; 2 Tables; 3 Math: 0` (inside table → Table Edit) | Create table from columns / table cell ops |
| F8 Underline | `[UND]`…`[und]` pair | wraps block |
| Shift-F8 Format | Format full-screen menu | `Protect block? No (Yes)` |
| Ctrl-F8 Font | `1 Size; 2 Appearance; 3 Normal; 4 Base Font; 5 Print Color: 0` | `Attribute: 1 Size; 2 Appearance: 0` |
| Alt-F8 Style | Styles list screen | applies paired style around block |
| F9 End Field | `{END FIELD}` + `[HRt]` (merge secondary files) | |
| Shift-F9 Merge Codes | `1 Field; 2 End Record; 3 Input; 4 Page Off; 5 Next Record; 6 More: 0` | |
| Ctrl-F9 Merge/Sort | `1 Merge; 2 Sort; 3 Convert Old Merge Codes: 0` | sort the block |
| Alt-F9 Graphics | `1 Figure; 2 Table Box; 3 Text Box; 4 User Box; 5 Line; 6 Equation: 0` | |
| F10 Save | `Document to be saved:` | `Block name:` (save block to file) |
| Shift-F10 Retrieve | `Document to be retrieved:` | |
| Ctrl-F10 Macro Define | `Define macro:` → `Description:` → recording (`Macro Def` blinks) / ends recording | |
| Home,Ctrl-F10 | Macro Editor directly (`Define macro:` name then editor) | |
| Alt-F10 Macro | `Macro:` prompt; runs named macro | |
| Alt-letter (A–Z) | runs `ALTx.WPM` macro if it exists | |

---

## 2. Cursor movement keys

All taken from WPHELP.FIL (screens "Home", "Left/Right arrow", "Up/Down arrow", "Go To",
"Page Up / Page Down", "Screen Up/Down", "Word Left/Right", "Paragraph Up/Down", "Item …").

| Keys | Action |
|---|---|
| Left / Right | one character (or one code — invisible in the normal screen, visible in Reveal Codes). At line start/end wraps to the adjacent line. |
| Up / Down | one line, keeps column ("desired column" is remembered) |
| Ctrl-Left / Ctrl-Right | previous / next word ("words" = groups of characters separated by spaces) |
| Ctrl-Up / Ctrl-Down | **Paragraph Up / Down** (Enhanced keyboard only; verified in ALTRNAT.WPK: E0 8D→Para Up, E0 91→Para Down) |
| Alt-Up/Down/Left/Right | **Item Up/Down/Left/Right**: moves by cell in tables, by paragraph number in outlines, by column in text columns (Enhanced keyboard; verified in WPK table) |
| Alt-Home | "Alt Home" prefix (used with Item keys: first/last item [?]) |
| End | end of line (also Home,Home,Right) |
| Home,Left / Home,Right | left / right edge of the screen, or line start/end, whichever comes first |
| Home,Home,Left | beginning of line **after** codes at line start |
| Home,Home,Home,Left | beginning of line **before** any codes |
| Home,Home,Right | far right of line |
| Home,Up / Home,Down | top / bottom of screen (= Screen Up/Down) |
| Home,Home,Up / Home,Home,Down | beginning / end of document |
| Home,Home,Home,Up | beginning of document **before** all codes |
| Keypad `-` / Keypad `+` | **Screen Up / Screen Down** (with Num Lock off): to top/bottom of screen; if already there, scroll a whole screen (24 lines) |
| PgUp / PgDn | top of previous / next page |
| Ctrl-Home | **Go To**: prompt `Go to ` (see below) |
| Esc (Alt kbd: F3) | **Repeat Value**: prompt `Repeat Value = 8` (see §3.1) |

### 2.1 Go To (Ctrl-Home) — prompt `Go to ` at bottom-left, then:

| Then press | Cursor goes to |
|---|---|
| Up | top of current page |
| Down | bottom of current page |
| Right / Left | next / previous text column |
| Home,Right / Home,Left | last / first column |
| *n* Enter | top of page *n* |
| any character *x* | next occurrence of *x* within the next 2,000 characters |
| Ctrl-Home again | last place a major motion command was given (undo last jump) |
| (Block on) Alt-F4 | beginning of the block |
| (Block on) Ctrl-Home | re-highlights the previous block |

---

## 3. Editing keys

| Keys | Action | Code inserted / notes |
|---|---|---|
| Enter | hard return | `[HRt]` (or `[HRt-SPg]` when it falls on a page break) |
| Ctrl-Enter | hard page | `[HPg]`; shown as a line of `=` across the screen [display.md] |
| Home,Enter | invisible soft return | `[ISRt]` |
| Tab | tab to next stop (insert mode) / move cursor (typeover) | `[Tab]`, `[Cntr Tab]`, `[Rgt Tab]`, `[Dec Tab]` depending on tab type at that stop |
| Home,Tab | hard tab | upper-case form: `[TAB]` etc. [?] |
| Shift-Tab | Left Margin Release | `[←Mar Rel]` |
| F4 (with hanging indent: F4,Shift-Tab) | indent / hanging indent | `[→Indent]` `[←Mar Rel]` |
| Home,Shift-F6 | hard centred tab | `[CNTR TAB]` [?] |
| Home,Alt-F6 | hard right aligned tab | `[RGT TAB]` [?] |
| Home,Space | **hard space** | `[ ]` |
| `-` (hyphen key) | hyphen character | `[-]` |
| Home,`-` | **hard hyphen** (minus sign; word never broken there) | shown as plain `-` in Reveal Codes [?] |
| Ctrl,`-` | **soft hyphen** | displayed only when the word falls in the hyphenation zone; in Reveal Codes a highlighted `-` [?] |
| Home,`/` | cancel hyphenation | `[/]` |
| Backspace | delete character/code left of cursor (typeover: replace with space) | |
| Del | delete character/code at cursor | |
| Ctrl-Backspace | **Delete Word** at cursor (if on a space: the word to the left) | |
| Home,Backspace | delete left to word boundary | |
| Home,Del | delete right to word boundary | |
| Ctrl-End | **Delete to End of Line** (EOL char not deleted) | |
| Ctrl-PgDn | **Delete to End of Page**: prompt `Delete Remainder of page? No (Yes)` | |
| Ins | toggle **Typeover** (`Typeover` shown bottom-left) | |
| Home,Ins | **forced Typeover** (per help screen "Insert/Typeover": "To force typeover mode … press Home, Ins"; the Home screen of the same help file says the opposite — WP 5.1 interim notes list Home,Ins = forced insert help and Home,Home,Ins = forced typeover help) **[?] — conflicting sources** |
| Home,Home,Ins | forced Insert (see conflict above) [?] |
| Ctrl-Del (Enhanced BIOS) | Block ON: Block Move (= Ctrl-F4,1,1) then `Move cursor; press Enter to retrieve.`; Block OFF: Delete Word; in a table row: delete row (confirm) |
| Ctrl-Ins (Enhanced BIOS) | Block ON: Block Copy (= Ctrl-F4,1,2); Block OFF: Hard Page [per help]; in table: insert a row above |
| Ctrl-PgUp | while defining a macro: `1 Pause; 2 Display; 3 Assign; 4 Comment: 0`; in Macro Editor: macro command list |
| Alt-= | activate the pull-down Menu Bar |
| Ctrl-V | **Compose** with prompt `Key = ` (only on editing screens) |
| Ctrl-2 | **Compose** with no prompt (works on any screen) |
| Ctrl-6 | revert to original keyboard (twice = restore previous) |
| Alt + keypad digits | enter IBM (CP437) character by number |

### 3.1 Esc / Repeat Value (Alt keyboard: F3)
Prompt `Repeat Value = 8` at bottom-left (8 = Setup→Initial Settings→Repeat Value). Type a number
to change it (Enter makes it the new value for this session), then:

| Then | Effect |
|---|---|
| Up/Down | *n* lines |
| Left/Right | *n* characters |
| PgUp/PgDn | *n* pages |
| Ctrl-Left/Right | *n* words |
| Alt-F10 (macro) | run macro *n* times |
| Del | delete *n* characters right |
| Ctrl-Backspace | delete *n* words |
| Ctrl-End | delete *n* lines |
| Ctrl-PgDn | delete *n* pages |
| any character | insert *n* copies |
| Esc (Alt kbd: F3) / Cancel | cancel |

### 3.2 Compose (Ctrl-V / Ctrl-2)
* Two-character digraph/diacritic: e.g. `A`,`E` → Æ; `e`,`'` → é; `a`,`"` → ä; `s`,`s` → ß
  (order of the two characters does not matter).
* `set,number` Enter: WP character set character, e.g. `1,23` Enter = ß (12 sets, ~1500 chars).
* Bullets: `*` + `.` small filled, `*` + `*` medium filled, `*` + `o` small hollow, `*` + `O` large hollow.
* Fractions: `/2` → ½, `/4` → ¼.
* Characters not displayable on the screen show as a placeholder block; Reveal Codes shows the
  character number [?format].

### 3.3 Block (Alt-F4 / F12) behaviour
* `Block on` blinks at the bottom-left status area (where the filename normally is).
* Extend with any cursor key, by typing a character (extends to its next occurrence), by
  Enter (to next `[HRt]`), by `.` `?` `!` (sentence) or by F2/Shift-F2 search.
* Block highlight uses the "Blocked" screen attribute (display.md).
* Del/Backspace with block: `Delete Block? No (Yes)`.
* Ctrl-Home,Alt-F4 → start of block; Ctrl-Home,Ctrl-Home → rehighlight previous block.

### 3.4 Yes/No prompts
All `X? No (Yes)` / `X? Yes (No)` prompts: the word before the parenthesis is the default.
`Y`/`y` = Yes, `N`/`n` = No, Enter = default, Cancel = cancel. (+ and - also answer Yes/No in 5.1
interim releases.) Any other key = default [?].

### 3.5 Menu selection keys (all menus)
Type the number **or** the mnemonic letter (case-insensitive). Enter / 0 / Space(?) = choose nothing
and exit (default `0` at the cursor). Cancel = back up one level. Exit (F7) = leave all menus,
saving changes. In list screens, `N` or `=` = Name Search; Up/Down/PgUp/PgDn/Home-Home-Up move the
highlight.

### 3.6 Dead keys (DE_*.WPK)
`DE_USA.WPK` defines dead keys for `"` and `^`; `DE_DOS.WPK` for `'`, `` ` ``, `^`, plus
German key positions (Z/Y swap etc.). Not active in her `.SET`, but present. [?] whether she used them.

---

## 4. Decoded key → function table (ALTRNAT.WPK)

Function codes are the indices of the WP.MRS command-name table (also the macro key tokens).
Scancodes are PC BIOS scancodes; `E0 xx` = enhanced keys.

| Scancode / key | Function code | Name |
|---|---|---|
| F1 (3B) | 0x22 | Help  *(original: Cancel 0x20)* |
| F2 | 0x21 | Search |
| F3 (3D) | 0x1B | Esc (Repeat) *(original: Help 0x22)* |
| F4..F10 | 0x23..0x29 | Indent, List, Bold, Exit, Underline, End Field, Save |
| Shift-F1..F10 | 0x2C..0x35 | Setup, Search Left, Switch, L/R Indent, Date/Outline, Center, Print, Format, Merge Codes, Retrieve |
| Ctrl-F1..F10 | 0x44..0x4D | Shell, Spell, Screen, Move, Text In/Out, Tab Align, Footnote, Font, Merge/Sort, Macro Define |
| Alt-F1..F10 | 0x38..0x41 | Thesaurus, Replace, Reveal Codes, Block, Mark Text, Flush Right, Columns/Tables, Style, Graphics, Macro |
| F11 / F12 | 0x3A / 0x3B | Reveal Codes / Block |
| Esc (01) | 0x20 | Cancel *(original: Esc)* |
| Backspace | 0x50 | Backspace |
| Tab | 0x09 | Tab |
| Enter | 0x0A | Enter |
| Ctrl-Enter (0A/1C) | 0x5F | HPg |
| Home E0 47 | 0x08 | Home |
| Up E0 48 / Down E0 50 | 0x17 / 0x1A | Up / Down |
| Left E0 4B / Right E0 4D | 0x19 / 0x18 | Left / Right |
| PgUp E0 49 / PgDn E0 51 | 0x59 / 0x5A | Page Up / Page Down |
| End E0 4F | 0x55 | End |
| Ins E0 52 | 0x5D | Typeover |
| Del E0 53 | 0x51 | Del |
| Ctrl-Left / Ctrl-Right | 0x54 / 0x53 | Word Left / Word Right |
| Ctrl-End | 0x0B | Del to EOL |
| Ctrl-PgDn | 0x0C | Del to EOP |
| Ctrl-Home | 0x58 | Goto |
| Ctrl-PgUp | special (FC2C) | Macro commands |
| Ctrl-Up / Ctrl-Down | 0x63 / 0x64 | Para Up / Para Down |
| Ctrl-Ins / Ctrl-Del | 0x6E / 0x6D | Block Copy / Block Move |
| Alt-Home | 0x69 | Alt Home |
| Alt-Up / Alt-Down | 0x67 / 0x68 | Item Up / Item Down |
| Alt-Left / Alt-Right | 0x65 / 0x66 | Item Left / Item Right |
| Alt-= | 0x6B | Menu Bar |
| Keypad - / Keypad + | 0x5C / 0x5B | Screen Up / Screen Down |
| Shift-Tab | 0x5E [?] | Left Mar Rel |
| Ctrl-letters | 0x01..0x1D | Ctrl-A … (Ctrl-V = Compose prompt, Ctrl-X etc. insert WP control chars; Ctrl-letters are bindable) |

Full MRS name list (index order, 0x00…): Compose, ^A..^G, Home, Tab, Enter, Del to EOL, Del to EOP,
^M..^V, Up, Right, Left, Down, Esc, ^\, ^], Keyboard, ^_, Cancel, Search, Help, Indent, List,
Bold, Exit, Underline, End Field, Save, F11, F12, Setup, Search Left, Switch, L/R Indent,
Date/Outline, Center, Print, Format, Merge Codes, Retrieve, Shft F11, Shft F12, Thesaurus,
Replace, Reveal Codes, Block, Mark Text, Flush Right, Columns/Tables, Style, Graphics, Macro,
Alt F11, ALT F12, Shell, Spell, Screen, Move, Text In/Out, Tab Align, Footnote, Font,
Merge/Sort, Macro Define, Ctrl F11, Ctrl F12, Backspace, Del, Del Word, Word Right, Word Left,
End, Home-Home-Left, Invalid, Goto, Page Up, Page Down, Screen Down, Screen Up, Typeover,
Left Mar Rel, HPg, SHy, (2 unnamed), Para Up, Para Down, Item Left, Item Right, Item Up,
Item Down, Alt Home, Del Word (Row), Menu Bar, Block Append, Block Move, Block Copy.

---

## 5. CUA / "Enhanced" keyboard differences

WP 5.1 for DOS has **no CUA mode** (CUA keyboards arrived with WP 5.1 for Windows / WP 6.0).
What does differ:

1. **101/102-key "Enhanced" keyboard vs 84-key PC/AT**: F11 (=Reveal Codes) and F12 (=Block),
   the separate cursor block, Ctrl-Up/Down (Para), Alt-arrows (Item), Ctrl-Ins/Ctrl-Del (Block
   Copy/Move) only exist with an enhanced BIOS. The help offers two templates: "IBM Layout"
   (function keys in two columns on the left, PC/XT) and "Enhanced Layout" (12 keys in a row).
2. **Alternate keyboard** (`Shift-F1,3,9` or `ALTRNAT.WPK`): F1 Help, Esc Cancel, F3 Repeat —
   the user's configuration.
3. Shipped optional layouts in `WPM/`: `ENHANCED.WPK` (Home,Home,Home,Left; italics, large,
   very large on Ctrl keys; Ctrl-Up/Down = move by sentence; Alt-F6 combos for column moves),
   `MACROS.WPK`, `SHORTCUT.WPK`, `EQUATION.WPK`, `US.WPK`. Implement the WPK concept (key →
   function | key → macro | key → text) so they can be loaded later [optional].

---

## Sources
* A. `/mnt/i/Claude Space/wp51/WPHELP.FIL` decoded (nibble-coded, 5 sections, 413 screens) —
  full text in `docs/spec/extracted/wphelp_screens.txt` (not published: original program text) (templates: section 0 screens 91–92;
  editing keys: section 4 screens ≈ "Hyphen" … "Ctrl-Insert"; feature index "Features [A]…").
* B. `/mnt/i/Claude Space/wp51/WP.MRS` (key-name table), `WPM/ALTRNAT.WPK`, `WP{WP}.SET`.
* C. `/mnt/i/Claude Space/wp51/WP.EXE` plain strings (`extracted/wpexe_strings.txt`), WP.FIL
  compressed menu strings (`extracted/wpfil_menus.txt`).
* D. WPDOS: https://mendelson.org/wpdos/ (general), WP 5.1 Workbook https://mendelson.org/wpdos/WP51WKBK.pdf
* E. WP 5.1 interim-release change notes: https://mendelson.org/wpdos/51enhan.txt
