# Speller and Thesaurus: data files

The Speller (Ctrl-F2) and Thesaurus (Alt-F1) are in `src/spell.rs`. They read
Hunspell dictionaries (through the pure-Rust `spellbook` crate) and LibreOffice
MyThes thesaurus files when the program runs. Nothing is built into the exe.

## Where the files are looked up (first match wins)

1. `<folder of wp.exe>\dict\…` and `<folder of wp.exe>\thes\…`
2. `%APPDATA%\WP51 Native\dict\…` and `…\thes\…`
3. `.\assets\dict|thes` (current directory), then the source tree's `assets\`

Dictionary: `dict\<ll_CC>\<ll_CC>.aff` + `.dic` (a flat `dict\<ll_CC>.aff` also works).
Thesaurus: `thes\th_<ll_CC>_v2.dat`. The `.idx` file is not needed because the
index is built in memory. To ship the program, copy `assets\dict` and `assets\thes` next to `wp.exe`.

## Language

WP language codes: DE (de_DE), US (en_US), UK (en_GB, which uses the en_US thesaurus), IT, FR, ES, RU.
The default is DE. The Speller's Check menu has `7` (an addition to WP 5.1), which picks the
language for both the Speller and the Thesaurus. The choice is saved in `%APPDATA%\WP51 Native\spell_lang.txt`.
Words added with `3 Add` go to `%APPDATA%\WP51 Native\sup_<lang>.txt`, one word per line.

## Sources and licences (all from github.com/LibreOffice/dictionaries)

| Files | Source | Licence |
|---|---|---|
| dict/de_DE | de/de_DE_frami (igerman98 + frami, new spelling; converted to UTF-8) | GPL 2 or 3 |
| dict/en_US | en/en_US (SCOWL) | BSD-like (see README_en_US.txt) |
| dict/en_GB | en/en_GB | LGPL |
| dict/it_IT | it_IT | GPL 3 |
| dict/fr_FR | fr_FR/dictionaries/fr (Dicollecte "moderne") | MPL 2.0 |
| dict/es_ES | es/es_ES (RLA-ES) | GPL3 / LGPL3 / MPL 1.1 |
| dict/ru_RU | ru_RU (A. Lebedev) | BSD-style (see README_ru_RU.txt) |
| thes/th_de_DE_v2 | OpenThesaurus.de | LGPL 2.1 |
| thes/th_en_US_v2 | WordNet | WordNet licence |
| thes/th_it_IT_v2 | it_IT | GPL 3 |
| thes/th_fr_FR_v2 | fr_FR thes_fr | LGPL 2.1+ |
| thes/th_es_ES_v2 | es th_es (converted to UTF-8) | LGPL 2.1 |
| thes/th_ru_RU_v2 | ru_RU th_ru_RU_M_aot_and_v2 | LGPL 2.1 |

Each dictionary folder includes its README or licence files. The thesaurus licence files are in `assets/thes/licenses/`.
