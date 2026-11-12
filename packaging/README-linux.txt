Blue Screen Writer 51 for Linux
===============================

Unpack the archive anywhere and start the program:

    cd "Blue Screen Writer 51"
    ./bsw51                (or:  ./bsw51 "My Letter.docx")

Keep the folders dict, thes, hyph and WPM next to the program.  It runs on
X11 and Wayland desktops (64-bit, glibc 2.35 or newer: Ubuntu 22.04,
Debian 12, Fedora 36 and later, and similar).

Menu entry (optional): edit bsw51.desktop (put the full path of this folder
into the Exec= and Icon= lines) and copy it to ~/.local/share/applications/

The keys are the same as on Windows: F1 Help, Esc Cancel, F3 Repeat
(Alternate keyboard), Alt-F4 Block, F11 Reveal Codes, Ctrl-Z Undo ...
If your desktop uses a function key or Alt-key for itself (for example
Alt-F4 = close window, Alt-F7 = move window), switch that shortcut off in
the desktop's keyboard settings.

Not available on Linux yet: printing and PDF (they use the Windows printer
system).  Save as .docx or .odt (Ctrl-F5, 3) and print from LibreOffice.
Registering as the default program (Setup 7) is Windows only.

Settings, revisions and recovery copies: ~/.config/blue-screen-writer-51

Full manual: https://github.com/ScarylePoo/Blue-Screen-Writer-51/wiki
The program is public domain (Unlicense); dictionaries, thesauri,
hyphenation patterns and the font have their own licences in their folders.
