# Troubleshooting and FAQ

## Starting

**Windows says "Windows protected your PC" when I start bsw51.exe.**
The program is new and not signed with a commercial certificate, so Windows SmartScreen may warn about it. Click **More info**, then **Run anyway**. This is needed only once.

**The speller, thesaurus or hyphenation does not work ("not found" errors).**
The folders `dict`, `thes` and `hyph` must be next to `bsw51.exe`. If you copied only the program file somewhere else, copy the folders too, or start the program from its own folder.

**My macros are not found.**
Put the `.WPM` files into the `WPM` folder next to `bsw51.exe`, or set their folder in **Shift-F1**, **6** Location of Files, **2** Keyboard/Macro Files.

**After a crash I was asked to recover a document, and I said No by mistake.**
The text is kept in the revision history. Open the document (or, for an unnamed document, see below), press **Ctrl-F5**, **6**, and look for the entry *Left without saving*.

## Keyboard

**F1 shows Help instead of Cancel (or the other way round).**
The program starts with the Alternate keyboard (F1 Help, Esc Cancel, F3 Repeat). To use the original keyboard (F1 Cancel, F3 Help, Esc Repeat): **Shift-F1**, **3**, **9**, **N**. If you selected a `.WPK` keyboard file, it decides; choose **A** (Alternate) or **6** (Original) in Shift-F1, 5 to go back to a built-in layout.

**Ctrl + minus does not make the letters smaller.**
Ctrl + minus is the soft hyphen, as in WordPerfect 5.1. Use **Ctrl + Shift + minus** to zoom out, **Ctrl + =** to zoom in, and **Ctrl + 0** for normal.

**I cannot type accented letters / Cyrillic / Arabic.**
Add the keyboard layout in Windows (Settings, *Time & Language*, *Language & region*) and switch with **Windows key + Space**. Or use Compose: **Ctrl-V**, then for example **a** and **"** for ä.

**AltGr does not work.**
AltGr characters (for example @ or € on European keyboards) are typed normally; the program treats AltGr as text, not as Ctrl+Alt.

## Files

**Where are my documents saved?**
Where you saved them. The file name on the status line shows the full path. If you typed only a name, the document went into the current folder (see F5).

**I opened an old .WP5 file, pressed F10, and now there is a .docx file.**
That is intended: documents are saved as Word files. The old file is left untouched. If you need a different format, use **Ctrl-F5**, **3** Save As, or type a name ending in another extension (.rtf, .odt, .txt…).

**Can I still save in WordPerfect 5.1 format?**
No. Documents are saved as .docx (and the other formats listed under Save As). Every code you see in Reveal Codes is stored in the .docx file and comes back exactly. Old WordPerfect files can still be opened.

**A file opens as strange text.**
The program opens any file. If it does not recognise the format, it shows the readable text in it, like Notepad. Saving it creates a new .docx; the original is never changed.

**I forgot the password of a document.**
There is no way to open an encrypted document without its password. Keep a note of your passwords in a safe place.

**Saving to a network drive fails.**
Check that you have write access to that folder. The program saves through a temporary file `~name.wptmp` in the same folder; if a share does not allow renaming, it writes the file directly.

**There are files ending in .bk! next to my documents.**
These are the previous versions kept by *Original Document Backup*. You can delete them, or switch the feature off in **Shift-F1**, **3**, **1**, **2**.

## Safety

**How do I get back yesterday's version of a document?**
Open the document, press **Ctrl-F5**, **6** (Revisions), choose the version, and press **1** to look at it, **2** to open it next to the current one, **3** to compare, or **4** to restore it.

**I worked on a new document that I never saved, and it is gone.**
If the program crashed, start it again; it offers the recovery copy. Otherwise look in `%APPDATA%\Blue Screen Writer 51\Recovery` (recovery copies) and `%APPDATA%\Blue Screen Writer 51\Backup` (timed backups, `BSW51.BK1.docx` and `BSW51.BK2.docx`). The `.docx` backup files can be opened with Shift-F10.

**Automatic saving changed my file and I did not want that.**
Every automatic save is also a revision; restore the earlier one with Ctrl-F5, 6. To switch automatic saving off: **Shift-F1**, **3**, **1**, **4**, type **0**.

## Screen

**The letters are too small / too large.**
Use **Ctrl + =** and **Ctrl + Shift + minus**, or **Shift-F1**, **2**, **5** Screen Size.

**I see coloured text instead of bold or italics.**
That is the classic look. To see real bold, italic and underlined letters: **Shift-F1**, **2**, **2**, **1**.

**I cannot see headers, footers or footnotes on the screen.**
As in 5.1, they are not shown on the editing screen. Use **View Document** (Shift-F7, 6), or look at their codes in Reveal Codes.

**Comment boxes are missing.**
Check **Shift-F1**, **2**, **8** Comments Display. Inside newspaper columns, comment boxes are not shown on the editing screen in this version.

## Printing

**Nothing is printed.**
Check which printer is selected: **Shift-F7**, **S**. The status line shows where the pages were sent. Look at the printer's queue in Windows.

**PDF does not work.**
Save As PDF uses the Windows printer *Microsoft Print to PDF*. If it is missing, add it again in Windows Settings.

**The printout looks different from the screen.**
The editing screen uses a fixed-width screen font; printing uses the real fonts. **View Document** (Shift-F7, 6) shows exactly what will be printed.

## Features not available in this version

These menu choices exist but show "not available yet in this version" or do nothing:

- **Ctrl-F8**, **5** Print Color;
- **Ctrl-F7** footnote/endnote **3 New Number** and **4 Options**, and **3 Endnote Placement**;
- Table Edit **3 Lines**, **4 Header**, **6 Options**, **7 Join**, **8 Split**, and Move/Copy in Table Edit;
- **Ctrl-F4** with a Block: **2 Tabular Column** and **3 Rectangle**; without a Block: **4 Append** after choosing a sentence, paragraph or page;
- **Shift-F7** **7 Initialize Printer**, and changing **B**, **U**, **G**, **T** on the Print screen;
- **F5** List Files, Look: **1 Next** and **2 Previous**;
- Extended search (**Home**, **F2**) searches only the main text;
- **Ctrl-F5**, **1**, **3** (Retrieve with CR/LF to [SRt]) works like choice 2.

## Where to get help

- Press **F1** (Help) in the program: press a letter for the feature index, or a function key for its description.
- The pull-down menu bar (**Alt-=**) shows every function with its key.
- Report problems on the project's GitHub page (Issues).
