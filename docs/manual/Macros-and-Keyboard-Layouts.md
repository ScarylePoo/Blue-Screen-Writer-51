# Macros and Keyboard Layouts

A **macro** records a series of keystrokes (and, if you like, programming commands) and plays them back with one key. A **keyboard layout** (`.WPK` file) changes what the keys do. Blue Screen Writer 51 reads and writes the real WordPerfect 5.1 file formats for both, so **your old macros and keyboards work unchanged**, including the macro programming language.

## Where macros and keyboards are kept

Put your `.WPM` and `.WPK` files into the **WPM** folder next to `bsw51.exe`. The program looks for macros:

1. in the current folder, if you type a name there;
2. in the folder set in **Shift-F1**, **6** Location of Files, **2** Keyboard/Macro Files — or, if none is set, in the `WPM` folder next to the program and in the program folder itself.

New macros you record are saved in the first of these folders that exists (normally the `WPM` folder).

## Running a macro

### Alt-F10

1. Press **Alt-F10** (Macro).
2. Type the macro name at `Macro:` (without `.WPM`) and press **Enter**.

If the file cannot be found: `ERROR: File not found -- NAME.WPM`.

### Alt-letter macros

Macros named `ALTA.WPM` to `ALTZ.WPM` run directly with **Alt** and that letter. For example, **Alt-S** runs `ALTS.WPM`.

### Stopping a macro

Press **Esc** (Cancel) while a macro runs. (A macro can switch this off with its own commands, as in 5.1.)

## Recording a macro (Ctrl-F10)

1. Press **Ctrl-F10** (Macro Define).
2. At `Define macro:` type a name and press **Enter** — or press **Alt** and a letter to make an Alt-letter macro.
3. At `Description:` type a short description (optional) and press **Enter**.
4. *Macro Def* blinks on the status line. Everything you type and every key you press is now recorded — and also carried out, so you see what happens.
5. Press **Ctrl-F10** again to finish recording.

If a macro with that name already exists, you are asked:

```
1 Replace; 2 Edit; 3 Description: 0
```

**1** records a new macro in its place, **2** opens the macro editor, **3** changes only the description.

### Macro commands while recording (Ctrl-PgUp)

While recording, press **Ctrl-PgUp**:

```
1 Pause; 2 Display; 3 Assign; 4 Comment
```

| Choice | Effect when the macro runs |
|---|---|
| **1 Pause** | The macro stops and lets the user type (for example a name); pressing **Enter** continues. |
| **2 Display** | Whether the screen is updated while the macro runs. |
| **3 Assign** | Puts a value into a variable (1–9). |
| **4 Comment** | A note in the macro that does nothing when it runs. |

Outside recording, **Ctrl-PgUp** lets you assign a value to a variable directly.

## The macro editor

Open it with **Home**, **Ctrl-F10** (and type the macro name), or with **Ctrl-F10**, name, **2** Edit.

The screen shows:

```
1 - Description    Letter heading
2 - Action
   ┌──────────────────────────────────────────────┐
   │{DISPLAY OFF}{Home}{Home}{Up}Dear {Enter}     │
   │...                                           │
   └──────────────────────────────────────────────┘
```

- **1** edits the description, **2** the action (the macro's contents).
- In the action box, the macro is shown as text and **key codes** in braces, for example `{Enter}`, `{Bold}`, `{Format}`, `{Home}`.
- The **arrow keys** move over whole codes. Home, End, Page Up/Down, Ctrl-Left/Right and Home, Home, Up/Down work as usual.
- Typing inserts text. **Backspace** and **Del** delete one character or one whole code.
- **Function keys** insert their codes (for example pressing F6 inserts `{Bold}`).
- **Enter** and **Tab** only arrange the layout of the box, as in 5.1. To insert the *key* Enter or Tab, press **Ctrl-V** first, then the key: Ctrl-V, Enter gives `{Enter}`. Ctrl-V works for any key, including Esc and F7.
- **Ctrl-F10** switches recording on inside the editor: every key you press is inserted as its code until you press Ctrl-F10 again.
- **Ctrl-PgUp** shows the list of **programming commands** (`{IF}`, `{ELSE}`, `{END IF}`, `{ASSIGN}`, `{CHAR}`, `{LABEL}`, `{GO}`, `{WHILE}`, `{PROMPT}`, `{WAIT}`, `{SYSTEM}`…). Type the start of a name to jump to it, and press **Enter** to insert it.
- **F7** (Exit) saves the macro. **Esc** leaves without saving.

The programming commands work as in the WordPerfect 5.1 macro language.

## Keyboard layouts (Shift-F1, 5)

Press **Shift-F1** (Setup), **5** (Keyboard Layout). You see the list of `.WPK` keyboard files in your macro folder:

```
1 Select; 2 Delete; 3 Rename; 4 Create; 5 Copy; 6 Original;
7 Edit; 8 Map; A Alternate (built in); N Name search: 0
```

| Choice | What it does |
|---|---|
| **1 Select** (or Enter, or a double-click) | Uses the highlighted keyboard from now on. |
| **2 Delete** | Deletes the keyboard file (after asking). |
| **3 Rename** | Gives the keyboard file a new name. |
| **4 Create** | Makes a new, empty keyboard file. |
| **5 Copy** | Copies the highlighted keyboard to a new name. |
| **6 Original** | Goes back to the original keyboard (F1 Cancel, F3 Help, Esc Repeat), with no keyboard file. |
| **7 Edit** | Opens the *Keyboard: Edit* screen for the highlighted keyboard. |
| **8 Map** | Opens the *Keyboard: Map* screen for the highlighted keyboard. |
| **A Alternate (built in)** | Uses the built-in Alternate keyboard (F1 Help, Esc Cancel, F3 Repeat), with no keyboard file. This is the standard. |
| **N Name search** | Type the start of a keyboard name. |

The current choice is shown in the Setup menu next to "Keyboard Layout" (for example *ALTRNAT (built in)*, *Original*, or the file name). It is kept for next time.

When a keyboard file is selected, it decides all keys; the setting "Alternate Keyboard" in Setup, Environment then has no effect.

### Keyboard: Edit

Shows a list of the keys this keyboard changes, with **Key**, **Action** and **Description**:

```
1 Action; 2 Dscrptn; 3 Original; 4 Create; 5 Move; Macro: 6 Save; 7 Retrieve: 0
```

| Choice | What it does |
|---|---|
| **1 Action** (or Enter) | Edits what the highlighted key does, in the macro editor. A key can do anything a macro can. |
| **2 Description** | Changes the description. |
| **3 Original** | Returns the highlighted key to its normal function. |
| **4 Create** | Press the key you want to change, then define its action. |
| **5 Move** | Moves the highlighted definition to another key (press the new key). |
| **6 Save** | Saves the highlighted key's action as a macro file. |
| **7 Retrieve** | Press a key, then type a macro name: the macro becomes that key's action. |

**F7** saves the keyboard. **Esc** asks `Save changes to the keyboard?`.

### Keyboard: Map

Shows the whole keyboard as a map, with what each key types or does. Move to a key with the arrow keys, then:

```
1 Key; 2 Macro; 3 Description; 4 Original; 5 Compose: 0
```

| Choice | What it does |
|---|---|
| **1 Key** | Press another key: the highlighted key then does what that key normally does. |
| **2 Macro** | Edit the highlighted key's action in the macro editor. |
| **3 Description** | Change the description. |
| **4 Original** | Return the key to its normal function. |
| **5 Compose** | Type a character: the key then types that character. |

**F7** saves the keyboard.

### The {Keyboard} key

A macro or keyboard can contain the `{Keyboard}` code, which opens Setup, Keyboard Layout directly.

## Tips

- Copy your old `.WPM` and `.WPK` files from the WordPerfect folder (often `C:\WP51`) into the `WPM` folder, then select your keyboard in Shift-F1, 5.
- The menus you know from 5.1 behave exactly as before, so old macros that type menu numbers keep working.
- Macros can also be used to automate the new features (for example Ctrl-F5, 3 Save As).
