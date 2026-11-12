# Safety: Undo, Recovery and Revisions

With Blue Screen Writer 51, work is very hard to lose. Several safety nets work together, most of them automatically:

| Safety net | Protects against | Automatic? |
|---|---|---|
| **Undo / Redo** (Ctrl-Z / Ctrl-Y) | A wrong change a moment ago | You press the key |
| **Undelete** (Esc) | Text you deleted | You press the key |
| **Recovery copy** every 30 seconds | A crash or power cut | Yes |
| **Automatic saving** every 5 minutes | Forgetting to save | Yes (named .docx documents) |
| **Revision history** of every save | Wanting an older version back | Yes |
| **Original document backup** (.bk!) | A bad save | Yes |
| **Timed backup** every 10 minutes | Extra copies of unsaved work | Yes |

## Undo and Redo

- **Ctrl-Z** takes back the last change. Press it again to go further back, step by step (up to about a thousand steps).
- **Ctrl-Y** puts back what Ctrl-Z took away.

Undo works for typing, deleting, formatting, Replace, Sort, merges, restoring a revision and so on. The pull-down menu has it too: **Alt-=**, Edit, Undo.

## Undelete

When no menu is open and no Block is on, **Esc** (Cancel; F1 on the original keyboard) shows the last deleted text at the cursor:

```
Undelete: 1 Restore; 2 Previous Deletion: 0
```

**1** puts it back; **2** shows the deletion before it. The last three deletions are kept. See [Editing Basics](Editing-Basics).

## Crash recovery

While a document has unsaved changes, the program quietly writes a **recovery copy every 30 seconds**. The copy is written safely (first to a temporary file, then renamed), so even a power cut in the middle cannot damage it.

If the program or the computer stops unexpectedly, the next start asks:

```
Recover C:\Users\...\Letter.docx (autosaved 11-12-26 02:32p)? Yes (No)
```

(For a document that never had a name, it says *(unnamed document)*.)

- Press **Y**: the document comes back on the screen, marked as changed, and you continue where you stopped. At most about 30 seconds of work are lost. Save it with F10.
- Press **N**: the recovery copy is removed. The text is still kept in the revision history as "Left without saving", so you can still get it back with Ctrl-F5, 6.

Recovery copies are kept in `%APPDATA%\Blue Screen Writer 51\Recovery`. A normal save, or leaving a document with "don't save", removes its recovery copy.

## Automatic saving

A document that already has a name and is a **Word document (.docx)** is saved to its own file automatically **every 5 minutes** while it has changes. The status line briefly shows `* Autosaved *`.

Because every save is also kept as a revision, automatic saving can always be undone: the previous version is in the revision list.

To change the number of minutes, or to switch automatic saving off (0 minutes):

1. Press **Shift-F1** (Setup), **3** (Environment), **1** (Backup Options).
2. Press **4** (Autosave Word documents) and type the minutes, then **Enter**.

Documents that have no name yet, or that are not .docx files (for example a .txt file you are editing), are not saved automatically; they are still protected by the recovery copy.

## Timed backup

In addition (as in WordPerfect 5.1), changed documents are copied to the **backup folder** every **10 minutes**, as `BSW51.BK1.docx` (Doc 1) and `BSW51.BK2.docx` (Doc 2). These files are overwritten each time.

Settings in **Shift-F1**, **3**, **1** Backup Options:

- **1 Timed Document Backup** — Yes/No, and the minutes between backups;
- **3 Backup Directory** — where the files go (standard: `%APPDATA%\Blue Screen Writer 51\Backup`).

## Original document backup (.bk!)

Each time a document is saved over an existing file, the old file is first kept as a copy with `.bk!` added to its name, in the same folder. For example, saving *Letter.docx* keeps the previous file as *Letter.docx.bk!*. Only the most recent previous file is kept.

To get it back, rename the `.bk!` file (for example in F5 List Files, 3 Move/Rename) to a name ending in `.docx`.

To switch this off: **Shift-F1**, **3**, **1**, **2** Original Document Backup, **N**.

## Revisions: every saved version is kept

Every time a document is saved — by you, by automatic saving, or when you leave without saving — the version is kept in the document's **revision history**. Up to 500 versions are kept per document; when there are more, the oldest are removed. A version identical to the previous one is not stored twice.

### Opening the revision list

1. Open the document.
2. Press **Ctrl-F5** (Text In/Out), then **6** (Revisions).

If the document has never been saved, the status line says `No revisions yet — they are kept every time the document is saved.`

You see a list, newest first (dates are shown as month-day-year, as in the 5.1 file list):

```
Revisions of C:\Users\...\Letter.docx

 Date      Time    Words   Kind
 11-12-26  02:35p     812   Saved
 11-12-26  02:30p     790   Autosaved
 11-12-26  11:02a     640   Changed outside (e.g. in Word)
 11-11-26  05:45p     612   Left without saving
 ...
1 Look; 2 Retrieve as new document; 3 Compare with current; 4 Restore: 1
```

The **Kind** column tells you how the version came about:

| Kind | Meaning |
|---|---|
| Saved | You saved with F10 or F7. |
| Autosaved | Automatic saving. |
| Changed outside (e.g. in Word) | The file was changed by another program between two sessions here. |
| Recovered after crash | You recovered this text after a crash. |
| Left without saving | You left the document (or declined a recovery) without saving. |

Move with the **arrow keys** or **Page Up/Down**, or click with the mouse. Then choose:

1. **Look** (or **Enter**, or a double-click) — shows the text of that version, read only. Any key except the arrow and page keys returns to the list.
2. **Retrieve as new document** — opens that version in the **other** document window (Doc 2 if you are in Doc 1), as a new, unnamed document. Use it to copy a paragraph back. **Shift-F3** switches between the two. If the other document has unsaved changes, you get an error message instead; save or clear it first.
3. **Compare with current** — marks in your current text what changed since that version: added text as tracked insertions, removed text as tracked deletions. Accept or reject the differences with **Alt-F5**, **7** (see [Track Changes and Comments](Track-Changes-and-Comments)), or undo the marking with **Ctrl-Z**.
4. **Restore** — asks `Replace the current text with this version? No (Yes)`. **Y** puts that version on the screen in place of your current text. Nothing is lost: all newer versions stay in the list, and **Ctrl-Z** brings back the text you had before. Save afterwards to keep the restored version.

**Esc**, **F7**, **Space** or **0** leave the list.

### Revisions survive editing in Word

The history belongs to the document itself, not to its file name: a hidden identification number is stored inside the .docx file (in its document properties, which Word keeps). So:

- the history stays with the document even if you rename or move the file;
- if someone edits the document in Word in between, the changes made there are recorded as their own version ("Changed outside") when you open it again.

A copy made with Save As to another name keeps the same identification and therefore shares the history. A version retrieved as a new document (choice 2) gets its own new history.

### Where revisions are kept

In `%APPDATA%\Blue Screen Writer 51\History`, one folder per document. Back up this folder if you want to keep the history when you move to another computer.

## Leaving without saving

If you leave a document with **F7**, **N** (do not save), the text as it was is still recorded in the revision history as "Left without saving". Open the document again and use Ctrl-F5, 6 to get it back. (For a document that was never named, the revision cannot be found through a file later; save important new documents early.)
