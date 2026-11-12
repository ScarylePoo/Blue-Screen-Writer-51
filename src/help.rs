//! F3 Help (F1 on the Alternate keyboard), WP 5.1 style: an opening
//! screen, an alphabetical feature index (press a letter), a page per
//! function key (press the key) and the keyboard template (Help again).
//! All texts are our own and describe Blue Screen Writer 51.

use crate::grid::*;
use crate::keys::{Func, Input};
use crate::ui::{Outcome, Screen, ScreenKind, Wp};

pub fn open(wp: &mut Wp) {
    wp.push(Box::new(Help { page: Page::Start }));
}

pub fn open_index(wp: &mut Wp, letter: char) {
    wp.push(Box::new(Help { page: Page::Index(letter.to_ascii_uppercase(), 0) }));
}

pub fn open_template(wp: &mut Wp) {
    wp.push(Box::new(Help { page: Page::Template }));
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Page {
    Start,
    Template,
    /// Letter and continuation page.
    Index(char, usize),
    Key(Func),
}

struct Help {
    page: Page,
}

/// Alphabetical feature index: feature, key, keystrokes.
const INDEX: &[(&str, &str, &str)] = &[
    ("Advance (to Line or Position)", "Format", "Shft-F8,4,1"),
    ("Alt-=", "Menu Bar", "Alt-="),
    ("Alternate Keyboard", "Setup", "Shft-F1,3,9"),
    ("Append Block to a File", "Move", "Ctrl-F4,1,4"),
    ("Appearance of Text", "Font", "Ctrl-F8,2"),
    ("Autosave of Word Documents", "Setup", "Shft-F1,3,1,4"),
    ("Backup, Timed", "Setup", "Shft-F1,3,1"),
    ("Backup, Original Document", "Setup", "Shft-F1,3,1,2"),
    ("Base Font", "Font", "Ctrl-F8,4"),
    ("Block", "Block", "Alt-F4 or F12"),
    ("Block with the Mouse", "Mouse", "drag"),
    ("Bold", "Bold", "F6"),
    ("Cancel", "Cancel", "Esc (F1 orig.)"),
    ("Case Conversion (Block On)", "Switch", "Shft-F3"),
    ("Center Text", "Center", "Shft-F6"),
    ("Center Page Top to Bottom", "Format", "Shft-F8,2,1"),
    ("Change Directory", "List", "F5,="),
    ("Characters, Special", "Compose", "Ctrl-V"),
    ("Codes, Reveal", "Reveal Codes", "Alt-F3 or F11"),
    ("Colors of the Screen", "Setup", "Shft-F1,2,1"),
    ("Columns", "Columns/Table", "Alt-F7,1"),
    ("Comment", "Text In/Out", "Ctrl-F5,4"),
    ("Compare Documents", "Mark Text", "Alt-F5,7,7"),
    ("Copy Text (Block On)", "Move", "Ctrl-F4,1,2"),
    ("Copy a File", "List", "F5,Enter,8"),
    ("Crash Recovery", "(automatic)", "at start-up"),
    ("Date Code", "Date/Outline", "Shft-F5,2"),
    ("Date Format", "Date/Outline", "Shft-F5,3"),
    ("Date Text", "Date/Outline", "Shft-F5,1"),
    ("Delete Block", "Block", "Alt-F4,Del"),
    ("Delete File", "List", "F5,Enter,2"),
    ("Delete to End of Line", "Del to EOL", "Ctrl-End"),
    ("Delete to End of Page", "Del to EOP", "Ctrl-PgDn"),
    ("Delete Word", "Del Word", "Ctrl-Backspace"),
    ("Directory, Default", "List", "F5,="),
    ("Document Summary", "Format", "Shft-F8,3,5"),
    ("Double Indent", "->Indent<-", "Shft-F4"),
    ("DOS Text Files", "Text In/Out", "Ctrl-F5,1"),
    ("Endnote", "Footnote", "Ctrl-F7,2"),
    ("Exit", "Exit", "F7"),
    ("Exit Without Saving", "Exit", "F7,N,Y"),
    ("Flush Right", "Flush Right", "Alt-F6"),
    ("Font", "Font", "Ctrl-F8"),
    ("Footers", "Format", "Shft-F8,2,4"),
    ("Footnote", "Footnote", "Ctrl-F7,1"),
    ("Format", "Format", "Shft-F8"),
    ("Function Key Template", "Help", "F1,F1"),
    ("Go to Page", "Goto", "Ctrl-Home"),
    ("Go to Shell (Command Prompt)", "Shell", "Ctrl-F1,1"),
    ("Hard Page Break", "Hard Page", "Ctrl-Enter"),
    ("Hard Space", "Home,Space", "Home,Space"),
    ("Headers", "Format", "Shft-F8,2,3"),
    ("Help", "Help", "F1"),
    ("History of a Document", "Text In/Out", "Ctrl-F5,6"),
    ("Hyphenation", "Format", "Shft-F8,1,1"),
    ("Indent", "->Indent", "F4"),
    ("Initial Codes", "Setup", "Shft-F1,4,5"),
    ("Insert / Typeover", "Typeover", "Ins"),
    ("Italics", "Font", "Ctrl-F8,2,4"),
    ("Justification", "Format", "Shft-F8,1,3"),
    ("Keyboard Layout", "Setup", "Shft-F1,5"),
    ("Keyboard Layout, Edit", "Setup", "Shft-F1,5,7"),
    ("Keyboard Map", "Setup", "Shft-F1,5,8"),
    ("Line Draw", "Screen", "Ctrl-F3,2"),
    ("Line Spacing", "Format", "Shft-F8,1,6"),
    ("List Files", "List", "F5"),
    ("Look at a File", "List", "F5,Enter,6"),
    ("Macro, Define (Record)", "Macro Define", "Ctrl-F10"),
    ("Macro, Edit", "Macro Define", "Home,Ctrl-F10"),
    ("Macro, Run", "Macro", "Alt-F10"),
    ("Macro Commands", "Macro Commands", "Ctrl-PgUp"),
    ("Margins, Left and Right", "Format", "Shft-F8,1,7"),
    ("Margins, Top and Bottom", "Format", "Shft-F8,2,5"),
    ("Menu Bar", "Menu Bar", "Alt-="),
    ("Menu Options", "Setup", "Shft-F1,2,4"),
    ("Mouse", "Mouse", "click, drag, wheel"),
    ("Move Text (Block On)", "Move", "Ctrl-F4,1,1"),
    ("Name Search", "List", "F5,Enter,N"),
    ("New Page", "Hard Page", "Ctrl-Enter"),
    ("Normal Text (Attributes Off)", "Font", "Ctrl-F8,3"),
    ("Open a Document", "Retrieve", "Shft-F10"),
    ("Original Keyboard", "Setup", "Shft-F1,5,6"),
    ("Page Numbering", "Format", "Shft-F8,2,6"),
    ("Paper Size", "Format", "Shft-F8,2,7"),
    ("PDF, Save As", "Text In/Out", "Ctrl-F5,3,2"),
    ("Print", "Print", "Shft-F7"),
    ("Printer, Select", "Print", "Shft-F7,s"),
    ("Recovery after a Crash", "(automatic)", "at start-up"),
    ("Repeat Value", "Esc", "F3 (Esc orig.)"),
    ("Replace", "Replace", "Alt-F2"),
    ("Retrieve a Document", "Retrieve", "Shft-F10"),
    ("Reveal Codes", "Reveal Codes", "Alt-F3 or F11"),
    ("Revisions (Earlier Versions)", "Text In/Out", "Ctrl-F5,6"),
    ("Rewrite the Screen", "Screen", "Ctrl-F3,3"),
    ("Save", "Save", "F10"),
    ("Save As Word, PDF, Text", "Text In/Out", "Ctrl-F5,3"),
    ("Search Forward", "->Search", "F2"),
    ("Search Backward", "<-Search", "Shft-F2"),
    ("Setup", "Setup", "Shft-F1"),
    ("Shell Command", "Shell", "Ctrl-F1,2"),
    ("Size of Text", "Font", "Ctrl-F8,1"),
    ("Spell Check", "Spell", "Ctrl-F2"),
    ("Status Line, File Name", "Setup", "Shft-F1,2,7"),
    ("Strikeout", "Font", "Ctrl-F8,2,9"),
    ("Superscript / Subscript", "Font", "Ctrl-F8,1"),
    ("Switch Documents", "Switch", "Shft-F3"),
    ("Tab Set", "Format", "Shft-F8,1,8"),
    ("Tables", "Columns/Table", "Alt-F7,2"),
    ("Template, Keyboard", "Help", "F1,F1"),
    ("Thesaurus", "Thesaurus", "Alt-F1"),
    ("Track Changes", "Mark Text", "Alt-F5,7"),
    ("Typeover", "Typeover", "Ins"),
    ("Underline", "Underline", "F8"),
    ("Undelete", "Cancel", "Esc (F1 orig.)"),
    ("Undo", "Undo", "Ctrl-Z"),
    ("Units of Measure", "Setup", "Shft-F1,3,8"),
    ("Uppercase / Lowercase (Block On)", "Switch", "Shft-F3"),
    ("View Document", "Print", "Shft-F7,6"),
    ("Widow/Orphan Protection", "Format", "Shft-F8,1,9"),
    ("Window", "Screen", "Ctrl-F3,1"),
    ("Word Count", "Spell", "Ctrl-F2,6"),
    ("Word Documents (.docx, .doc, RTF)", "Retrieve", "Shft-F10"),
    ("WP 5.x Documents", "Retrieve", "Shft-F10"),
    ("Zoom (View Document)", "Print", "Shft-F7,6"),
];

/// Help text for a function key: (title, lines).
fn key_help(f: Func) -> (&'static str, &'static [&'static str]) {
    use Func::*;
    match f {
        Cancel => (
            "Cancel",
            &[
                "Backs out of whatever you started: a prompt, a menu or a question",
                "goes away without changing anything.  Pressed while a macro runs,",
                "it stops the macro.",
                "",
                "Undelete",
                "When nothing else is going on, Cancel brings back what you deleted",
                "most recently.  The deleted text appears at the cursor; choose",
                "1 Restore to put it back, or 2 Previous Deletion to see the one",
                "before (the last three deletions are kept).",
                "",
                "With Block on, Cancel switches Block off.",
                "",
                "Ctrl-Z undoes your last change, step by step.",
            ],
        ),
        SearchFwd | SearchBack => (
            "Search",
            &[
                "Finds text or codes.  F2 searches forward (towards the end of the",
                "document), Shift-F2 backward.  Type what you are looking for after",
                "\"-> Srch:\" and press F2 again (or Esc) to start the search.",
                "",
                "Codes can be searched for by pressing their keys at the prompt:",
                "Enter gives [HRt], Tab [Tab], F6 [BOLD] (again: [bold]), F8 [UND],",
                "F4 [->Indent], Shift-F6 [Center], Ctrl-Enter [HPg].",
                "",
                "Lowercase letters find both lower- and uppercase; uppercase",
                "letters find only uppercase.",
                "",
                "Home before F2 extends the search into headers, footers and notes.",
                "If nothing is found, \"* Not found *\" is shown.",
            ],
        ),
        Replace => (
            "Replace",
            &[
                "Replaces text or codes with something else.",
                "",
                "First you are asked \"w/Confirm?\" - answer Yes to approve every",
                "single replacement, No to replace all at once.  Then type the",
                "text to find, press F2, type the replacement and press F2 again.",
                "Codes are entered with their keys, as for Search.",
                "",
                "Leaving the replacement empty deletes every occurrence.",
            ],
        ),
        Help => (
            "Help",
            &[
                "Shows help.  On the first help screen, press a letter for the list",
                "of features beginning with that letter, or press any function key",
                "to read what that key does.  Press Help once more to see the",
                "keyboard template.  Enter or the Space bar leaves Help.",
            ],
        ),
        Indent => (
            "->Indent",
            &[
                "Indents the whole paragraph to the next tab stop.  All lines up",
                "to the next hard return [HRt] start there.  Press it several",
                "times to indent further.  Creates the [->Indent] code.",
            ],
        ),
        DblIndent => (
            "->Indent<-",
            &[
                "Indents the paragraph from both margins by one tab stop each,",
                "for quotations.  Creates the [->Indent<-] code.",
            ],
        ),
        List => (
            "List (List Files)",
            &[
                "Shows the files in a folder.  Press Enter to list the current",
                "folder, or type another one (\"=\" changes the default folder).",
                "",
                "In the list, move with the arrow keys or the mouse and choose:",
                "1 Retrieve; 2 Delete; 3 Move/Rename; 4 Print; 5 Short/Long",
                "Display; 6 Look; 7 Other Directory; 8 Copy; 9 Find; N Name",
                "Search.  A double click opens a document.",
                "",
                "Word documents (.docx, .doc), RTF, plain text and old WP 5.x",
                "files can all be opened directly from here.",
            ],
        ),
        Bold => (
            "Bold",
            &[
                "Switches bold on; press it again to switch it off.  With Block",
                "on, the blocked text is made bold.  Codes: [BOLD] [bold].",
            ],
        ),
        Underline => (
            "Underline",
            &[
                "Switches underlining on and off.  With Block on, the blocked",
                "text is underlined.  Codes: [UND] [und].",
            ],
        ),
        Exit => (
            "Exit",
            &[
                "Leaves the document.  You are asked \"Save document?\" - answer",
                "Yes to save it (you can change the name), then whether to leave",
                "the program or this document.",
                "",
                "In menus, Exit returns to the document and keeps your changes.",
                "When editing a footnote, header or macro, Exit finishes the",
                "editing.",
                "",
                "Nothing is ever lost: if you leave without saving, the last state",
                "is kept in the document history (Ctrl-F5, 6 Revisions).",
            ],
        ),
        EndField => ("End Field", &["Marks the end of a field in a merge file (merge is not yet", "available in this version)."]),
        Save => (
            "Save",
            &[
                "Saves the document and lets you keep working.  The name you type",
                "can be any length, contain spaces, and include a folder or a",
                "network path (\\\\server\\share\\...).",
                "",
                "Documents are stored as Word files (.docx) unless you give",
                "another extension, so anybody can open them.  Every save is also",
                "kept as a revision (Ctrl-F5, 6) and Word documents are saved",
                "automatically every few minutes (Setup, Environment, Backup).",
                "",
                "With Block on, only the blocked text is saved.",
            ],
        ),
        RevealCodes => (
            "Reveal Codes",
            &[
                "Splits the screen: the lower part shows the text together with",
                "the hidden formatting codes, such as [BOLD] or [HRt].  The cursor",
                "is shown as a highlighted bar; codes can be deleted there with",
                "Del or Backspace.  Press the key again to close the window.",
                "",
                "The window size is set in Setup, Display.",
            ],
        ),
        Block => (
            "Block",
            &[
                "Marks a piece of text.  Press Block, then move the cursor (or",
                "type a character to extend to it, Enter to the end of the",
                "paragraph).  Then press the key for what to do with it: Bold,",
                "Underline, Font, Move (copy or cut), Delete, Save, Print, Switch",
                "(upper/lower case), Spell ...",
                "",
                "With the mouse, drag across the text to block it; a double",
                "click blocks a word.",
            ],
        ),
        Setup => (
            "Setup",
            &[
                "Settings that stay in effect every time you start the program:",
                "",
                "1 Mouse; 2 Display (colours, screen size, menu options, Reveal",
                "Codes window); 3 Environment (backup, autosave, units, Alternate",
                "Keyboard); 4 Initial Settings (date format, initial codes, repeat",
                "value); 5 Keyboard Layout (choose, edit or map .WPK keyboards);",
                "6 Location of Files.",
            ],
        ),
        Switch => (
            "Switch",
            &[
                "Changes between Doc 1 and Doc 2 - two documents can be open at",
                "the same time.",
                "",
                "With Block on: 1 Uppercase; 2 Lowercase converts the blocked text.",
            ],
        ),
        DateOutline => (
            "Date/Outline",
            &[
                "1 Date Text types today's date; 2 Date Code inserts a code that",
                "always shows the current date; 3 Date Format chooses how dates",
                "look (German month names are available in Setup).",
                "4 Outline, 5 Para Num and 6 Define number paragraphs.",
            ],
        ),
        Center => (
            "Center",
            &["Centers the current line between the margins.  With Block on,", "the blocked lines are centered."],
        ),
        Print => (
            "Print",
            &[
                "Prints the full document, a page, or a block (Block on).  Choose",
                "the printer with \"s\".  6 View Document shows the pages as they",
                "will be printed.",
                "",
                "To make a PDF, choose \"Microsoft Print to PDF\" as the printer,",
                "or use Text In/Out (Ctrl-F5), 3 Save As, 2 PDF.",
            ],
        ),
        Format => (
            "Format",
            &[
                "1 Line: hyphenation, justification, line spacing, margins, tabs.",
                "2 Page: centering, headers, footers, margins, page numbers, paper.",
                "3 Document: display pitch, initial codes, summary.",
                "4 Other: advance, conditional end of page, language, and more.",
                "",
                "While typing a value, Enter accepts it; Exit accepts it too and",
                "returns to the menu.",
            ],
        ),
        MergeCodes => ("Merge Codes", &["Inserts merge codes (merge is not yet available in this version)."]),
        Retrieve => (
            "Retrieve",
            &[
                "Opens a document.  Type its name (and folder).  If a document is",
                "already on the screen, you are asked whether to insert the file",
                "at the cursor.",
                "",
                "Word (.docx, .doc), RTF, plain text and WP 5.x documents are",
                "recognised automatically.",
            ],
        ),
        Thesaurus => (
            "Thesaurus",
            &[
                "Looks up synonyms and antonyms for the word at the cursor.",
                "Press the letter in front of a word after choosing 1 Replace Word",
                "to put it into the document.",
            ],
        ),
        MarkText => (
            "Mark Text",
            &[
                "Marks text for indexes and tables (mostly not yet available).",
                "",
                "7 Track Changes: switch change tracking on or off, move to the",
                "next change, accept or reject changes, or compare the document",
                "with the version on disk.  Insertions are shown in green,",
                "deletions in red, and Word sees them as tracked changes.",
            ],
        ),
        FlushRight => ("Flush Right", &["Aligns the line at the right margin.  With Block on, the", "blocked lines are aligned right."]),
        ColumnsTable => (
            "Columns/Table",
            &[
                "1 Columns; 2 Tables; 3 Math.  Tables: 1 Create asks for the",
                "number of columns and rows.  Inside a table the key opens Table",
                "Edit, where columns can be widened, rows inserted and deleted.",
            ],
        ),
        Style => ("Style", &["Styles are not yet available in this version."]),
        Graphics => ("Graphics", &["Graphics boxes are not yet available in this version."]),
        Macro => (
            "Macro",
            &[
                "Runs a macro: type its name at \"Macro:\" and press Enter.",
                "Macros named ALTx.WPM run directly with Alt and that letter.",
                "WordPerfect 5.1 macros (.WPM) work unchanged, including the",
                "programming commands.",
                "",
                "Cancel stops a running macro.",
            ],
        ),
        MacroDefine => (
            "Macro Define",
            &[
                "Records a macro.  Type a name (or press Alt and a letter), then a",
                "description.  \"Macro Def\" blinks while everything you type is",
                "recorded; press Macro Define again to finish.",
                "",
                "If the macro exists: 1 Replace; 2 Edit; 3 Description.",
                "Home, Macro Define opens the macro editor directly.",
                "",
                "In the macro editor the arrow keys move over the commands, Ctrl-",
                "PgUp lists the programming commands, Ctrl-V inserts the next key",
                "as a command (e.g. {Enter}), and Exit saves the macro.",
            ],
        ),
        Shell => (
            "Shell",
            &[
                "1 Go to Shell opens a Windows command prompt in the current",
                "folder (type EXIT there to close it).",
                "",
                "2 Shell Command runs a single command and shows what it",
                "printed.  Press any key to return to your document.",
            ],
        ),
        Spell => (
            "Spell",
            &[
                "Checks spelling of a word, the page or the whole document, in",
                "the language of the text (German, English, French, Spanish ...).",
                "Unknown words can be skipped, added, edited or replaced by a",
                "suggestion (press its letter).  6 Count counts the words.",
            ],
        ),
        Screen => (
            "Screen",
            &[
                "1 Window; 2 Line Draw (draw boxes with the arrow keys);",
                "3 Rewrite redraws the screen.  Pressing Screen twice also",
                "rewrites the screen.",
            ],
        ),
        Move => (
            "Move",
            &[
                "Moves or copies a sentence, paragraph or page - or, with Block",
                "on, the blocked text.  Choose 1 Move (cut) or 2 Copy, move the",
                "cursor to the new place and press Enter.",
                "",
                "4 Retrieve puts the last moved or copied text in again.",
            ],
        ),
        TextInOut => (
            "Text In/Out",
            &[
                "1 DOS Text: save or open plain text files.",
                "2 Password.",
                "3 Save As: Word (.docx), PDF, plain text.",
                "4 Comment: notes in the document that are not printed.",
                "6 Revisions: every saved version of this document, with date and",
                "time - open or restore an earlier version.",
            ],
        ),
        TabAlign => ("Tab Align", &["Aligns text on the decimal point at the next tab stop (not yet", "available in this version)."]),
        Footnote => (
            "Footnote",
            &[
                "1 Footnote; 2 Endnote.  Create opens an empty note: type the",
                "text and press Exit.  Edit asks for the note number.",
            ],
        ),
        Font => (
            "Font",
            &[
                "1 Size (superscript, subscript, fine ... extra large);",
                "2 Appearance (bold, underline, double underline, italics,",
                "outline, shadow, small caps, redline, strikeout);",
                "3 Normal ends all attributes; 4 Base Font chooses the typeface",
                "(all Windows fonts); 5 Print Color.",
            ],
        ),
        MergeSort => ("Merge/Sort", &["Merge and sort are not yet available in this version."]),
        Esc => (
            "Esc (Repeat)",
            &[
                "Repeats the next key or character several times.  Press it,",
                "type a number (default 8), then the key: e.g. Esc, 20, Down",
                "moves the cursor down 20 lines.",
                "",
                "On the Alternate keyboard this is F3, and Esc is Cancel.",
            ],
        ),
        MenuBar => (
            "Menu Bar",
            &[
                "Alt-= shows the menu bar at the top: File, Edit, Search,",
                "Layout, Mark, Tools, Font, Graphics, Help.  Press the highlighted",
                "letter or Enter to open a menu, move with the arrow keys, and",
                "choose with Enter or the highlighted letter.  Cancel goes back.",
                "",
                "The right mouse button also opens the menu bar, and you can click",
                "the entries.  In Setup, Display, Menu Options the bar can stay",
                "visible all the time.",
            ],
        ),
        Compose => (
            "Compose",
            &[
                "Types special characters.  Press Ctrl-V, then two characters that",
                "make up the one you want: a and \" gives ä, s and s gives ß, e and",
                "' gives é, c and , gives ç ...",
            ],
        ),
        GoTo => (
            "Go to",
            &[
                "Ctrl-Home, then a page number and Enter jumps to that page.",
                "Ctrl-Home and a character jumps to its next occurrence;",
                "Ctrl-Home, Up / Down goes to the top / bottom of the page.",
            ],
        ),
        Home | Left | Right | Up | Down | End | PgUp | PgDn | WordLeft | WordRight | ScreenUp | ScreenDown | ParaUp | ParaDown => (
            "Cursor Keys",
            &[
                "Arrows move by character and line; Ctrl-Left / Right by word;",
                "Ctrl-Up / Down by paragraph.  Home, arrow goes to the edge of",
                "the screen; Home, Home, arrow to the start or end of the",
                "document; End to the end of the line.  PgUp / PgDn move by page,",
                "grey - / + by screen.",
                "",
                "With the mouse: click to place the cursor, use the wheel to",
                "scroll.",
            ],
        ),
        Typeover => ("Typeover", &["Ins switches between inserting and typing over existing text.", "\"Typeover\" is shown on the status line while it is on."]),
        Backspace | Del | DelWord | DelEol | DelEop => (
            "Deleting",
            &[
                "Backspace deletes the character to the left, Del the one at the",
                "cursor.  Ctrl-Backspace deletes a word, Ctrl-End to the end of",
                "the line, Ctrl-PgDn to the end of the page.  Hidden codes ask",
                "before they are deleted.  Cancel brings deletions back.",
            ],
        ),
        Tab | MarRel => ("Tab", &["Moves to the next tab stop.  Shift-Tab is Margin Release.", "In a table, Tab moves to the next cell."]),
        Enter | HardPage => (
            "Enter",
            &["Ends a paragraph ([HRt]).  Ctrl-Enter starts a new page ([HPg])."],
        ),
        MacroCommands => (
            "Macro Commands",
            &[
                "Ctrl-PgUp while recording a macro: 1 Pause; 2 Display; 3 Assign",
                "a variable; 4 Comment.  In the macro editor it lists all the",
                "programming commands ({IF}, {ASSIGN}, {CHAR} ...) to insert.",
            ],
        ),
        CtrlLetter('Z') => ("Undo", &["Ctrl-Z takes back the last change.  Press it again to go further", "back."]),
        ItemLeft | ItemRight | ItemUp | ItemDown | AltHome => (
            "Item Keys",
            &["Alt and an arrow key move to the neighbouring cell in a table."],
        ),
        Keyboard => ("Keyboard", &["Opens Setup: Keyboard Layout."]),
        _ => ("Help", &["There is no help for this key."]),
    }
}

const TEMPLATE: &[(&str, &str, &str, &str, &str)] = &[
    // key, Ctrl, Shift, Alt, plain
    ("F1", "Shell", "Setup", "Thesaurus", "Cancel"),
    ("F2", "Spell", "<-Search", "Replace", "->Search"),
    ("F3", "Screen", "Switch", "Reveal Codes", "Help"),
    ("F4", "Move", "->Indent<-", "Block", "->Indent"),
    ("F5", "Text In/Out", "Date/Outline", "Mark Text", "List"),
    ("F6", "Tab Align", "Center", "Flush Right", "Bold"),
    ("F7", "Footnote", "Print", "Columns/Table", "Exit"),
    ("F8", "Font", "Format", "Style", "Underline"),
    ("F9", "Merge/Sort", "Merge Codes", "Graphics", "End Field"),
    ("F10", "Macro Define", "Retrieve", "Macro", "Save"),
    ("F11", "", "", "", "Reveal Codes"),
    ("F12", "", "", "", "Block"),
];

impl Help {
    /// Pages of the index for `letter`.
    fn index_pages(letter: char, per_page: usize) -> Vec<Vec<&'static (&'static str, &'static str, &'static str)>> {
        let items: Vec<_> = INDEX.iter().filter(|(f, _, _)| f.chars().next().map(|c| c.to_ascii_uppercase()) == Some(letter)).collect();
        let mut pages: Vec<Vec<_>> = items.chunks(per_page.max(1)).map(|c| c.to_vec()).collect();
        if pages.is_empty() {
            pages.push(Vec::new());
        }
        pages
    }

    fn per_page(rows: usize) -> usize {
        rows.saturating_sub(6)
    }
}

impl Screen for Help {
    fn full_screen(&self) -> bool {
        true
    }
    fn kind(&self) -> ScreenKind {
        ScreenKind::Help
    }
    fn input(&mut self, wp: &mut Wp, i: &Input) -> Outcome {
        match i {
            Input::Func(Func::Enter) | Input::Char(' ') | Input::Char('0') | Input::Func(Func::Exit) => Outcome::Close,
            Input::Func(Func::Help) => {
                self.page = if self.page == Page::Start { Page::Template } else { Page::Start };
                Outcome::Keep
            }
            Input::Char(c) if c.is_ascii_alphabetic() => {
                let l = c.to_ascii_uppercase();
                self.page = match self.page {
                    Page::Index(x, n) if x == l => {
                        let pages = Help::index_pages(l, Help::per_page(wp.rows)).len();
                        Page::Index(l, (n + 1) % pages)
                    }
                    _ => Page::Index(l, 0),
                };
                Outcome::Keep
            }
            Input::Char('1') if self.page == Page::Template => Outcome::Keep,
            Input::Func(f) => {
                self.page = Page::Key(*f);
                Outcome::Keep
            }
            _ => Outcome::Keep,
        }
    }
    fn draw(&self, wp: &Wp, g: &mut Grid) {
        let st = wp.theme.normal;
        let hi = wp.theme.bold;
        let und = st.flags(F_UNDER);
        g.clear(st);
        let bottom = |g: &mut Grid| {
            let r = g.rows - 1;
            let c = g.text(0, r, "Selection: ", hi);
            g.text(c, r, "0", st);
            g.text(50, r, "(Press ENTER to exit Help)", st);
            g.cursor = Some((c, r));
        };
        match self.page {
            Page::Start => {
                g.text(0, 0, "Help", hi);
                g.text(80 - 22, 0, "Blue Screen Writer 51", st);
                let lines = [
                    "",
                    "",
                    "     Press any letter to get an alphabetical list of features.",
                    "",
                    "          The list shows the features that start with that letter",
                    "          and the key (and keystrokes) that use them.  Then press",
                    "          that key to read how the feature works.",
                    "",
                    "",
                    "     Press any function key to get information about the use of the key.",
                    "",
                    "          Some keys show a menu; their help describes the choices.",
                    "          Press HELP again to display the keyboard template.",
                    "",
                    "",
                    "     Modern extras: mouse support, Alt-= menu bar, Ctrl-Z undo,",
                    "     Word documents (.docx), PDF, track changes (Alt-F5, 7),",
                    "     revisions of every save (Ctrl-F5, 6), autosave and crash recovery.",
                ];
                for (k, l) in lines.iter().enumerate() {
                    g.text(0, 1 + k, l, st);
                }
                bottom(g);
            }
            Page::Template => {
                g.text(0, 0, "Function Key Template", hi);
                g.text(2, 2, "Key   Ctrl            Shift           Alt             Plain", und);
                for (r, (k, c, s, a, p)) in TEMPLATE.iter().enumerate() {
                    g.text(2, 3 + r, &format!("{k:<6}{c:<16}{s:<16}{a:<16}{p}"), st);
                }
                let y = 4 + TEMPLATE.len();
                let alt = if wp.keyboard.is_none() && wp.alternate_keys {
                    "Alternate keyboard is on: F1 = Help, Esc = Cancel, F3 = Repeat (Esc)."
                } else {
                    "Original keyboard: F1 = Cancel, F3 = Help, Esc = Repeat."
                };
                g.text(2, y, alt, st);
                g.text(2, y + 1, "Alt-= Menu Bar   Ctrl-Z Undo   Ctrl-V Compose   Ctrl-PgUp Macro Commands", st);
                g.text(2, y + 2, "Mouse: click = cursor, drag = block, right button = menu bar.", st);
                bottom(g);
            }
            Page::Index(l, n) => {
                let pages = Help::index_pages(l, Help::per_page(g.rows));
                let n = n.min(pages.len() - 1);
                let title = if n > 0 { format!("Features [{l}] (continued)") } else { format!("Features [{l}]") };
                g.text(0, 0, &title, hi);
                g.text(39, 0, "Key", hi);
                g.text(57, 0, "Keystrokes", hi);
                if pages[n].is_empty() {
                    g.text(0, 2, &format!("(No features beginning with {l}.)"), st);
                }
                for (k, (feat, key, ks)) in pages[n].iter().enumerate() {
                    g.text(0, 2 + k, feat, st);
                    g.text(39, 2 + k, key, st);
                    g.text(57, 2 + k, ks, st);
                }
                if pages.len() > 1 {
                    let msg = format!("More... Press {} to continue.", l.to_ascii_lowercase());
                    g.text(0, g.rows - 3, &msg, hi);
                }
                bottom(g);
            }
            Page::Key(f) => {
                let (title, lines) = key_help(f);
                let keyname = key_label(wp, f);
                let t = if keyname.is_empty() { title.to_string() } else { format!("{title} ({keyname})") };
                g.text(5, 0, &t, hi);
                for (k, l) in lines.iter().enumerate() {
                    if 2 + k >= g.rows - 2 {
                        break;
                    }
                    g.text(5, 2 + k, l, st);
                }
                bottom(g);
            }
        }
    }
}

/// Which key does `f` on the active keyboard (for the help title).
fn key_label(wp: &Wp, f: Func) -> String {
    let alt = wp.keyboard.is_none() && wp.alternate_keys;
    for (k, c, s, a, p) in TEMPLATE {
        let key = |m: &str| format!("{m}{k}");
        let name = crate::keys::Func::macro_name(f);
        let n = name.trim_matches(|c| c == '{' || c == '}');
        let eq = |x: &str| !x.is_empty() && x.replace("->", "").replace("<-", "").replace('/', "").eq_ignore_ascii_case(&n.replace('/', ""));
        if eq(p) {
            if alt && *k == "F1" {
                return "Esc".into();
            }
            if alt && *k == "F3" {
                return "F1".into();
            }
            return key("");
        }
        if eq(c) {
            return key("Ctrl-");
        }
        if eq(s) {
            return key("Shift-");
        }
        if eq(a) {
            return key("Alt-");
        }
    }
    match f {
        Func::Esc if alt => "F3".into(),
        Func::Esc => "Esc".into(),
        Func::MenuBar => "Alt-=".into(),
        Func::Compose => "Ctrl-V".into(),
        Func::GoTo => "Ctrl-Home".into(),
        Func::MacroCommands => "Ctrl-PgUp".into(),
        Func::CtrlLetter('Z') => "Ctrl-Z".into(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(wp: &Wp) -> String {
        let mut g = Grid::new(80, 25, wp.theme.normal);
        wp.draw(&mut g);
        (0..25).map(|r| (0..80).map(|c| g.get(c, r).unwrap().ch).collect::<String>().trim_end().to_string() + "\n").collect()
    }

    #[test]
    fn help_pages() {
        let mut wp = Wp::new(80, 25);
        open(&mut wp);
        assert!(screen(&wp).contains("Press any letter"));
        wp.input(Input::Char('m'));
        let s = screen(&wp);
        assert!(s.contains("Features [M]") && s.contains("Macro, Edit") && s.contains("Home,Ctrl-F10"), "{s}");
        wp.input(Input::Func(Func::Bold));
        let s = screen(&wp);
        assert!(s.contains("Bold (F6)"), "{s}");
        wp.input(Input::Func(Func::Shell));
        assert!(screen(&wp).contains("Go to Shell"));
        wp.input(Input::Func(Func::Help));
        wp.input(Input::Func(Func::Help));
        assert!(screen(&wp).contains("Function Key Template"));
        wp.input(Input::Func(Func::Enter));
        assert!(!wp.has_screens());
    }

    #[test]
    fn index_is_sorted_and_fits() {
        for w in INDEX.windows(2) {
            let a = w[0].0.chars().next().unwrap().to_ascii_uppercase();
            let b = w[1].0.chars().next().unwrap().to_ascii_uppercase();
            assert!(a <= b, "{} / {}", w[0].0, w[1].0);
        }
        for (f, k, _) in INDEX {
            assert!(f.len() <= 38 && k.len() <= 17, "{f}");
        }
    }
}
