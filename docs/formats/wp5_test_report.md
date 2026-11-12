# WP 5.x parser test report

Tool: `docs/formats/tools/wp5_dump.py`. It is a reference walker for `wp5_document.md`: Python 3 standard library only, read-only. It reads the 16-byte header, follows the prefix index chain, decodes packets 1/2/3/7/8/12/15 and the style packets, then walks the body. Nested text fields (headers, notes, boxes, comments and so on) are decoded recursively. Output uses Reveal Codes style:

* `[...]` is a code that WordPerfect displays.
* `{...}` is formatter bookkeeping that WordPerfect hides.

```
python3 -I docs/formats/tools/wp5_dump.py FILE [--prefix] [--stats] [--hex] [--password PW]
```

## 1. Required test files (WP 5.1 install, `/mnt/i/Claude Space/wp51`)

| file | version | body | codes walked | unknown or inconsistent | notes |
|---|---|---|---|---|---|
| KEYBRDS.WP | 5.1 (0.1) | 6 967 B at 896 | 1 235 codes + 45 [HRt], 1 648 text bytes | **0** | 7 deleted prefix packets (Fast Save). 1 161 extended characters: set 3 box drawing ×1 132, set 1 (ä ö ü ß ê é …) ×24, set 4 ×3, set 6 ×2. All mapped. |
| KERN.TST | **5.0** (0.0) | 2 481 B at 741 | 67 functions | **0** | WP 5.0 layout of packet 2 (82-byte record); "reserved" packets 4 and 5 present. 25 `[AdvLft]` manual-kerning codes, `[Kern:On/Off]`. |
| PRINTER.TST | 5.1 (0.1) | 6 288 B at 2377 | 150 codes + 58 control characters | **0** | The richest sample: footer with a date code; 2 tables (`[Tbl Def][Row][Cell]…[Tbl Off]`); a footnote; figure MOUSE.WPG embedded in packet 8 (1 803 bytes); an equation box (DF group, source text containing Γ); user boxes holding text; parallel columns; horizontal lines; soft hyphens; cancel-hyphenation codes. Extended characters in sets 4, 5, 8 and 11 all map (ざ © ☜ Δ Γ). |

**All three files parse with zero unknown codes and zero framing errors.**

* Every fixed-length function ended with its code byte at the expected size.
* Every variable-length function had a matching trailing length, subfunction and code.
* The walk consumed each body exactly to end of file.

Code histograms from `--stats`:

```
KEYBRDS.WP : ctl0A 45 | C0 1161 C1 12 C3 21 C4 21 | A9 8 | D0/01 1 D0/04 1 D4/01 10
KERN.TST   : ctl0A 19 ctl0D 28 | C3 5 C4 5 | A9 28 | D1/01 1 D3/06 25 D9/03 3
PRINTER.TST: ctl0A 36 0C 1 0D 21 | C0 4 C1 8 C3 21 C4 21 | 83 7 87 1 8D 1 9A 3 9F 1 A9 2 AC 2 AD 2 AF 1 |
             D0/01 3 D0/02 2 D0/05 1 D2/01 1 D2/05 1 D2/08 1 D2/0B 2 D4/00 2 D4/01 3 D4/02 14 D4/05 13
             D5/02 1 D6/00 1 D8/00 1 DA/00 1 DA/03 2 DA/04 1 DA/05 3 DC/00 16 DC/01 4 DC/02 2 DF/00 1
```

## 2. Additional files (WP 5.1 SDK diskette and STYLE directory)

| file | version | result |
|---|---|---|
| PPDEF.DOC (70 KB) | 5.1 | 0 problems. 2 669 tabs, 727 D4/01 codes, a text box. |
| PLFORMAT.DOC (64 KB) | 5.1 | 0 problems. Headers and footers, comment, open style, `[Bline]` (D3/08). |
| ADDALL.DOC | 5.0 | 0 problems. Indents and end-of-indent codes. |
| CALLSTRT.DOC | 5.1 | 0 problems |
| SDK KERN.TST and PRINTER.TST | 5.0 / 5.1 | 0 problems |
| STYLE/LIBRARY.STY, SUB, T | 5.1 | 0 problems. Empty bodies; 22 style packets decode, outline styles named `\x7Fa<n>…`. |
| SDK CHAR.TST | 5.0 | 2 framing errors at +7028 and +9706. The file contains **raw 0xDA bytes typed as text** in a ruler line. The resync rule (skip 1 byte when framing fails, as libwpd does) recovers, and the rest parses. |
| MOUSEASM.DOC | — | Not a WP file (plain ASCII). Rejected correctly. |
| WP{WP}.SET | product 1, type 17, version 1.0 | Rejected as "not a 5.x document". |

## 3. Output excerpts

PRINTER.TST, start of body. It opens with a footer whose nested text contains a date code:

```
[Bold][Footer A:Every page;[Center][Fine]This document printed in WordPerfect 5.1  - [C/A/Flrt][SRt]
[Date:3 1, 4  8:9 0].[fine]][T/B Mar:0.5",0.5"][Center][Ext Large]WordPerfect 5.1 Printer Test[bold][ext large][C/A/Flrt][HRt]
{Fmt grp 1}[HLine:6.5",0.021"][HRt]
...
mouse support, and more advanced macro and merge func[-soft]tions. [SRt]
...
[Tbl Def:Center;4,1.693",1.844",1.245",1.417"][Row][Cell]{Fmt grp 5}Japanese ざ[Cell]{Fmt grp 5}Copyright ©[Cell]{Fmt grp 5}Hand ☜[Cell]{Fmt grp 5}Greek Δ[Tbl Off][HRt]
...
edits tables easily.  The Tables Options[Footnote:1;[Note Num][Fine]Although only double and single lines are used in this table,[SRt]
...
[Fig Opt][Figure:1.1;MOUSE.WPG;[Center]MOUSE]{<WPG image #0 in packet 8>}{Fmt grp 2}[HRt]
{Fmt grp 2}[Col Def:Parallel;2;1",3.593",4.907",7.5"][Col On]{Fmt grp 2}Graphic images can be[SRt]
```

KEYBRDS.WP. German text with umlauts stored as `C0 char 01 C0`, and a relative tab set:

```
{Fmt grp 1}[L/R Mar:0.29",0.25"][Bold][bold][Bold]Tastatur-Layout: Deutsch[bold][Tab][Tab][Bold]DE_USA[bold][HRt]
 ┌────┬────┬────┬────┬────┬────┬────┬────┬────┬────┬────┬────┬────┬────────┐[HRt]
...
{Fmt grp 1}[Tab Set:Rel; -1"L,-0.5"L,0.24"L,0.54"L][Tab]Wenn Sie die Taste [Bold]^[bold], [Bold]'[bold] oder ...
[Tab][Tab]tippen, wird das Symbol über dem gewählten Buchstaben (z.B. ê,é)[HRt]
```

KERN.TST. A WP 5.0 file with manual kerning through relative horizontal Advance codes:

```
[Font:Courier 10pt 10 Pitch][Kern:On][Bold]Common ASCII Kern Pairs:[bold][HRt]
-05:  M[AdvLft:0.017"]N  M[AdvLft:0.02"]N ...
```

LIBRARY.STY with `--prefix`:

```
  type   263 (0x0107) Style                      len    116 at 843
      style 'Bibliogrphy' (Paired, enter=3) 'Bibliography'
        on : [→Indent][←Mar Rel]
        off: {End Indent}[HRt]\n[HRt]\n
```

## 4. Findings that refine the published specification

1. **Packet 8 (graphics):** the index length is always 2, and the image data that follows is not counted. A reader that trusts the index length reads nothing; a strict reader fails. The dumper originally crashed on PRINTER.TST because of this.
2. **Packet 2 (WP 5.0 fonts):** the record is 82 bytes, not the documented 86. The point size is at +22 as documented.
3. **Extended character byte order** is `C0 char set C0`, with the character before the set. Confirmed by `C0 1F 01 C0` = ä (1,31).
4. **Style libraries** are ordinary type-10 documents with an empty body.
5. **Fast-Saved files** contain deleted prefix packets (type 0xFFFF) scattered between live ones. Packet file order is arbitrary.
6. Line-end variants are as documented: `0xAD` soft hyphen at end of line, `0xAF` columns off at end of line. Every `[SRt]` in these files replaces a space.

7. **Box numbers** (DA +4) come out as `1.1` for the first figure: level 1 = 1, level 2 = 1, even though numbering level 2 is not in use. The level-2 field apparently holds 1 when it is unused. The value is calculated by the formatter, so an importer should renumber boxes itself.

## 5. Not exercised by the available files (spec-only, untested)

* Prefix packets 1 (summary) and 3 (initial codes). None of the test files has them.
* Encryption.
* Endnotes, comments inside the main body, merge codes (DE group), cross-references and index marks (D7), outline and paragraph numbering in the body (D8/01 is seen only inside styles), Overstrike, Language codes, Cyrillic and other set-10 text.

**Recommendation:** before the Rust importer is finalised, run the dumper over a sample of the user's own documents, especially German, Italian, French, Russian and Spanish translation files. Check for:

* `== problems` lines;
* PUA characters (U+E000–U+ECFF) in the output, which would reveal any charset gaps that matter in practice.
