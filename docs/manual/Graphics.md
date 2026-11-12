# Graphics

**Alt-F9** (Graphics) puts pictures, framed text, equations and lines into a document, as in WordPerfect 5.1. Besides the old **.WPG** pictures you can use **PNG, JPEG, BMP and GIF** pictures, for example from a camera or the internet.

Press **Alt-F9**:

```
1 Figure; 2 Table Box; 3 Text Box; 4 User Box; 5 Line; 6 Equation: 0
```

The four box types and Equation work the same way. They differ only in their standard look (borders, caption) and in their separate numbering: Figure 1, Figure 2…; Table 1, Table 2…; and so on.

| Box | Typical use | Standard look |
|---|---|---|
| **Figure** | Pictures | Single border, caption "Figure 1" |
| **Table Box** | Tables, figures shown as tables | Thick border top and bottom |
| **Text Box** | Quotations, side notes | Thick border top and bottom, grey shading |
| **User Box** | Anything, without a frame | No border |
| **Equation** | Mathematical formulas | No border, full width |

## Creating a box

1. Put the cursor where the box belongs (usually at the start of a paragraph).
2. Press **Alt-F9**, choose the box type (for example **1** Figure), then **1** (Create).
3. The **Definition** screen appears:

```
Definition: Figure

     1 - Filename
     2 - Contents                  None
     3 - Caption
     4 - Anchor Type               Paragraph
     5 - Vertical Position         0"
     6 - Horizontal Position       Right
     7 - Size                      3.25" wide x 3.25" (high)
     8 - Wrap Text Around Box      Yes
     9 - Edit
```

4. Set what you need (see below). Usually: **1**, type the picture's file name, **Enter**.
5. Press **F7** (Exit). The box is inserted as a code (for example **[Fig Box:1;PHOTO.JPG;]**).

On the editing screen, the box is shown as an outline with its type and number. **View Document** (Shift-F7, 6) and printing show the real picture.

### The Definition options

| Option | What it does |
|---|---|
| **1 Filename** | The file to put in the box. A picture file (PNG, JPEG, BMP, GIF, WPG) becomes the box contents; it is stored inside the document, so the document stays complete when you send it. For an Equation box, a text file with the equation; for other boxes, a document file becomes the box text. |
| **2 Contents** | 1 None, 2 Graphic, 3 Text, 4 Equation. |
| **3 Caption** | Opens an editing screen for the caption. The box number is already there as [Box Num]; type your text after it and press F7. |
| **4 Anchor Type** | **1 Paragraph**: the box moves with its paragraph. **2 Page**: the box stays at a fixed place on the page. **3 Character**: the box is treated like a (large) character in the line. |
| **5 Vertical Position** | Paragraph anchor: the distance from the top of the paragraph. Page anchor: Full Page, Top, Center, Bottom or Set Position. Character anchor: Top, Center, Bottom or Baseline of the line. |
| **6 Horizontal Position** | Left, Right, Center or Full (the width between the margins). Page anchor: Margins, Columns or Set Position. |
| **7 Size** | 1 Set Width/Auto Height, 2 Set Height/Auto Width, 3 Set Both, 4 Auto Both. Then type the width and/or height. |
| **8 Wrap Text Around Box** | Yes: text flows around the box. No: text runs over or under it. |
| **9 Edit** | For a picture: opens the image editor (below). For text or equation contents: opens an editing screen for the box text or the equation. Press F7 when done. |

## Editing a box

1. Press **Alt-F9**, the box type, **2** (Edit).
2. Type the box number at the prompt (the next box after the cursor is offered) and press **Enter**.
3. The Definition screen appears. Change what you need and press **F7**.

### The image editor

On the Definition screen of a picture, press **9** (Edit). The picture is shown in its box, and the bottom line shows:

```
1 Move; 2 Scale; 3 Rotate; 4 Invert; 5 B&W: 0                     (10%)
```

| Key | Effect |
|---|---|
| **Arrow keys** | Move the picture inside the box |
| **Page Up** / **Page Down** | Make the picture larger / smaller |
| **+** / **-** (or grey + / -) | Rotate the picture |
| **Ins** | Change the step size shown on the right (for example 10%, 25%, 1%) |
| **1 Move** | Type the horizontal and vertical offset exactly |
| **2 Scale** | Type the horizontal and vertical scale in per cent |
| **3 Rotate** | Type the angle (0–360 degrees), and whether to mirror the picture |
| **4 Invert** | Swaps light and dark |
| **5 B&W** | Shows the picture in black and white |
| **Home** or **Ctrl-Home** | Resets all changes |
| **F7**, **Enter** or **Esc** | Back to the Definition screen |

## 3 New Number

Starts the numbering of this box type at a new number from the cursor on. Type the number. Code: **[New Fig Num:n]** (and similar).

## 4 Options

Sets the look of all boxes of this type from the cursor on. Code: **[Fig Opt]** (and similar).

| Option | What it does |
|---|---|
| **1 Border Style** | Left, right, top and bottom border: None, Single, Double, Dashed, Dotted, Thick or Extra Thick. |
| **2 Outside Border Space** | Space between the border and the surrounding text (left, right, top, bottom). |
| **3 Inside Border Space** | Space between the border and the contents. |
| **4 First Level Numbering Method** | Off, Numbers, Letters or Roman Numerals. |
| **5 Second Level Numbering Method** | The same, for a second level (for example 1a, 1b). |
| **6 Caption Number Style** | The text of the number in the caption, for example `[BOLD]Figure 1[bold]`. |
| **7 Position of Caption** | Below or above the box, outside or inside the border. |
| **8 Minimum Offset from Paragraph** | How far a paragraph-anchored box may move up when there is not enough room. |
| **9 Gray Shading** | Background shading in per cent of black (0 = none). |

The width and shading of the border styles themselves are set in **Shift-F8**, **4**, **8** Border Options.

## Graphics lines (Alt-F9, 5)

```
Create Line: 1 Horizontal; 2 Vertical; 3 Edit Line: Horizontal; 4 Edit Line: Vertical
```

Choose **1** or **2** to create a line, or **3** or **4** to edit the nearest line of that kind. The line screen shows:

| Option | Horizontal line | Vertical line |
|---|---|---|
| **1 Horizontal Position** | Left, Right, Center, Full or Set Position | Left, Right, Between Columns or Set Position |
| **2 Vertical Position** | Baseline or Set Position | Full Page, Top, Center, Bottom or Set Position |
| **3 Length of Line** | The length (Auto = as long as possible) | |
| **4 Width of Line** | The thickness | |
| **5 Gray Shading** | In per cent of black (100% = black) | |

Press **F7** to insert the line. Codes: **[HLine:…]**, **[VLine:…]**.

## Equations

An Equation box holds a formula written in the WordPerfect 5.1 equation language. Create an Equation box (**Alt-F9**, **6**, **1**), press **9** (Edit), type the equation, and press **F7**. Examples:

| You type | You get |
|---|---|
| `x SUP 2` or `x^2` | x² |
| `a SUB i` or `a_i` | aᵢ |
| `{a+b} OVER {c+d}` | a fraction |
| `SQRT {x^2 + y^2}` | a square root |
| `NROOT 3 {x}` | a cube root |
| `alpha`, `beta`, `pi`, `SUM`, `INT` | Greek letters and common symbols |

Use `{` and `}` to group parts. The equation is drawn in View Document and when printing.

## Pictures and Word

Pictures and boxes are saved in the .docx file and appear in Word. Pictures in Word documents you open appear here as Figure boxes.
