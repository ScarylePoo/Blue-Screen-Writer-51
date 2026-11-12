# Files and Formats

This page explains how to open (retrieve), save and manage documents: F10 Save, Shift-F10 Retrieve, F7 Exit, F5 List Files, Ctrl-F5 Text In/Out, the file formats the program reads and writes, and passwords.

## File names and folders

File names can be as long as you like and may contain spaces, umlauts and accents, for example *Contract Miller 2026 – final.docx*. You do not need 8.3 names any more.

Folders work the same way: *Documents*, *Desktop*, mapped network drives such as *S:*, and network paths such as `\\server\share\Translations` all work. A name without a folder is taken from the **current folder**, which you set with F5 (see below) or permanently in **Shift-F1**, **6** Location of Files, **7** Documents.

## Saving: documents are Word files

**F10** (Save) and **F7** (Exit) save your document as a **Word document (.docx)**. This has big advantages:

- anyone can open it directly in Word, LibreOffice, or on a phone;
- you can open .docx files that others send you, and send them back;
- everything you do here is kept: every code you see in Reveal Codes (styles, tables, footnotes, headers, pictures, comments, tracked changes…) is stored in the file and comes back exactly, even after the file was edited in Word.

### Save (F10)

1. Press **F10**. The prompt shows `Document to be saved:` with the current name.
2. Press **Enter** to keep the name, or type a new name (with or without a folder) and press **Enter**.
3. If a file with that name exists, you are asked `Replace <name>? No (Yes)`. Press **Y** to replace it, **N** to type another name.

If you type a name **without an extension**, `.docx` is added. If you type another extension, the document is saved in that format: `.txt` (plain text), `.rtf`, `.odt`, `.ott`, `.fodt`, `.htm`/`.html`, `.md` (Markdown) or `.wps` (the program's own code-stream format). Any other extension gives a Word document.

If you opened an old or foreign file (for example *LETTER.WP5*, *REPORT.DOC* or *notes.rtf*), F10 offers the same name ending in **.docx**. The old file stays untouched next to it.

**With a Block on**, F10 saves only the Block. You are asked for a `Block name:`.

Every save is also kept in the document's revision history (Ctrl-F5, 6) and the previous file is kept as a `.bk!` backup. See [Safety: Undo, Recovery and Revisions](Safety-Undo-Recovery-and-Revisions).

### Exit (F7)

Press **F7**. You are asked `Save document? Yes (No)`, then whether to leave the program (`Exit Blue Screen Writer 51?`) or, when both documents are in use, the current document (`Exit doc 1?`). Answering **N** to the second question clears the screen for a new document.

## Retrieving (opening) documents

### Shift-F10 Retrieve

1. Press **Shift-F10**. The prompt shows `Document to be retrieved:`.
2. Type the name (and folder if needed) and press **Enter**.

If a document is already on the screen, you are asked `Retrieve into current document? No (Yes)`. **Y** inserts the file at the cursor; **N** leaves things as they are. To open a document on its own, first clear the screen (F7, N, N) or switch to the other document (Shift-F3).

### Recent documents

At `Document to be retrieved:`, press the **Up arrow**. It shows the last document you used; press **Up** again for the one before, up to the last **9** documents. **Down** goes back. Press **Enter** to open the one shown.

### Opening a document from Windows

Start the program with a file (drag the file onto `bsw51.exe`), or register the program as the Windows program for documents (**Shift-F1**, **7**, see [Installation and Starting](Installation-and-Starting)). Then a double-click in Explorer or in an e-mail opens the document directly.

## F5 List Files

1. Press **F5**. The prompt shows `Dir C:\Users\…\Documents\*.*`: your **home directory** if you set one (Shift-F1, 6 Location of Files, 7 Documents), otherwise the current folder. With a home directory, F5 always starts there, wherever you looked last; just edit the path to look elsewhere this time.
2. Press **Enter** to list this folder, or type another folder or a pattern (for example `S:\Letters\*.docx`) and press **Enter**.
   - To **change the current folder** permanently for this session, type **=** first. The prompt changes to `New directory =`; type the folder and press Enter. With a home directory set, the prompt reads `Home directory =` and changes your home directory (kept for next time).

The list shows all files with their full long names, sizes, dates and times, the parent folder (`..`) and sub-folders. At the top are the folder name, the size of the current document, the free disk space and the number of files.

Move with the **arrow keys** and **Page Up/Down**; **End** goes to the last file and **Ctrl-Home** to the first. You can also click with the mouse. Then choose:

| Key | Action |
|---|---|
| **1** or **R** Retrieve | Opens the highlighted document. (Double-click does the same.) |
| **2** or **D** Delete | Deletes the file (or the marked files) after asking. |
| **3** or **M** Move/Rename | Type a new name or a folder to move it there. |
| **4** or **P** Print | Prints the file without opening it. |
| **5** or **S** Short/Long Display | Switches between a compact and a detailed list. |
| **6** or **L** Look | Shows the text of the file without opening it. **Enter** does the same. On a folder, Enter or 6 lists that folder. |
| **7** or **O** Other Directory | Changes the current folder. |
| **8** or **C** Copy | Copies the file (or the marked files) to another name or folder. |
| **9** or **F** Find | Type a word or phrase; only the files that contain it remain in the list. |
| **N** Name Search | Type the start of a file name; the highlight jumps there. **F2** does the same. |
| **\*** | Marks or unmarks the highlighted file (for Delete or Copy of several files). |
| **Alt-F5** | Marks all files, or unmarks all if some are marked. |
| **Esc**, **F7**, **Space**, **0** | Leaves the list. |

In **Look**, use the **Up/Down arrows** and **Page Up/Down** to read. Any other key returns to the list. (The bottom line shows `Look: 1 Next; 2 Previous`, but in this version these two choices also just return to the list.)

## Which documents can be opened

Shift-F10 and F5 open almost any word-processor file. The program recognises the format from the file's contents, not only from its name, so files without an extension or with the "wrong" extension also open.

| Format | Typical extensions |
|---|---|
| Word 2007 and later | .docx, .dotx, .docm |
| Word 97–2003 | .doc |
| Old Word for DOS and Word for Windows 2 | .doc |
| Word 2003 XML | .xml |
| WordPerfect 4.2, 5.0, 5.1 | .wp, .wp5, .doc, files without an extension |
| WordPerfect 6 and later, WordPerfect for Macintosh | .wpd, .wp6 |
| OpenDocument (LibreOffice, OpenOffice) | .odt, .ott, .fodt |
| Rich Text Format | .rtf |
| HTML web pages | .htm, .html |
| Markdown | .md |
| WordStar | .ws, .wsd and others |
| Microsoft Works | .wps |
| Windows Write | .wri |
| Ami Pro | .sam |
| AbiWord | .abw, .zabw |
| Outlook e-mails | .msg |
| PDF (the text only) | .pdf |
| Plain text in any common encoding | .txt and others |
| Blue Screen Writer 51 code stream | .wps (written by Ctrl-F5, 3, 7) |

**Outlook e-mails (.msg)** are shown with From, To, Date, Subject and the names of the attachments, followed by the message text.

**PDF files**: only the text is taken over, not the layout.

**Plain text** is recognised in UTF-8, UTF-16, Windows-1252, DOS code pages 850/437, Windows-1251 (Cyrillic), and the Chinese, Japanese and Korean encodings GBK, Big5, Shift-JIS and EUC.

**Anything else** opens as text, like in Notepad. For program files or pictures you see the readable text inside them. Saving such a file always makes a new `.docx` next to it; the original is never changed.

Password-protected Word documents ask for the password. See below.

## Ctrl-F5 Text In/Out

Press **Ctrl-F5**:

```
1 Dos Text; 2 Password; 3 Save As; 4 Comment; 5 Spreadsheet; 6 Revisions; 7 Document Info; 8 Link/Bookmark: 0
```

### 1 Dos Text

1. **Save** — saves the document as plain text (Windows line ends), without any formatting. You are asked for the name; the current name with `.txt` is offered.
2. **Retrieve (CR/LF to [HRt])** — inserts a plain text file at the cursor; every line end becomes a hard return.
3. **Retrieve (CR/LF to [SRt] in HZone)** — inserts a plain text file. *In this version this works the same as choice 2.*

### 2 Password

1. **Add/Change** — type the password, then type it again at `Re-Enter Password:`. The next save writes the document **encrypted**, the same way Word does it. Word can open it with the same password, and so can Blue Screen Writer 51.
2. **Remove** — the next save writes the document without a password.

Only Word documents (.docx) are saved encrypted. When you open an encrypted document, you are asked `Enter Password:`.

**Keep your passwords safe.** Without the password, nobody, including the program, can open an encrypted document.

### 3 Save As

Saves a **copy** in another format. The document on the screen keeps its own name.

```
1 Word (.docx); 2 PDF; 3 OpenDocument (.odt); 4 RTF; 5 HTML (e-mail); 6 Text;
7 Blue Screen Writer stream (.wps); 8 Flat ODF (.fodt); 9 ODF Template (.ott)
```

1. Choose the format.
2. For **6 Text**, choose the encoding: **1** UTF-8, **2** Windows-1252 (Western), **3** DOS 850, **4** Windows-1251 (Cyrillic). If some characters do not exist in the chosen encoding, they are written as `?` and the status line tells you how many.
3. Type the name at `Document to be saved:` (the current name with the new extension is offered) and press **Enter**.

| Format | Good for |
|---|---|
| Word (.docx) | Everything; the program's own format. |
| PDF | Invoices, finished translations, anything that must look exactly the same everywhere. Uses the Windows printer "Microsoft Print to PDF". |
| OpenDocument (.odt) | LibreOffice and OpenOffice users. |
| RTF | Very old word processors. |
| HTML | Pasting into e-mails or web pages. |
| Text | Plain text without formatting. |
| .wps stream | A plain-text file containing every code exactly; useful for troubleshooting. |
| Flat ODF, ODF Template | Special OpenDocument variants. |

### 4 to 8

- **4 Comment** — see [Track Changes and Comments](Track-Changes-and-Comments).
- **5 Spreadsheet** — see [Spreadsheets](Spreadsheets).
- **6 Revisions** — see [Safety: Undo, Recovery and Revisions](Safety-Undo-Recovery-and-Revisions).
- **7 Document Info** and **8 Link/Bookmark** — see [Document Information](Document-Information).

## Network drives

Documents on mapped drives (S:, T:…) and on `\\server\share\...` paths can be opened, saved and listed like local files. Saving writes a temporary file first and then replaces the old file, so an interrupted network connection cannot leave a half-written document. Some network shares do not allow this; the program then falls back to writing the file directly.

## Two programs, one file

If a document was changed by another program (for example in Word) since you last saved it here, the program notices when you open it, keeps the outside version as its own revision, and shows `Note: changed by another program since last saved here — revision kept`.
