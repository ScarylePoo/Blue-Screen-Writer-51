# Document Information

This page covers **Ctrl-F5, 7 Document Info** (counts for invoices, including standard lines and the invoice amount), **Ctrl-F5, 8 Link/Bookmark** (hyperlinks and bookmarks), and the **document summary**.

## Document Info (Ctrl-F5, 7)

Press **Ctrl-F5** (Text In/Out), then **7** (Document Info). A screen like this appears:

```
Document Information                          Text   +Notes/Headers      Block

Words                                          812             845         120
Characters (with spaces)                     5,123           5,330         702
Characters (without spaces)                  4,301           4,476         590
Paragraphs                                      24
Standard lines (55 characters)               93.15           96.91       12.76
Standard pages (1800 characters)              2.85            2.96        0.39
Amount at 1,20 per line                     111.78          116.29       15.31
Lines (as printed)                             160
Pages                                            3
Footnotes/endnotes                               4

1 Price per Line; 2 Characters per Line; 3 Copy to Clipboard: 0
```

### The columns

| Column | Counts |
|---|---|
| **Text** | The main text of the document only. |
| **+Notes/Headers** | The main text plus footnotes, endnotes, headers and footers. |
| **Block** | Only the marked Block. This column appears when a Block was on when you pressed Ctrl-F5. |

### The rows

| Row | Meaning |
|---|---|
| **Words** | Number of words. Every Chinese or Japanese character counts as one word, as in Word. |
| **Characters (with spaces)** | All characters, including spaces (line ends and soft hyphens are not counted). |
| **Characters (without spaces)** | All characters except spaces, tabs and line ends. |
| **Paragraphs** | Paragraphs that contain text (empty lines are not counted; table cells count as paragraphs). |
| **Standard lines** | Characters with spaces divided by the characters per standard line (55 unless you change it). Translators in many countries are paid per standard line ("Normzeile"). |
| **Standard pages** | Characters with spaces divided by 1800. |
| **Amount at … per line** | The invoice amount: standard lines (rounded up to two decimals) multiplied by your price per line. Shown only when a price is set. |
| **Lines (as printed)** | The number of lines as laid out on the pages. |
| **Pages** | Number of pages. |
| **Footnotes/endnotes** | Number of notes. |

Numbers are shown with a decimal comma and thousands points (German style) unless the speller language is English, in which case a decimal point is used.

### Options

| Choice | What it does |
|---|---|
| **1 Price per Line** | Type your price per standard line, for example `1,20` or `1.20` (both are understood; a currency sign may be added). Leave it empty to hide the amount. The price is kept for next time. |
| **2 Characters per Line** | Change the size of a standard line (20–200; standard 55). Kept for next time. |
| **3 Copy to Clipboard** | Copies all figures as a small tab-separated table to the Windows clipboard, so you can paste them into an invoice, an e-mail or Excel. |

**Esc**, **F7**, **Enter**, **Space** or **0** close the screen.

### Counting part of a document

1. Mark the part as a Block (Alt-F4 or F12, or drag with the mouse).
2. Press **Ctrl-F5**, **7**.

The **Block** column shows the figures for the Block.

For a quick word count, the speller also counts: **Ctrl-F2**, **6** (Count).

## Links and bookmarks (Ctrl-F5, 8)

Press **Ctrl-F5**, **8** (Link/Bookmark):

```
1 Hyperlink; 2 Bookmark; 3 Go to Bookmark; 4 Open Link at Cursor: 0
```

### 1 Hyperlink

Makes a clickable link to a web page, an e-mail address or a bookmark in the document.

1. Either mark the text that should become the link as a Block, or just put the cursor where the link should go.
2. Press **Ctrl-F5**, **8**, **1**.
3. At `Address (URL, e-mail or #bookmark):` type the address, for example:
   - `www.example.com` (http:// is added if needed),
   - an e-mail address (it becomes an e-mail link),
   - `#Chapter2` (a bookmark in this document).
   If the Block itself is an address (for example *www.example.com*), it is offered already.
4. Press **Enter**.

With a Block, the Block becomes the link text. Without a Block, the address itself is inserted as the link text. Codes: **[Link:address]** … **[Link End]**.

Links are saved as real hyperlinks in Word, PDF and HTML files.

### 2 Bookmark

Marks a place in the document with a name. Type the name at `Bookmark name:` and press **Enter**. Code: **[Bookmark:name]**.

### 3 Go to Bookmark

Type the bookmark name (the first bookmark is offered) and press **Enter**. The cursor jumps there.

### 4 Open Link at Cursor

Opens the link the cursor is in: a web address opens in your web browser, an e-mail address in your e-mail program, a bookmark link jumps to the bookmark. If there is no link at the cursor: `No link at the cursor`.

Other ways to open a link:

- **Ctrl + click** on the link with the mouse;
- in Reveal Codes, put the cursor on the **[Link:…]** code and press **Enter**.

## Document summary (Shift-F8, 3, 5)

**Shift-F8** (Format), **3** (Document), **5** (Summary) holds information about the document:

1. Descriptive Name
2. Subject/Account
3. Author
4. Typist
5. Comments

Press the number, type the text, and press **Enter**. **F7** returns to the document. The summary is saved with the document and appears in Word under *File, Info, Properties*. The pull-down menu has it under **File**, **Summary**.
