# Setup (Shift-F1)

**Shift-F1** (Setup) holds the settings that stay in effect every time you start the program. They are saved at once in `%APPDATA%\Blue Screen Writer 51\settings.txt`.

How the Setup screens work: press the number (or highlighted letter) of an item. For Yes/No items press **Y** or **N**; for choices press the number shown at the bottom; for values type the value and press **Enter**. **Esc**, **Enter** or **0** go back one screen; **F7** returns to the document.

```
Setup

     1 - Mouse                                   (Windows mouse)
     2 - Display
     3 - Environment
     4 - Initial Settings
     5 - Keyboard Layout                         ALTRNAT (built in)
     6 - Location of Files
     7 - Windows: open .docx/.doc/.wpd… files    with Blue Screen Writer 51
```

## 1 Mouse

Information only. The program uses the normal Windows mouse settings (set them in Windows Settings). See [The Screen and Keyboard](The-Screen-and-Keyboard) for what the mouse does.

## 2 Display

| Item | Choices | Standard |
|---|---|---|
| **1 Colors/Fonts/Attributes** | 1 Your own colours (white on blue, each attribute in its own colour, Doc 2 white on cyan); 2 Classic blue (the factory colours of WordPerfect 5.1); 3 Black & White | Your own colours |
| **2 Screen Font Attributes** | 1 Real bold/italic/underline (letters are drawn bold, slanted, underlined, on top of the colours); 2 Colours only (as on a DOS screen) | Real bold/italic/underline |
| **3 Text Screen Type** | 1 80x25; 2 80x43; 3 80x50 (number of lines) | 80x25 |
| **4 Menu Options** | See below | |
| **5 Screen Size** | 1 Large letters (automatic: 80 columns fill the window; documents with Chinese, Japanese or Korean get just enough extra columns for their lines); 2 Whole page; 3 Classic 80x25; 4 Larger text; 5 Smaller text | Large letters (automatic) |
| **6 Reveal Codes Window Size** | Number of lines of the Reveal Codes part (1–20), for a 25-line screen | 10 |
| **7 Filename on the Status Line** | Yes / No | Yes |
| **8 Comments Display** | Yes / No: show document comments in boxes on the screen | Yes |

The zoom keys (**Ctrl + =**, **Ctrl + Shift + minus**, **Ctrl + 0**, Ctrl + mouse wheel) also change the screen size, and the result is kept.

### Menu Options

| Item | Choices | Standard |
|---|---|---|
| **1 Menu Letter Display** | How the highlighted letters in menus are shown: 1 Bold, 2 Underline, 3 Reverse, 4 Normal | Bold |
| **7 Menu Bar Separator Line** | Yes / No: a line below the menu bar when it stays visible | No |
| **8 Menu Bar Remains Visible** | Yes / No: the pull-down menu bar stays at the top of the screen | No |

Alt-= or the right mouse button shows the menu bar at any time.

## 3 Environment

| Item | What it does |
|---|---|
| **1 Backup Options** | See below. |
| **2 Beep Options** | Information only; the program uses Windows sounds. |
| **T Typing Aids** | Smart quotes, spell check as you type, autocorrect. See below. |
| **7 Prompt for Hyphenation** | When automatic hyphenation is on: 1 Never ask (hyphenate automatically), 2 When Required (ask only in some cases, when the program cannot decide by itself; the standard), 3 Always ask. |
| **8 Units of Measure** | See below. |
| **9 Alternate Keyboard** | Yes: F1 = Help, Esc = Cancel, F3 = Repeat (the standard). No: the original keyboard, F1 = Cancel, F3 = Help, Esc = Repeat. Has no effect while a .WPK keyboard file is selected. |

### Backup Options (Shift-F1, 3, 1)

| Item | What it does | Standard |
|---|---|---|
| **1 Timed Document Backup** | Yes/No, and **Minutes Between Backups**. Changed documents are copied to the backup folder as `BSW51.BK1.docx` and `BSW51.BK2.docx`. | Yes, 10 minutes |
| **4 Autosave Word documents** | Minutes between automatic saves of named .docx documents to their own file (0 = off). | 5 |
| **2 Original Document Backup (.BK!)** | Yes: before a document is saved over an existing file, the old file is kept as *name*`.bk!`. | Yes |
| **3 Backup Directory** | The folder for timed backups. Empty = `%APPDATA%\Blue Screen Writer 51\Backup`. | |

The crash recovery copy (every 30 seconds) and the revision history are always on and have no settings. See [Safety: Undo, Recovery and Revisions](Safety-Undo-Recovery-and-Revisions).

### Typing Aids (Shift-F1, 3, T)

| Item | What it does | Standard |
|---|---|---|
| **1 Smart Quotes** | Typing `"` and `'` gives the typographic quotes of the language. | Yes |
| **2 Quote Style Language** | Auto (the language of the paragraph), or always Deutsch „…“, Français « … », Italiano «…», Español «…», Russian «…», US English “…”, UK English “…”. A [Lang] code in the text always wins. | Auto |
| **3 Spell Check As You Type** | Red underline for misspelled words. | Yes |
| **4 Autocorrect Typing Errors** | Corrects common typing errors when you finish a word. | Yes |
| **5 Add to Autocorrect List** | Add your own correction: `Wrong word:`, then `Replace with:`. The screen shows how many entries your list has and where the file is. | |

See [Spelling, Thesaurus and Languages](Spelling-Thesaurus-and-Languages).

### Units of Measure (Shift-F1, 3, 8)

| Item | What it does |
|---|---|
| **1 Display and Entry of Numbers for Margins, Tabs, etc.** | The unit used in menus and when you type a number without a unit. |
| **2 Status Line Display** | The unit of *Ln* and *Pos* on the status line. |

Choices: 1 `"` inches (the standard), 2 `c` centimetres, 3 `p` points, 4 `w` 1200ths of an inch, 5 `u` 4.2 units (lines and columns).

Whatever the setting, you can always type a value with its own unit, for example `2.5c` or `1"`.

## 4 Initial Settings

| Item | What it does | Standard |
|---|---|---|
| **2 Date Format** | The pattern for Shift-F5 Date. The bottom line explains the numbers: 1 Day, 2 Month, 3 Month name, 4 Year, 5 Year (2 digits), 6 Weekday, 7 Hour (24), 8 Hour (12), 9 Minute, 0 am/pm. See [Formatting](Formatting). | `3 1, 4` |
| **3 Date Language** | English or Deutsch month and day names. | English |
| **5 Initial Codes** | Opens an editing screen (with Reveal Codes) for codes that every **new** document starts with, for example your usual margins, font or justification. Press F7 when done. | none |
| **6 Repeat Value** | How many times F3 (Repeat) repeats a key when you do not type a number. | 8 |

Initial codes of an individual document are set in **Shift-F8**, **3**, **2**.

## 5 Keyboard Layout

Opens the list of `.WPK` keyboard files: select, create, edit or map keyboards, or go back to the built-in Alternate or Original keyboard. See [Macros and Keyboard Layouts](Macros-and-Keyboard-Layouts).

## 6 Location of Files

| Item | What it is | Standard (when empty) |
|---|---|---|
| **1 Backup Files** | Folder for timed backups (the same as Backup Directory above). | `%APPDATA%\Blue Screen Writer 51\Backup` |
| **2 Keyboard/Macro Files** | Folder for `.WPM` macros and `.WPK` keyboards. | The `WPM` folder next to the program, then the program folder |
| **5 Style Files** and **Library Filename** | Folder for style files, and the style library used by Alt-F8. | `STYLE\LIBRARY.STY` next to the program, if it exists |
| **7 Documents (home directory)** | Your home directory: the program starts in it, and **F5 always suggests it**, even after you looked at other folders. In F5, typing **=** changes it. | None: F5 suggests the current folder, starting with the folder the program was started from |

A folder that does not exist is refused with `ERROR: Directory not found`.

## 7 Windows: open .docx/.doc/.wpd… files

| Choice | What it does |
|---|---|
| **1 Register and choose as default** | Registers Blue Screen Writer 51 for word-processing file types (for your Windows user only), then opens the Windows *Default apps* window, where you choose it for .docx, .doc and the other types you want. |
| **2 Remove registration** | Removes the registration again. |

See [Installation and Starting](Installation-and-Starting).

## Settings kept elsewhere

A few settings are made where they are used and remembered automatically:

| Setting | Where you set it |
|---|---|
| Printer | Shift-F7, S Select Printer |
| Speller language | Ctrl-F2, 7 |
| Price per standard line, characters per line | Ctrl-F5, 7 |
| Recent documents | Automatic (Shift-F10, Up arrow) |
| Screen zoom | Ctrl + = / Ctrl + Shift + minus / Ctrl + 0 |
