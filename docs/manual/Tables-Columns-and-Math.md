# Tables, Columns and Math

**Alt-F7** (Columns/Table) offers:

```
1 Columns; 2 Tables; 3 Math: 0
```

When the cursor is already inside a table, Alt-F7 goes straight to **Table Edit**.

## Tables

### Creating a table

1. Put the cursor where the table should go (it always starts on its own line).
2. Press **Alt-F7**, **2** (Tables), **1** (Create).
3. Type the `Number of Columns:` (1 to 32) and press **Enter**.
4. Type the `Number of Rows:` and press **Enter**.

The table is created across the full width between the margins, with equal columns, and **Table Edit** opens (see below). Press **F7** to leave Table Edit and type into the cells.

### Typing in a table

- Type into a cell as usual. Text wraps inside the cell; Enter starts a new line inside the cell.
- **Tab** moves to the next cell; in the last cell of a row it goes to the first cell of the next row, and in the very last cell it leaves the table. **Shift-Tab** moves to the previous cell.
- **Alt + arrow keys** move to the neighbouring cell in that direction.
- The status line shows the cell address, for example *Cell B3* (column B, row 3), in Table Edit.

Codes: **[Tbl Def:…]** starts the table, **[Row]** and **[Cell]** start rows and cells, **[Tbl Off]** ends it. Rows and cells cannot be deleted with Del in the text; use Table Edit. Deleting the [Tbl Def] code asks `Delete table structure (text remains)?` — answer Y to turn the table back into plain text.

### Table Edit

Put the cursor in the table and press **Alt-F7** (or Alt-F7, 2, 2 Edit). The status line shows `Table Edit: Press Exit when done` and the cell address. The bottom lines show:

```
Ctrl-Arrows Column Widths; Ins Insert; Del Delete; Move Copy;
1 Size; 2 Format; 3 Lines; 4 Header; 5 Math; 6 Options; 7 Join; 8 Split: 0
```

| Key | Action |
|---|---|
| **Arrow keys**, **Tab**, **Shift-Tab** | Move from cell to cell. |
| **Ctrl-Right** / **Ctrl-Left** | Make the current column wider / narrower (by 0.1" each time). |
| **Ins** | Insert rows or columns: choose 1 Rows or 2 Columns, then type how many. They are inserted at the cursor. |
| **Del** | Delete rows or columns: choose 1 Rows or 2 Columns, then type how many, starting at the cursor. |
| **1 Size** | Set the total number of rows or columns. Rows and columns are added or removed at the end of the table. |
| **2 Format** | Justification of 1 Cell or 2 Column: Left, Center, Right or Full. |
| **5 Math** | Table formulas (see below). |
| **F7**, **Esc**, **Alt-F7** | Leave Table Edit. |

*Not available in this version:* 3 Lines, 4 Header, 6 Options, 7 Join, 8 Split, and Move/Copy inside Table Edit. Tables imported from Word or WordPerfect files keep their own lines and joined cells when saved.

### Table math

In Table Edit, press **5** (Math):

```
Math: 1 Calculate; 2 Formula; 3 Copy Formula; 4 +; 5 =; 6 *
```

- **2 Formula** — type a formula for the current cell, for example `B2*C2` or `A1+A2+A3` or `B4*1.19`. Cells are named by column letter and row number.
- **4 +** — a **subtotal**: adds the numbers above it in the column (up to the previous subtotal).
- **5 =** — a **total**: adds the subtotals above it.
- **6 \*** — a **grand total**: adds the totals above it.
- **3 Copy Formula** — copies the current cell's formula to 1 Cell (type the destination, for example `D5`), 2 Down or 3 Right (type how many times). Cell names in the formula are adjusted, as in a spreadsheet.
- **1 Calculate** — calculates all formulas in the table.

As in WordPerfect 5.1, formulas are calculated strictly **from left to right**: `A1+B1*2` means (A1+B1)×2. Use parentheses to change the order. A comma or a point can be used as decimal separator in numbers.

## Text columns

Text columns are defined once and then switched on and off.

### Defining columns

1. Put the cursor where the columns should start.
2. Press **Alt-F7**, **1** (Columns), **3** (Define). The *Text Column Definition* screen shows:

```
1 - Type                         Newspaper
2 - Number of Columns            2
3 - Distance Between Columns     0.5"
4 - Margins
     Column    Left      Right
       1       1"        4"
       2       4.5"      7.5"
```

3. Change what you need:
   - **1 Type**: 1 Newspaper, 2 Parallel, 3 Parallel with Block Protect;
   - **2 Number of Columns**: the margins are recalculated to equal widths;
   - **3 Distance Between Columns**;
   - **4 Margins**: set the left and right edge of each column yourself.
4. Press **F7** (Exit). A **[Col Def:…]** code is inserted and the `Columns: 1 On; 2 Off; 3 Define` menu appears again. Press **1** to switch the columns on right away.

### Switching columns on and off

- **Alt-F7**, **1**, **1** (On) — inserts **[Col On]**. From here, text is set in columns.
- **Alt-F7**, **1**, **2** (Off) — inserts **[Col Off]**. Text after it is normal again.

If you choose On without a definition, you get `ERROR: No text columns defined`.

### Newspaper columns

Text runs down the first column to the bottom of the page, then continues at the top of the next column, and so on, like a newspaper. **Ctrl-Enter** ends the current column early (a column break); at the last column it starts a new page.

### Parallel columns

Parallel columns keep related text side by side in rows, for example a term on the left and its translation on the right, or a script with speakers and lines.

1. Type the text of the first column.
2. Press **Ctrl-Enter** to go to the next column.
3. In the last column, **Ctrl-Enter** starts a new row (a new group of columns).

"Parallel with Block Protect" additionally keeps each row together on one page.

### Moving between columns

| Keys | Moves to |
|---|---|
| **Ctrl-Home**, **Right arrow** | The next column |
| **Ctrl-Home**, **Left arrow** | The previous column |
| **Ctrl-Home**, **Home**, **Right arrow** | The last column |
| **Ctrl-Home**, **Home**, **Left arrow** | The first column |

The status line shows *Col 1*, *Col 2*… while the cursor is in columns. On screen the columns are shown side by side.

## Math columns (Alt-F7, 3)

Math columns calculate sums in tab-separated columns of figures, the classic WordPerfect 5.1 way (for invoices, simple accounts).

```
Math: 1 On; 2 Off; 3 Define; 4 Calculate
```

### Defining math columns

Press **Alt-F7**, **3**, **3** (Define). The *Math Definition* screen shows columns **A** to **X** (one per tab stop). Use the **arrow keys** to move and type the settings:

- **Type**: **0** = Calculation (a formula), **1** = Text, **2** = Numeric (the standard), **3** = Total;
- **Negative Numbers**: **(** = shown in parentheses (50.00), **-** = with a minus sign -50.00;
- **Number of Digits to the Right**: 0 to 4 decimal places;
- **Calculation Formulas**: for up to 4 calculation columns, type the formula (for example `B*C`, using column letters). The special formulas `+`, `=`, `+/` and `=/` add up or average the numeric columns in the row.

Press **F7** when done. A **[Math Def]** code is inserted.

### Using math columns

1. Press **Alt-F7**, **3**, **1** (On) — inserts **[Math On]**; the status line shows *Math*.
2. Type each row: press **Tab** to go to the next column, type the number.
3. Where a sum should appear, type one of these operators instead of a number:

| Operator | Meaning |
|---|---|
| **+** | Subtotal of the numbers above |
| **=** | Total of the subtotals above |
| **\*** | Grand total of the totals above |
| **t** | Marks a number as an extra subtotal |
| **T** | Marks a number as an extra total |
| **N** | Makes the following number negative |
| **!** | In a calculation column: calculate the column formula here |

4. Press **Alt-F7**, **3**, **4** (Calculate). The results appear.
5. Press **Alt-F7**, **3**, **2** (Off) at the end — inserts **[Math Off]**.

After changing numbers, calculate again.

## Sorting a table

Tables, lines and paragraphs can be sorted with **Ctrl-F9**. See [Merge and Sort](Merge-and-Sort).

## Spreadsheet data as a table

Excel and CSV data can be brought in as a table with **Ctrl-F5**, **5**. See [Spreadsheets](Spreadsheets).
