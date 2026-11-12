# WordPerfect 5.1 (DOS) macro (.WPM), keyboard (.WPK) and setup (WP{WP}.SET) file formats

Clean-room specification written from the reference install at
`/mnt/i/Claude Space/wp51` (her real WP 5.1 setup: 413 `.WPM`, 11 `.WPK`, 1 `.SET`).
I found no public byte-level documentation of these formats. Everything below comes
from her files, plus the name tables WordPerfect ships in **`WP.MRS`** (the
macro resource file) and **`WP.EXE`**. Each claim carries a confidence tag:
**[H]** high (verified across many files or read directly from WP's own
tables), **[M]** medium (consistent but only lightly verified), **[L]** low or
inferred.

The reference decoder is `tools/wpm_decode.py`. Decoded listings of every file are in
`decoded_macros/` (not in the public repository: decoded private macros). The full code tables are in `wp51_keycodes.tsv` and
`wp51_macro_commands.tsv`.

All multi-byte integers are **little-endian** unless stated otherwise.

---------------------------------------------------------------------------

## 1. Common container: the WPC prefix and index blocks

All WP 5.x auxiliary files start with the same 16-byte prefix **[H]**.

| off | size | meaning |
|----:|-----:|---------|
| 0 | 4 | magic `FF 57 50 43` (`\xFFWPC`) |
| 4 | u32 | absolute offset of the "body" (WPM: macro words; WPK/SET: = file length, because all data lives in packets) |
| 8 | u8 | product type, always `1` (WordPerfect) |
| 9 | u8 | file type: `0x01` macro (.WPM), `0x03` keyboard (.WPK), `0x11` setup (.SET), `0x19` macro resource (WP.MRS) |
| 10 | u8 | major version: `1` for WP 5.1 macros/keyboards, `0` for WP 5.0 macros (9 of her macros: ALTE, ALTK, ALTM, ALTO, ALTP, ALTQ, ALTU, ALTV, ALTX). SET has `1`, minor `0`. |
| 11 | u8 | minor version (`1` for 5.1 WPM/WPK) |
| 12 | u16 | encryption key, `0` = not encrypted (all her files are 0) |
| 14 | u16 | reserved, 0 |

The 5.0 macros use exactly the same body encoding. All 0x80xx/0xFCxx codes in them
decode correctly, so a loader can accept major version 0 and 1 alike **[H]**.

### 1.1 Index blocks (at offset 16)

An index block is an array of 10-byte entries. Entry 0 is the block header **[H]**:

| off | size | meaning |
|----:|-----:|---------|
| 0 | u16 | `0xFFFB` (bytes `FB FF`) |
| 2 | u16 | number of 10-byte entries in this block, **including this header** |
| 4 | u16 | size of the block in bytes (= count × 10) |
| 6 | u32 | absolute offset of the **next** index block, `0` = last |

Entries 1..count-1 are packet descriptors:

| off | size | meaning |
|----:|-----:|---------|
| 0 | u16 | packet type (`0` = unused slot, skip it) |
| 2 | u32 | packet length in bytes |
| 6 | u32 | absolute packet offset |

WPM and WPK files have a single block (`next = 0`). The SET file chains four blocks
(16 → 1047 → 1565 → 2323 → 0). Loaders must follow the chain and must skip
type-0 slots.

---------------------------------------------------------------------------

## 2. Macro files (.WPM)

Layout of every 5.1 macro **[H]**:

```
0x00  WPC prefix (file type 1)
0x10  index block: FB FF 04 00 28 00 00 00 00 00      (4 entries, 40 bytes, no next)
0x1A  packet 1: type 0x0001, len = strlen(desc)+1, offset 0x38
0x24  two empty slots (20 zero bytes)
0x38  description: NUL-terminated string  (packet 1)
body  u32@4 = 0x38 + len(desc)  -- macro words, often at an ODD offset
```

* **Description** (packet type 1): up to 39 characters plus NUL, as typed at the
  "Description:" prompt. It uses **WP document text encoding**: plain bytes are ASCII,
  and an extended character is the 4-byte sequence `C0 <index> <set> C0`. For example,
  COE.WPM's description "capitalized Ö" ends in `C0 3E 01 C0` (set 1, index 62) **[H]**.
  An empty description is a single NUL (77 of her macros).
* **Body** starts at the u32 at offset 4. **It is not word-aligned** (236 of the 410 files in `WPM/`
  have an odd body offset), so read bytes, not aligned words. The body is a sequence of
  u16 LE words terminated by `0x0000`. Some files end exactly at the body offset with no
  terminator (ACCENTED.WPM is a bare 0x39-byte file), so EOF also ends the macro. 21 of
  her macros have empty bodies.

### 2.1 Body word encoding

| word range | meaning | editor display |
|---|---|---|
| `0x0000` | end of macro | – |
| `0x0009` | editor formatting tab, **ignored on playback** **[H]** | a tab |
| `0x000A` | editor hard line break, **ignored on playback** **[H]** | new line |
| `0x000D` | editor soft line break (word-wrap inside long keystroke runs, e.g. CDOUT1, CLAIM, POWERPAK), **ignored on playback** **[M-H]** | new line |
| `0x0002` | unknown marker. Seen only as the first word of 17 keyboard macros in her (user-edited) SHORTCUT.WPK and in WPM/MACROS.WPK; never in a .WPM file. Treat it as a no-op **[L]** | (nothing) |
| `0x0020`–`0x007E` | literal character, typed as text. `0x007E` `~` is also the argument terminator (§2.3) **[H]** | the char |
| `0x0100`–`0x0CFF` | WP character: high byte = character set (1–12), low byte = index. Typed as that character **[H]** | the glyph |
| `0x8000`–`0x8074` | key/function codes (cursor keys, F-keys, editing functions), see `wp51_keycodes.tsv` **[H]** | `{Name}` |
| `0xFC00`–`0xFC3F` | macro programming commands, see `wp51_macro_commands.tsv` **[H]** | `{NAME}` |
| `0xFD01`–`0xFDFF` | `{KEY MACRO n}`: run keyboard macro *n* of the active .WPK **[H]** | `{KEY MACRO n}` |
| `0xFE41`–`0xFE5A` | `{ALT x}`: run Alt-letter macro `ALTx.WPM` (low byte = ASCII letter) **[H for meaning, M for display spelling]** | `{ALT X}` |
| `0xFF00`–`0xFF09` | `{VAR n}`: the Alt-*n* variable; typing it inserts the variable's contents **[H]** | `{VAR n}` |

Character-set evidence: 28 one-character accent macros have descriptions such as
"umlaut ae" for `0x011F`, giving 1,31 = ä. Together they confirm WP set 1 indices
23 = ß and 26..83 = `ÁáÂâÄäÀàÅåÆæÇçÉéÊêËëÈèÍíÎîÏïÌìÑñÓóÔôÖöÒòÚúÛûÜüÙùŸÿÃãĐđØøÕõ`.
Further confirmed: 4,0 •; 4,6 §; 4,7 ¡; 4,20 ²; 4,21 ³; 6,36 °; 6,37 µ (the last
five from the German AltGr keys in DE_DOS.WPK). Sets 3 (box drawing, used heavily in
CALC.WPM prompts), 5, 6 and 7 appear too. A full WP-charset→Unicode table is a
separate task. WP.DRS/.CRS hold WP's own tables.

### 2.2 Names: how WP displays codes

WP.MRS holds a table of 16-byte Pascal strings (length byte + up to 15 chars) at
file offset `0x34C` **[H]**:

* 3 prefixes: `"VAR "`, `"ALT "`, `"KEY MACRO "` (at 0x34C, 0x35C, 0x36C), used to
  build `{VAR n}`, `{ALT x}` and `{KEY MACRO n}`. WP.EXE contains the same three
  strings.
* 64 command names for `0xFC00+i` (at 0x37C). Name 0x0B is `;`, so `{;}` is the
  comment command. Name 0x3A is `~` (`{~}`, a literal tilde).
* 117 key names for `0x8000+i` (at 0x77C), up to 0x8074.

WP.MRS also contains the Ctrl-PgUp command menu (from 0xF18, 32-byte records:
**big-endian** u16 code + 30 chars of `{NAME}syntax`). That list supplies the argument
syntax in `wp51_macro_commands.tsv`. It spells 0xFC25 as `{RETURN NOT FOUND}`, while
the 15-char name table has `RETURN NOT FND`. The decoder uses the long form.

Rendering rule: `{` + name + `}`. Text, tabs and newlines are shown as-is. Example:
BOLD.WPM ("to left, block to end of line, bold") =
`FC0C 8008 8008 8019 803B 8055 8025` → `{DISPLAY OFF}{Home}{Home}{Left}{Block}{End}{Bold}`.

### 2.3 Programming-command arguments

Arguments are **not length-prefixed**. They follow the command word inline and each one
ends with an ASCII `~` (`0x007E`) **[H]**. The argument count per command is fixed
(`args` column of `wp51_macro_commands.tsv`):

```
{ASSIGN}var~expr~          {IF}expr~ ... {ELSE} ... {END IF}
{LABEL}name~   {GO}name~   {CALL}name~   {CHAIN}macro~   {NEST}macro~
{FOR}var~start~stop~step~ ... {END FOR}
{CASE}expr~ case1~label1~ case2~label2~ ... ~     (list ends with an empty argument)
{FOR EACH}var~ e1~ e2~ ... ~                        (list ends with an empty argument)
{ON NOT FOUND}action~       e.g. {ON NOT FOUND}{GO}end~~
```

Parsing rules, validated by `parse_structure()` in the decoder against all 99,496
words. Only INLINE.WPM fails, and that is a real authoring error (§2.6):

1. **Nesting.** An argument may contain other commands that have arguments. Those
   consume their own `~`. Examples: `{IF}{SYSTEM}4~!=0~`,
   `{ASSIGN}count~{VARIABLE}count~+1~`, `{ON NOT FOUND}{GO}end~~`,
   `{IF}{KTON}{SYSTEM}18~~=126~`. Parse recursively.
2. **Formatting words** `0009/000A/000D` inside arguments are ignored. `{CASE}` lists
   are routinely laid out one pair per line. Spaces (0x0020) **are** significant.
3. **Comments** `{;}text~` may appear between `{CASE}` items (MACROS.WPK key macro 12)
   and anywhere else. Strip them before evaluating. When you decide whether a list item
   is the empty terminator, skip formatting and comments first.
4. **Literal tilde** inside an argument is `0xFC3A` `{~}`, as in
   `{ASSIGN}addisp~{~}~` (REVEALCO, REVEALTX).
5. **Expansion.** `{VARIABLE}v~`, `{VAR n}`, `{SYSTEM}x~`, `{STATE}`, `{MID}…`,
   `{LEN}`, `{KTON}`, `{NTOK}` inside an argument are expanded textually before the
   expression is evaluated. Outside arguments, `{VARIABLE}`/`{VAR n}` "type" the
   contents into WP.
6. **Expression operators** seen in her files: `= != < > & | ! + - * /`, parentheses,
   and double-quoted string comparison (`{IF}"{VAR 6}"!="="|"{VAR 6}"!="{Enter}"~`).
   WP 5.1 arithmetic is integer. `&`, `|` and `!` are bitwise/logical AND/OR/NOT,
   which is how `{STATE}&4` and `{SYSTEM}1~&256` are used.
7. Inside arguments, non-text words (e.g. `{Enter}` in the string comparison above)
   are compared as keystroke tokens. Store argument values as **word sequences, not
   byte strings**.

### 2.4 Labels, control flow and nesting

`{LABEL}x~` marks a position. `{GO}`/`{CALL}`/`{CASE}` search the whole macro for the
label (labels may be after the jump; REVEALBX jumps to `{LABEL}end~` further down).
`{CALL}` and `{CASE CALL}` push a return address, and `{RETURN}` pops it.
`{NEST}macro~` runs another .WPM file as a subroutine. `{CHAIN}macro~` queues a macro to
start when the current one ends. `{ALT x}` (0xFExx) and `{KEY MACRO n}` (0xFDxx)
inside a body invoke `ALTx.WPM` and the keyboard's macro *n*. WP 5.1 treats these as
nested invocations **[M]**. `{ON CANCEL}/{ON ERROR}/{ON NOT FOUND}action~` install a
handler. `{RETURN CANCEL/ERROR/NOT FOUND}` raise the condition in the caller. Macro
names in `{NEST}/{CHAIN}` are file names without `.WPM`, looked up in the
Keyboard/Macro directory from the .SET file (§4) **[M]**.

### 2.5 `{STATE}` bits and `{SYSTEM}` variables

`{STATE}` bits, as used and commented in her macros **[H for 4, 8, 128, 256, 512,
1024; M for others]**:

| bit | meaning (evidence) |
|---:|---|
| 1, 2 (`&3`) | current document number (1 or 2) |
| 4 | main editing screen (52 uses, comments "Editing screen") |
| 8 | editing a substructure: header, footnote, etc. (`{STATE}&8~{Exit}`) |
| 16 | macro definition active |
| 32 | macro execution active (always set while running; `({STATE}&4092)=32` = "assume you're in help") |
| 64 | merge active (`{STATE}&64~{Merge Codes}3`) |
| 128 | block on ("Block on") |
| 256 | typeover on (`&256~{Typeover}`) |
| 512 | Reveal Codes on ("reveal codes") |
| 1024 | yes/no question pending ("if at a yes/no question") |
| 2048 | in a list ("if in a list or a macro") |

`{SYSTEM}` variables can be given by number or by name. The names come from WP.EXE
at file offset ≈0x188A0, together with a 29-entry pointer table that gives the
numbering. The 1-based numbers are confirmed by comparing the user-edited root
SHORTCUT.WPK (names) with the original WPM/SHORTCUT.WPK (numbers): attrib=1, left=9,
menu=13, print=17, right=18 **[H for those, M for the rest]**:

| # | name | # | name | # | name |
|--:|---|--:|---|--:|---|
| 1 | ATTRIB (bit set: 1 XLarge … 256 Italics … 4096 Bold, 16384 Underline) | 11 | LIST | 21 | USERBOX |
| 2 | CELL ("Remember cell we are in") | 12 | NAME (document name) | 22 | ROW |
| 3 | COLUMN | 13 | MENU (current menu id; 65535 = edit screen; 297/298 table menus) | 23 | CELLATTR |
| 4 | DOCUMENT (bit 512 = in a table) | 14 | PAGE | 24 | CELLSTATE |
| 5 | ENDNOTE | 15 | PATH | 25 | SHELLVER |
| 6 | EQUATION | 16 | POS | 26 | KEYSTATE (bit 64 used in DE_DOS dead keys) |
| 7 | FIGURE | 17 | PRINT (bit 8 = jobs waiting) | 27 | ROWSTATE |
| 8 | FOOTNOTE | 18 | RIGHT (char right of cursor) | 28 | VERSION |
| 9 | LEFT (char left of cursor) | 19 | TABLEBOX | 29 | ENTRY |
| 10 | LINE | 20 | TEXTBOX | | |

WP.EXE also contains the string `PARA`, which is not in the numbered table, so it is
probably an alias **[L]**. Name matching is case-insensitive (`{SYSTEM}Attrib~` occurs).

### 2.6 Robustness notes for the interpreter

* INLINE.WPM's last `{;}` comment has no closing `~`. The comment therefore swallows
  the final `{END IF}`, and the comment runs to end of body. End of body must silently
  close any open argument or comment and any open block.
* `{Macro Commands}` (0xFC2C) sits in the command range but is a **keystroke** (the
  Ctrl-PgUp key). In ALTJ/ALTR, `{Macro Commands}31` assigns the blocked text to
  variable 1.
* Keystrokes are interpreted by WP's own state machine: menus, prompts, mnemonic
  letters (`{Format}161.5{Exit}`). Exact replay therefore needs the interactive
  emulation to accept the same key sequences.

---------------------------------------------------------------------------

## 3. Keyboard files (.WPK)

```
0x00  WPC prefix (file type 3, version 1.1)
0x10  index block: FB FF 05 00 32 00 00 00 00 00  (5 entries)
      packet 1 (type 1)  len 718   @ 0x42 : key table
      packet 2 (type 2)  len var   @ 0x310: keyboard macros (absent if none)
      packet 3 (type 3)  len 45*n  @ after: key descriptions (may be len 0)
```

The original keyboard (no .WPK selected) uses the identical 718-byte table stored in
WP.MRS packet 2 (offset 0x5E). WPM/US.WPK has exactly the same bytes. The decoder
embeds this table as `DEFAULT_TABLE` **[H]**.

### 3.1 Packet 1: key table (718 bytes = 359 words)

The table is indexed by what BIOS INT 16h returns (ASCII byte, scan byte) **[H]**:

```
+0x000  u8 first, u8 last = 0x0F, 0xA6      range A: extended keys (ASCII = 0), by scan code
+0x002  152 × u16 action, for scan = 0x0F..0xA6   (entry offset = 2 + 2*(scan-0x0F))
+0x132  u8 first, u8 last = 0x01, 0x7F      range B: ASCII codes
+0x134  127 × u16 action, for ASCII 0x01..0x7F    (entry offset = 0x134 + 2*(c-1))
+0x232  pair lists: records {u8 ascii, u8 scan, u16 action}, each list ends with
        record FF FF 00 00:
        list 1: E0-prefixed grey keys (ascii 0xE0): grey Home/Up/PgUp/Left/Right/End/
                Down/PgDn/Ins/Del and Ctrl-/Alt- variants
        list 2: overrides for ambiguous ASCII keys: Esc(1B,01), Backspace(08,0E),
                Tab(09,0F), Ctrl-Enter(0A,1C)→{HPg}, Enter(0D,1C), keypad Enter (E0)
        list 3: keypad * - + / , NumLock OFF (default: - → {Screen Up}, + → {Screen Down})
        list 4: same keys, NumLock ON (default: the literal characters)  [M: which list is which]
```

Scan-code assignment was verified against the default table: 0x10–0x19 Alt-Q…P →
`{ALT Q}`…, 0x3B–0x44 F1–F10 → 8020–8029, 0x54/0x5E/0x68 Shift/Ctrl/Alt-F1…,
0x73/0x74 Ctrl-←/→ → Word Left/Right, 0x78–0x81 Alt-1…0 → `{VAR n}`, 0x84 Ctrl-PgUp →
`{Macro Commands}`, 0x85/0x86 F11/F12 → Reveal Codes/Block, 0x8D/0x91 Ctrl-↑/↓ → Para
Up/Down, 0x92/0x93 Ctrl-Ins/Del → Block Copy/Move, 0x97–0xA0 Alt-cursor → Item
keys. Further checks: DE_DOS.WPK maps Alt-Q (scan 0x10) to `@` and Alt-M to µ (German
AltGr), and ENHANCED.WPK's description records name the keys they redefine **[H]**.
The decoder's `SCAN_NAMES` gives a name to every slot.

**Action word values** use the same encoding as macro bodies: `0x00xx` char,
`0x01xx–0x0Cxx` WP char, `0x80xx` function, `0xFC2C` Macro Commands, `0xFDnn` run
keyboard macro *n*, `0xFExx` run ALTx.WPM, `0xFFnn` `{VAR n}`. Special values:
`0x0000` = key does nothing **[M-H]**. `0xFFFF` = slot is not a real key (Ctrl, Shift,
Alt, Space, CapsLock, NumLock and ScrollLock scans, and in the ASCII range Ctrl-^
(0x1E)) **[M]**. `0x7FFF` is the default only for ext-scan 0x4A (Alt-keypad-minus);
its meaning is unknown, and DE_USA sets it to 0 **[L]**.

Key resolution for an incoming (ascii, scan) pair **[M]**:
1. ascii = 0xE0 → list 1.
2. (ascii, scan) found in list 2 → that action.
3. keypad * - + / → list 3 or 4 by NumLock.
4. ascii = 0 → range A by scan.
5. otherwise range B by ascii, and characters ≥ 0x80 are typed directly.

Ctrl-2 (NUL) and Ctrl-V compose are handled inside WP.

### 3.2 Packet 2: keyboard macros

```
u8 count
count × { u8 n (macro number, the nn of 0xFDnn); u16 byte_length; byte_length bytes of
          macro words (same encoding as a .WPM body), ending in 0x0000 inside byte_length }
```

Records are not sorted by *n*. Keyboard macros have no description of their own
(see packet 3). Verified for all 7 WPKs that have packet 2, where the parse lands
exactly on the packet end **[H]**. Macro sizes run up to 15,992 bytes (MACROS.WPK #2).

### 3.3 Packet 3: key descriptions

An array of 45-byte records. Packet length = 45 × n **[H]**:

| off | size | meaning |
|---:|---:|---|
| 0 | u8 | ASCII byte of the key (0 for extended keys) |
| 1 | u8 | scan byte (or E0 etc., as in the pair lists) |
| 2 | u16 | **byte offset within packet 1** of the action word this key uses (e.g. 0x5A = F1, 0x288 = Esc in list 2) |
| 4 | u8 | description length including NUL |
| 5 | 40 | description, WP-encoded (C0 c s C0 extended chars), NUL-terminated. Bytes after the NUL are stale garbage: ignore them. |

Descriptions exist for keys mapped to macros and also for keys remapped to other
keys/characters. ALTRNAT.WPK: F1 → `{Help}` "Help", F3 → `{Esc}` "Esc",
Esc → `{Cancel}` "Cancel".

### 3.4 Her keyboards

The active keyboard in WP{WP}.SET is **ALTRNAT.WPK** (F1 = Help, F3 = Esc/Repeat,
Esc = Cancel). The other keyboards are: SHORTCUT.WPK (two versions: root = her edited
copy, WPM/ = WordPerfect original), MACROS.WPK (18 keyboard macros incl. a
glossary/expander and calculator), ENHANCED.WPK, EQUATION.WPK (Greek letters as WP
set-8 characters), DE_DOS.WPK and DE_USA.WPK (German layouts, dead-key macros using
`{^V}` compose and `{SYSTEM}keystate~`), and US.WPK (identical to the default). See
`decoded_macros/*.WPK.txt` (private) for the full per-key tables, with every deviation from the
default marked `*changed`.

---------------------------------------------------------------------------

## 4. Setup file (WP{WP}.SET)

Prefix: file type `0x11`, version 1.0, body offset = file length. The data is in
packets spread over 4 chained index blocks. Her file has these packets
(`python3 -I tools/wpm_decode.py WP{WP}.SET`):

| type | len | content | conf |
|---|---:|---|---|
| 0x0005 | 15 | registration/licence number, ASCII "6344603", NUL padded | M |
| 0x005E | 703 | **Location of Files**: 10 fields, each `u16 length` + path bytes, NUL padded. Field sizes 69,69,69,69,**82**,69,69,69,69,69. Her values in order: `C:\WP51\POWERPAK\`, `C:\WP51\`, `C:\WP51\`, `C:\WP51\JOBS\AUTOBACK\`, `LIBRARY.STY`, `C:\WP51\WPM\`, `C:\WP51\WPG\`, `C:\WP51\JOBS\`, `C:\WP51\STYLE\`, empty. Labels inferred from contents: printer files, thesaurus/spell main, supplementary, backup, style library *filename* (82-byte field), **keyboard/macro files**, graphic files, documents, style directory, unused | values H, field roles M |
| 0x0051 | 9 | `FF FF 07 01 3E 3A 04 3E 00`, unknown | L |
| 0x0052 | 254 | display and environment: bytes 0..14 unknown flags (bytes 9..11 = `" i i` look like units-of-measure codes, `"` = inches, `i` = inches) [L]; doc-summary subject search text "RE:" @+0x0F; graphics display name "VGA 640x480 16 Color" + driver STANDARD.VRS; text-mode names "80x25 16 Color" / "80x43 16 Color" + STANDARD.VRS; mouse driver STANDARD.IRS / "Mouse Driver (MOUSE.COM)" | strings H, numbers L |
| 0x0053 | 28 | **keyboard layout file name** "ALTRNAT.WPK" (NUL padded; empty = original keyboard) | H |
| 0x0054 | 241 | current printer: name (37-byte field) "Standard Printer", then .PRS file "STANDARD.PRS", then printer data | strings H |
| 0x0055 | 176 | initial settings: **date format** string "3 1, 4" at +3 (WP's default "Month Day, Year"); other words unknown | M |
| 0x0056 | 23 | unknown (`01 00 … 23 7C 00 00 C8 00 96 00 78 00 50 00 3C 00 3C 00`) | L |
| 0x0057 | 86 | printer-related state | L |
| 0x005A | 0 | empty | – |
| 0x0021, 0x0024 | 311 each | **screen colours**: two 155-byte tables each, `u8 count (20)` + 20 × {u32 attribute mask, u8 PC text attribute (bg<<4 \| fg)}. Masks in order: 0 Normal, 0x20000 Blocked, 0x4000 Underline, 0x2000 Strikeout, 0x1000 Bold, 0x800 Dbl Underline, 0x400 Redline, 0x200 Shadow, 0x100 Italics, 0x8000 Small Caps, 0x80 Outline, 0x40 Subscript, 0x20 Superscript, 0x10 Fine, 8 Small, 4 Large, 2 Very Large, 1 Extra Large, 0x5000 Bold+Underline, 0 (Other combinations). Her Normal = 0x1F (bright white on blue). The two tables are probably Doc 1 / Doc 2, and the two packet types two display modes. | table layout H, roles M/L |
| 0x005D | 448 | printer font width table | L |
| 0x005C | 1896 | list of installed printers (e.g. "Apple LaserWriter IINTX", APLASIIN.PRS) | M |
| 0x0059 | 17 | initial (base) font: "Courier 10 Pitch" | M |
| 0xFFFF | 4 | trailing junk `al)\0` | L |

The attribute bits match `{SYSTEM}attrib` (256 = italics in her SHORTCUT macro). So
the same 16-bit attribute mask serves both colours and macros.

Not present or not located in her SET: backup timer, initial codes, units for each
field, cursor speed, beep options. They may sit inside the unknown packets 0x51,
0x52, 0x55 and 0x56. A safe loader decodes the H/M items above and keeps every
packet's bytes verbatim so that it can write them back unchanged.

---------------------------------------------------------------------------

## 5. What a Rust loader needs (summary)

1. Parse the WPC prefix, then walk the index-block chain to get packets.
2. WPM: description = packet 1 (WP-encoded string); words = u16 LE from `body_off`
   (unaligned) to `0x0000` or EOF.
3. Tokenise the words: text / WP char / formatting (drop 09, 0A, 0D, and the leading
   0x0002) / key 0x80xx / command 0xFCxx with N `~`-terminated arguments parsed
   recursively / FD, FE, FF specials.
4. WPK: 718-byte key table (2 ranges + 4 pair lists), macros packet (u8 count +
   {u8 n, u16 len, words}), 45-byte description records.
5. SET: keyboard file name (0x53), macro directory (0x5E field 6), colours (0x21/0x24),
   date format (0x55). Keep everything else opaque.

Sources: her files under `/mnt/i/Claude Space/wp51` (WP.MRS name and command tables;
WP.EXE system-variable table; macro descriptions and comments). A web search
(mendelson.org/wpdos, discmaster.textfiles.com WPK.DOC) turned up only user-level
documentation and no byte-level format description. The `{STATE}` and
`{SYSTEM}` meanings without a file citation follow the WP 5.1 reference manual
(mendelson.org/wpdos/manuals.html) as recalled. Treat those as [M].
