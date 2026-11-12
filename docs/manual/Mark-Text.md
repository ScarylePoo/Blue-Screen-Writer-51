# Mark Text

**Alt-F5** (Mark Text) builds tables of contents, lists, indexes and tables of authorities, keeps cross-references up to date, combines master documents, compares documents, and gives access to Track Changes. It works as in WordPerfect 5.1, in three steps:

1. **Mark** the text that should appear (headings, index words…).
2. **Define** where the table, list or index goes and how it looks.
3. **Generate**: the program collects the entries with their page numbers.

Without a Block, **Alt-F5** shows:

```
1 Cross-Ref; 2 Subdoc; 3 Index; 4 ToA Short Form; 5 Define; 6 Generate; 7 Track Changes: 0
```

With a **Block** on, **Alt-F5** shows:

```
Mark for: 1 ToC; 2 List; 3 Index; 4 ToA: 0
```

## Table of contents

### Marking headings

1. Mark the heading as a Block (Alt-F4 or F12, then move to the end of the heading).
2. Press **Alt-F5**, **1** (ToC).
3. Type the level at `ToC Level:` (1 to 5; 1 for main headings, 2 for sub-headings…) and press **Enter**.

The heading is enclosed in **[Mark:ToC,1]** … **[End Mark:ToC,1]**.

Tip: a heading style can contain the marks, so every heading is marked automatically.

### Defining the table of contents

1. Put the cursor where the table of contents should appear (usually on its own page at the beginning; end that page with Ctrl-Enter).
2. Press **Alt-F5**, **5** (Define), **1** (Define Table of Contents). The *Table of Contents Definition* screen appears:
   - **1 Number of Levels** — 1 to 5;
   - **2 Display Last Level in Wrapped Format** — Yes puts the entries of the last level one after another in a paragraph instead of one per line;
   - **3 Page Numbering** — for each level: 1 No page numbers, 2 Page numbers follow entries, 3 (Page numbers) follow entries, 4 Flush right page numbers, 5 Flush right page numbers with leaders (dots).
3. Press **F7**. A **[Def Mark:ToC,…]** code is inserted.

## Lists

Lists collect other marked items, for example a list of figures or of examples. Up to 10 different lists are possible.

1. Mark each item as a Block and press **Alt-F5**, **2** (List). Type the list number (1–10).
2. Put the cursor where the list should appear and press **Alt-F5**, **5** (Define), **2** (Define List). Type the list number, then choose the page number style (1–5 as above).

## Index

### Marking index entries

- **One word:** put the cursor on the word, press **Alt-F5**, **3** (Index).
- **Several words:** mark them as a Block, press **Alt-F5**, **3** (Index).

Then:

1. At `Index heading:` the word or Block is offered. Press **Enter** to accept it, or type another heading.
2. At `Subheading:` type a subheading if you want one (the entry then appears below the heading), or just press **Enter**.

Code: **[Index:Heading;Subheading]**.

### Concordance file

Instead of marking each occurrence, you can make a **concordance file**: an ordinary document with one word or phrase per line. Every occurrence of these words in your document becomes an index entry when you generate.

### Defining the index

1. Put the cursor where the index should appear (usually at the end of the document, on a new page).
2. Press **Alt-F5**, **5** (Define), **3** (Define Index).
3. At `Concordance Filename (Enter=none):` type the name of the concordance file, or press **Enter** for none.
4. Choose the page number style (1–5).

## Table of authorities

A table of authorities lists the cases, laws and other sources cited in a legal document, grouped in up to 16 sections (for example 1 Cases, 2 Statutes).

### Marking the first occurrence (full form)

1. Mark the citation as a Block.
2. Press **Alt-F5**, **4** (ToA).
3. Type the section number (1–16) at `ToA Section Number (Press Enter for Short Form only):`.
4. An editing screen *ToA Full Form* opens with the citation as it will appear in the table. Edit it if needed and press **F7**.
5. At `Short Form:` type a short name for this authority (for example *Smith v. Jones*) and press **Enter**.

### Marking later occurrences (short form)

Put the cursor at the later citation and press **Alt-F5**, **4** (ToA Short Form). The last short form is offered; press **Enter** or type another.

### Defining the table

Press **Alt-F5**, **5** (Define), **4** (Define Table of Authorities), type the section number, and choose **1 Dot Leaders**, **2 Underlining Allowed**, **3 Blank Line Between Authorities** (Yes/No each). Press **F7**. Do this once for each section.

To change the full form later: **Alt-F5**, **5**, **5** (Edit Table of Authorities Full Form) edits the nearest full form before the cursor.

## Cross-references

A cross-reference is text such as "see page 12" or "see figure 3" that is kept correct automatically.

1. Press **Alt-F5**, **1** (Cross-Ref). The *Cross-Reference* screen offers:
   1. **Mark Reference** — the place where the number should be printed ("see page _");
   2. **Mark Target** — the place the reference points to;
   3. **Mark Both Reference and Target**.
2. For a reference, choose what to tie it to: **1** Page Number, **2** Paragraph/Outline Number, **3** Footnote Number, **4** Endnote Number, **5** Graphics Box Number (then the box type).
3. Type the **Target Name** — any name you like, the same for the reference and its target.

For **Mark Both**: put the cursor where the reference goes before you start. After choosing what to tie it to, move the cursor to the target and press **Enter**, then type the target name.

Codes: **[Ref(name):…]** and **[Target(name)]**. Until you generate, an unresolved reference shows `?`.

## Master documents

A master document combines several documents (for example the chapters of a book) so you can number pages, make one table of contents and print them together.

1. In the master document, put the cursor where a chapter belongs and press **Alt-F5**, **2** (Subdoc). Type its file name at `Subdoc Filename:`. A **[Subdoc:name]** code is inserted. Repeat for each chapter.
2. **Expand**: **Alt-F5**, **6** (Generate), **3** (Expand Master Document). The text of each subdocument is brought in between **[Subdoc Start]** and **[Subdoc End]** codes.
3. Work with the whole book: generate, print, check.
4. **Condense**: **Alt-F5**, **6**, **4** (Condense Master Document). Answer `Save Subdocs?` with **Y** to save your changes back to the chapter files, or **N** to discard them. The text is replaced by the [Subdoc] codes again.

File names without a folder are looked for in the master document's folder.

## Generate (Alt-F5, 6)

The *Mark Text: Generate* screen offers:

1. **Remove Redline Markings and Strikeout Text from Document** — after asking, removes all redline marks (the text stays) and all text marked as strikeout (the text goes). Use this when corrections are final.
2. **Compare Screen and Disk Documents and Add Redline and Strikeout** — type the name of the other document (the current file name is offered). Differences are marked as tracked changes, which you can accept or reject with **Alt-F5**, **7**. See [Track Changes and Comments](Track-Changes-and-Comments).
3. **Expand Master Document** — see above.
4. **Condense Master Document** — see above.
5. **Generate Tables, Indexes, Automatic References, etc.** — asks `Existing tables, lists, and indexes will be replaced. Continue? Yes (No)`. Press **Y**.

When generating, the program fills every [Def Mark] … [End Def] area with the entries and their real page numbers, and updates every cross-reference. Because the inserted text can move pages, it repeats the work until everything is correct.

**Generate again** whenever you have changed the text, before printing.

## Track Changes (Alt-F5, 7)

Option 7 opens the Track Changes menu (track on/off, next change, accept, reject, compare with disk document). See [Track Changes and Comments](Track-Changes-and-Comments).

## The pull-down menu

**Alt-=**, **Mark** offers the same functions: Index, Table of Contents, List, Cross-Reference (Reference, Target, Both), Table of Authorities (Mark Short, Mark Full, Edit Full), Define, Generate, Master Documents (Expand, Condense), Subdocument, Track Changes, and Document Compare (Compare with Disk Document, Accept All Changes, Reject All Changes).
