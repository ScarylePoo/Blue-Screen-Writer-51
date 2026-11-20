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

Before anything is asked, every recovery copy is also stored in the revision history ("Crash recovery copy"), so whatever you answer, the text can still be found with Ctrl-F5, 6.

If all nine documents are already in use, each further copy you accept opens in a new program window of its own.

Recovery copies are kept in `%APPDATA%\Blue Screen Writer 51\Recovery`, in one folder per open program window. A normal save, or leaving a document with "don't save", removes its recovery copy. Only copies of windows that are no longer running are offered: if you start the program a second time (for example by double-clicking a document while it is already open), the second window does not offer the first window's copies.

If no recovery copy can be written (for example because the disk is full), the status line says so once; save your work with F10.

## Two windows, one file

If you open a file that is already open in another Blue Screen Writer 51 window, or in another document of the same window (Doc 1 to Doc 9), the status line warns you. Saving in both places would overwrite one set of changes with the other, so close one of them.

## Automatic saving

A document that already has a name and is a **Word document (.docx)** is saved to its own file automatically **every 5 minutes** while it has changes. The status line briefly shows `* Autosaved *`.

Because every save is also kept as a revision, automatic saving can always be undone: the previous version is in the revision list.

To change the number of minutes, or to switch automatic saving off (0 minutes):

1. Press **Shift-F1** (Setup), **3** (Environment), **1** (Backup Options).
2. Press **4** (Autosave Word documents) and type the minutes, then **Enter**.

**Documents without a name** are saved automatically too, as a Word file in the folder **Blue Screen Writer 51 - Unsaved** inside your home directory (Setup, Location of Files, Documents) or, if none is set, inside your Documents folder. The file is called `BSW-Unsaved-` followed by a six-character code, for example `BSW-Unsaved-3F9A2C.docx`, so in Explorer you can always tell what it is and open it by double-clicking. The document still counts as unsaved: F7 still asks for a name. As soon as you save it under a name, its BSW-Unsaved file is deleted. If you leave it deliberately without saving, the file is kept, with the text exactly as you left it (it is also in the revision history: Ctrl-F5, 6 on an empty screen). Delete such files in Explorer or F5 List Files when you no longer need them.

Documents in **other formats** (for example a .txt, .doc or .wpd file) are not written to their own file automatically, because that could change the file's format. Instead, their unsaved changes are stored in the revision history every few minutes ("Unsaved changes (kept)"); the file itself is untouched until you save.

Automatic saving never overwrites a newer file. If the file on disk was saved by another program or window since you opened it (or the document came back from a crash), the status line says `Note: Letter.docx is not autosaved — the file on disk is a different version. Save with F10.` Saving with F10 then asks `Replace …? (it was changed elsewhere since opened here)`; after that, automatic saving continues. If automatic saving fails (for example because the file is locked), an error is shown once.

## Timed backup

In addition (as in WordPerfect 5.1), changed documents are copied to the **backup folder** every **10 minutes**, as `BSW51.BK1.docx` (Doc 1), `BSW51.BK2.docx` (Doc 2) and so on. These files are overwritten each time.

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

If the document has a name but has never been saved here, the status line says `No revisions yet — they are kept every time the document is saved.`

In an **unnamed** document (for example an empty screen), Ctrl-F5, 6 instead lists **Documents that were never saved**: texts left without saving or declined after a crash, newest first, with their first words. The same choices (Look, Retrieve, Compare, Restore) bring them back.

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
| Text before a restore | The text you had just before choosing 4 Restore. |
| Crash recovery copy | A recovery copy found at start-up after a crash. |
| Unsaved changes (kept) | Changes that automatic saving kept in the history without writing the file (other formats, or the file was changed elsewhere). |

Move with the **arrow keys** or **Page Up/Down**, or click with the mouse. Then choose:

1. **Look** (or **Enter**, or a double-click) — shows the text of that version, read only. Any key except the arrow and page keys returns to the list.
2. **Retrieve as new document** — opens that version in the first document that is not in use (normally Doc 2), as a new, unnamed document. Use it to copy a paragraph back. **Shift-F3** switches between the two. If all nine documents are in use, you get an error message instead; leave one with F7 first.
3. **Compare with current** — marks in your current text what changed since that version: added text as tracked insertions, removed text as tracked deletions. Accept or reject the differences with **Alt-F5**, **7** (see [Track Changes and Comments](Track-Changes-and-Comments)), or undo the marking with **Ctrl-Z**.
4. **Restore** — asks `Replace the current text with this version? No (Yes)`. **Y** puts that version on the screen in place of your current text. Nothing is lost: all newer versions stay in the list, and **Ctrl-Z** brings back the text you had before. The text you had is also kept in the list as "Text before a restore". Save afterwards to keep the restored version.

**Esc**, **F7**, **Space** or **0** leave the list.

### Revisions survive editing in Word

The history belongs to the document itself, not to its file name: a hidden identification number is stored inside the .docx file (in its document properties, which Word keeps). So:

- the history stays with the document even if you rename or move the file;
- files that can't store the number (.txt, .rtf, .doc, .wpd …, or Word files that were never saved here) find their history by their file name;
- if someone edits the document in Word in between, the changes made there are recorded as their own version ("Changed outside") when you open it again.

Saving under another name (F10 with a new name) makes a separate document with its own new history, from that save on; the earlier versions stay with the original file. A copy made in Explorer is recognised the same way when you open it while the original still exists. A version retrieved as a new document (choice 2) also gets its own new history.

### Where revisions are kept

In `%APPDATA%\Blue Screen Writer 51\History`, one folder per document. Back up this folder if you want to keep the history when you move to another computer.

## Leaving without saving

If you leave a document with **F7**, **N** (do not save), the text as it was is still recorded in the revision history as "Left without saving". Open the document again and use Ctrl-F5, 6 to get it back. For a document that never had a name, press Ctrl-F5, 6 on an empty screen: it is in the list of documents that were never saved.
