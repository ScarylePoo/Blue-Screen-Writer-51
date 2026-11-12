# The Screen and Keyboard

## The editing screen

When you start Blue Screen Writer 51 you see an almost empty blue screen. That is your page. There are no toolbars and no ribbons: everything is done with the function keys, just as in WordPerfect 5.1.

At the bottom of the screen is the **status line**:

```
C:\Users\...\Letter.docx                             Doc 1 Pg 1 Ln 1" Pos 1"
```

| Part | Meaning |
|---|---|
| left side | The file name of the document (can be switched off in Setup, Display, 7). Messages and prompts also appear here. |
| **Doc 1** / **Doc 2** | Which of the two documents you are working in. |
| **Pg** | The page the cursor is on. |
| **Ln** | The distance of the cursor line from the top of the page. |
| **Pos** | The distance of the cursor from the left edge of the paper. The number is shown in the colour or style of the text attributes at the cursor (for example bold), as in 5.1. |
| **Col** | Shown in front of "Doc" when the cursor is inside text columns. |

The left side of the status line also shows these words while they apply:

| Word | Meaning |
|---|---|
| **Block on** (blinking) | A Block is being marked. |
| **Macro Def** (blinking) | A macro is being recorded. |
| **Typeover** | Typing replaces text instead of inserting it. |
| **Math** | The cursor is in a Math area. |
| **Outline** | Outline mode is on. |
| **[Track]** | Track Changes is on. |

The units of Ln and Pos (inches, centimetres, points…) are set in **Shift-F1**, **3**, **8** Units of Measure.

### Two documents

You can have two documents open at the same time, **Doc 1** and **Doc 2**. **Shift-F3** (Switch) changes between them. Doc 2 has its own screen colour (white on cyan with the standard colours), so you always know where you are.

To see both at the same time, press **Ctrl-F3**, **1** Window and type the number of lines for the top window (for example 11). Each document then gets half of the screen. To go back to one full window, use Ctrl-F3, 1 again and type 0 (or a number as large as the screen). With the mouse, click into the other window to switch to it.

### Reveal Codes

**F11** or **Alt-F3** splits the screen: the lower part shows the text with all the hidden formatting codes. Press the key again to close it. See [Reveal Codes](Reveal-Codes).

### Large, clear letters and zoom

The letters are large and clear, like the old full-screen DOS mode: 80 columns fill the window. You can make the window as large as you like or maximise it; the letters grow with it.

| Key | Effect |
|---|---|
| **Ctrl + Shift + minus** | Smaller letters. You see more of the line; the page gets margins and is centred. |
| **Ctrl + =** | Larger letters again. |
| **Ctrl + 0** | Back to normal (automatic size). |
| **Ctrl + mouse wheel** | The same as these keys. |

(Plain **Ctrl + minus** is not a zoom key: it types a soft hyphen, as in 5.1.)

To choose a fixed screen size, go to **Shift-F1**, **2** Display, **5** Screen Size. The choices are:

1. Large letters (automatic) — the standard;
2. Whole page — the screen grows with the window and shows the full width of the page;
3. Classic 80x25;
4. Larger text;
5. Smaller text.

**Shift-F1**, **2**, **3** Text Screen Type chooses how many lines the classic screen has: 80x25, 80x43 or 80x50.

When a document contains Chinese, Japanese or Korean text, the screen uses a few more columns so that whole lines stay visible.

### Colours

**Shift-F1**, **2** Display, **1** Colors/Fonts/Attributes chooses one of three colour sets:

1. **Your own colours** — the standard set: bright white text on blue, each attribute (bold, underline, italics, redline…) in its own colour, and Doc 2 in white on cyan;
2. **Classic blue** — the factory colours of WordPerfect 5.1 (light grey on blue);
3. **Black & White**.

**Shift-F1**, **2**, **2** Screen Font Attributes decides whether bold, italics and underline are shown with real bold, slanted and underlined letters on top of the colours ("Real bold/italic/underline", the standard), or with colours only, as on a DOS screen.

## The keyboard

### Function keys

Every function key works as in WordPerfect 5.1, alone and together with **Shift**, **Ctrl** and **Alt**. The keyboard template (press **F1** twice) shows them all:

| Key | Ctrl | Shift | Alt | Alone |
|---|---|---|---|---|
| F1 | Shell | Setup | Thesaurus | Help * |
| F2 | Spell | Search backward | Replace | Search forward |
| F3 | Screen | Switch | Reveal Codes | Repeat * |
| F4 | Move | Indent both sides | Block | Indent |
| F5 | Text In/Out | Date/Outline | Mark Text | List Files |
| F6 | Tab Align | Center | Flush Right | Bold |
| F7 | Footnote | Print | Columns/Table | Exit |
| F8 | Font | Format | Style | Underline |
| F9 | Merge/Sort | Merge Codes | Graphics | End Field |
| F10 | Macro Define | Retrieve | Macro | Save |
| F11 | | | | Reveal Codes |
| F12 | | | | Block |

\* On the Alternate keyboard (the standard). See below.

The complete list of keys, including the cursor keys and the new keys, is in the [Keyboard Reference](Keyboard-Reference).

### The Alternate keyboard and the original keyboard

WordPerfect 5.1 came with two layouts, and many people used the "Alternate keyboard" (the ALTRNAT keyboard file). Blue Screen Writer 51 has both built in. **The Alternate keyboard is switched on when you first start the program.**

Only three keys differ:

| Key | Alternate keyboard (standard) | Original keyboard |
|---|---|---|
| **F1** | Help | Cancel / Undelete |
| **F3** | Repeat (the old Esc function) | Help |
| **Esc** | Cancel / Undelete | Repeat |

All other keys are the same on both.

**In this manual, keys are given for the Alternate keyboard.** If you use the original keyboard, read "F1" wherever this manual says "Esc (Cancel)", and "F3" wherever it says "F1 (Help)".

**To switch:** press **Shift-F1** (Setup), **3** (Environment), **9** (Alternate Keyboard), then **Y** for the Alternate keyboard or **N** for the original keyboard. The choice is kept for next time.

You can also load your own keyboard file (`.WPK`) in **Shift-F1**, **5** Keyboard Layout. A loaded keyboard file decides all keys itself. See [Macros and Keyboard Layouts](Macros-and-Keyboard-Layouts).

### How menus work

Most function keys show a **menu line** at the bottom of the screen, for example Ctrl-F5:

```
1 Dos Text; 2 Password; 3 Save As; 4 Comment; 5 Spreadsheet; 6 Revisions; 7 Document Info; 8 Link/Bookmark: 0
```

- Press the **number**, or the **highlighted letter** (the "mnemonic", shown in bold), to choose.
- Press **Esc** (Cancel) to go back one step, or **0**, **Enter** or the **Space bar** to leave the menu.
- Larger menus fill the whole screen (for example Shift-F8 Format). They work the same way: press the number or the letter in front of an item.
- To type a value (for example a margin), press its number, type the value and press **Enter**. **F7** (Exit) also accepts the value; a second F7 leaves all menus and returns to the document.
- With the mouse you can click a menu choice.

Measurements can be typed in any unit by adding a letter: `1"` or `1i` (inches), `2.5c` (centimetres), `72p` (points), `1200w` (1200ths of an inch). Without a letter, the unit chosen in Setup is used.

### Typing special characters and other languages

You can type with your normal Windows keyboard layout, including German, French, Russian, Arabic or any other layout you have installed in Windows. Switch layouts with **Windows key + Space**.

The classic **Compose** key also works: press **Ctrl-V** (or **Ctrl-2**), then two characters that make up the one you want:

| Type | Result | Type | Result |
|---|---|---|---|
| a " | ä | s s | ß |
| e ' | é | e \` | è |
| e ^ | ê | n ~ | ñ |
| c , | ç | a e | æ |
| o / | ø | a o | å |
| < < | « | > > | » |
| - - | – (en dash) | - = | — (em dash) |
| E = | € | L - | £ |
| c o | © | r o | ® |
| t m | ™ | s o | § |
| + - | ± | 1 2 | ½ |
| x * | × | : - | ÷ |
| . . | … | ^ o | ° |
| , , | „ | * . | • |

The two characters can be typed in either order. Capital letters work too (A " gives Ä).

You can also give a WordPerfect character set number: press **Ctrl-V**, type the set and the number separated by a comma (for example `1,23`) and press **Enter**.

For Chinese, Japanese and Korean, use the Windows input method (IME). See [Spelling, Thesaurus and Languages](Spelling-Thesaurus-and-Languages).

## The mouse (optional)

You never need the mouse. If you want it:

| Mouse action | Effect |
|---|---|
| **Click** in the text | Puts the cursor there. |
| **Drag** across text | Marks a Block. |
| **Double-click** a word | Marks that word as a Block. |
| **Mouse wheel** | Scrolls the text. |
| **Ctrl + mouse wheel** | Larger or smaller letters. |
| **Right mouse button** | Opens the pull-down menu bar. Inside a menu or prompt it works like Cancel. |
| **Ctrl + click** on a hyperlink | Opens the link. |
| Click in a menu | Chooses that item. |
| Click in a list (F5 List Files, Revisions, printers…) | Selects that line; a double-click opens it. |
| Click in the other window (split screen) | Switches to that document. |

## The pull-down menu bar

Press **Alt-=** (or click the right mouse button) to open the menu bar at the top of the screen, as in WordPerfect 5.1:

```
File  Edit  Search  Layout  Mark  Tools  Font  Graphics  Help
```

- Press the highlighted letter, or move with the **arrow keys** and press **Enter**, to open a menu.
- Choose an item with its highlighted letter, or with the arrow keys and **Enter**, or click it.
- **Esc** (Cancel) goes back one level.
- Items shown in `[ ]` cannot be used at the moment (some need a Block, some need no Block).

Every item does exactly the same as the matching function key, so the menu bar is a good way to find a feature whose key you have forgotten. The key is shown on the right of each item.

| Menu | Contains |
|---|---|
| **File** | Retrieve, Save, Text In, Text Out, Password, Revisions, List Files, Summary, Print, Setup, Go to Shell, Exit |
| **Edit** | Move (Cut), Copy, Paste, Append, Delete, Undelete, Undo, Block, Select (Sentence, Paragraph, Page), Comment, Convert Case, Switch Document, Window, Reveal Codes |
| **Search** | Forward, Backward, Next, Previous, Replace, Extended, Go to |
| **Layout** | Line, Page, Document, Other, Columns, Tables, Math, Footnote, Endnote, Justify, Align, Styles |
| **Mark** | Index, Table of Contents, List, Cross-Reference, Table of Authorities, Define, Generate, Master Documents, Subdocument, Track Changes, Document Compare |
| **Tools** | Spell, Thesaurus, Macro, Date Text, Date Code, Date Format, Outline, Paragraph Number, Define, Merge Codes, Merge, Sort, Line Draw |
| **Font** | Base Font, Normal, Appearance, Superscript, Subscript, Fine, Small, Large, Very Large, Extra Large, Print Color, Characters |
| **Graphics** | Figure, Table Box, Text Box, User Box, Equation, Line |
| **Help** | Help, Index, Template |

The menu bar can stay visible all the time: **Shift-F1**, **2** Display, **M** Menu Options, **8** Menu Bar Remains Visible. See [Setup](Setup).

## Help

Press **F1** (Help; F3 on the original keyboard).

- On the first Help screen, **press a letter** to see an alphabetical list of features that start with that letter, with the key and keystrokes for each one. If the list is long, press the same letter again for the next part.
- **Press any function key** (alone or with Shift, Ctrl or Alt) to read what that key does.
- **Press Help again** to see the function key template.
- **Enter** or the **Space bar** leaves Help.


## Shell: the Windows command prompt

**Ctrl-F1** (Shell) offers:

1. **Go to Shell** — opens a Windows command prompt in the current folder. Type `EXIT` there to close it.
2. **Shell Command** — runs one command and shows what it printed. Press any key to return to your document.
