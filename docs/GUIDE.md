# Blue Screen Writer 51 — What Is New

This guide is for someone who has used WordPerfect 5.1 for DOS for years, most recently inside vDos or DOSBox.

Blue Screen Writer 51 works the way you are used to: the same blue screen, the same function keys, the same menus, and Reveal Codes. Your fingers do not have to learn anything new. Everything in this guide is **extra**. You can ignore all of it and simply keep working as before.

It is a real Windows program, not an emulator. That means:

- no more vDos window, no more "drive C: is a folder somewhere";
- no more 8-character file names;
- printing goes straight to your Windows printer;
- copy and paste work with other Windows programs.

**Note on the keyboard:** the program uses the *Alternate keyboard* that you know: **F1 = Help**, **Esc = Cancel / Undelete**, **F3 = Repeat**. To change this, go to Shift-F1 Setup, 3 Environment, 9 Alternate Keyboard.

## 1. Starting the program

Double-click **bsw51.exe** in the folder *Blue Screen Writer 51*. Keep the folders *dict*, *thes*, *hyph* and *WPM* next to it. They hold the dictionaries, the thesaurus, hyphenation and your macros.

### Opening documents by double-clicking them in Windows

You can make Windows open documents in Blue Screen Writer 51 when you double-click them in Explorer or in an e-mail.

1. Press **Shift-F1** (Setup), then **7** (Windows: open .docx/.doc/.wpd files), then **1** (Register and choose as default).
2. The Windows "Default apps" window opens. Choose *Blue Screen Writer 51* for .docx, .doc and any other types you want.

To undo this, use Shift-F1, 7, 2.

### The window

You can make the window as large as you like, or maximise it. The letters are large and clear, like the old full screen: 80 columns fill the window.

- **Ctrl + Shift + minus**: smaller letters. You see more of the line; the page gets margins and is centred.
- **Ctrl + =**: larger letters again.
- **Ctrl + 0**: back to normal.
- **Ctrl + mouse wheel**: does the same as these keys.

To choose a fixed screen size, go to Shift-F1, 2 Display, 4 Screen Size. The choices are large letters, whole page, or the classic 80 x 25.

## 2. Files and folders

### Long names, spaces and network drives

File names can be as long as you like and can contain spaces, umlauts and accents, for example *Übersetzung Vertrag Müller 2026.docx*.

Folders work the same way: *Documents*, *Desktop*, mapped network drives such as *S:*, and network paths such as *\\\\server\\share\\Translations* all work.

**F5 List Files** shows full names, with their dates and sizes. The keys are the ones you know (1 Retrieve, 3 Move/Rename, 6 Look, and so on). Long names are shown in full.

### Retrieving recent documents

Press **Shift-F10** (Retrieve). At *Document to be retrieved:* press the **Up arrow**. It goes through the last 9 documents you used. Press Enter to open the one shown.

### Which documents can be opened

Shift-F10 and F5 open almost any word-processor file:

- **Word**: .docx, .doc, also old Word for DOS and Word 2 files;
- **your old WordPerfect files**: 4.2, 5.0, 5.1 and later versions (.wp, .wp5, .wpd and files without an extension);
- **OpenOffice / LibreOffice** (.odt), **RTF**, **HTML**, plain text in any encoding;
- WordStar, Works, Write, Ami Pro, AbiWord, Markdown, and the text of PDF files.

### Saving: documents are now Word files

**F10** (Save) and **F7** (Exit) save your document as a **Word document (.docx)**. That has big advantages:

- your clients can open it directly in Word, LibreOffice or on a phone;
- you can open .docx files that clients send you, and send them back;
- everything you do here is kept: Reveal Codes, styles, tables, footnotes, headers, pictures and so on.

If you open an old file (for example *BRIEF.WP5* or *LETTER.DOC*) and press F10, the program offers the same name ending in **.docx** (*BRIEF.docx*). The old file stays untouched next to it.

### Saving in other formats

Press **Ctrl-F5** (Text In/Out), then **3 Save As**. Choose:

1. Word (.docx)
2. PDF, which is good for sending invoices and finished translations
3. OpenDocument (.odt)
4. RTF
5. HTML (for e-mail)
6. Text, with a choice of encoding: UTF-8, Windows-1252, DOS 850 or Windows-1251 Cyrillic

Ctrl-F5, **1 DOS Text** still works as before.

### Password

Ctrl-F5, **2 Password** locks a document with a password. The file is then encrypted the same way Word does it. Word can open it too, with the same password.

## 3. Nothing gets lost any more

### Undo and Redo

- **Ctrl-Z** takes back the last change. Press it again to go further back, many steps.
- **Ctrl-Y** puts it back again.

The classic **Esc** Undelete still works as before.

### Automatic recovery copy (power cut, crash)

While you work, the program quietly makes a safe copy of your unsaved work **every 30 seconds**. If the computer crashes or the power goes out, the next time you start the program it asks whether you want to recover the document. Answer **Y**, and you continue where you stopped (at most 30 seconds are lost).

### Automatic saving

A document that already has a name is saved to its own file automatically **every 5 minutes**. You can change the number of minutes, or set it to 0 to switch automatic saving off, in Shift-F1, 3 Environment, 1 Backup Options.

### Revisions: every saved version is kept

Every time a document is saved, by you or automatically, the previous version is kept.

Press **Ctrl-F5**, then **6 Revisions**. You see a list with the date, time and number of words of each saved version. Move with the arrow keys to the version you want, then:

1. **Look**: read that version.
2. **Retrieve as new document**: open it next to your current text, for example to copy a paragraph back.
3. **Compare with current**: shows what changed between that version and now.
4. **Restore**: puts that version on the screen in place of your current text (it asks first). Nothing is lost: all newer saved versions stay in the list, and Ctrl-Z undoes it.

This history is kept **even when the document is edited in Word in between**. The changes made outside are recorded as their own version.

## 4. Track changes (like in Word)

When a client sends a Word document with tracked changes, or wants to see your corrections, use Track Changes.

Press **Alt-F5** (Mark Text), then **7 Track Changes**:

1. **Track On/Off**: from now on, everything you type is marked as inserted and everything you delete is marked as deleted, instead of disappearing.
2. **Next Change**: jumps to the next change.
3. **Accept** / 4. **Reject**: decides one change.
5. **Accept All** / 6. **Reject All**.
7. **Compare with Disk Document**: marks everything that is different from the saved file.

While tracking is on, the status line shows **[Track]**. Inserted text is shown underlined in colour, and deleted text is struck through. In Reveal Codes you see codes like **[INS:name date]** and **[DEL:...]**.

Word shows these changes as normal Word tracked changes, and the changes clients make in Word appear here the same way.

## 5. Spelling and languages

### Spell check as you type

Misspelled words get a **red wavy underline** on the screen while you type. This is never printed.

The program **recognises the language of each paragraph by itself**: German, English, French, Italian, Spanish and Russian. A Russian sentence in a German letter, or an English paragraph in a French document, is checked in its own language. All-capital abbreviations (ICAO, EU, OACI) are not marked.

**Ctrl-F2** (Spell) works as before. It also switches language paragraph by paragraph. If you choose a language yourself in its menu, it uses that language for the whole document.

Thesaurus (**Alt-F1**) and hyphenation also exist for these languages.

### Typing aids

Shift-F1, 3 Environment, **T Typing Aids** has these settings, and each can be switched on or off:

- **Smart Quotes**: typing " gives the correct quotation marks for the language of the paragraph: „German", "English", « French », «Italian».
- **Spell Check As You Type**: the red underline.
- **Autocorrect**: fixes common typing errors automatically. You can add your own wrong word and its correction ("Add to Autocorrect List").

### Accents and Cyrillic

You can type with your normal Windows keyboard, including the German, Russian or any other Windows keyboard layout. The classic **Ctrl-2** / **Ctrl-V** Compose also still works (for example Ctrl-2, a, " gives ä).

## 6. Mouse (optional)

You never need the mouse. But if you want it:

- **Click** puts the cursor there.
- **Drag** marks a Block.
- **Double-click** selects a word.
- **Mouse wheel** scrolls.
- **Right mouse button** or **Alt + =** opens the **pull-down menu bar** at the top (File, Edit, Search, Layout, Mark, Tools, Font, Graphics, Help), just like the menus of WordPerfect 5.1. Choose with the mouse, or with the arrow keys and Enter.
- In **F5 List Files**: click selects a file, and double-click opens it.
- In menus: click a choice.

## 7. Working together with other Windows programs

### Copy and paste

- **Ctrl-Ins**: copies the marked Block to the Windows clipboard.
- **Shift-Del**: cuts it.
- **Shift-Ins**: pastes text from any Windows program, such as an e-mail, a web page or Word.

The classic Ctrl-F4 Move/Copy still works inside the program.

### Two documents

**Shift-F3** switches between Doc 1 and Doc 2, as before. Doc 2 has its own screen colour, so you always know where you are.

**Ctrl-F3, 1 Window** shows both at once, one above the other.

## 8. Printing and PDF

**Shift-F7** (Print) works as before, but it prints to **any Windows printer**. Choose the printer with **S Select Printer**.

**6 View Document** shows the page exactly as it will be printed, with real fonts and pictures. Choose 1 = 100 %, 2 = 200 %, 3 = Full Page or 4 = Facing Pages.

To make a **PDF**, use Ctrl-F5, 3 Save As, 2 PDF, or choose "Microsoft Print to PDF" as the printer.

### Fonts

**Ctrl-F8** (Font) offers all the fonts installed in Windows, such as Times New Roman, Arial or Garamond. Your old WordPerfect font names are translated to the matching Windows font automatically.

### Pictures

**Alt-F9** (Graphics) works as before. Besides the old .WPG pictures you can now use **PNG, JPEG, BMP and GIF** pictures from your camera or from the internet.

## 9. New things for your work as a translator

### Document information: words, lines, invoice amount

Press **Ctrl-F5, 7 Document Info**. It shows:

- words;
- characters with and without spaces;
- **standard lines** (55 characters, which you can change);
- standard pages;
- when you enter your price per line, **the invoice amount**.

If a Block is marked, it counts only the Block.

### Spreadsheets: Excel and CSV

**Ctrl-F5, 5 Spreadsheet** works as in 5.1, but now reads **Excel (.xlsx)** and **CSV** files. Choose 1 Import (copy the numbers in once) or 2 Create Link (they can be refreshed later with 4 Link Options, Update All Links). At *Range* you can give cells such as *A1:D10* or a sheet name; leave it empty for the whole first sheet. The data arrives as a table.

### Comments

**Ctrl-F5, 4 Comment** still creates comments, as before. Comments are exchanged with Word: comments a client writes in Word appear here, and yours appear in Word.

### Links and bookmarks

**Ctrl-F5, 8 Link/Bookmark** creates hyperlinks (web addresses, e-mail) and bookmarks, and jumps to them.

## 10. Your macros and keyboards

Your old macros (.WPM files) work: **Alt-F10** and your **Alt-letter** macros run as before. Ctrl-F10 records new ones, and **Home, Ctrl-F10** opens the macro editor.

Your keyboard files (.WPK) are chosen in Shift-F1, **5 Keyboard Layout**.

## 11. Where things are kept

- **Settings, recovery copies and revisions**: in the folder *%APPDATA%\Blue Screen Writer 51*. Type this into the Windows Explorer address bar to get there.
- **Your documents**: wherever you save them.

## Quick reference: the new keys

- **Ctrl-Z / Ctrl-Y**: Undo / Redo
- **Ctrl-Ins / Shift-Del / Shift-Ins**: Copy / Cut / Paste with Windows
- **Ctrl + = / Ctrl + Shift + minus / Ctrl + 0**: larger / smaller / normal letters
- **Alt + =** or right mouse button: pull-down menu bar
- **Shift-F10, then Up arrow**: last documents used
- **Ctrl-F5, 2**: Password
- **Ctrl-F5, 3**: Save As (Word, PDF, ODT, RTF, HTML, text)
- **Ctrl-F5, 4**: Comment
- **Ctrl-F5, 5**: Spreadsheet (Excel .xlsx, CSV)
- **Ctrl-F5, 6**: Revisions (every saved version)
- **Ctrl-F5, 7**: Document Info (words, lines, invoice amount)
- **Ctrl-F5, 8**: Links and bookmarks
- **Alt-F5, 7**: Track Changes
- **Shift-F1, 3, T**: Typing Aids (quotes, spelling underline, autocorrect)
- **Shift-F1, 7**: make Windows open documents with Blue Screen Writer 51
