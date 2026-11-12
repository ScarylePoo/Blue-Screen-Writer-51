# Spelling, Thesaurus and Languages

Blue Screen Writer 51 checks spelling in **German, English (US and UK), French, Italian, Spanish, Russian and Arabic**, and in **Hebrew** if you add a Hebrew dictionary. It recognises the language of each paragraph by itself, underlines mistakes while you type, and offers a thesaurus and hyphenation for these languages. It also handles right-to-left text (Arabic, Hebrew) and Chinese, Japanese and Korean.

## Spell check as you type

Misspelled words get a **red wavy underline** on the screen while you type. The underline is never printed.

- Each paragraph is checked in **its own language**, detected automatically: a Russian sentence in a German letter, or an English paragraph in a French document, is checked correctly. A **[Lang:XX]** code (Shift-F8, 4, 4) overrides the detection.
- Words in **all capitals** (abbreviations such as ICAO, EU, OACI) are not marked.
- Chinese, Japanese and Korean words are never underlined (there is no spell checking for these languages).

To switch the underline off: **Shift-F1**, **3** Environment, **T** Typing Aids, **3** Spell Check As You Type, **N**.

## The Speller (Ctrl-F2)

Press **Ctrl-F2** (Spell). The bottom of the screen shows the language and the menu:

```
Language: US English US  (7 Change Language)
Check: 1 Word; 2 Page; 3 Document; 4 New Sup. Dictionary; 5 Look Up; 6 Count: 0
```

| Choice | What it does |
|---|---|
| **1 Word** | Checks the word at the cursor, then moves to the next word. |
| **2 Page** | Checks the current page. |
| **3 Document** | Checks the whole document from the beginning. |
| **4 New Sup. Dictionary** | Uses another supplementary dictionary file (for the words you add) for this session. Type its name. |
| **5 Look Up** | Type a word or a pattern to see matching words. `?` stands for one letter, `*` for any number of letters (for example `beh*ve`). |
| **6 Count** | Counts the words in the document. (For more figures, see [Document Information](Document-Information).) |
| **7** | Change the language (see below). |

**With a Block on**, Ctrl-F2 checks only the Block.

### When a word is not found

The word is highlighted, and a list of suggestions appears in the upper part of the screen, each with a letter (A, B, C…). If there are more, `Press Enter for more words` is shown.

- **Press the letter** of a suggestion to replace the word with it.

Or choose from the menu:

```
Not Found: 1 Skip Once; 2 Skip; 3 Add; 4 Edit; 5 Look Up; 6 Ignore Numbers: 0
```

| Choice | What it does |
|---|---|
| **1 Skip Once** | Leaves the word and goes on. It will be stopped at again later. |
| **2 Skip** | Leaves the word everywhere in this check. |
| **3 Add** | Adds the word to your supplementary dictionary, so it is never marked again. |
| **4 Edit** | Lets you correct the word yourself in the text. Press **F7** when done. |
| **5 Look Up** | Type a pattern to look up other spellings. |
| **6 Ignore Numbers** | From now on, words containing digits (such as A4 or 3D) are not checked. |

**Double words** ("the the"): `Double Word: 1 2 Skip; 3 Delete 2nd; 4 Edit; 5 Disable Double Word Checking`.

**Irregular case** ("THis", "tHE"): `Irregular Case: 1 2 Skip; 3 Replace; 4 Edit; 5 Disable Case Checking`. 3 Replace corrects the capitals.

When the check is finished, the number of words is shown: `Word count: …  Press any key to continue`.

**Esc** (Cancel) or **F7** stops the speller at any time.

### Choosing the speller language

Normally the speller switches language paragraph by paragraph, by itself. If you choose a language yourself, it uses that language for the whole check:

1. In the Speller, press **7**.
2. The bottom line shows `Language: 1 DE; 2 US; 3 UK; 4 IT; 5 FR; 6 ES; 7 RU; 8 AR; 9 HE`.
3. Press the number.

The choice is remembered for next time (it becomes the default language when a paragraph's language cannot be detected).

| Code | Language | Dictionary folder |
|---|---|---|
| DE | German | de_DE |
| US | English (US) | en_US |
| UK | English (UK) | en_GB |
| IT | Italian | it_IT |
| FR | French | fr_FR |
| ES | Spanish | es_ES |
| RU | Russian | ru_RU |
| AR | Arabic | ar |
| HE | Hebrew | he_IL (not included; see below) |

The old WordPerfect language codes are understood too (for example GE or DA for German, EN or AM for US English, GB or OZ for UK English, SP for Spanish).

### Your own words

Words you add with **3 Add** are kept in `%APPDATA%\Blue Screen Writer 51\sup_xx.txt` (xx = the language, for example `sup_de.txt`). It is a plain text file with one word per line; you can edit it with Notepad.

## Adding or replacing dictionaries

The dictionaries are **Hunspell** dictionaries, the same as in LibreOffice and Firefox. Each language needs two files, `xx_YY.aff` and `xx_YY.dic`.

The program looks for them, in this order:

1. in the `dict` folder next to `bsw51.exe`: `dict\xx_YY\xx_YY.aff` and `.dic` (or directly `dict\xx_YY.aff` and `.dic`);
2. in `%APPDATA%\Blue Screen Writer 51\dict` in the same arrangement.

**To add Hebrew:** get a Hebrew Hunspell dictionary (for example from the LibreOffice dictionaries), and put `he_IL.aff` and `he_IL.dic` into a new folder `dict\he_IL`. Then choose language 9 (HE) in the speller, or let detection find Hebrew paragraphs.

**To use a different dictionary** for one of the languages above (for example Swiss German or Austrian rules), replace the files in its folder, keeping the folder and file names listed in the table.

Only the languages in the table can be used. Dictionaries for other languages are not picked up.

## Thesaurus (Alt-F1)

1. Put the cursor on a word.
2. Press **Alt-F1**.

The thesaurus shows the word's meanings with synonyms (and antonyms where available) in columns, each word with a letter. The bottom line shows:

```
1 Replace Word; 2 View Doc; 3 Look Up Word; 4 Clear Column: 0
```

| Choice | What it does |
|---|---|
| **1 Replace Word** | Then press the letter of a word to put it into the document in place of the word at the cursor. |
| **2 View Doc** | Lets you look at the document (move with the arrow keys); F7 returns to the thesaurus. |
| **3 Look Up Word** | Type another word to look up. |
| **4 Clear Column** | Removes a column of results. |
| a letter | Looks up the words for that entry (a new column appears). |
| **Left/Right arrow** | Move between columns. |
| **Up/Down**, **Page Up/Down** | Scroll a long column. |

**F7**, **Esc**, **Enter** or **0** leave the thesaurus.

The thesaurus is available in German, English, French, Italian, Spanish, Russian and Arabic (UK English uses the English thesaurus). It uses the language of the paragraph. The files are LibreOffice "MyThes" files (`th_xx_YY_v2.dat`) in the `thes` folder.

## Hyphenation

Hyphenation patterns are included for German, English, French, Italian, Spanish and Russian (not for Arabic). Switch hyphenation on with **Shift-F8**, **1**, **1**, **Y**; see [Formatting](Formatting). The language of each paragraph decides which rules are used.

## Smart quotes, autocorrect and typing aids

**Shift-F1**, **3** Environment, **T** Typing Aids has these settings:

| Option | What it does |
|---|---|
| **1 Smart Quotes** | Typing `"` or `'` gives the correct quotation marks for the language: „German“, “English”, « French » (with spaces), «Italian», «Spanish», «Russian». Standard: Yes. |
| **2 Quote Style Language** | Auto (follows the language of the paragraph, the standard), or always Deutsch, Français, Italiano, Español, Russian, US English or UK English. A [Lang] code in the text always wins. |
| **3 Spell Check As You Type** | The red underline. Standard: Yes. |
| **4 Autocorrect Typing Errors** | Fixes common typing errors when you finish a word (for example "teh" becomes "the", "wich" becomes "which"). A small list is built in for each language. Standard: Yes. |
| **5 Add to Autocorrect List** | Adds your own correction: type the `Wrong word:`, press Enter, type the `Replace with:` text, press Enter. |

Your own list is kept in `%APPDATA%\Blue Screen Writer 51\autocorrect.txt`, one entry per line in the form `wrong=right`, or `XX:wrong=right` for one language only (for example `DE:dsa=das`). You can edit it with Notepad.

Smart quotes and autocorrect are not applied in Typeover mode or while a Block is on. Autocorrect is also not applied while Track Changes is on.

## Typing accents and other alphabets

- Use your normal **Windows keyboard layout**, including the German, French, Russian, Arabic or any other layout. Add layouts in Windows Settings, *Time & Language*, *Language & region*; switch with **Windows key + Space**.
- Or use **Compose**: **Ctrl-V** (or **Ctrl-2**), then two characters (a and " gives ä). See [The Screen and Keyboard](The-Screen-and-Keyboard).
- Cyrillic text works everywhere: on screen, in the speller, in print and in Word files.

## Arabic and Hebrew (right to left)

Arabic text is shown the way it is read: **from right to left, with the letters joined**. Hebrew is shown right to left too.

- A paragraph that **starts** in Arabic or Hebrew is set against the **right** margin.
- An Arabic name inside an English or German sentence simply reads right to left within the line.
- **Numbers and Latin words** inside Arabic text stay left to right.
- The **cursor keys** move through the text in the order it was typed: in Arabic, Right arrow moves to the next letter, which is on its left on the screen.
- **Reveal Codes** shows the text in typed order with the letters unjoined, just as it shows all other codes.
- Word, LibreOffice and printing show Arabic paragraphs right to left as well.
- Spell checking and the thesaurus work for Arabic; hyphenation does not.

Type Arabic with the Windows Arabic keyboard (add it in Windows Settings, then switch with Windows key + Space).

## Chinese, Japanese and Korean

Chinese, Japanese and Korean documents open, show and print correctly.

- Each of these characters takes **two columns** on the blue screen, as on an Asian DOS screen.
- **Line breaking:** lines break between characters (there are no spaces), and never before a closing mark such as 。 or 」. Korean breaks at spaces.
- **Fonts:** Microsoft YaHei for Chinese, Yu Gothic for Japanese, Malgun Gothic for Korean, from Windows. A paragraph that contains Japanese kana is shown with the Japanese font.
- When a document contains these characters, the screen uses a few more columns so that whole lines stay visible; the letters get only a little smaller. The zoom keys still work as usual.
- **Typing:** add the language in Windows (Settings, *Time & Language*, *Language & region*) and switch with **Windows key + Space**. The input method's candidate list appears right next to the cursor; the text being composed is shown underlined until you confirm it.
- **Word counts:** every Chinese or Japanese character counts as one word, as in Word. Korean words are counted by spaces. See [Document Information](Document-Information).
- **Old text files** in GBK, Big5, Shift-JIS and EUC encodings are recognised automatically.
- There is no spell checking for these languages.
