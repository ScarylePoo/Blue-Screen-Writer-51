# Formatting

This page covers the look of your text: fonts and attributes (**Ctrl-F8**), all the **Shift-F8** Format menus, and the alignment keys (Center, Flush Right, Indent, Tab Align) and dates.

Every formatting change inserts a code into the text. You can see and delete these codes in [Reveal Codes](Reveal-Codes). A format code works from where it is to the next code of the same kind, so put it where the change should start.

## Bold and Underline (F6, F8)

- **F6** switches **bold** on; press it again to switch it off.
- **F8** switches **underline** on and off.
- With a **Block** on, F6 or F8 makes the whole Block bold or underlined.

Codes: **[BOLD]**…**[bold]**, **[UND]**…**[und]**.

## Font (Ctrl-F8)

Press **Ctrl-F8**. The menu shows:

```
1 Size; 2 Appearance; 3 Normal; 4 Base Font; 5 Print Color: 0
```

With a Block on, only **1 Size** and **2 Appearance** are offered, and they apply to the Block.

### 1 Size

```
1 Suprscpt; 2 Subscpt; 3 Fine; 4 Small; 5 Large; 6 Vry Large; 7 Ext Large
```

- **Superscript** and **Subscript** raise or lower the text (for example m² or H₂O).
- **Fine**, **Small**, **Large**, **Very Large** and **Extra Large** print the text in a smaller or larger size, relative to the base font.

Without a Block, the choice switches the size on at the cursor. Type the text, then press **Ctrl-F8**, **3** Normal (or press the right arrow past the off code) to continue in the normal size.

### 2 Appearance

```
1 Bold; 2 Undln; 3 Dbl Und; 4 Italc; 5 Outln; 6 Shadow; 7 Sm Cap; 8 Redln; 9 Stkout
```

Bold, underline, double underline, italics, outline, shadow, small capitals, **redline** (marks added text) and **strikeout** (marks removed text). Each choice switches the attribute on; choose it again to switch it off. With a Block, it is applied to the Block.

On screen, each attribute is shown in its own colour, and with real bold, italic or underlined letters unless you switched that off in Setup (Shift-F1, 2, 2).

### 3 Normal

Ends all attributes and sizes at the cursor: the text you type next is plain.

### 4 Base Font

Shows the list of all TrueType and OpenType fonts installed in Windows (Times New Roman, Arial, Garamond and so on).

1. Move to a font with the arrow keys (or press **N** and type the start of its name).
2. Press **1** (Select) or **Enter**.
3. Type the size in points at `Point size:` and press **Enter**.

The code **[Font:Name size pt]** is inserted at the cursor. To change the font of the whole document, use **Shift-F8**, **3** Document, **3** Initial Base Font instead.

Old WordPerfect font names in old documents (for example "Helvetica" or "Times Roman") are translated to the matching Windows font automatically.

### 5 Print Color

*Not available in this version.*

## Center, Flush Right, Indent

| Key | Effect | Code |
|---|---|---|
| **Shift-F6** | Centres the line between the margins. Type the text, then press Enter. | [Center] |
| **Alt-F6** | Aligns the line at the right margin (for example a date at the top of a letter). | [Flsh Rgt] |
| **Alt-F6**, **Alt-F6** | Flush right with a dot leader (dots fill the gap). | [Dot Flsh Rgt] |
| **F4** | Indents the whole paragraph to the next tab stop, up to the next Enter. Press several times to indent further. | [→Indent] |
| **Shift-F4** | Indents the paragraph from both margins by one tab stop each (for quotations). | [→Indent←] |
| **Shift-Tab** | Margin release: moves the first line one tab stop to the left (for hanging indents: F4, then Shift-Tab). | [←Mar Rel] |
| **Ctrl-F6** | Tab Align: the text at the next tab stop is aligned on the decimal point (for columns of figures). The status line shows the align character. | [Dec Tab] |

**With a Block:** Shift-F6 asks `[Just:Center]?` and Alt-F6 asks `[Just:Right]?`. Press **Y** to centre or right-align all the blocked lines.

**Hard tabs** (tabs of a fixed kind, whatever the tab settings say): **Home**, **Tab** (left), **Home**, **Shift-F6** (centre), **Home**, **Alt-F6** (right), **Home**, **Ctrl-F6** (decimal).

## Date (Shift-F5)

Press **Shift-F5** (Date/Outline):

```
1 Date Text; 2 Date Code; 3 Date Format; 4 Outline; 5 Para Num; 6 Define: 0
```

- **1 Date Text** types today's date as ordinary text.
- **2 Date Code** inserts a code **[Date:…]** that always shows the current date (on screen and when printed).
- **3 Date Format** chooses how dates look. Type a pattern with these numbers:

| Character | Meaning |
|---|---|
| 1 | Day of the month |
| 2 | Month (number) |
| 3 | Month (word) |
| 4 | Year (all four digits) |
| 5 | Year (last two digits) |
| 6 | Day of the week (word) |
| 7 | Hour (24-hour clock) |
| 8 | Hour (12-hour clock) |
| 9 | Minute |
| 0 | am / pm |
| % | Before a number: with a leading zero (%1 gives 05) |
| $ | Before a word: abbreviated to three letters ($3 gives Jan) |

Other characters are typed as they are. Examples: `3 1, 4` gives *November 12, 2026* (the standard); `1. 3 4` gives *12. November 2026*; `%1.%2.4` gives *12.11.2026*.

Month and day names are in English, or in German when chosen in **Shift-F1**, **4** Initial Settings, **3** Date Language, or when the text has a German language code (Shift-F8, 4, 4).

Options 4 to 6 (outlines and paragraph numbers) are described in [Styles and Outlines](Styles-and-Outlines).

## Format (Shift-F8)

Press **Shift-F8**. The Format screen shows four groups:

```
1 - Line        Hyphenation, Line Spacing, Justification, Margins Left/Right,
                Line Height, Tab Set, Line Numbering, Widow/Orphan Protection
2 - Page        Center Page, Page Numbering, Force Odd/Even Page, Paper Size/Type,
                Headers, Suppress, Footers, Margins Top/Bottom
3 - Document    Display Pitch, Redline Method, Initial Codes/Font, Summary
4 - Other       Advance, Overstrike, Conditional End of Page, Printer Functions,
                Decimal Characters, Underline Spaces/Tabs, Language, Border Options
```

Press the number of a group. In each group screen, the current value of every setting is shown on the right. Press the number of a setting to change it.

- For a **Yes/No** setting, press **Y** or **N**.
- For a **choice**, the choices appear at the bottom; press the number or first letter.
- For **values**, type the value and press **Enter** (for two values, Enter moves to the second one).
- **F7** (Exit) returns to the document. **Esc**, **Enter** or **0** go back one screen.

The new setting is inserted as a code at the cursor position.

**With a Block on**, Shift-F8 asks `Protect block?`. Answer **Y** to keep the blocked text together on one page ([Block Pro:On] … [Block Pro:Off]).

### 1 Line

| Option | What it does |
|---|---|
| **1 Hyphenation** | Yes switches automatic hyphenation on from here. Long words at the end of a line are then divided according to the language's hyphenation rules. See also "Prompt for Hyphenation" in [Setup](Setup). |
| **2 Hyphenation Zone** | Left and right zone in per cent of the line width (standard 10% and 4%). A smaller zone means more hyphenation. |
| **3 Justification** | 1 Left, 2 Center, 3 Right, 4 Full (both margins straight). |
| **4 Line Height** | 1 Auto (follows the font size) or 2 Fixed (type a height, for example 0.25"). |
| **5 Line Numbering** | Numbers the lines in the left margin, for legal and translation work. Answer Y, then set: 1 Count Blank Lines, 2 Number Every n Lines, 3 Position of Number from Left Edge, 4 Starting Number, 5 Restart Numbering on Each Page. Press F7 when done. Answer N to switch it off from here. |
| **6 Line Spacing** | 1 for single, 2 for double, 1.5 and so on. |
| **7 Margins Left/Right** | The distance from the left and right edges of the paper (standard 1"). |
| **8 Tab Set** | Opens the tab ruler (see below). |
| **9 Widow/Orphan Protection** | Yes keeps a paragraph from leaving a single first or last line alone at the bottom or top of a page. |

#### Setting tabs

**Shift-F8**, **1**, **8** shows the tab ruler with the current tab stops. The bottom of the screen explains the keys:

- **Left/Right arrow** move along the ruler.
- **Ctrl-End** (Delete EOL) clears all tabs from the cursor to the right.
- Type a **number** and press **Enter** to set a tab at that position.
- **Del** clears the tab at the cursor.
- **L**, **C**, **R**, **D** set a Left, Centre, Right or Decimal tab at the cursor.
- **.** (period) adds a dot leader to the tab at the cursor.
- **T** switches the type between "Absolute" (measured from the edge of the paper) and "Relative to Margin".
- **F7** (Exit) saves the tab settings and inserts a [Tab Set] code.

### 2 Page

| Option | What it does |
|---|---|
| **1 Center Page (top to bottom)** | Centres the text of this page vertically (for title pages, short letters). Put it at the top of the page. |
| **2 Force Odd/Even Page** | Forces this page to have an odd or even number (for example so that chapters start on a right-hand page). |
| **3 Headers** | Header A and Header B. See [Footnotes, Endnotes, Headers and Footers](Footnotes-Endnotes-Headers-and-Footers). |
| **4 Footers** | Footer A and Footer B. Same page. |
| **5 Margins Top/Bottom** | The top and bottom margins (standard 1"). |
| **6 Page Numbering** | See below. |
| **7 Paper Size/Type** | See below. |
| **8 Suppress (this page only)** | Leaves out page numbers, headers or footers on the current page only (for example on the first page of a letter). Choose with Y/N: 1 Suppress All Page Numbering, Headers and Footers; 2 Suppress Headers and Footers; 3 Print Page Number at Bottom Center; 4 Suppress Page Numbering; 5 Suppress Header A; 6 Header B; 7 Footer A; 8 Footer B. Press F7 when done. |

#### Page numbering

**Shift-F8**, **2**, **6** shows:

1. **New Page Number** — the number this page should have. Type a number (for example `5`) or a Roman numeral (for example `iv`) for Roman page numbers.
2. **Page Number Style** — the text printed around the number. `^B` stands for the number, for example `Page ^B` or `- ^B -`.
3. **Insert Page Number** — puts the page number (in the current style) into the text at the cursor, for example inside a header.
4. **Page Number Position** — shows a picture of the page. Press **1** to **3** for top left, centre or right on every page, **4** for alternating top corners, **5** to **7** for bottom left, centre or right, **8** for alternating bottom corners, **9** for no page numbers.

#### Paper size and type

**Shift-F8**, **2**, **7** shows the list of paper definitions:

```
Standard (8.5" x 11")        Standard Landscape (11" x 8.5")
Legal (8.5" x 14")           Legal Landscape (14" x 8.5")
Envelope (9.5" x 4")         Half Sheet (5.5" x 8.5")
US Government (8" x 11")     A4 (210mm x 297mm)
A4 Landscape (297mm x 210mm) Other
```

- **1 Select** (or Enter) — uses the highlighted paper from here. A paper chosen at the very beginning of the document becomes the document's page size.
- **2 Add** — creates a new paper definition. You choose the paper type (Standard, Bond, Letterhead, Labels, Envelope, Transparency, Cardstock, [ALL OTHERS] or Other) and then the size.
- **3 Copy** — copies the highlighted definition so you can change it.
- **4 Delete** — removes a definition.
- **5 Edit** — changes the size, type, name and label settings of a definition.
- **N** — name search.

**Labels.** A paper definition of type *Labels* has a label form: label size (width, height), number of labels (columns, rows), top left corner, distance between labels, and label margins. With such a paper selected, each page of text (end each label with **Ctrl-Enter**) is printed on one label.

**Envelopes.** Use the *Envelope* paper (or one you add) at the start of a separate document, type the address, and print.

### 3 Document

| Option | What it does |
|---|---|
| **1 Display Pitch** | How wide one screen column is (Automatic Yes/No, and the width). Only affects the screen. |
| **2 Initial Codes** | Opens an editing screen (with Reveal Codes) for codes that apply to the whole document, for example margins, justification or line spacing. Put your codes there and press F7. These codes cannot be deleted by accident in the text. |
| **3 Initial Base Font** | The font and size of the whole document. Choose from the Windows font list, then type the point size. |
| **4 Redline Method** | How redlined text is marked when printed: Printer Dependent, Left (a bar in the left margin), or Alternating (left or right margin on alternating pages). |
| **5 Summary** | Descriptive Name, Subject/Account, Author, Typist and Comments. These are saved with the document (and appear in Word's document properties). |

To give **all new documents** the same initial codes, use **Shift-F1**, **4** Initial Settings, **5** Initial Codes. See [Setup](Setup).

### 4 Other

| Option | What it does |
|---|---|
| **1 Advance** | Moves the printing position: 1 Up, 2 Down, 3 Line (to a line from the top), 4 Left, 5 Right, 6 Position (to a position from the left edge). Type the distance. Useful for filling in forms. |
| **2 Conditional End of Page** | Type a number of lines to keep together on one page (for example a heading with the first lines of the next paragraph). Put the code on the line above them. |
| **3 Decimal/Align Character, Thousands' Separator** | The character Tab Align and math use as the decimal point (for example `,` for German), and the thousands' separator. |
| **4 Language** | A two-letter language code for the following text: DE German, US English (US), UK English (UK), FR French, IT Italian, ES Spanish, RU Russian, AR Arabic, HE Hebrew. The speller, thesaurus, hyphenation, smart quotes and dates follow this code. |
| **5 Overstrike** | Prints several characters on top of each other (for example `e` and `´`). 1 Create types the characters; 2 Edit changes the nearest overstrike. On screen the combined character is shown when possible. |
| **6 Printer Functions** | 1 Kerning; 2 Printer Command (a command or a file name sent to the printer); 3 Word Spacing and Letter Spacing; 4 Word Spacing Justification Limits (how much spaces may shrink or stretch with full justification); 5 Baseline Placement for Typesetters; 6 Leading Adjustment (extra space after soft and hard returns). |
| **7 Underline Spaces/Tabs** | Whether underlining continues through spaces and through tabs. |
| **8 Border Options** | The width and shading of the border styles used by graphics boxes and tables: Single, Double, Dashed, Dotted, Thick and Extra Thick. |

## Hyphenation in practice

1. Switch hyphenation on with **Shift-F8**, **1**, **1**, **Y** (at the beginning of the document or in Initial Codes).
2. Words are divided according to the language of the text (German, English, French, Italian, Spanish and Russian patterns are included).
3. Depending on **Shift-F1**, **3**, **7** Prompt for Hyphenation (Never, When Required, Always), the program may ask where to divide a word. The bottom line shows `Position hyphen; Press ESC` and the word with a hyphen. Move the hyphen with the **Left/Right arrows**, then:
   - **Esc** (or **Enter**) hyphenates at that place;
   - **F1** on the Alternate keyboard (Cancel on the original keyboard), **/** or **F7** does not hyphenate this word (a [/] code is inserted).

You can also put in hyphens yourself: **Ctrl-minus** inserts a soft hyphen (shown and printed only when the word is divided at the end of a line), and **Home**, **/** prevents the next word from being hyphenated.
