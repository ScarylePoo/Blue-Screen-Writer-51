# Footnotes, Endnotes, Headers and Footers

Footnotes, endnotes, headers and footers all contain text of their own. You write that text on a separate editing screen, exactly as in WordPerfect 5.1: the normal editor, with a title such as *Footnote: Press Exit when done* on the status line. You can use all the usual keys there (bold, fonts, Reveal Codes…). Press **F7** (Exit) to finish and return to the document.

## Footnotes and endnotes (Ctrl-F7)

Press **Ctrl-F7** (Footnote):

```
1 Footnote; 2 Endnote; 3 Endnote Placement: 0
```

Choose **1** (Footnote) or **2** (Endnote). The next menu is:

```
1 Create; 2 Edit; 3 New Number; 4 Options: 0
```

### Creating a note

1. Put the cursor right after the word the note belongs to.
2. Press **Ctrl-F7**, **1** (Footnote) or **2** (Endnote), then **1** (Create).
3. Type the text of the note.
4. Press **F7** (Exit).

The note number appears in the text, and a code such as **[Footnote:#;[Note Num]text…]** is placed there. Notes are numbered automatically: when you add or delete a note, all the others are renumbered.

**Footnotes** are printed at the bottom of the same page, below a short separator line. **Endnotes** are collected at the end of the document.

### Editing a note

1. Press **Ctrl-F7**, **1** or **2**, then **2** (Edit).
2. At `Footnote number?` the number of the next note after the cursor is offered. Press **Enter**, or type another number.
3. Change the text and press **F7**.

### Deleting a note

Delete its code like any other code: in Reveal Codes, put the cursor on **[Footnote:…]** and press **Del**. The remaining notes are renumbered.

### Not available in this version

**3 New Number**, **4 Options** (for footnotes and endnotes) and **3 Endnote Placement** show the message "not available yet in this version". Notes always start at 1, use the standard look, and endnotes are printed at the end of the document. Notes in documents from Word or WordPerfect keep their own numbering settings in the file.

## Headers and footers (Shift-F8, 2, 3 and 4)

A **header** is printed at the top of every page (or of odd or even pages), a **footer** at the bottom. You can have two of each: **Header A** and **Header B**, **Footer A** and **Footer B** — for example Header A on odd pages and Header B on even pages.

### Creating a header or footer

1. Go to the **top of the first page** where it should appear (for the whole document: **Home**, **Home**, **Up**).
2. Press **Shift-F8** (Format), **2** (Page), then **3** (Headers) or **4** (Footers).
3. Choose **1** Header A (Footer A) or **2** Header B (Footer B).
4. Choose when it is printed:

```
1 Discontinue; 2 Every Page; 3 Odd Pages; 4 Even Pages; 5 Edit: 0
```

5. Type the text of the header or footer. Use the normal keys: **Shift-F6** to centre, **Alt-F6** for flush right, **F6** for bold, and so on.
6. Press **F7** (Exit) twice: once to finish the header, once more to leave the Format menu.

A code such as **[Header A:Every page;text…]** is inserted. The header or footer is printed from this page on, until it is discontinued or replaced. It is not shown on the editing screen; use **View Document** (Shift-F7, 6) to see it.

### Page numbers in a header or footer

While typing the header or footer, press **Shift-F8**, **2** (Page), **6** (Page Numbering), **3** (Insert Page Number). The current page number is printed at that place on every page.

For example, a footer "Page 3" at the right margin: in the footer, press **Alt-F6**, type `Page `, then Shift-F8, 2, 6, 3, then F7.

Simple page numbers without a header or footer are set with **Shift-F8**, **2**, **6**, **4** Page Number Position. See [Formatting](Formatting).

### Editing a header or footer

1. Put the cursor after the header code (for example anywhere below it in the document).
2. Press **Shift-F8**, **2**, **3** (or **4**), choose **A** or **B**, then **5** (Edit).
3. Change the text and press **F7**.

The last such header or footer **before the cursor** is edited. If there is none, the message `ERROR: Header A not found` (or similar) appears.

### Stopping a header or footer

At the page where it should stop, press **Shift-F8**, **2**, **3** (or **4**), choose **A** or **B**, then **1** (Discontinue). A code such as **[Header A:Discontinue]** is inserted.

### Leaving out headers and footers on one page

To leave out the header, footer or page number on a single page, for example the first page of a letter:

1. Go to the top of that page.
2. Press **Shift-F8**, **2** (Page), **8** (Suppress (this page only)).
3. Answer **Y** for what to suppress: 1 all page numbering, headers and footers; 2 headers and footers; 3 print the page number at bottom centre instead; 4 page numbering; 5 Header A; 6 Header B; 7 Footer A; 8 Footer B.
4. Press **F7**.

### Headers, footers, notes and Word

Headers, footers, footnotes and endnotes are saved as their Word equivalents, and those in Word documents you open appear here as these codes. Odd and even page headers become Word's "different odd and even pages" headers.

### Searching in headers, footers and notes

Search (F2) looks only in the main text. *Extended search* (Home, F2) is accepted but, in this version, also searches only the main text. To find text in a note or header, open it for editing and search there.
