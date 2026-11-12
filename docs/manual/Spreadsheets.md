# Spreadsheets

**Ctrl-F5**, **5** (Spreadsheet) brings figures from a spreadsheet into your document. It works as in WordPerfect 5.1, but reads today's files: **Excel workbooks (.xlsx)** and **CSV** or tab-separated text files.

Press **Ctrl-F5**, **5**:

```
Spreadsheet: 1 Import; 2 Create Link; 3 Edit Link; 4 Link Options: 0
```

- **Import** copies the data into the document once. Later changes in the spreadsheet do not affect your document.
- **Create Link** copies the data too, but remembers where it came from, so you can update it later when the spreadsheet changes.

## Importing data

1. Put the cursor where the data should go.
2. Press **Ctrl-F5**, **5**, **1** (Import). The *Spreadsheet: Import* screen shows:

```
1 - Filename
2 - Range                (All)
3 - Type                 Table
4 - Perform Import
```

3. Type the spreadsheet's file name (with its folder if needed) and press **Enter**.
4. If you want only part of it, press **2** and type the **Range** (see below). Leave it empty for the whole first sheet.
5. Press **3** to choose the **Type**: **1 Table** (the data becomes a table, one cell per spreadsheet cell — the standard) or **2 Text** (the columns are separated by tabs, one line per row).
6. Press **4** (Perform Import).

## Creating a link

Do the same as for Import, but choose **Ctrl-F5**, **5**, **2** (Create Link), and at the end **4** (Perform Link).

The data is placed between two codes: **[Link:file;range]** at the start and **[Link End]** at the end. You can see them in Reveal Codes.

Do not edit the text between the link codes: it is replaced the next time the link is updated.

## Updating links

When the spreadsheet has changed:

1. Press **Ctrl-F5**, **5**, **4** (Link Options).
2. Press **1** (Update All Links).

Every link in the document is read again from its file.

## Editing a link

To change the file, range or type of a link:

1. Put the cursor inside the linked data.
2. Press **Ctrl-F5**, **5**, **3** (Edit Link).
3. Change the settings and press **4** (Perform Link).

If the cursor is not inside a link, the message `ERROR: Not in a spreadsheet link` appears.

## Ranges

| You type | You get |
|---|---|
| *(nothing)* | The whole first sheet (empty rows and columns at the end are left out) |
| `A1:D10` | Cells A1 to D10 of the first sheet |
| `B3` | Only cell B3 |
| `Sheet2` | The whole sheet named Sheet2 |
| `Sheet2!B2:C9` | Cells B2 to C9 of Sheet2 |
| a defined name | A named range from the workbook |

`A1..D10` (the old WordPerfect way) also works.

## What is read

- **Excel (.xlsx):** the values of the cells as Excel last calculated and displayed them. Formulas themselves are not taken over, only their results.
- **CSV:** comma- or semicolon-separated text files, as written by Excel and most other programs.
- **Tab-separated text** (.tsv, .txt).

Older Excel files (.xls) and other spreadsheet formats are not read. Save them as .xlsx or CSV in Excel or LibreOffice first.

## Other ways to use spreadsheet data

- **Merge:** an .xlsx or CSV file can be the data file (secondary file) for form letters. See [Merge and Sort](Merge-and-Sort).
- **Clipboard:** you can also copy cells in Excel and paste them with **Shift-Ins**.
