# Editing Basics

This page covers everyday typing and editing: moving around, deleting, Undo, Block, Move and Copy, Search and Replace, and working with two documents.

Keys are given for the standard **Alternate keyboard** (Esc = Cancel, F1 = Help, F3 = Repeat). See [The Screen and Keyboard](The-Screen-and-Keyboard) if you use the original keyboard.

## Typing

Just type. Text wraps to the next line by itself at the right margin; press **Enter** only at the end of a paragraph. Enter inserts a hard return code **[HRt]**; the automatic line ends are soft returns **[SRt]**.

| Key | Effect | Code |
|---|---|---|
| **Enter** | End of paragraph | [HRt] |
| **Ctrl-Enter** | New page (hard page break) | [HPg] |
| **Tab** | Move to the next tab stop | [Tab] |
| **Shift-Tab** | Margin release (move left one tab stop) | [←Mar Rel] |
| **Home**, **Space** | Hard space: the two words are never split at a line end | [ ] |
| **Home**, **-** (minus) | Hard hyphen: a hyphen where the line is never broken | |
| **Ctrl-minus** | Soft hyphen: a place where a word may be hyphenated | |
| **Home**, **/** | Do not hyphenate the next word | [/] |
| **Home**, **Enter** | Invisible soft return: a line break point without a hyphen | [ISRt] |

**Insert and Typeover.** Normally, typed text is inserted and the rest of the line moves to the right. Press **Ins** to switch to **Typeover**: typed text replaces what is there. The word *Typeover* is shown on the status line. Press **Ins** again to go back.

**Repeat.** Press **F3** (Repeat; Esc on the original keyboard). The status line shows `Repeat Value = 8`. Type a number if you want a different count, then press a key or type a character: it is repeated that many times. Examples:

- **F3**, **20**, **Down arrow** moves the cursor down 20 lines;
- **F3**, **40**, **-** types 40 hyphens;
- **F3**, **Del** deletes 8 characters.

The standard value 8 can be changed in **Shift-F1**, **4** Initial Settings, **6** Repeat Value.

## Moving the cursor

| Key | Moves the cursor |
|---|---|
| **Left** / **Right arrow** | One character |
| **Up** / **Down arrow** | One line |
| **Ctrl-Left** / **Ctrl-Right** | One word left / right |
| **Ctrl-Up** / **Ctrl-Down** | One paragraph up / down |
| **End** | To the end of the line |
| **Home**, **Left** | To the start of the line |
| **Home**, **Right** | To the end of the line |
| **Home**, **Up** | To the top of the screen; again: up one more screen |
| **Home**, **Down** | To the bottom of the screen; again: down one more screen |
| grey **-** (on the number pad) | Screen up (same as Home, Up) |
| grey **+** (on the number pad) | Screen down (same as Home, Down) |
| **Page Up** / **Page Down** | To the top of the previous / next page |
| **Home**, **Home**, **Up** | To the beginning of the document |
| **Home**, **Home**, **Down** | To the end of the document |
| **Home**, **Home**, **Home**, **Up** | To the very beginning, before all codes |
| **Home**, **Home**, **Home**, **Left** | To the start of the line, before all codes |
| **Alt-Left/Right/Up/Down** | In a table: to the neighbouring cell. Elsewhere: word left/right, paragraph up/down. |

### Go To (Ctrl-Home)

Press **Ctrl-Home**. The status line shows `Go to`. Then:

| Then press | Effect |
|---|---|
| a page number and **Enter** | Jumps to the top of that page |
| a character (for example **.**) | Jumps to just after the next occurrence of that character |
| **Up arrow** | Top of the current page |
| **Down arrow** | Bottom of the current page |
| **Right** / **Left arrow** | In text columns: the next / previous column |
| **Home**, then **Right** or **Left** | In text columns: the last / first column |

### Mouse

Click to put the cursor anywhere. The mouse wheel scrolls.

## Deleting

| Key | Deletes |
|---|---|
| **Backspace** | The character to the left of the cursor |
| **Del** | The character at the cursor |
| **Ctrl-Backspace** | The word at the cursor |
| **Home**, **Backspace** | From the cursor to the start of the word |
| **Home**, **Del** | From the cursor to the end of the word |
| **Ctrl-End** | From the cursor to the end of the line |
| **Ctrl-Page Down** | From the cursor to the end of the page (asks first: `Delete Remainder of page?`) |
| **Block** then **Del** or **Backspace** | The marked Block (asks `Delete Block?`) |

**Hidden codes.** When Backspace or Del reaches a hidden code (for example a bold code) in the normal view, the program asks, for example `Delete [BOLD]? No (Yes)`. Press **Y** to delete it, or any other key to step over it. In Reveal Codes the code is deleted without asking, because you can see it. See [Reveal Codes](Reveal-Codes).

## Undelete (Esc)

The program remembers your **last three deletions**. When no menu is open and no Block is on, press **Esc** (Cancel). The most recently deleted text appears at the cursor, highlighted, and the menu shows:

```
Undelete: 1 Restore; 2 Previous Deletion: 0
```

- **1** (or **R**) puts the highlighted text back at the cursor.
- **2** (or **P**, or **Up arrow**) shows the deletion before it; **Down arrow** goes forward again.
- Any other key closes Undelete without changing anything.

Tip: Undelete can be used to move text: delete it, move the cursor, press Esc, 1.

## Undo and Redo (Ctrl-Z, Ctrl-Y)

- **Ctrl-Z** takes back the last change. Press it again to go further back, step by step (up to about a thousand steps). Typing is undone a word or so at a time.
- **Ctrl-Y** puts back what Ctrl-Z took away.

Undo also takes back larger operations, such as Replace, Sort, Restore of a revision, or a merge. When there is nothing left, the status line says `Nothing to undo`.

Undo and Undelete work side by side: Undelete brings back deleted text at the cursor; Undo returns the whole document to how it was.

## Block (Alt-F4 or F12)

A **Block** is a piece of marked text. Many keys work on the Block instead of on the whole document.

1. Put the cursor at one end of the text.
2. Press **Alt-F4** or **F12**. *Block on* blinks on the status line.
3. Move the cursor to the other end. The marked text is highlighted. Shortcuts while Block is on:
   - type a character to extend the Block to its next occurrence (for example **.** extends to the end of the sentence);
   - **Enter** extends the Block to the end of the paragraph;
   - all cursor keys, Page Down, Ctrl-Home and so on work as usual.
4. Press the key for what you want to do.

With the mouse: **drag** across the text, or **double-click** a word.

To switch Block off without doing anything, press **Alt-F4** (or **F12**) again, or **Esc** (Cancel).

### What you can do with a Block

| Key | Effect on the Block |
|---|---|
| **F6** / **F8** | Bold / Underline |
| **Ctrl-F8** | 1 Size or 2 Appearance (italics, double underline, small caps, redline, strikeout…) |
| **Del** or **Backspace** | Delete (asks first) |
| **Ctrl-F4** | Move, copy, delete or append (see below) |
| **Ctrl-Ins** / **Shift-Del** | Copy / cut to the Windows clipboard |
| **Shift-F3** | 1 Uppercase / 2 Lowercase |
| **Shift-F6** | Center the lines (asks `[Just:Center]?`) |
| **Alt-F6** | Align the lines at the right margin (asks `[Just:Right]?`) |
| **Shift-F8** | Protect the Block from being split by a page break (asks `Protect block?`) |
| **F10** | Save only the Block to a file (asks `Block name:`) |
| **Shift-F7** | Print only the Block |
| **Ctrl-F2** | Spell check only the Block |
| **Ctrl-F5** | Turn the Block into a comment |
| **Alt-F5** | Mark the Block for a table of contents, list, index or table of authorities |
| **Alt-F8** | Put a style around the Block |
| **Ctrl-F9** | Sort the Block |
| **Ctrl-F5**, **7** | Document Info counts the Block too |
| **Ctrl-F5**, **8**, **1** | Make the Block a hyperlink |
| **Alt-F5**, **7**, **3/4** | Accept or reject the tracked changes inside the Block |

## Move and Copy (Ctrl-F4)

### Without a Block: sentence, paragraph or page

1. Put the cursor anywhere in the sentence, paragraph or page.
2. Press **Ctrl-F4**. The menu shows `1 Sentence; 2 Paragraph; 3 Page; 4 Retrieve`.
3. Choose **1**, **2** or **3**. The text is highlighted.
4. Choose **1 Move** (cut it out), **2 Copy**, or **3 Delete**.
5. For Move and Copy, the status line says `Move cursor; press Enter to retrieve.` Move the cursor to the new place and press **Enter**. (**Esc** cancels; the text then stays in memory.)

### With a Block

1. Mark the Block (Alt-F4 or F12).
2. Press **Ctrl-F4**, then **1 Block**.
3. Choose **1 Move**, **2 Copy**, **3 Delete** or **4 Append**.
4. For Move and Copy, move the cursor and press **Enter**.

**Append** adds the Block to the end of another document on disk. Type the file name at `Append to:`. If the file does not exist yet, it is created.

### Retrieving the moved text again

**Ctrl-F4**, **4 Retrieve** (then any of 1–3) inserts the last moved or copied text again at the cursor. Use it to make several copies.

Moved and copied text also goes onto the Windows clipboard, so you can paste it into other programs.

*Not available in this version:* the "Tabular Column" and "Rectangle" choices of Ctrl-F4 with a Block, and "Append" for sentence/paragraph/page without a Block.

## The Windows clipboard

| Key | Effect |
|---|---|
| **Ctrl-Ins** | Copy the Block to the Windows clipboard |
| **Shift-Del** | Cut the Block (copy it, then delete it) |
| **Shift-Ins** | Paste text from the clipboard at the cursor (from an e-mail, a web page, Word…). With a Block on, the Block is replaced. |

Formatting such as bold and italics is kept when copying between Blue Screen Writer 51 documents, and as far as possible with other programs.

## Changing upper and lower case

1. Mark a Block.
2. Press **Shift-F3** (Switch).
3. Press **1** (Uppercase) or **2** (Lowercase).

## Search (F2, Shift-F2)

1. Press **F2** to search forward, or **Shift-F2** to search backward.
2. Type the text after `-> Srch:`. The last search text is offered; just start typing to replace it.
3. Press **F2** again (or **Esc**) to start the search.

The cursor stops just after the text found (searching forward) or at its start (searching backward). If nothing is found, `* Not found *` is shown. To find the next occurrence, press **F2**, **F2**.

- **Lowercase** letters find both lower- and uppercase. **Uppercase** letters find only uppercase.
- **Codes** can be searched for by pressing their keys at the prompt: **Enter** gives [HRt], **Tab** [Tab], **F6** [BOLD] (press again for [bold]), **F8** [UND], **F4** [→Indent], **Shift-F6** [Center], **Ctrl-Enter** [HPg]. You can also type a code name in brackets, for example `[HRt]`.
- *Extended search* (**Home**, **F2**, which in 5.1 also looks in headers, footers and notes) is accepted, but in this version it searches only the main text, like F2.

The pull-down menu (Alt-=, Search) also has *Next* and *Previous*.

## Replace (Alt-F2)

1. Press **Alt-F2**. The question `w/Confirm? No (Yes)` appears.
   - **Y**: you confirm each replacement one by one.
   - **N**: everything is replaced at once.
2. Type the text to find after `-> Srch:` and press **F2**.
3. Type the replacement after `Replace with:` and press **F2**.

With confirmation, the cursor stops at each match and asks `Confirm? No (Yes)`: press **Y** to replace this one, any other key to skip it, **Esc** to stop.

Replace works from the cursor to the end of the document; to replace everywhere, go to the beginning first (**Home**, **Home**, **Up**). Codes are entered with their keys, as for Search. Leaving the replacement empty deletes every occurrence. **Ctrl-Z** undoes a replace.

## Two documents (Shift-F3) and split screen (Ctrl-F3)

- **Shift-F3** switches between **Doc 1** and **Doc 2**. Each has its own file, cursor, Undo history and Reveal Codes setting.
- **Ctrl-F3**, **1 Window**: type the number of lines for the current window to see both documents one above the other. Type 0 to go back to one full window.
- **Ctrl-F3**, **3 Rewrite** (or pressing Ctrl-F3 twice) redraws the screen.

## Line Draw (Ctrl-F3, 2)

Line Draw draws boxes and lines with the arrow keys, as in 5.1. The menu at the bottom is:

```
1 │; 2 ║; 3 *; 4 Change; 5 Erase; 6 Move
```

Choose the line type (single, double, or a character of your choice via 4 Change), then move the arrow keys to draw. **5 Erase** removes lines, **6 Move** moves the cursor without drawing. Press **F7** (Exit) when done.

## Center, Flush Right and Indent

These are explained on the [Formatting](Formatting) page: **Shift-F6** Center, **Alt-F6** Flush Right, **F4** Indent, **Shift-F4** Indent from both sides.

## Leaving a document (F7)

Press **F7** (Exit).

1. `Save document? Yes (No)` — press **Y** to save (you can change the name), **N** not to.
2. Then `Exit Blue Screen Writer 51?` (or `Exit doc 1?` when the other document is in use):
   - **Y** closes the program (or this document);
   - **N** clears the screen for a new document and keeps the program open.

Even if you leave without saving, the last state is kept in the document's revision history. See [Safety: Undo, Recovery and Revisions](Safety-Undo-Recovery-and-Revisions).

Clicking the window's close button (X) works like F7.

In a menu, F7 returns to the document. When you edit a footnote, header, comment or macro, F7 finishes the editing.
