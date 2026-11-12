# Open items

State of version 1.2.0 (2026-11-12).  Everything in the original feature backlog
(autosave and crash recovery, revisions, track changes, mouse, columns, graphics,
styles, outlines, merge/sort, mark text, RTL and CJK, Linux/macOS builds, home
directory) is done.

## 5.1 menu choices still missing
They show "not available yet in this version" or do nothing (also listed in the
manual, Troubleshooting and FAQ):
- Ctrl-F8, 5 Print Color
- Ctrl-F7: footnote/endnote 3 New Number and 4 Options; 3 Endnote Placement
- Table Edit: 3 Lines, 4 Header, 6 Options, 7 Join, 8 Split; Move/Copy in Table Edit
- Ctrl-F4 with Block: 2 Tabular Column, 3 Rectangle; without Block: 4 Append
- Shift-F7: 7 Initialize Printer; changing B, U, G, T on the Print screen
- F5 Look: 1 Next, 2 Previous
- Extended search (Home, F2) searches only the main text
- Ctrl-F5, 1, 3 (Retrieve with CR/LF to [SRt]) works like choice 2

## Other platforms
- Linux/macOS: no printing or PDF (both go through the Windows printer system).
  A built-in PDF writer would cover PDF and printing on all systems.
- macOS app is not notarised (right-click → Open the first time).

## Scripts
- Open (from docs/HANDOFF-RTL-CJK.md): vertical text, Ruby, East Asian spell
  checking, Hebrew dictionary (not shipped).

## Untested
- A physical printer (only Microsoft Print to PDF tested), real Microsoft Word
  round trips (only LibreOffice and generated files), the macOS app on a real Mac.

## Performance and display
- Typing in a 100-page document: ~17 ms per keystroke (full relayout).  Next step
  if needed: incremental layout.
- Comment boxes are not shown on the edit screen inside newspaper columns.
- Columns: Alt-arrow item keys, Setup "Side-by-side Columns Display", tables in columns.
- PRINTER.TST in View Document: line numbers overlap text; the "Print Attributes"
  table imports as plain text; "Extra Large" wraps oddly.
