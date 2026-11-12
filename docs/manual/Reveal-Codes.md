# Reveal Codes

Reveal Codes is the heart of the program, exactly as in WordPerfect 5.1. It shows you the hidden **codes** that control how your text looks, and lets you change or delete them directly.

## What codes are

When you press **F6** to start bold, the program does not change the letters. It puts an invisible **code** into the text: **[BOLD]** where bold starts, and **[bold]** where it ends. In the same way, a margin change, a tab, a page break, a footnote or a font change is a code at a certain place in the text. Everything after a code is affected by it, until the next code of the same kind.

On the normal screen you see only the result (for example bold letters). In Reveal Codes you see the codes themselves.

## Opening and closing Reveal Codes

Press **F11** or **Alt-F3**. The screen is split:

- the **upper part** shows the text as usual;
- a **ruler bar** in the middle shows the margins **[ ]** and tab stops;
- the **lower part** shows the same text with all codes, for example:

```
Dear [BOLD]Mr Smith[bold],[HRt]
[HRt]
thank you for your letter of [Date:3 1, 4].[SRt]
```

The cursor in the lower part is a highlighted bar. It moves together with the cursor in the upper part.

Press **F11** or **Alt-F3** again to close Reveal Codes.

The size of the lower part is set in **Shift-F1**, **2** Display, **6** Reveal Codes Window Size (standard: 10 lines on a 25-line screen).

## Working in Reveal Codes

You can type and edit normally while Reveal Codes is open. The arrow keys move over the codes one by one: a code counts as one position.

### Deleting a code

1. Open Reveal Codes (F11).
2. Move the cursor so that the highlighted bar is **on** the code.
3. Press **Del**. (Or put the cursor just after the code and press **Backspace**.)

The code disappears and the text changes at once. For example, deleting [BOLD] (or [bold]) removes the bold from that passage.

In Reveal Codes the code is deleted without a question, because you can see what you are deleting. On the normal screen the program asks first, for example `Delete [BOLD]? No (Yes)`.

If you deleted a code by mistake, press **Ctrl-Z** (Undo), or **Esc** for Undelete.

### Changing a code

To change a setting, it is usually best to delete the old code and insert a new one. For example, to change the left and right margins of a letter:

1. Open Reveal Codes and find the **[L/R Mar:…]** code.
2. Put the cursor on it and press **Del**.
3. Press **Shift-F8**, **1** Line, **7** Margins, and type the new values.

Many codes that contain text, such as footnotes, headers, footers and comments, are edited with their own key (for example **Ctrl-F7**, **1**, **2** Edit for a footnote).

### Following a link

In Reveal Codes, with the cursor on a **[Link:…]** code, press **Enter** to open the link (a web address, an e-mail address, or a bookmark in the document). On the normal screen, use **Ctrl + click**, or **Ctrl-F5**, **8**, **4** (Open Link at Cursor).

### Arabic and Hebrew

For right-to-left text, Reveal Codes shows the letters in the order they were typed, unjoined, just as it shows all other codes. The upper part shows them right to left and joined.

## Where to put a code

Codes take effect from where they are. Some practical rules, as in 5.1:

- Put format codes for the whole document at the very beginning. **Home**, **Home**, **Home**, **Up** goes to the beginning, before all codes. Even better: put them into the document's **Initial Codes** (**Shift-F8**, **3** Document, **2** Initial Codes), so they cannot be deleted by accident.
- Line formats (margins, spacing, justification, tabs) should be at the start of a paragraph.
- Page formats (top/bottom margins, paper size, headers) should be at the top of the page.

## Codes reference

The table lists the codes you will meet most often. Codes in pairs (on and off) are shown with the "on" code in capitals and the "off" code in lowercase, for example **[BOLD]** … **[bold]**.

### Text and line endings

| Code | Meaning | Created by |
|---|---|---|
| [HRt] | Hard return (end of paragraph) | Enter |
| [SRt] | Soft return (automatic line end) | Word wrap |
| [HPg] | Hard page break | Ctrl-Enter |
| [SPg] | Soft page break (automatic) | Pagination |
| [HRt-SPg] | A hard return that also ends a page | Pagination |
| [ISRt] | Invisible soft return | Home, Enter |
| [ ] | Hard space | Home, Space |
| [-] | A hyphen character | Typing - |
| [/] | Cancel hyphenation of the next word | Home, / |
| [Tab] | Tab | Tab |
| [TAB] [CNTR TAB] [RGT TAB] [DEC TAB] | Hard tabs (left, centre, right, decimal) | Home, Tab / Home, Shift-F6 / Home, Alt-F6 / Home, Ctrl-F6 |
| [Dec Tab] | Decimal-aligned tab | Ctrl-F6 |
| [→Indent] | Indent | F4 |
| [→Indent←] | Indent from both sides | Shift-F4 |
| [←Mar Rel] | Margin release | Shift-Tab |
| [Center] | Centred line | Shift-F6 |
| [Flsh Rgt] | Flush right | Alt-F6 |
| [Dot Flsh Rgt] | Flush right with dot leader | Alt-F6 twice |

### Attributes (Ctrl-F8, F6, F8)

| Code pair | Meaning |
|---|---|
| [BOLD] [bold] | Bold |
| [UND] [und] | Underline |
| [DBL UND] [dbl und] | Double underline |
| [ITALC] [italc] | Italics |
| [OUTLN] [outln] | Outline |
| [SHADW] [shadw] | Shadow |
| [SM CAP] [sm cap] | Small capitals |
| [REDLN] [redln] | Redline |
| [STKOUT] [stkout] | Strikeout |
| [SUPRSCPT] [suprscpt] | Superscript |
| [SUBSCPT] [subscpt] | Subscript |
| [FINE] [fine], [SMALL] [small] | Fine, small print |
| [LARGE] [large], [VRY LARGE] [vry large], [EXT LARGE] [ext large] | Large, very large, extra large print |
| [Font:Times New Roman 12pt] | Base font change (Ctrl-F8, 4) |

### Line format (Shift-F8, 1)

| Code | Meaning |
|---|---|
| [Hyph On] [Hyph Off] | Hyphenation on / off |
| [HZone:10%,4%] | Hyphenation zone |
| [Just:Left] [Just:Center] [Just:Right] [Just:Full] | Justification |
| [Ln Height:Auto] [Ln Height:0.2"] | Line height |
| [Ln Num:On] [Ln Num:Off] | Line numbering |
| [Ln Spacing:2] | Line spacing |
| [L/R Mar:1",1"] | Left and right margins |
| [Tab Set:…] | Tab stops |
| [W/O On] [W/O Off] | Widow/orphan protection |

### Page format (Shift-F8, 2)

| Code | Meaning |
|---|---|
| [Center Pg] | Centre this page top to bottom |
| [Force:Odd] [Force:Even] | Force odd / even page number |
| [Header A:Every page;…] [Footer B:…] | Header or footer (with a preview of its text) |
| [T/B Mar:1",1"] | Top and bottom margins |
| [Pg Num:5] | New page number |
| [Pg Num Style:…] | Page number style |
| [Insert Pg Num:…] | Page number inserted in the text |
| [Pg Numbering:Bottom Center] | Page number position |
| [Paper Sz/Typ:8.5" x 11",Standard] | Paper size and type |
| [Suppress:…] | Suppress page number, headers or footers on this page |
| [Block Pro:On] [Block Pro:Off] | Block protect |
| [Cndl EOP:3] | Conditional end of page (keep 3 lines together) |

### Other format (Shift-F8, 4)

| Code | Meaning |
|---|---|
| [AdvUp:…] [AdvDn:…] [AdvToLn:…] [AdvLft:…] [AdvRgt:…] [AdvToPos:…] | Advance |
| [Decml/Algn Char:.,,] | Decimal/align character and thousands' separator |
| [Lang:DE] | Language of the following text |
| [Ovrstk:e´] | Overstrike |
| [Kern:On] [Ptr Cmnd:…] [Wrd/Ltr Spacing:…] [Just Lim:…] [Bline:On] [Leading Adj:…] | Printer functions |
| [Undrln:Spaces,Tabs] | Whether spaces and tabs are underlined |
| [Brdr Opt] | Border options |

### Notes, comments, dates

| Code | Meaning |
|---|---|
| [Footnote:#;[Note Num]…] | Footnote (with the start of its text) |
| [Endnote:#;[Note Num]…] | Endnote |
| [Comment] | Document comment (shown in a box on screen, never printed) |
| [Date:3 1, 4] | Date code (shows the current date) |

### Tables, columns, math

| Code | Meaning |
|---|---|
| [Tbl Def:I;3,…] | Table definition |
| [Row] [Cell] | Start of a table row / cell |
| [Tbl Off] | End of a table |
| [Col Def:…] | Text column definition |
| [Col On] [Col Off] | Text columns on / off |
| [Math Def] [Math On] [Math Off] | Math columns |
| [Formula:…] | A table formula |

### Graphics

| Code | Meaning |
|---|---|
| [Fig Box:#;file;caption] [Tbl Box:…] [Text Box:…] [Usr Box:…] [Equ Box:…] | Graphics boxes (figure, table box, text box, user box, equation) |
| [Fig Opt] [Tbl Opt] [Txt Opt] [Usr Opt] [Equ Opt] | Box options |
| [New Fig Num:3] and similar | New box number |
| [Box Num] | The box number inside a caption |
| [HLine:…] [VLine:…] | Horizontal / vertical graphics line |

### Styles, outlines, mark text

| Code | Meaning |
|---|---|
| [Style On:Name] [Style Off:Name] | Paired style |
| [Open Style:Name] | Open style |
| [Outline On] [Outline Off] | Outline mode |
| [Par Num:Auto] [Par Num:2] | Paragraph number (automatic or fixed level) |
| [Par Num Def] | Paragraph numbering definition |
| [Mark:ToC,1] [End Mark:ToC,1] | Text marked for a table of contents (or List) |
| [Index:Heading;Subheading] | Index entry |
| [ToA:…] | Table of authorities entry |
| [Def Mark:…] … [End Def] | Where a table of contents, list, index or ToA is generated |
| [Ref(name):…] [Target(name)] | Cross-reference and its target |
| [Subdoc:file] [Subdoc Start:file] [Subdoc End:file] | Master document parts |

### Track changes, links, merge

| Code | Meaning |
|---|---|
| [INS:author date] … [ins] | Tracked insertion |
| [DEL:author date] … [del] | Tracked deletion |
| [Link:address] … [Link End] | Hyperlink |
| [Bookmark:name] | Bookmark |
| [Link:file;range] … [Link End] | Spreadsheet link |
| [Mrg:FIELD] and other [Mrg:…] codes | Merge commands (shown as {FIELD} on the normal screen) |

### Codes the program does not know

Documents from other programs can contain things that Blue Screen Writer 51 does not handle itself. These are kept as codes with their original name in brackets, so they are not lost when you save. Leave them alone unless you know you do not need them.
