# WordPerfect 5.0 / 5.1 document format (import specification)

Status: implementation-ready for import. Verified against 13 real files (see `wp5_test_report.md`).
Companion files:

* `wp5_charsets.tsv`: WP character sets 0–12 mapped to Unicode.
* `wp5_dos_codepages.tsv`: DOS code pages 437 and 850 mapped to WP characters.
* `tools/wp5_dump.py`: a reference walker for this spec. It prints Reveal-Codes-style output.

Sources (abbreviations used below):

| tag | source |
|---|---|
| **TK** | *WordPerfect Developer's Toolkit for DOS*, section "WordPerfect 5.1 Document Format" (WPCorp, c.1992). PDF made from the Novell Envoy edition, at mendelson.org/wpdos/WP51DevelopersToolkit.pdf. Primary authority. |
| **INC** | `WPFUNC.INC` and `PREFIX.INC` from the WP 5.1 SDK diskette (`wp51sdk.zip`, mendelson.org/wpdos). These are WPCorp's own assembler equates. |
| **SDK** | Other files on the same diskette: `CHARMAP.ASM`, `CHAR.TST`, `PREFIX.QCK`, and the sample documents. |
| **LWPD** | libwpd (git.code.sf.net/p/libwpd/code): `src/lib/WP5*.cpp` and `libwpd_internal.cpp`. Its WP5 coverage is partial, but it records behaviour seen in real files. |
| **OBS** | Observed in the real files tested with `tools/wp5_dump.py`. |

Where sources disagree, this document says so and states which one we follow.

---

## 1. Conventions

* All integers are **little-endian**. *byte* = u8, *short* = u16, *long* = u32. A field is unsigned unless marked signed (s16).
* **WPU** (WordPerfect unit) = 1/1200 inch. Nearly every position, margin and size in a 5.x document is in WPU.
  * Font point sizes are in 1/3600 inch, so points = value / 50.
  * Some fields hold "screen units" (su = text-screen columns). Ignore them on import; WordPerfect recomputes them.
* Many functions store **old** and **new** values. Only the *new* values matter. "Old" values are formatter state from before the code was inserted; when writing, fill them with 0 (TK p.17).
* **Hash** fields are internal formatter checksums. Ignore them on read and write 0 (TK p.17).
* Offsets in the function tables are measured **from the function's first byte**, as in TK. For a variable-length function, the payload starts at offset 4, so payload-relative offset = TK offset − 4. `tools/wp5_dump.py` uses payload-relative offsets.
* **WP string**: bytes 0x20–0x7E are ASCII. A `C0 char set C0` sequence (§5.3) encodes any other character. A string may contain NUL bytes inside an extended-character code, so a reader must parse C0 codes before it looks for a terminating NUL (TK p.47).

---

## 2. File header (16 bytes, offset 0)

| off | size | field | value in a 5.x document |
|---|---|---|---|
| 0 | 4 | WPCorp file ID | `FF 57 50 43` (0xFF,"WPC") |
| 4 | long | Start of document area | Absolute file offset of the first body byte |
| 8 | byte | Product type | 1 = WordPerfect |
| 9 | byte | File type | 10 (0x0A) = document. Other values are listed below. |
| 10 | byte | Major version | **0** for 5.0 and 5.1 |
| 11 | byte | Minor version | 0 = WP 5.0, 1 = WP 5.1 (also WPWin 5.1/5.2) |
| 12 | short | Encryption key | 0 = not encrypted. Otherwise a hash of the password (§2.2). |
| 14 | short | Reserved | 0 |

### 2.1 Identifying a file

* Accept the file as a 5.x document when the ID is `FF "WPC"`, product = 1, file type = 10 and major = 0. Minor 0 or 1 makes no structural difference: 5.1 features show up in 5.0 as `[Unknown]` and survive round-trips.
* Major = 2 means a WP 6.x/7+ document, which is a different format. Major = 1 occurs in WP 5.1 settings files (OBS: `WP{WP}.SET` has product 1, type 17, version 1.0).
* Other WP file types that a user may try to open (TK p.5): 0x0D block, 0x0E rectangular block, 0x0F column block, 0x10 .PRS, 0x11 setup (.SET), 0x13 .ALL, 0x14 .DRS, 0x16 .WPG, 0x01 macro (.WPM), 0x03 keyboard (.WPK).
* **Style libraries (`*.STY`) are ordinary type-10 documents.** They have an empty body: the start-of-document offset equals the file size. Their styles live in prefix packets 0x100–0x1FF (OBS: `STYLE/LIBRARY.STY`).
* **No header → WP 4.2 or plain text.** WP 5.1 decides by looking at the end of the *first line*. CR LF means a DOS text file. A lone 0x0A or 0x0D means a WP 4.2 document (TK p.100). WP 4.2 stores IBM code-page bytes directly. Its format is in `WP42FF.TXT` in the SDK and is out of scope here.

### 2.2 Password protection (WP 5.x)

The algorithm is not documented by WPCorp (PRONOM says so). The description below is reconstructed from libwpd `WPXEncryption.cpp` (LWPD) and has not been verified on a real encrypted file.

* Password: convert to upper case (ASCII a–z only). Let `L` = its length.
* Check value, stored at header offset 12: start with `c = 0`. For each password byte `p`, set `c = ror16(c, 1) XOR (p << 8)`.
* Decryption: bytes 0–15 are in clear. For every file byte at offset `i ≥ 16`, let `k = i − 16`. Then `plain = cipher XOR pw[k mod L] XOR ((L + 1 + k) & 0xFF)`.
* This covers the prefix as well as the body. Decrypt the whole file before parsing.

---

## 3. Prefix area ("additional prefix")

The prefix runs from offset 16 up to the start-of-document offset. It is a **chain of index blocks**. Each block has exactly 5 entries of 10 bytes each (50 bytes): one *special header index* followed by four *general indexes*. Packet data can lie anywhere in the file; in practice it follows the index block (TK p.7–8).

**Special header index** (first 10 bytes of each block):

| off | size | field |
|---|---|---|
| 0 | short | 0xFFFB (marks an index block) |
| 2 | short | Number of indexes in the block, including this one. Always 5. |
| 4 | short | Size of the index block = 50 |
| 6 | long | Absolute offset of the next index block. 0 = this is the last block. |

**General index** (four per block):

| off | size | field |
|---|---|---|
| 0 | short | Packet type. 0 = end of prefix (also unused entry), 0xFFFF = deleted packet. 0xFFFC–0xFFFE are reserved. |
| 2 | long | Packet length in bytes |
| 6 | long | Absolute file offset of the packet data |

Reading rules:

* Follow `next` until it is 0 or points to a block already seen. libwpd guards against cycles in this chain.
* Skip type 0 and type 0xFFFF entries.
  * With Fast Save on, deleted packets stay in the file (OBS: KEYBRDS.WP has 7 of them). Their data is garbage.
  * Packet order in the file is arbitrary (OBS).
* Stop if a block header is not 0xFFFB/5/50; treat the rest of the prefix as corrupt (LWPD does the same).
* If the same type occurs twice, the last non-deleted one wins.

### 3.1 Packet types

| type | content | import use |
|---|---|---|
| 1 | Document summary | metadata → docx core properties |
| 2 | Fonts used (WP 5.0 layout) | font names and sizes |
| 3 | Document initial codes | **Important.** A stream of body codes (§5) that apply before the first body byte. Equivalent to Shift-F8 → Document → Initial Codes. |
| 4, 5 | "Reserved" | Present in real 5.0 files (OBS: KERN.TST). Ignore. |
| 6 | Document-specific flags | ignore (display/print quality, redline method) |
| 7 | Font name string pool | Font names: NUL-terminated strings, addressed by byte offset into the packet |
| 8 | Graphics images (embedded WPG) | images for figure boxes |
| 9 | Form (paper) hash table | ignore |
| 12 (0x0C) | Selected printer | informational |
| 14 (0x0E) | Supplemental dictionary words | ignore |
| 15 (0x0F) | Fonts used (WP 5.1 layout) | font names and sizes |
| 16 (0x10) | WPWin: DDE link present. UNIX: page count. | ignore |
| 17, 19 (0x11, 0x13) | WPWin macro code and info | ignore |
| 21 (0x15) | WPWin 5.2 OLE objects | ignore or preserve |
| 0x100–0x1FF | **Styles**, one packet per style | style definitions |
| 0x200–0x2FF | PS (proportional spacing) width tables | ignore |

### 3.2 Packet 1: Document summary

The packet is a sequence of **NUL-terminated WP strings**, in this order (maximum lengths include the NUL):

creation date (26), descriptive name (5.1: 68 / 5.0: 41), descriptive type (21, 5.1 only), subject (161 / 40), author (61 / 40), typist (61 / 40), abstract (781 / 780).

WP 5.1 then adds: account (161), keywords (161), the byte 0xFF, a 10-byte creation date, and the byte 0xFF.

* Creation date, name and type are padded with spaces to their full length.
* A packet that **ends in 0xFF** is in 5.1 layout; one that ends in 0x00 is in 5.0 layout. Early WP 5.1 releases (11/6/89) kept a 5.0-layout summary inside a 5.1 file, so test the last byte rather than the header version.
* 10-byte date: year (short), month, day, hour, minute (bytes). The remaining 4 bytes are unused (0).

### 3.3 Packets 7, 15 and 2: fonts

* **Packet 7**: concatenated NUL-terminated font names. Records in packets 15/2 point to a name by its *byte offset* within this packet.
* **Packet 15** (5.1): an array of **86-byte** records, one per font used. The *matched font number* in a Font Change code (§6.2) indexes this array (TK p.103).

| off | size | field |
|---|---|---|
| 0 | short | font ID |
| 2 | short | PS-table cache ID (0xFFFF = none) |
| 4..17 | | shoulder heights, widths (psu/wpu) |
| 18 | short | **offset of font name in packet 7** |
| 20 | byte | flags |
| 21 | short | descriptor hash. Compare with the hash in the Font Change code; a mismatch means the file was Fast-Saved and the font link is unreliable. |
| 23 | 27 | font descriptor. Within it: +0 cell height (3600ths), +2 optimal width, +12 typeface descriptor (3 bytes, style bits), +15 typeface flags, +24 **point size (3600ths)**, +26 typeface flags (1 small caps, 2 shadow, 4 outline, 8 italic, 0x10 PS; weight in the top 3 bits: 0x00 extra light … 0x40 normal, 0x80 bold … 0xE0 ultra-black). |
| 50 | 16 | attribute AFC list: alternate font per attribute 0–15 (0xFF = none, 0xFE = unavailable) |
| 66 | 16 | character-set AFC list: alternate font per WP character set 0–12 |
| 82 | 4 | orientation AFC list |

* **Packet 2** (5.0): TK documents an 86-byte record with the name pointer at +18 and the point size (3600ths) at +22. **In the real file tested the record is 82 bytes**: KERN.TST has one font and a packet length of 82, which suggests the 4-byte orientation AFC list is absent in 5.0. Treat the record size as 82 when the packet length is a multiple of 82 but not of 86. libwpd assumes 86. The point size at +22 is confirmed: 500/50 = 10 pt for "Courier 10pt 10 Pitch". Packet 15 keeps the point size at +47 (descriptor +24).
* **Default font**: record 0 of packet 15 (or 2) is the document's initial font (TK p.103, LWPD).
* **AFC chaining** (automatic font changes, TK p.104) chooses a different printer font for an attribute, an extended character set or an orientation. It is a printing concern only. **Ignore it on import.**

### 3.4 Packet 8: embedded graphics

**The index length of packet 8 is always 2.** The image data follows it and is not counted in that length. The packet must be the last one in the prefix (TK p.12; OBS: PRINTER.TST).

| off | size | field |
|---|---|---|
| 0 | short | `N` = number of images |
| 2 | long × N | size of each image |
| 2+4N | … | image data, back to back. Each image is a WPG 1.0 file (`FF "WPC"`, product 0x01 / type 0x16). Some embedded images lack the 16-byte WPG header; libwpd synthesises one (`FF 57 50 43 10 00 00 00 01 16 01 00 00 00 00 00`). |

A figure box (§6.10) refers to an image by *image index #* (box offset 113). The index is 0-based in the order the images appear in this packet.

### 3.5 Packet 12: selected printer

| off | size | field |
|---|---|---|
| 0 | 37 | printer name |
| 37 | 13 | .PRS file name |
| 50 | 24 | initial font descriptor |
| 74..81 | 4 × short | minimum top, bottom, left and right margins (wpu) |
| 82 | | flags, then the .PRS date and time |

The name fields may be blank (OBS: PRINTER.TST).

### 3.6 Packet 3: document initial codes

The payload is a body-code stream (§5 rules). It typically holds `[L/R Mar]`, `[Tab Set]`, `[Font]`, `[Just]` and so on, set through Document Initial Codes. Apply it as the starting format state, and for docx write it into the default paragraph and run properties. Packet 3 did not occur in any of the files tested here.

### 3.7 Packets 0x100–0x1FF: styles (and the .STY library format)

| off | size | field |
|---|---|---|
| 0 | 21 | style name, NUL-padded WP string (see note) |
| 21 | byte | type. Bit 0: 1 = **paired**, 0 = **open** (global). Bits 6–7, what Enter does in a paired style: 0 = inserts [HRt], 2 = turns the style off, 3 = off then on again. |
| 22 | 55 | description |
| 77 | byte | unique style number. It matches the "unique style #" in body style codes. |
| 78 | short | BegSize |
| 80 | short | BegHash |
| 82 | short | EndSize |
| 84 | short | EndHash |
| 86 | short | XtrSize |
| 88 | BegSize | **on-codes**: a body-code stream (§5) |
| … | EndSize | **off-codes** (paired styles only) |
| … | XtrSize | extra codes appended to the off-codes |

**Outline styles**: the name begins with the 3 bytes `7F 'a' n`, where `n` is an ASCII digit giving the outline level (TK p.80). The text after them is the outline-style family name. OBS: LIBRARY.STY contains `\x7Fa8Document`, `\x7Fa2Document`, and so on.

Verified on LIBRARY.STY: all 22 styles decode cleanly, for example `Bibliogrphy` → on = `[→Indent][←Mar Rel]`, off = `{End Indent}[HRt][HRt]`.

---

## 4. Document area: byte classes

The body runs from the start-of-document offset to end of file. There is no terminator.

| byte | class | length |
|---|---|---|
| 0x00 | invalid. Must not occur; skip it. | 1 |
| 0x01–0x1F | control characters (§5.1) | 1 |
| 0x20–0x7E | ASCII text (WP character set 0) | 1 |
| 0x7F | invalid; skip | 1 |
| 0x80–0xBF | single-byte functions (§5.2) | 1 |
| 0xC0–0xCF | fixed-length functions (§5.3) | fixed: `C0`=4, `C1`=9, `C2`=11, `C3`=3, `C4`=3, `C5`=5, `C6`=6, `C7`=7, `C8`=4, `C9`=5, `CA`=6, `CB`=6, `CC`=8, `CD`=10, `CE`=10, `CF`=12 |
| 0xD0–0xFF | variable-length functions (§6) | 4 + `len` |

Key point for international text: **body bytes 0x80–0xFF are never characters.** Every non-ASCII character, including German umlauts, accented Italian, French and Spanish letters, and all Cyrillic, is stored as a 4-byte `C0 char set C0` code (OBS: `bet C0 1F 01 C0 tigen` decodes to "betätigen", character 1,31 = ä).

**Fixed-length function**: `code, payload[size−2], code`. The final byte repeats the code.

**Variable-length function**:

```
code(1) sub(1) len(2) payload[len−4] len(2) sub(1) code(1)        total = len + 4
```

`len` counts everything after the first length word, including the trailing `len sub code`. The smallest possible function is 8 bytes (TK says 9). For example, the D4/0E data marker has `len` = 4 and no payload.

**Parsing loop** (TK p.17–18, LWPD `WP5Parser`, `WP5Part`):

1. Read byte `c`.
2. If `c` is 0xC0–0xCF:
   * Let `size = FIXED[c]`. Check that `byte[pos + size − 1] == c`.
   * If the check passes, consume `size` bytes. Otherwise treat `c` as a stray byte, skip 1 byte and continue.
3. If `c` ≥ 0xD0:
   * Read `sub` and `len`. Check that the trailing `len` word, `sub` and `c` sit at `pos + len` − 4, −2 and −1.
   * If all three match, consume `len + 4` bytes. Otherwise skip 1 byte and continue. LWPD notes stray 0xE8 bytes in real files; OBS: CHAR.TST from the SDK contains raw 0xDA bytes typed as text.
4. Unknown function codes or subfunctions with consistent framing must be **skipped and preserved**: store them as an opaque "unknown" code so a later export can round-trip them. WP itself shows them as `[Unknown]` (TK p.18).
5. **Nesting.** Some variable-length functions carry a *text field*: headers, footers, footnotes, endnotes, comments, box captions and contents, overstrike, styles, hidden text. A text field is a complete body stream and can contain any code, including other functions (TK p.18). Parse it recursively with the same loop.
   * A bounded nesting depth is enough. WP itself nests at most a few levels, for example a footnote inside a text box inside a header.

---

## 5. Single-byte and fixed-length codes

### 5.1 Control characters 0x01–0x1F

| byte | meaning | Reveal Codes | import |
|---|---|---|---|
| 0x02 | page-number placeholder (^B), used in headers and footers | `^B` | page-number field |
| 0x03–0x07, 0x0E–0x16 | old-style (WP 4.2/5.0) merge codes ^C ^D ^E ^F ^G ^N ^O ^P ^Q ^R ^S ^T ^U ^V | `^C` … | keep as merge-code objects |
| 0x0A | **hard return** | `[HRt]` | paragraph end |
| 0x0B | **soft page break** | `[SPg]` | layout artefact: drop it. libwpd outputs a space here; in WP an SPg replaces the space at a line end, as SRt does. |
| 0x0C | **hard page break** | `[HPg]` | page break |
| 0x0D | **soft return** (word-wrap point) | `[SRt]` | layout artefact. **Replace with a single space**: WP turns the space at the wrap point into the SRt (LWPD; OBS: KERN.TST lines end in "by[SRt]choosing"). |
| 0x01, 0x08, 0x09, 0x17–0x1F | reserved | – | skip |

### 5.2 Single-byte functions 0x80–0xBF

Sources: TK p.21–22 and INC. In the "Reveal Codes" column, `—` means WordPerfect never displays the code; it is formatter bookkeeping.

| byte | meaning | Reveal Codes | import |
|---|---|---|---|
| 0x80 | temporary, always deleted | — | drop |
| 0x81 / 0x82 | right justification on / off (WP 5.0) | `[Just On]` / `[Just Off]` (5.0 names) | Justification Full / Left |
| 0x83 | **soft** end of center / align / flush right | `[C/A/Flrt]` | ends a C1 center/align/flush-right run |
| 0x84 | reserved (INC: "exited hyphenation") | — | drop |
| 0x85 | temporary place-saver | — | drop |
| 0x86 | center page top to bottom | `[Center Pg]` | page vertical centering |
| 0x87 | columns on | `[Col On]` | |
| 0x88 | columns off (at top of page) | `[Col Off]` | |
| 0x89 | formatter scan marker | — | drop |
| 0x8A / 0x8B | widow/orphan protection on / off | `[W/O On]` / `[W/O Off]` | |
| 0x8C | hard return that is also a soft page break (5.1) | `[HRt-SPg]` | = hard return |
| 0x8D | footnote/endnote number, inside a note's text | `[Note Num]` | note-reference field |
| 0x8E | box number, inside a caption | `[Box Num]` | caption-number field |
| 0x8F | **hard** end of center/align (5.1) | `[C/A/Flrt]` | as 0x83 |
| 0x90 | deletable soft return at end of line | `[DSRt]` | layout artefact: drop (no space) |
| 0x91 | deletable soft return at end of page | `[DSPg]`? | drop |
| 0x92 | end of page, deleted when formatted | — | drop |
| 0x93 | invisible soft return within a line (Home+Enter) | `[ISRt]` | optional line-break point, U+200B |
| 0x94 / 0x95 | 0x93 that fell at end of line / end of page | `[ISRt]` | as 0x93 |
| 0x96 / 0x97 | block on / off (saved selection) | `[Block]` / — | drop |
| 0x98 | table-of-contents page-number placeholder | — | drop |
| 0x99 | dormant hard return (5.1): an HRt suppressed at the top of a page | `[Dorm HRt]` | paragraph end |
| 0x9A | cancel hyphenation of the following word | `/` | drop, or a no-hyphenate run flag |
| 0x9B | end of generated text (ToC, index…) | `[End Def]` | generated-region end |
| 0x9C, 0x9D | reserved. In WP 4.2 these were bold off and on (TK p.101). | — | skip |
| 0x9E / 0x9F | hyphenation off / on | `[Hyph Off]` / `[Hyph On]` | |
| 0xA0 | **hard space** | `[ ]` | U+00A0 |
| 0xA1 | math: do subtotal | `+` | |
| 0xA2 | math: subtotal entry | `t` | |
| 0xA3 | math: do total | `=` | |
| 0xA4 | math: total entry | `T` | |
| 0xA5 | math: do grand total | `*` | |
| 0xA6 | math: calculation column | `!` | |
| 0xA7 / 0xA8 | math on / off | `[Math On]` / `[Math Off]` | |
| 0xA9 | **hard hyphen** (Home,-), in line | `-` | U+2011 non-breaking hyphen, or "-" |
| 0xAA / 0xAB | hard hyphen at end of line / end of page | `-` | as 0xA9 |
| 0xAC | **soft hyphen** (Ctrl+-), in line | `-` (highlighted) | U+00AD |
| 0xAD / 0xAE | soft hyphen at end of line / end of page; the line was broken here | `-` | U+00AD. **No space added.** |
| 0xAF / 0xB0 | columns off at end of line / end of page | `[Col Off]` | |
| 0xB1 | math negate | `N` | |
| 0xB2 | outline off (5.1) | `[Outline Off]` | |
| 0xB3 / 0xB4 | reverse direction on / off (Hebrew/Arabic WP only) | | |
| 0xB5–0xBC | reserved | | skip |
| 0xBD / 0xBE | auto code placement on / off (UNIX only) | | skip |
| 0xBF | reserved "unknown" | `[Unknown]` | skip |

Hyphens and spaces in plain text:

* ASCII `-` (0x2D) is shown in Reveal Codes as `[-]`, a *hyphen that may break*. Treat it as an ordinary U+002D.
* ASCII space 0x20 is a breaking space.

### 5.3 Fixed-length functions 0xC0–0xCF

Offsets are from the code byte. The last byte always repeats the code.

**0xC0 Extended character**, 4 bytes: `C0 char set C0`.

* +1 is the character number; +2 is the **character set (0–12)**. Note the order: the character comes first. INC describes the pair as one little-endian word whose high byte is the set (OBS: `C0 1F 01 C0` = set 1, char 31 = ä).
* Map it through `wp5_charsets.tsv`.
* For an unmapped pair, use U+E000 + set×256 + char (Private Use Area). Never drop it.
* Set 0 with char < 0x20 or > 0x7E should not occur; map it to PUA.
* Reveal Codes shows the character itself, or a solid block when the screen font lacks the glyph.

**0xC1 Tab / Center / Align / Flush Right / Margin Release**, 9 bytes:

| off | size | field |
|---|---|---|
| 1 | byte | flags (below) |
| 3 | short | absolute position of start of text (wpu) |
| 5 | short | absolute tab / center / align position (wpu) |
| 7 | short | screen column (su) |

Flags byte (TK p.23; INC confirms the bit meanings):

* bits 7–6 = kind: 0 = tab, 1 = align (decimal), 2 = left margin release, 3 = center.
* bit 5 = 1: center between margins (kind 3), or flush right to the right margin (kind 1).
* bit 4 = 1: dot leader.
* bit 3 = 1: right-aligned tab (kind 0) or centered tab (kind 3). bit 3 = 0: align on the alignment character (kind 1) or center on the current position (kind 3).
* bit 2 = 1: tab typed inside a table cell (Ctrl+Tab).
* bit 1 = 0: **hard** code; it keeps its type regardless of the tab set. bit 1 = 1: **soft** code; it adopts the type of the tab stop it lands on.

| flags pattern | Reveal Codes (soft / hard) |
|---|---|
| kind 0, bit3 = 0 | `[Tab]` / `[TAB]` |
| kind 0, bit3 = 1 | `[Rgt Tab]` / `[RGT TAB]` |
| kind 3, bit3 = 1 | `[Cntr Tab]` / `[CNTR TAB]` |
| kind 1, bit5 = 0 | `[Dec Tab]` / `[DEC TAB]` (5.0 called this `[Align]`) |
| kind 1, bit5 = 1 | `[Flsh Rgt]` (Alt+F6) |
| kind 3, bit5 = 1 or bit3 = 0 | `[Center]` (Shift+F6) |
| kind 2 | `[←Mar Rel]` (Shift+Tab) |
| + bit4 | dot-leader variant: `[Dot …]`. WP shows these as `[DEC TAB]` and so on with a dot leader; the exact display name is unverified. |

Center, Align and Flush Right are **paired**. The text that follows is centered or aligned until a `0x83` or `0x8F` code (`[C/A/Flrt]`), or until the end of the line. Tabs and margin release are open codes.

OBS: in KEYBRDS.WP `C1 02 …` is a soft left tab. In LIBRARY.STY `C1 40 …` is `[DEC TAB]`.

**0xC2 Indent**, 11 bytes:

| off | size | field |
|---|---|---|
| 1 | byte | flags. bit 0: 0 = left indent `[→Indent]`, 1 = left+right indent `[→Indent←]`. bit 4 = dot leader. |
| 2 | short | difference between old and new temporary margin |
| 4 | short | start of text (wpu) |
| 6 | short | **absolute indent position (wpu)** |
| 8 | short | screen column |

An indent lasts until the next hard return. WP stores a 0xC6 at the paragraph end to restore margins.

**0xC3 / 0xC4 Attribute on / off**, 3 bytes: `C3 attr C3` / `C4 attr C4`.

| attr | on / off display | | attr | on / off display |
|---|---|---|---|---|
| 0 | `[Ext Large]` / `[ext large]` | | 8 | `[Italc]` / `[italc]` |
| 1 | `[Vry Large]` / `[vry large]` | | 9 | `[Shadw]` / `[shadw]` |
| 2 | `[Large]` / `[large]` | | 10 | `[Redln]` / `[redln]` |
| 3 | `[Small]` / `[small]` | | 11 | `[Dbl Und]` / `[dbl und]` |
| 4 | `[Fine]` / `[fine]` | | 12 | `[Bold]` / `[bold]` |
| 5 | `[Suprscpt]` / `[suprscpt]` | | 13 | `[StkOut]` / `[stkout]` |
| 6 | `[Subscpt]` / `[subscpt]` | | 14 | `[Und]` / `[und]` |
| 7 | `[Outln]` / `[outln]` | | 15 | `[Sm Cap]` / `[sm cap]` |

* Pairs need not nest properly. OBS: `[Bold][Und]…[bold][und]`. Treat each attribute as an independent on/off toggle.
* The same 16-bit order (bit n = attribute n) is used by table cell and column attribute words.
* INC also defines 0x10 blink and 0x11 reverse video. Those are display-only and never appear in files.

**0xC5 Block Protect**, 5 bytes:

* +1 flags: bit 0 = 0 for `[Block Pro:On]`, 1 for `[Block Pro:Off]`. TK lists values 0, 1 and 3; bit 1 marks an Off code inside columns.
* +2 short: height of the block in WPU.

**0xC6 End of Indent**, 6 bytes: +1 the temporary right margin to restore, +3 the temporary left margin (wpu). Not shown in Reveal Codes; drop it.

**0xC7 Different display character when hyphenated**, 7 bytes: +1 flags, +2 the character in line (WP char word), +4 the character when hyphenated. Used by language modules for cases like Dutch "ck" → "k-k". Not shown in Reveal Codes. Import the in-line character.

**0xC8–0xCF**: reserved, with fixed sizes 4, 5, 6, 6, 8, 10, 10, 12. Skip them by size and preserve them as unknown.

---

## 6. Variable-length functions 0xD0–0xFF

Offsets are from the code byte. The payload starts at +4. "len" is the value in the length word, so the total size is len + 4. Text fields are nested body streams (§4, step 5).

Groups in 5.1: 0xD0–0xDF, 0xE1 (Intellitag only, WPWin) and 0xFE ("Unknown"). Any other code from 0xE0 to 0xFF with valid framing is an unknown function: skip and preserve it.

### 6.1 0xD0 Page Format group

| sub | name | layout (offsets from code byte) | Reveal Codes |
|---|---|---|---|
| 0 | Line Height | len 8. +4 old, **+6 new height (wpu); 0 = auto** | `[Ln Height:Auto]` / `[Ln Height:0.167"]` |
| 1 | Left/Right Margin | len 12. +4/+6 old, **+8 new left, +10 new right (wpu)**. Both are measured from their own paper edge. | `[L/R Mar:1",1"]` |
| 2 | Line Spacing | len 8. +4 old, **+6 new**: high byte = whole lines, low byte = 1/256 fraction (0x0180 = 1.5) | `[Ln Spacing:1.5]` |
| 3 | Hyphenation Zone | len 8. +4/+5 old, **+6 left, +7 right**, as fractions /256 of the line (64 = 25 %) | `[HZone:10%,4%]` |
| 4 | **Tab Set** | len 208 (5.0: 204). +4 old positions 40×short, +84 old types 20 bytes, **+104 new positions 40×short (0xFFFF = unused; list ends at the first 0xFFFF), +184 new types** (one nibble per stop, **high nibble = even stop**), +204 old relative margin (5.1), **+206 new relative-tab left margin (0xFFFF = absolute tabs)** | `[Tab Set:…]` |
| 5 | Top/Bottom Margin | len 12. **+8 new top, +10 new bottom (wpu)** | `[T/B Mar:1",1"]` |
| 6 | Justification (5.1) | len 6. +4 old, **+5 new**: 0 left, 1 full, 2 center, 3 right, 4 decimal (tables only) | `[Just:Left]` / `[Just:Full]` / `[Just:Center]` / `[Just:Right]` |
| 7 | Suppress | len 6. **+5 new** bits: 1 page numbering, 2 page number at bottom center, 4 header A, 8 header B, 0x10 footer A, 0x20 footer B, 0x80 code is not at top of page | `[Suppress:PgNum,HA,…]` |
| 8 | Page Number Position | len 10. **+7 new position**: 0 none, 1 TL, 2 TC, 3 TR, 4 alternating top, 5 BL, 6 BC, 7 BR, 8 alternating bottom. +8 font height. | `[Pg Numbering:Bottom Center]` |
| 11 | **Form (paper size/type)** | len 247 (5.0: 197). +99 **new desired length**, +101 **new desired width** (wpu), +103 type, +104 name length, +105 name (41 bytes). +146 new effective length, +148 effective width, +150 type, +151 name length, +152 name (41 bytes). **+193 effective orientation (0 portrait, 1 landscape)**. +194 matched form #. 5.1 adds labels at +197…+245: margins, label flags at +222, rows and columns, offsets, sizes, gaps. | `[Paper Sz/Typ:8.5" x 11",Standard]` |

Tab Set details:

* Type nibble: bits 0–1 give 0 left, 1 center, 2 right, 3 decimal. Bit 2 adds a dot leader.
* Positions are absolute from the left paper edge. When +206 ≠ 0xFFFF the tabs are **relative**: subtract that value to get positions relative to the left margin. That is how 5.1 displays them (`Rel; -1",-0.5",…`).
* OBS (KEYBRDS.WP): new positions 0, 600, 1488, 1848 with relative margin 1200 → `-1",-0.5",0.24",0.54"`.
* libwpd reads the type nibble with the masks 0x30 and 0x03. That is equivalent, because both top bits are 0 when there is no leader.

### 6.2 0xD1 Font group

| sub | name | layout | Reveal Codes |
|---|---|---|---|
| 0 | Print color | len 10. +4 old RGB (3 bytes), **+7 new R, G, B** | `[Color:…]` (WP shows a color name, or the RGB values for a custom color) |
| 1 | **Font change** | len 35 (5.0: 33). +4 old font #, +5 desired font descriptor (24 bytes), **+29 matched font # (index into packet 15/2)**, +30 matched-font hash, **+32 point size (3600ths; 5.1 only)**, +34 typeface flags | `[Font:Courier 10cpi]` |
| 2 | Color (DrawPerfect palette) | len 6 | — |
| 3 | Fill pattern (DrawPerfect) | len 10 | — |

* Font name: take packet 15 record [matched #], read its +18 pointer, and look the name up in packet 7.
* Point size: use +32 when len ≥ 35. Otherwise use the record's descriptor point size.
* If the hash does not match, the file was Fast-Saved and the font may be wrong (TK p.36, p.103).

### 6.3 0xD2 Definition group

| sub | name | layout (key fields) | Reveal Codes |
|---|---|---|---|
| 0 | Math definition | len 212. +108 new definition (24 bytes, one per column A–X), +132 four calc formulas of 20 bytes each (RPN, nibble-coded constants, TK p.38–39) | `[Math Def]` |
| 1 | Column definition | len 198. +4 old count, +5 old margins (48 shorts). **+101 flags: bits 0–4 = number of columns (2–24), bit 6 = parallel, bit 7 = parallel with block protect. +102 left/right margin pairs (2 × count shorts, absolute wpu).** | `[Col Def:Newspaper;2;1",4";4.5",7.5"]` |
| 2 | Paragraph-number definition | len 140 (5.0: 84). +44 new 8-level formats (3 bytes each: punct, style `I` `i` `A` `a` `1`, punct; a first byte of 0 means the 2nd and 3rd bytes are bullets). +68 new starting numbers (8 shorts). 5.1: +104 attach flags, +105 outline flags, +106 outline style name (18 bytes). | `[Par Num Def]` |
| 3 | Footnote options | len 160. +82 spacing in notes, +84 spacing between notes, +86 amount to keep together. +88 flags: bits 0–1 numbers / characters / letters, bit 2 restart each page, bit 3 "continued" message, bit 4 at bottom of page, bits 5–6 separator line. +89 count of numbering characters, +90 the characters (5 words). +100 / +120 format strings for the number in text and in note (20 bytes each). | `[Ftn Opt]` |
| 4 | Endnote options | len 160, layout like sub 3 | `[End Opt]` |
| 5–9 | Box options: Figure, Table, Text box, User box, Equation | len 128. +66 new values: flags (numbering styles, caption position), shading %, border styles word (4 bits each in the order L R T B, high to low; 0 none, 1 single, 2 double, 3 dashed, 4 dotted, 5 thick, 6 extra thick), min offset, outside spacing ×4, inside spacing ×4, caption-number string (20 bytes), … | `[Fig Opt]` `[Tbl Opt]` `[Txt Opt]` `[Usr Opt]` `[Equ Opt]` |
| 11 | **Table definition** (5.1) | variable len, see below | `[Tbl Def:I;3,2.17",2.17",2.17"]` |
| 13 / 14 | Spreadsheet link start / end (5.1) | filename and range with length bytes | `[Link:…]` / `[Link End]` |
| 15 / 16 | DDE link start / end (WPWin) | | |
| 17 | Border options | len 57: width and shade per line style, dash and dot spacing | — |

**Table definition (D2/0B).** The old and new blocks have the same shape. The new block starts at offset `4 + 24 + 5 × oldCols`, where oldCols is the short at +6.

| rel | size | field |
|---|---|---|
| +0 | byte | flags. bits 0–2 position: 0 left, 1 right, 2 center, 3 full, 4 absolute (set). bit 5 = negative numbers in parentheses. |
| +1 | byte | shading % |
| +2 | short | **number of columns** (≤ 32) |
| +4 | short | table number |
| +6 | short | left edge relative to margin |
| +8, +10 | short | left and right gutter |
| +12, +14 | short | top and bottom gutter |
| +16 | short | header row count |
| +18, +20 | | formatter fields |
| +22 | short | absolute offset from paper edge |
| +24 | short × cols | column widths (wpu) |
| … | short × cols | column attribute words (attribute bit order as §5.3) |
| … | byte × cols | column alignment: bits 0–2 = 0 left, 1 right ("flush right"), 2 center, 3 full, 4 decimal. Bits 4–7 = digits after the decimal point. |
| | 2 | reserved |

> **Conflict.** TK p.44 says alignment 1 = flush right and 3 = justified for column defaults (D2/0B), but for **cell** justification (DC/00 +14) it says 1 = justified and 3 = flush right. libwpd uses the same reading for both. Verify against a real table with right-aligned columns before relying on either.

OBS (PRINTER.TST): `[Tbl Def:Center;4,1.693",1.844",1.245",1.417"]`.

### 6.4 0xD3 Set group

| sub | name | layout | Reveal Codes |
|---|---|---|---|
| 0 | Decimal/alignment and thousands characters | len 12. **+8 new align char, +10 new thousands separator** (WP char words) | `[Decml/Algn Char:.,,]` |
| 1 | Underline mode | len 6. **+5**: bit 0 underline spaces, bit 1 underline tabs | `[Undrln:Spaces,Tabs]` |
| 2 | New footnote number | len 8. **+6** | `[New Ftn Num:n]` |
| 3 | New endnote number | len 8. **+6** | `[New End Num:n]` |
| 4 | New page number | len 10 (5.0: 8). **+6 number** (bit 15 = Roman in 5.0), **+9 type** (5.1): 0 arabic, 1 roman | `[Pg Num:n]` |
| 5 | Line numbering | len 14. **+9 flags**: bits 0–4 interval, bit 5 restart each page, bit 6 count blank lines, bit 7 **on**. +10 position, +12 starting number. | `[Ln Num:On]` / `[Ln Num:Off]` |
| 6 | Advance | len 9. +4 flags: bit 0 absolute, bit 1 horizontal. **+7 new position (s16 wpu; relative values are signed: + = down/right)**. | `[AdvUp:]` `[AdvDn:]` `[AdvLft:]` `[AdvRgt:]` (relative); `[AdvToLn:]` (absolute vertical); `[AdvToPos:]` (absolute horizontal) |
| 7 | Force odd/even page | len 7. **+6**: 0 even, 1 odd | `[Force:Odd]` / `[Force:Even]` |
| 8 | Baseline placement for typesetters | len 6. +5 new | `[Bline:On]`? (unverified) |
| 10 | Word/letter spacing | len 12. **+8 % character width, +10 % space width**; 0 = normal | `[Wrd/Ltr Spacing:…]` |
| 11 | Justification limits | len 12. **+8 min %, +10 max %** (0 = unlimited) | `[Just Lim:60,400]` |
| 12–16 | New box number: figure, table, text, user, equation | len 8. **+6**: bits 5–15 = level 1, bits 0–4 = level 2 | `[New Fig Num:n]` `[New Tbl Num:n]` `[New Txt Num:n]` `[New Usr Num:n]` `[New Equ Num:n]` |
| 17 | **Language** | len 8. **+6 two ASCII letters**: US UK OZ CF FR DE SD IT ES PO BR NL DK NO SV SU IS CA GA RU YK CZ SL HR MA GR | `[Lang:DE]` → docx `w:lang` |
| 18 | Page number style | len 64. **+34 new style text** (30 bytes WP string; ^B = the number) | `[Pg Num Style:…]` |
| 19 | Text direction (Hebrew/Arabic WP) | len 6 | |

OBS: KERN.TST manual kerning is `D3/06` with flags 2 (relative, horizontal) and new position −4 wpu, which is `[AdvLft:0.003"]`.

### 6.5 0xD4 Format group (formatter bookkeeping)

Nothing in this group is shown in Reveal Codes. **Drop all of it on import except sub 4.** WP regenerates it on load.

| sub | name | notes |
|---|---|---|
| 0 | End of page | Precedes [HPg]/[SPg]. Holds page number, footnote state and column groups. |
| 1 | Beginning of line | Line metrics and full-justify spacing. The most common code in 5.1 files (OBS: 727 in PPDEF.DOC). |
| 2 | Graphics box information | Margins temporarily changed by boxes. |
| 3 | Repositioning marker | |
| 4 | **Non-editable displayable text** (5.1) | +4 hash, +6 text. Holds hidden overflow text in non-wrapping table cells. Keep the text if feasible. |
| 5 | Justification information | Start position for center, right or decimal alignment. |
| 6–8 | Undo markers (WPWin) | |
| 9 | Table row display info (WPWin) | |
| 14 | Intellitag data marker | |

### 6.6 0xD5 Header/Footer group

Subfunction: 0 header A, 1 header B, 2 footer A, 3 footer B.

| off | size | field |
|---|---|---|
| 4 | byte | old occurrence |
| 5..10 | | old formatter fields |
| 11 | byte | **new occurrence**: 0 discontinue, 1 every page, 2 odd pages, 4 even pages |
| 12 | short | formatter lines |
| 14, 16 | | unused |
| 18 | short | number of boxes inside |
| 20 | short | hash |
| 22 | … | **text** (nested stream) |

If occurrence = 0, offsets 18 onward may be absent; check `len`. LWPD computes the text length as `len + 4 − 26`. Reveal Codes: `[Header A:Every page;text…]`, `[Footer B:Discontinue]`.

OBS: PRINTER.TST footer A contains `[Center][Fine]…[C/A/Flrt][SRt][Date:3 1, 4  8:9 0].[fine]`.

### 6.7 0xD6 Footnote/Endnote group

* **Sub 0, Footnote.**
  * +4 flags: bit 7 = the note uses a character instead of a number; bits 0–3 = character count.
  * +5 number or character.
  * +7 `n` = additional pages.
  * +8 a work area of 2 × (n + 1) bytes. Then: short lines on page, short footnote lines, byte footnote pages, **short box count**, short hash.
  * **Text starts at +8 + 2(n+1) + 9.**
* **Sub 1, Endnote.** +4 flags, +5 number, +7 box count, +9 hash, **text at +11.**

The text usually begins with `[Note Num]` (0x8D), the number inside the note.

Reveal Codes: `[Footnote:1;[Note Num]text]` and `[Endnote:1;…]`. In the body, the reference mark goes where the D6 code sits.

### 6.8 0xD7 Generate group (lists, index, ToC, cross-references, subdocuments)

| sub | name | layout | Reveal Codes |
|---|---|---|---|
| 0 / 1 | Begin / end marked text | len 5. +4 flags: low nibble = level or list 0–4, high nibble 0 ToC, 2 list | `[Mark:ToC,1]` … `[End Mark:ToC,1]`; `[Mark:List,n]` |
| 2 | Define marked text | +4 flags: high nibble 0 ToC, 1 index, 2 list, 3 ToA. +5 five definition bytes (page-number style 0–4). +10 concordance filename (NUL-terminated). | `[Def Mark:ToC,n]` `[Def Mark:Index]` `[Def Mark:List,n]` `[Def Mark:ToA,n]` |
| 3 | Index entry | +4 heading, NUL, subheading (not terminated) | `[Index:heading;subheading]` |
| 4 | Table-of-authorities entry | +4 section (0x20 = short form only), +5 short form, NUL, full form | `[ToA:n;short;Full Form]` |
| 5 | Endnote placement | len 19 | `[Endnote Placement]` |
| 6 | Save page info | internal | — |
| 7 | Cross-reference (reference) | +4 type: 0 page, 1 paragraph, 2 footnote, 3 endnote, 4 figure, 5 table, 6 text box, 7 user box, 8 equation. +5 target name, NUL, current text. | `[Ref(name):Pg 3]` |
| 8 | Cross-reference target | +4 target name, NUL | `[Target(name)]` |
| 9 / 10 / 11 | Subdocument / subdocument start / subdocument end | +4 screen column, +6 lines, +7 filename (NUL-terminated) | `[Subdoc:file]` `[Subdoc Start:file]` `[Subdoc End:file]` |
| 12 | VAX path | | |

### 6.9 0xD8 Display group

| sub | name | layout | Reveal Codes |
|---|---|---|---|
| 0 | **Date/time code** | +4 format string. Digits stand for fields: `1` day, `2` month number, `3` month name, `4` 4-digit year, `5` 2-digit year, `6` weekday, `7` hour (24 h), `8` hour (12 h), `9` minute, `0` am/pm. Prefix `%` = zero-pad, `$` = abbreviate. | `[Date:3 1, 4]` → docx DATE field |
| 1 | Paragraph/outline number | +4 level byte: bits 0–6 level, **bit 7 = fixed level** (otherwise automatic). +5 eight level counters. **+21 display text** (WP string, as last generated). | `[Par Num:Auto]` / `[Par Num:2]` |
| 2 | **Overstrike** | +4 maximum width (su), **+6 characters** (WP chars and attributes) | `[Ovrstk:e´]`. Import as one composed character: NFC of the base plus combining marks (see the charsets TSV note). |
| 3 | Insert page number (5.1) | +4 style string | `[Insert Pg Num:…]` |

### 6.10 0xD9 Miscellaneous group

| sub | name | layout | Reveal Codes |
|---|---|---|---|
| 0 | Printer command | +4: 0 = command, 1 = filename. +5 string. Commands use `<n>` for control codes. | `[Ptr Cmnd:…]` |
| 1 | Conditional end of page | len 5. +4 number of lines to keep together | `[Cndl EOP:n]` |
| 2 | **Comment** | +4 screen column, +6 lines displayed, **+7 text** (nested) | `[Comment]` → docx comment or hidden text |
| 3 | Kerning | len 6. **+5**: 0 off, 1 on | `[Kern:On]` / `[Kern:Off]` |
| 4 | Outline on (5.1) | len 20. Eight saved level numbers. | `[Outline On]` |
| 5 | Leading adjustment (5.1) | len 12. **+8 new [SRt] leading, +10 new [HRt] leading** (s16 wpu) | `[Leading Adj:…]` |
| 6 | Kerning value (graphics text) | len 6 | — |
| 7 | Hidden data for converters | +4 product, type, major, minor; +8 opaque data | — (preserve) |
| 8 | Macro (WPWin) | | — |

### 6.11 0xDA Box group (graphics boxes, lines)

Subfunction: 0 Figure, 1 Table box, 2 Text box, 3 User box, 4 Equation (5.1), 5 Horizontal line, 6 Vertical line.

**Subs 0–4: box.** len = 117 + caption length + contents length.

| off | size | field |
|---|---|---|
| 4 | short | box number: bits 5–15 = level 1, bits 0–4 = level 2 |
| 6 | byte | anchor/position. Bits 0–1: 0 paragraph, 1 page, 2 character. Bits 2–4 (page boxes): 0 full, 1 top, 2 middle, 3 bottom, 4 absolute. Bit 5 bumped. Bit 6 page offset. Bit 7 equation printed as text. |
| 7 | byte | alignment. Bits 0–1: 0 left, 1 right, 2 center, 3 full. Bits 2–3: relative to 0 margin, 1 columns, 2 absolute. Bit 4 fixed height, bit 5 fixed width, bit 7 = **no text wrap**. |
| 8, 10 | short | width, height (wpu, calculated) |
| 12, 14 | short | x, y (calculated) |
| 16..30 | 8 × short | outside spacing L R T B, inside spacing L R T B |
| 32, 34 | s16 | horizontal and vertical offset (user-entered position) |
| 36, 37 | byte | column X, Y |
| 38, 40 | short | source image width and height. For text boxes, 40 = text height. |
| 42 | short | rotation: bits 0–11 degrees, bit 14 invert, bit 15 mirror |
| 44, 46 | short | scale x, y (100 = 100 %) |
| 48, 50 | short | crop x, y. For text boxes: 48 = hash, 50 = rotation 0–3 (×90°). |
| **52** | byte | **contents type**: 0 empty, 2 graphic file on disk, 8 equation (WP text), 0x10 text (WP), 0x11 text (ASCII), 0x40 table, 0x41–0x43 spreadsheet, **0x80 WPG stored in packet 8**, 0x81–0x93 foreign graphic formats (PIC, TIFF, PCX, MSP, CGM, DXF, MacPaint, HPGL, Halo, PC Paint, IMG, EPS, PS, …) |
| 53 | 21 | **source filename** (NUL-terminated) |
| 74 | 12 | reserved (5.0: 33 bytes up to 107) |
| 86 | short | OLE flag (WPWin 5.2) |
| 102 | byte | equation justification: 0 center, 1 left, 2 right |
| 103 | short | absolute page |
| 105, 106 | byte | pages bumped, page offset |
| **107, 109** | short | **desired width and height as entered by the user** (prefer these for docx) |
| 111 | short | space between caption and box |
| **113** | short | **image index into packet 8** |
| 115, 117 | short | caption formatter lines, hash |
| **119** | short | **caption length (bytes)** |
| **121** | … | **caption text** (nested; usually begins `[Box Num]`), then **box contents**: text-box or equation text (nested stream), up to the trailer |

Equations (contents type 8) wrap their source in a **0xDF** function: +4 flags (bits 0–2 horizontal alignment, bits 3–5 vertical alignment), +6 base font size, +8 offset (0), +10 text length, **+12 equation source text** in WP's equation language (`INT_0^{INF} … OVER …`), then a compact compiled form.

OBS: PRINTER.TST has a figure `MOUSE.WPG` (type 0x80, image 0, 1803 bytes in packet 8), an equation box, and user boxes holding text.

**Subs 5–6: horizontal / vertical line.** len 121.

* +6 vertical position (bits 2–4). For vertical lines: 0 full page, 1 top, 2 middle, 3 bottom, 4 absolute. For horizontal lines: 0 baseline, 4 absolute.
* +7 horizontal alignment (bits 0–2). For horizontal lines: 0 left, 1 right, 2 center, 3 full, 4 absolute. For vertical lines: 0 left margin, 1 right margin, 2 between columns, 3 absolute.
* **+8 width, +10 height (thickness), +12 x, +14 y** (wpu).
* **+36 shading % (gray)**, +37 column.

Reveal Codes: `[HLine:…]` and `[VLine:…]`. OBS: `[HLine:6.5",0.021"]`.

The box number is calculated by the formatter. OBS: the first figure carries level 1 = 1 and level 2 = 1 even when level-2 numbering is unused, so renumber boxes on import.

Reveal Codes for boxes: `[Figure:1;MOUSE.WPG;]`, `[Tbl Box:n;…]`, `[Text Box:n;…]`, `[Usr Box:n;…]`, `[Equation:n;…]`.

### 6.12 0xDB Style group

| sub | name | layout | Reveal Codes |
|---|---|---|---|
| 0 | Paired style ON (begin) | +4..11 old fields, +12 new Enter-key type, **+13 unique style #**, +14 hash, **+16 style name** (NUL-terminated, max 21) | `[Style On:name]` |
| 1 | Paired style OFF | … **+19 unique #**, **+22 style name** | `[Style Off:name]` |
| 2 | Open style | **+4 unique #**, +5 hash, **+7 style name** | `[Open Style:name]` |
| 3 | End of style definition | len 5. +4 flag (1 = the formatter modified the text) | — |

The file contains the style's **expanded codes** in line, as a cached copy. Each Style On (or Open Style) code is followed by a copy of the style's on-codes and then a DB/03. For paired styles, the Style Off code is followed by the off-codes and another DB/03 (TK p.79).

* On import, **skip everything from the style code up to the matching DB/03**, and apply the style definition from prefix packets 0x100+, matched by name or unique #.
* For a lossy fallback, keep the expanded codes instead.

For outline styles the name begins with `7F 'a' level`. Display it as the style family name plus the level.

### 6.13 0xDC / 0xDD Table structure codes (5.1)

Tables are linear: `[Tbl Def]` is followed by `[Row][Cell]text[Cell]text…[Row]…[Tbl Off]`.

| code | name | Reveal Codes |
|---|---|---|
| DC/0 | Cell begins (the code at the end of a line) | `[Cell]` |
| DC/1 | Row begins | `[Row]` |
| DC/2 | Table off | `[Tbl Off]` |
| DD/1 | Row begins at a soft page break | `[Row]` + [SPg] |
| DD/2 | Table off at a page break | `[Tbl Off]` |
| DD/3 | Row begins at a hard page break | `[Row]` + [HPg] |

**DC/0 Cell** (+offsets from the code byte):

| off | size | field |
|---|---|---|
| 4 | byte | flags. Bit 0 = use the cell's own justification. Bit 1 = use the cell's own attributes. Bits 2–3 vertical alignment: 0 top, 1 bottom, 2 center. Bit 4 = text cell. Bit 5 = has formula. Bit 6 = locked. |
| 5 | byte | column number (0-based) |
| 6 | byte | bits 0–5 = **column span**. Bit 7 = **this cell is covered by a cell above** (continuation of a row span). |
| 7 | byte | **row span** |
| 8, 10 | short | formatter fields |
| 12 | short | cell attributes (bit order as §5.3) |
| 14 | byte | cell justification. Bits 0–2: 0 left, 1 full, 2 center, 3 right, 4 decimal (see the conflict note in §6.3). |
| 15 | … | optional subgroups: byte id (1 = formula, in math RPN with cell references `A6 s16 rowDelta s16 colDelta`) followed by the data |

**DC/1 Row** (also DD/1 and DD/3):

| off | size | field |
|---|---|---|
| 4 | byte | old row-height flags |
| 5 | short | old height |
| 7 | byte | old border byte count `k`. Bit 7 = 1: one word per entry. Bit 7 = 0: one byte per entry (pre-release 5.1 format). |
| 8 | k bytes | old border data |

The new values follow in the same shape: flags (bit 0 = multi-line, bit 1 = auto height), height (wpu), byte count, border data. Then comes a short holding the formatter top-of-row value.

Border words, one per cell:

* bits 0–2 top, bits 3–5 left, bits 8–10 bottom, bits 11–13 right. Style values: 0 none, 1 single, 2 double, 3 dashed, 4 dotted, 5 thick, 6 extra thick.
* Bit 14 = shaded.
* Bit 15 = **run-length count word**: bits 0–14 give the number of columns that use the *next* word.

**DC/2 Table off** has the same shape as the old part of a row, plus row count, header size and column count.

OBS: PRINTER.TST contains 2 tables, 4 rows and 16 cells, all with consistent framing.

### 6.14 0xDE Merge commands (5.1)

`DE sub len=4 … 04 00 sub DE`. Most commands are framing only. The command's arguments follow **in the body text** as ordinary characters, each argument ending with `~`. Sub 0x34 {END RECORD} has len 6 and carries a field-count short.

| sub | command | sub | command | sub | command |
|---|---|---|---|---|---|
| 0x20 | ASSIGN | 0x36 | FIELD | 0x4C | ON ERROR |
| 0x21 | BELL | 0x37 | FOR | 0x4D | PAGE OFF |
| 0x22 | BREAK | 0x38 | FOR EACH | 0x4E | PAGE ON |
| 0x23 | CALL | 0x39 | GO | 0x4F | PRINT |
| 0x24 | CANCEL OFF | 0x3A | IF | 0x50 | PROMPT |
| 0x25 | CANCEL ON | 0x3B | IF BLANK | 0x51 | QUIT |
| 0x26 | CASE | 0x3C | IF EXISTS | 0x52 | RETURN |
| 0x27 | CASE CALL | 0x3D | IF NOT BLANK | 0x53 | RETURN CANCEL |
| 0x28 | CHAIN MACRO | 0x3E | KEYBOARD | 0x54 | RETURN ERROR |
| 0x29 | CHAIN PRIMARY | 0x3F | LABEL | 0x55 | REWRITE |
| 0x2A | CHAIN SECONDARY | 0x40 | LOCAL | 0x56 | STEP OFF |
| 0x2B | CHAR | 0x41 | LOOK | 0x57 | STEP ON |
| 0x2C | COMMENT | 0x42 | MID | 0x58 | SUBST PRIMARY |
| 0x2D | CTON | 0x43 | MRG CMND | 0x59 | SUBST SECONDARY |
| 0x2E | DATE | 0x44 | NEST MACRO | 0x5A | SYSTEM |
| 0x2F | DOCUMENT | 0x45 | NEST PRIMARY | 0x5B | TEXT |
| 0x30 | ELSE | 0x46 | NEST SECONDARY | 0x5C | VARIABLE |
| 0x31 | END FIELD | 0x47 | NEXT | 0x5D | WAIT |
| 0x32 | END FOR | 0x48 | NEXT RECORD | 0x5E | WHILE |
| 0x33 | END IF | 0x49 | NTOC | 0x5F | STATUS PROMPT |
| 0x34 | END RECORD | 0x4A | PROCESS | 0x60 | INPUT |
| 0x35 | END WHILE | 0x4B | ON CANCEL | 0x61 | LEN |
| | | | | 0x62 | FIELD NAMES |

Sub 0x63 marks the end of the merge-command range. 0x4F has no name in TK ("Old ^T"); PRINT is our reading.

The editing screen shows these as `{FIELD}name~`; Reveal Codes shows `[Mrg:FIELD]name~`.

### 6.15 Other groups

* **0xDF** Equation nested function: see §6.11.
* **0xE1** Tag group (Intellitag SGML, WPWin only). Subs 0–6: start tag, end tag, conversion rule, ignore on/off, entity on/off. Skip.
* **0xFE / sub 0xFE** "Unknown": opaque data preserved by WP. Reveal Codes `[Unknown]`.

---

## 7. Group/subgroup → Reveal Codes display text

Conventions in this table:

* Measurements are shown as WP 5.1 shows them: inches with up to 3 decimals and a `"` sign.
* "—" means the code is never displayed.
* **Confidence**: **H** = displayed name confirmed by TK wording or widely documented 5.1 behaviour. **M** = standard 5.1 display text from experience, not checked against WP's binaries.

| code | Reveal Codes text | conf. |
|---|---|---|
| 0x0A / 0x0D / 0x0B / 0x0C / 0x8C | `[HRt]` `[SRt]` `[SPg]` `[HPg]` `[HRt-SPg]` | H |
| 0x02 | `^B` | H |
| 0x86 / 0x87 / 0x88, 0xAF, 0xB0 | `[Center Pg]` `[Col On]` `[Col Off]` | H |
| 0x8A / 0x8B | `[W/O On]` / `[W/O Off]` | H |
| 0x8D / 0x8E | `[Note Num]` / `[Box Num]` | H |
| 0x83 / 0x8F | `[C/A/Flrt]` | H |
| 0x90 / 0x93 | `[DSRt]` / `[ISRt]` | M |
| 0x99 | `[Dorm HRt]` | M |
| 0x9A | `/` (cancel hyphenation) | M |
| 0x9B | `[End Def]` | H |
| 0x9E / 0x9F | `[Hyph Off]` / `[Hyph On]` | H |
| 0xA0 | `[ ]` | H |
| 0xA7 / 0xA8 | `[Math On]` / `[Math Off]` | H |
| 0xA1–0xA6, 0xB1 | `+` `t` `=` `T` `*` `!` `N` | H |
| 0xA9–0xAB | `-` (hyphen character) | H |
| 0xAC–0xAE | `-` (soft hyphen, highlighted) | H |
| 0x2D (ASCII) | `[-]` | H |
| 0xB2 / D9-4 | `[Outline Off]` / `[Outline On]` | M |
| C0 | the character itself | H |
| C1 | `[Tab]` `[TAB]` `[Rgt Tab]` `[RGT TAB]` `[Cntr Tab]` `[CNTR TAB]` `[Dec Tab]` `[DEC TAB]` `[Center]` `[Flsh Rgt]` `[←Mar Rel]` | M |
| C2 | `[→Indent]` / `[→Indent←]` | H |
| C3 / C4 | see §5.3 table (`[Bold]` / `[bold]`, …) | H |
| C5 | `[Block Pro:On]` / `[Block Pro:Off]` | H |
| C6, C7 | — | H |
| D0-0 | `[Ln Height:Auto]` / `[Ln Height:x"]` | H |
| D0-1 | `[L/R Mar:x",y"]` | H |
| D0-2 | `[Ln Spacing:n]` | H |
| D0-3 | `[HZone:l%,r%]` | H |
| D0-4 | `[Tab Set:Rel; …]` / `[Tab Set:Abs; …]` (5.1 lists the stops) | M |
| D0-5 | `[T/B Mar:x",y"]` | H |
| D0-6 | `[Just:Left]` `[Just:Full]` `[Just:Center]` `[Just:Right]` | H |
| D0-7 | `[Suppress:PgNum,HA,HB,FA,FB]` | M |
| D0-8 | `[Pg Numbering:Bottom Center]` | M |
| D0-11 | `[Paper Sz/Typ:8.5" x 11",Standard]` | H |
| D1-0 / D1-1 | `[Color:…]` / `[Font:name]` | H |
| D2-0 / 1 / 2 | `[Math Def]` / `[Col Def:…]` / `[Par Num Def]` | H |
| D2-3 / 4 | `[Ftn Opt]` / `[End Opt]` | H |
| D2-5…9 | `[Fig Opt]` `[Tbl Opt]` `[Txt Opt]` `[Usr Opt]` `[Equ Opt]` | H |
| D2-11 | `[Tbl Def:…]` | H |
| D2-13 / 14 | `[Link:…]` / `[Link End]` | M |
| D3-0 | `[Decml/Algn Char:…]` | M |
| D3-1 | `[Undrln:Spaces,Tabs]` | M |
| D3-2 / 3 / 4 | `[New Ftn Num:n]` / `[New End Num:n]` / `[Pg Num:n]` | H |
| D3-5 | `[Ln Num:On]` / `[Ln Num:Off]` | H |
| D3-6 | `[AdvUp:]` `[AdvDn:]` `[AdvLft:]` `[AdvRgt:]` `[AdvToLn:]` `[AdvToPos:]` | M |
| D3-7 | `[Force:Odd]` / `[Force:Even]` | H |
| D3-8 | `[Bline:On]` / `[Bline:Off]` | M (unverified) |
| D3-10 / 11 | `[Wrd/Ltr Spacing:…]` / `[Just Lim:…]` | M |
| D3-12…16 | `[New Fig Num]` `[New Tbl Num]` `[New Txt Num]` `[New Usr Num]` `[New Equ Num]` | H |
| D3-17 | `[Lang:XX]` | H |
| D3-18 | `[Pg Num Style:…]` | M |
| D4-* | — | H |
| D5-0…3 | `[Header A:Every page;…]` `[Header B:…]` `[Footer A:…]` `[Footer B:…]` | H |
| D6-0 / 1 | `[Footnote:n;…]` / `[Endnote:n;…]` | H |
| D7-0 / 1 | `[Mark:ToC,n]` / `[End Mark:ToC,n]` (also List) | H |
| D7-2 | `[Def Mark:ToC,n]` `[Def Mark:Index]` `[Def Mark:List,n]` `[Def Mark:ToA,n]` | H |
| D7-3 / 4 | `[Index:…]` / `[ToA:…]` | H |
| D7-5 | `[Endnote Placement]` | H |
| D7-7 / 8 | `[Ref(name):Pg n]` / `[Target(name)]` | H |
| D7-9 / 10 / 11 | `[Subdoc:…]` / `[Subdoc Start:…]` / `[Subdoc End:…]` | H |
| D8-0 / 1 / 2 / 3 | `[Date:fmt]` / `[Par Num:Auto]` / `[Ovrstk:…]` / `[Insert Pg Num:…]` | H |
| D9-0 / 1 / 2 / 3 / 5 | `[Ptr Cmnd:…]` / `[Cndl EOP:n]` / `[Comment]` / `[Kern:On]` / `[Leading Adj:…]` | H/M |
| DA-0…4 | `[Figure:n;file;]` `[Tbl Box:…]` `[Text Box:…]` `[Usr Box:…]` `[Equation:…]` | H |
| DA-5 / 6 | `[HLine:…]` / `[VLine:…]` | H |
| DB-0 / 1 / 2 | `[Style On:name]` / `[Style Off:name]` / `[Open Style:name]` | H |
| DC-0 / 1 / 2, DD-* | `[Cell]` / `[Row]` / `[Tbl Off]` | H |
| DE-xx | `[Mrg:COMMAND]` | H |
| FE, unknown | `[Unknown]` | H |

---

## 8. Import notes for the WP-style internal model

1. **What to drop.** The model only needs what Reveal Codes would show. Drop:
   * SRt (keep it as a space), SPg, DSRt, DSPg, 0x92;
   * every D4 code except sub 4;
   * C6, block markers, 0x80, 0x85, 0x89, 0x98;
   * style expansions between a DB code and its DB/03.
   * WP 5.1 regenerates all of these on retrieve.
2. **Fast Save.** End-of-line and end-of-page variants (0xAA vs 0xA9, 0xAD vs 0xAC, 0x94 vs 0x93, 0xAF vs 0x88, D4/0) may be stale. Treat each family as one code (TK p.22, p.56).
3. **Paired codes** (attributes, center/align, styles) may be unbalanced in real documents. A missing off-code means the attribute ends at the end of the text field or paragraph, as WP does.
4. **Nested streams** inherit nothing from the surrounding format state, except that headers and footers follow the document initial codes.
5. **Characters.**
   * `C0` → `wp5_charsets.tsv`; unmapped → PUA U+E000 + set·256 + char.
   * Set 1 diacritics 1,0–1,22 and set 2 marks are combining in Unicode. Use the *spacing* form given in the TSV note when they stand alone. Use the combining form inside `[Ovrstk]` and NFC-compose the result.
   * Cyrillic stressed vowels 10,110–147 decompose to base + U+0301 or U+0300.
6. **Units.** WPU values convert to docx twips as twips = wpu × 1440 / 1200 = wpu × 1.2.

---

## 9. DOS code pages and keyboard input

WP 5.1 never stores code-page bytes in a document. When the user types a DOS character, through Alt+number or a national keyboard driver, WP converts it to a WP character and stores it as a `C0` code. CP437/CP850 bytes do occur in:

* WP 4.2 documents;
* DOS text files retrieved with Ctrl+F5 (WP converts them to WP characters on retrieve, and back on save);
* `.WPK` keyboard files.

`wp5_dos_codepages.tsv` lists, for every byte 128–255 of CP437 and CP850, the WP character it maps to. Selected rows:

| CP437 | char | WP | | CP437 | char | WP |
|---|---|---|---|---|---|---|
| 0x80 | Ç | 1,38 | | 0xA0 | á | 1,27 |
| 0x81 | ü | 1,71 | | 0xA4 | ñ | 1,57 |
| 0x82 | é | 1,41 | | 0xA8 | ¿ | 4,8 |
| 0x84 | ä | 1,31 | | 0xAD | ¡ | 4,7 |
| 0x85 | à | 1,33 | | 0xAE / 0xAF | « » | 4,9 / 4,10 |
| 0x8A | è | 1,47 | | 0xB0–0xDF | box drawing | set 3 |
| 0x8E | Ä | 1,30 | | 0xE1 | ß | 1,23 |
| 0x94 | ö | 1,63 | | 0xE0–0xEE | Greek | set 8 |
| 0x99 / 0x9A | Ö / Ü | 1,62 / 1,70 | | 0xF8 | ° | 6,36 |
| 0x9C | £ | 4,11 | | 0xFF | NBSP | body 0xA0 |

How the table was built, and its status:

* Each byte was decoded with the code page's Unicode mapping and then looked up in the charsets TSV.
* When several WP characters share the same Unicode value, the order of preference is Multinational 1, Typographic, Box, Math, Greek.
* The resulting Latin-letter rows agree with the WP character names in SDK `CHARMAP.ASM`.
* The exact WP choice for a few symbols has not been verified against WP.EXE's internal table: 0xE6 µ, 0xF9 ∙, 0xFA ·, and 0xFE ■.
* CP850 0xDD (¦) has no WP 5.x equivalent.

---

## 10. Known gaps and uncertainties

* **Reveal Codes spelling** for rarely used codes is not verified against WP 5.1 itself; see the M rows in §7. A one-time check in a running WP 5.1 under DOSBox would settle it.
* **Table justification codes**: the 1 ↔ 3 conflict noted in §6.3.
* **Packet 2** (5.0 fonts): the 82-byte record size comes from one file only.
* **Charset tables** (see the TSV header):
  * Sets 1, 3, 6 and 7, and the basic Greek and Hebrew letters, agree with the WP SDK's own character names in `CHARMAP.ASM` (the roughly 270 named entries in its WP-character half were checked; the thorn-case conflict at 1,88/1,89 and 6 corrected set-7 entries are flagged in the TSV).
  * Sets 2, 5, 9 (points and accents), 11, and the upper ranges of 4, 6 and 7 come from libwpd. libwpd is LGPL code but its tables are factual data. Its typos that were evident from kana order, duplicates or wrong Unicode blocks have been corrected and are flagged in the TSV.
  * Cyrillic 10,0–65 (the Russian alphabet in upper/lower pairs, with Ё at 12/13) is reliable. Positions ≥ 66 follow libwpd.
  * PUA use: 176 positions have no known Unicode equivalent and map to the PUA. Most are set 7 bracket and operator *pieces* (large-delimiter building blocks used by the equation editor), plus 4,89–99, a few in set 6, and 2,12, 2,21 and 9,82. The other 255 PUA positions are the user-defined set 12.
  * Defined range per set (from SDK `CHAR.TST`, the WP 5.0 diskette; 5.1 extends some sets): 1: 0–233, 2: 0–27, 3: 0–87, 4: ≥0–75, 5: 0–34, 6: ≥0–218, 7: ≥0–222, 8: ≥0–198, 9: ≥0–43, 10: ≥0–101. The TSV also lists libwpd positions beyond these; that is harmless for import.
* **Encryption** has not been verified on a real encrypted 5.x file.
* **WPWin-only codes** (D4/6–9, D2/15–16, D9/8, 0xE1) are documented for skipping only.
