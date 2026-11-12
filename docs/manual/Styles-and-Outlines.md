# Styles and Outlines

## Styles (Alt-F8)

A **style** is a named set of formatting codes, for example "Heading" (bold, large, centred) or "Quote" (indented from both sides, italics). Instead of putting the same codes in by hand each time, you switch the style on. If you later change the style, every place that uses it changes too.

Styles are saved inside the document. They are also exchanged with Word: they appear as Word styles.

### The style list

Press **Alt-F8**. The *Styles* screen lists the document's styles with their **Name**, **Type** and **Description**. If the document has no styles yet and a style library is set up (see below), the library's styles are brought in first.

Move with the **arrow keys** (or click). The bottom line shows:

```
1 On; 2 Off; 3 Create; 4 Edit; 5 Delete; 6 Save; 7 Retrieve; 8 Update: 1
```

| Choice | What it does |
|---|---|
| **1 On** | Switches the highlighted style on at the cursor. With a Block on, a paired style is put around the Block. |
| **2 Off** | Switches the current paired style off (moves the cursor past its [Style Off] code). |
| **3 Create** | Creates a new style and opens the edit screen. |
| **4 Edit** | Edits the highlighted style. |
| **5 Delete** | Deletes the highlighted style: 1 Leaving Codes (its codes stay in the text where it was used), 2 Including Codes (the style codes are removed from the text), 3 Definition Only. |
| **6 Save** | Saves the document's styles to a style file. |
| **7 Retrieve** | Brings in the styles of a style file (or of any document). If names clash, you are asked `Replace Existing Styles?`. |
| **8 Update** | Brings in the styles of the style library set up in Setup. |
| **N** | Name search: type the start of a style name. |

**F7**, **Esc**, **Space** or **0** leave the list.

### Types of style

| Type | How it works | Codes |
|---|---|---|
| **Paired** | Has a start and an end: switch it on, type, switch it off. Good for headings and quotations. | [Style On:Name] … [Style Off:Name] |
| **Open** | Switched on once and stays in effect, like a format code. Good for a document's basic layout (margins, font). | [Open Style:Name] |
| **Outline** | A paired style used for one level of outline numbering. | |

### Creating or editing a style

On the Styles screen press **3** (Create) or **4** (Edit). The *Styles: Edit* screen shows:

1. **Name** — type a name and press Enter.
2. **Type** — 1 Paired, 2 Open, 3 Outline.
3. **Description** — a short explanation shown in the list.
4. **Codes** — opens an editing screen with Reveal Codes. Put in the codes you want, using the normal keys (for example **F6** for bold, **Shift-F8** for margins). For a paired style there is a **[Comment]** code: codes **before** it are applied when the style is switched on, codes **after** it when it is switched off. Press **F7** when done.
5. **Enter** (paired styles only) — what the Enter key does while the style is on: 1 **HRt** (a normal new line), 2 **Off** (switches the style off), 3 **Off/On** (switches it off and on again, so each paragraph gets the style).

Press **F7** to go back to the list.

### Using a style

- **Paired:** press **Alt-F8**, highlight the style, press **1** (On). Type the text. Press **Alt-F8**, **2** (Off) — or press **Enter** if the style's Enter key is set to Off.
- **With a Block:** mark the text, press **Alt-F8**, highlight the style, **1** (On).
- **Open:** press **Alt-F8**, highlight the style, **1** (On).

### Style libraries

A style library is a file of styles you use in many documents.

- Set its folder and name in **Shift-F1**, **6** Location of Files, **5** Style Files (directory, and Library Filename). If none is set, the program uses `STYLE\LIBRARY.STY` in its own folder, if that file exists.
- **6 Save** writes a style file; **7 Retrieve** reads one; **8 Update** reads the library.
- Style files from WordPerfect 5.x (`.STY`) can be retrieved. Any document the program can open can also serve as a style source: its styles are taken.

## Outlines and paragraph numbering (Shift-F5)

Press **Shift-F5** (Date/Outline):

```
1 Date Text; 2 Date Code; 3 Date Format; 4 Outline; 5 Para Num; 6 Define: 0
```

Choices 1 to 3 are described in [Formatting](Formatting).

### 4 Outline

```
Outline: 1 On; 2 Off; 3 Move Family; 4 Copy Family; 5 Delete Family
```

**1 On** switches **outline mode** on ([Outline On]); the status line shows *Outline*. In outline mode:

- **Enter** starts a new numbered paragraph at the same level.
- **Tab**, right after the number, moves the paragraph one level **down** (I. becomes A.).
- **Shift-Tab**, right after the number, moves it one level **up**.
- Pressing **Enter** on an empty numbered line moves the number down, so you can add blank lines.

Numbers are adjusted automatically when you add, delete or move paragraphs.

**2 Off** switches outline mode off ([Outline Off]).

A **family** is a numbered paragraph together with all the paragraphs below it at deeper levels (for example section II. with its A., B., 1., 2.). Put the cursor on the paragraph number, then:

- **3 Move Family** — cuts the family out; move the cursor and press **Enter** to put it back elsewhere.
- **4 Copy Family** — copies it; move the cursor and press **Enter**.
- **5 Delete Family** — deletes it after asking `Delete Outline Family?`.

The numbers are corrected automatically.

### 5 Para Num

Inserts a single paragraph number outside outline mode.

At `Paragraph Level (Press Enter for Automatic):`

- press **Enter** for an automatic number, whose level follows the tab position where it stands ([Par Num:Auto]);
- or type a level from **1** to **8** for a fixed level ([Par Num:2]).

### 6 Define

The *Paragraph Number Definition* screen sets how numbers look, from the cursor on:

| Option | What it does |
|---|---|
| **1 Starting Paragraph Number** | The number to start with (for example `3` or `1.4` for several levels). |
| **2 Paragraph** | The style 1. a. i. (1) (a) (i) 1) a) |
| **3 Outline** | The style I. A. 1. a. (1) (a) i) a) |
| **4 Legal (1.1.1)** | The style 1 .1 .1 .1 … (1.1, 1.1.1 …) |
| **5 Bullets** | Bullet characters at every level |
| **6 User-defined** | Type your own: levels 1–8 separated by spaces. `1` = Arabic numbers, `a`/`A` = lower/upper letters, `i`/`I` = lower/upper Roman numerals, with any punctuation around them, for example `1. a) (i)`; any other character is used as a bullet. |
| **Attach Previous Level** | For each level 2–8: Yes means the number includes the higher levels (1.1, 1.1.1). |
| **7 Enter Inserts Paragraph Number** | Yes: in outline mode, Enter inserts a number. |
| **8 Automatically Adjust to Current Level** | Yes: the level follows the tab position. |
| **9 Outline Style Name** | The name of an Outline style to use for the levels. |

Press **F7** (or Enter) when done. A **[Par Num Def]** code is inserted.
