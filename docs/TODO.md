# Open items

## From the macro engine (wpm.rs) work
All 7 follow-ups are done (search prompts take codes, Ctrl-F3 twice = Rewrite, F7 in a
Format field returns to the menu, `{WAIT}` wakes the event loop, screens report their
kind for `{STATE}`/`{SYSTEM}menu`, "Define macro:" takes Alt-letters, no key codes left
as `Func::Other` except mouse/invalid ones).  COUNT, UX and REVEALCO now finish; REVEALTX
loops until Cancel (its own {ON CANCEL} ends it, as in WP) — test `formerly_endless_macros_finish`.

## Feature backlog (user requests, 2026-11-11)
- Autosave + crash recovery (in progress)
- Revision history (doc id in docx custom props + history store)
- Track changes (Word-compatible w:ins/w:del, accept/reject, Document Compare)
- Mouse support (click cursor, drag block, wheel, menus, F5 list double-click)
- Program name (suggested: Reveal)
- Later: columns, graphics boxes/images (incl. WPG), styles, outline/para numbering, merge/sort, mark text (ToC/index), window split, RTL/CJK handoff doc

## Performance
- Typing in a 100-page doc: ~17 ms/keystroke (full relayout). Next step if needed: incremental layout (resume from the edited line with a saved State, splice the unchanged tail).
- Comment boxes are not shown on the edit screen inside newspaper columns (columns::regroup rebuilds drows without DRow::Comment).
- Columns: Alt-arrow item keys, Setup "Side-by-side Columns Display", tables inside columns.
- PRINTER.TST View Document: line-numbering demo overlaps text (numbers drawn inside the text area); the "Print Attributes" table (WP table with 2 cols) is shown as plain text — check wp5 table import with Fmt/columns merge; "Extra Large" wraps oddly.
