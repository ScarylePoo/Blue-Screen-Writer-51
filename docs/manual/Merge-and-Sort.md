# Merge and Sort

**Merge** fills a document (a form letter, envelope or label) with data from a list, for example addresses. **Sort** puts lines, paragraphs, table rows or address records into order and can pick out records that match a condition.

Both work as in WordPerfect 5.1, with the full 5.1 merge language. In addition, the data can come from an **Excel (.xlsx)** or **CSV** file.

## Merge: the two files

- The **primary file** is the form: the letter with placeholders (merge codes) such as `{FIELD}Name~`.
- The **secondary file** is the data: one record per person (or item), each with several fields.

### Secondary files

A secondary file can be:

1. **A WordPerfect-style data document.** Each field ends with **{END FIELD}** (press **F9**), each record ends with **{END RECORD}** (press **Shift-F9**, **2**). For example:

```
Mr John Smith{END FIELD}
12 High Street{END FIELD}
London{END FIELD}
{END RECORD}
==================== (page break)
Ms Anna Brown{END FIELD}
...
```

   Fields are referred to by number (1, 2, 3…). Optionally, the first record can be a **{FIELD NAMES}** command listing names for the fields, so you can write `{FIELD}City~` instead of `{FIELD}3~`.

2. **A CSV or tab-separated text file**, or an **Excel workbook (.xlsx)** — the first row holds the field names, each further row is a record. This is often the easiest way: keep your addresses in Excel.

### Making the primary file

1. Start a new document and type the letter.
2. Where a field should go, press **Shift-F9** (Merge Codes), **1** (Field), and type the field number or name at the prompt. `{FIELD}Name~` appears.
3. Save the primary file.

Example:

```
{DATE}

{FIELD}Name~
{FIELD}Street~
{FIELD}City~

Dear {FIELD}Name~,
...
```

### Shift-F9 Merge Codes

```
1 Field; 2 End Record; 3 Input; 4 Page Off; 5 Next Record; 6 More: 0
```

| Choice | Inserts |
|---|---|
| **1 Field** | `{FIELD}name~` — the contents of a field of the current record. |
| **2 End Record** | `{END RECORD}` and a page break — the end of a record in a secondary file. |
| **3 Input** | `{INPUT}message~` — the merge stops and shows the message at the bottom of the screen. Type the text for that place and press **Enter** to continue. |
| **4 Page Off** | `{PAGE OFF}` — no page break between the merged letters (for lists). |
| **5 Next Record** | `{NEXT RECORD}` — moves on to the next record. |
| **6 More** | A box with **all** merge commands. Type the start of a name to jump there, or use the arrow keys; press **Enter** to insert. You are asked for each parameter. |

**F9** (End Field) inserts `{END FIELD}`.

The commands available under **6 More** are: ASSIGN, BELL, BREAK, CALL, CANCEL OFF, CANCEL ON, CASE, CASE CALL, CHAIN MACRO, CHAIN PRIMARY, CHAIN SECONDARY, CHAR, COMMENT, CTON, DATE, DOCUMENT, ELSE, END FIELD, END FOR, END IF, END RECORD, END WHILE, FIELD, FIELD NAMES, FOR, FOR EACH, GO, IF, IF BLANK, IF EXISTS, IF NOT BLANK, INPUT, KEYBOARD, LABEL, LEN, LOCAL, LOOK, MID, MRG CMND, NEST MACRO, NEST PRIMARY, NEST SECONDARY, NEXT, NEXT RECORD, NTOC, ON CANCEL, ON ERROR, PAGE OFF, PAGE ON, PRINT, PROCESS, PROMPT, QUIT, RETURN, RETURN CANCEL, RETURN ERROR, REWRITE, STATUS PROMPT, STEP OFF, STEP ON, SUBST PRIMARY, SUBST SECONDARY, SYSTEM, TEXT, VARIABLE, WAIT, WHILE.

They work as described in the WordPerfect 5.1 documentation. For example, `{IF NOT BLANK}Company~{FIELD}Company~{END IF}` prints the company line only when the field is filled.

In Reveal Codes, merge commands appear as **[Mrg:FIELD]** and so on; on the normal screen as `{FIELD}`.

### Running a merge (Ctrl-F9, 1)

1. Press **Ctrl-F9** (Merge/Sort), **1** (Merge).
2. Type the name of the primary file at `Primary file:` and press **Enter**. (Leave it empty and press Enter to use the document on the screen as the primary file.)
3. Type the name of the secondary file at `Secondary file:` and press **Enter**. (Leave it empty if the primary file needs no data, for example a form filled with {INPUT}.)

The status line shows `* Merging *`. The result is one letter per record, separated by page breaks:

- if you gave a primary file name, the result is inserted at the cursor — so start in an empty document (clear the screen with F7, N, N, or use Doc 2);
- if you used the document on the screen as the primary file, it is replaced by the result (**Ctrl-Z** brings it back).

Check the result, then print it (Shift-F7, 1) or save it under a new name.

Pressing **Esc** (Cancel) at an {INPUT} prompt stops the merge.

### Old merge codes

Primary and secondary files from WordPerfect 5.0 and 4.2 use codes like `^F1^` and `^E`. Open such a file and press **Ctrl-F9**, **3** (Convert Old Merge Codes) to turn them into the 5.1 commands.

### Envelopes and labels

- **Envelopes:** make a primary file with an *Envelope* paper size (Shift-F8, 2, 7) and the address fields.
- **Labels:** make a primary file with a *Labels* paper definition and the address fields. Each record of the merged result is one page, and each page is printed on one label.

## Sort (Ctrl-F9, 2)

### Starting a sort

- **To sort part of the document:** mark it as a Block (for example the list of names), then press **Ctrl-F9**.
- **To sort a whole document or file:** press **Ctrl-F9**, **2** (Sort). At `Input file to sort: (Screen)` press **Enter** for the document on the screen, or type a file name. At `Output file for sort: (Screen)` press **Enter** to put the result on the screen, or type a file name.

If the cursor is in a table when you press **Ctrl-F9** with Block on, the table's rows are sorted.

### The Sort screen

The upper part of the screen shows your text; the lower part shows the sort settings:

- **Keys** — what to sort by (up to 9 keys). Each key has a type (**a** = alphanumeric, **n** = numeric) and a position: field, line and word (counted from the start; a negative number counts from the end, so word -1 is the last word, for example a surname).
- **Select** — an optional condition to pick out records.
- **Action** — Sort, Select Only, or Select and Sort.
- **Order** — Ascending or Descending.
- **Type** — what a record is: Line, Paragraph, Merge (records of a secondary merge file), or Table (rows).

The menu at the bottom:

```
1 Perform Action; 2 View; 3 Keys; 4 Select; 5 Action; 6 Order; 7 Type: 0
```

| Choice | What it does |
|---|---|
| **1 Perform Action** | Sorts (and/or selects) now. |
| **2 View** | Lets you scroll the text in the upper part; F7 returns. |
| **3 Keys** | Edit the keys: use the arrow keys to move, type **a** or **n** for the type and the numbers for field, line and word. Press **F7** when done. |
| **4 Select** | Type a selection statement (see below). Press **F7** when done. |
| **5 Action** | 1 Select and Sort, 2 Select Only. |
| **6 Order** | 1 Ascending, 2 Descending. |
| **7 Type** | 1 Merge, 2 Line, 3 Paragraph. |

Sorting can be undone with **Ctrl-Z**.

**Fields:** in a line sort, fields are separated by tabs; in a merge sort they are the {END FIELD} fields; in a table sort they are the cells.

**Alphabetical order** follows German and international rules: ä is sorted like a, ö like o, ü like u, ß like ss; accents are ignored; upper and lower case only decide between otherwise equal words.

### Selecting records

A selection statement uses the keys as `key1`, `key2`… and compares them:

| Operator | Meaning |
|---|---|
| `=` `<>` `>` `<` `>=` `<=` | Equal, not equal, greater, less, greater or equal, less or equal |
| `*` | AND (both conditions) |
| `+` | OR (either condition) |
| `keyg` | Any word in the record (global) |

Examples:

- `key1=London` — only records whose key 1 is London;
- `key2>=1000 * key2<5000` — key 2 between 1000 and 4999;
- `keyg=urgent` — records that contain the word "urgent" anywhere.

Set **5 Action** to *Select Only* to keep the original order, or *Select and Sort* to sort the selected records as well. Records that do not match are removed from the result, so sort into a new output file (or use Ctrl-Z) if you need the original.
