# Installation and Starting

Blue Screen Writer 51 needs no installation and no administrator rights. You unzip it and start it.

**Requirements:** Windows 10 or Windows 11, 64-bit.

## Downloading

1. Open the project's **Releases** page on GitHub.
2. Under the newest release, download the file named `BlueScreenWriter51-…-windows-x64.zip`.
3. In Windows Explorer, right-click the downloaded zip file and choose **Extract All…**.
4. Choose a place you can easily find again, for example your *Documents* folder, and click **Extract**.

You now have a folder called *Blue Screen Writer 51*.

## What is in the folder

| Item | What it is |
|---|---|
| `bsw51.exe` | The program. |
| `dict` | Spelling dictionaries (German, English US and UK, French, Italian, Spanish, Russian, Arabic). |
| `thes` | Thesaurus files for the same languages. |
| `hyph` | Hyphenation patterns (all of the above except Arabic). |
| `WPM` | Your macros (`.WPM`) and keyboard files (`.WPK`). It starts empty; copy your own files into it. |
| `Guide - What Is New.docx` | A short guide for WordPerfect 5.1 users (it opens in the program or in Word). |
| `READ ME.txt` | A one-page summary. |
| `LICENSE.txt`, `DejaVu-LICENSE.txt` | The licence of the program (public domain) and of the screen font. The licences of the dictionaries, thesauri and hyphenation patterns are inside their folders. |

**Keep the folders `dict`, `thes`, `hyph` and `WPM` next to `bsw51.exe`.** The program looks for them there. If you move the program, move the whole folder.

## Starting the program

Double-click **bsw51.exe**. The blue editing screen appears, with the status line at the bottom right:

```
Doc 1 Pg 1 Ln 1" Pos 1"
```

Tip: to start it more easily, right-click `bsw51.exe` and choose **Show more options**, then **Send to**, **Desktop (create shortcut)**. Or right-click it and choose **Pin to Start** or **Pin to taskbar**.

You can also start the program with a document: drag a document file onto `bsw51.exe`, or set it up as the Windows program for documents (see below).

### The first start

On the first start, the program uses its standard settings:

- the **Alternate keyboard** (F1 = Help, Esc = Cancel, F3 = Repeat);
- large letters that fill the window (80 columns);
- a recovery copy of unsaved work every 30 seconds, automatic saving of named Word documents every 5 minutes, and a revision history of every save.

The settings you change in **Shift-F1** Setup are kept for next time. See [Setup](Setup).

### If you are asked to recover a document

If the computer crashed or lost power while a document had unsaved changes, the program asks at the next start:

```
Recover <document name> (autosaved <date> <time>)? Yes (No)
```

Press **Y** to get your work back (at most about 30 seconds are lost). Press **N** if you do not want it. Even then the text is not thrown away: it is kept in the document's revision history. See [Safety: Undo, Recovery and Revisions](Safety-Undo-Recovery-and-Revisions).

## Making Windows open documents with Blue Screen Writer 51

You can make Windows open documents in Blue Screen Writer 51 when you double-click them in Explorer or in an e-mail.

1. Start Blue Screen Writer 51.
2. Press **Shift-F1** (Setup).
3. Press **7** (Windows: open .docx/.doc/.wpd… files).
4. Press **1** (Register and choose as default).
5. The Windows **Default apps** window opens. Choose *Blue Screen Writer 51* for `.docx`, `.doc`, and any other file types you want.

Windows does not allow a program to make itself the default without asking you, so step 5 is needed once.

The registration is made for your Windows user only and needs no administrator rights. It offers the program for these file types: .docx, .doc, .dotx, .dot, .docm, .rtf, .odt, .ott, .fodt, .wpd, .wp, .wp5, .wp6, .wp4, .wpt, .wri, .wps, .abw, .sam, .txt, .md, .htm, .html and .pdf.

**If you move the program folder**, register again (Shift-F1, 7, 1), because Windows remembers where the program was.

**To undo the registration:** press **Shift-F1**, **7**, **2** (Remove registration).

## Where your settings are kept

Settings, recovery copies, revisions and your own word lists are kept in this folder:

```
%APPDATA%\Blue Screen Writer 51
```

To open it, type that line into the address bar of Windows Explorer and press Enter. Inside you find:

| File or folder | Contents |
|---|---|
| `settings.txt` | All Setup settings, the recent documents list, typing aids settings. |
| `Recovery` | Crash recovery copies of unsaved work. |
| `History` | The revision history of your documents (one folder per document). |
| `Backup` | Timed backup files (Setup, Backup Options). |
| `autocorrect.txt` | Your own autocorrect list. |
| `sup_xx.txt` | Words you added to the speller, one file per language (for example `sup_de.txt`). |
| `spell_lang.txt` | The speller language you chose last. |

Your documents are kept wherever you save them.

## Updating to a new version

1. Download the new zip file from the Releases page.
2. Unzip it over the old folder (or into a new folder).
3. If you put your own macros or keyboards into the `WPM` folder, make sure they are still there.

Your settings and revision history are in `%APPDATA%\Blue Screen Writer 51` and are not touched by an update.

## Uninstalling

1. If you registered the program for documents, start it and press **Shift-F1**, **7**, **2** (Remove registration).
2. Close the program and delete the *Blue Screen Writer 51* folder.
3. If you also want to remove your settings and revision history, delete the folder `%APPDATA%\Blue Screen Writer 51`. **This deletes all saved revisions and recovery copies.** Your own documents are not affected.

## Running from a USB stick

The program can keep its settings in a folder of your choice instead of `%APPDATA%`. Start it with the `--config` option, for example from a shortcut whose target is:

```
"E:\Blue Screen Writer 51\bsw51.exe" --config "E:\Blue Screen Writer 51\Settings"
```
