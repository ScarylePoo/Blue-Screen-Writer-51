# Track Changes and Comments

Blue Screen Writer 51 can mark your corrections the way Microsoft Word does ("track changes"), and it can hold comments. Both are exchanged with Word in both directions: what a client marks in Word appears here, and what you mark here appears in Word.

## Track Changes (Alt-F5, 7)

When a client sends a Word document with tracked changes, or wants to see your corrections, use Track Changes.

Press **Alt-F5** (Mark Text), then **7** (Track Changes). The menu shows:

```
1 Track On/Off; 2 Next Change; 3 Accept; 4 Reject; 5 Accept All; 6 Reject All (Undo); 7 Compare with Disk Document
```

### 1 Track On/Off

Switches change tracking on or off for this document. While it is on:

- everything you type is marked as **inserted**;
- everything you delete is marked as **deleted** instead of disappearing (it stays visible, struck through);
- pasting from the clipboard is marked as inserted too;
- the status line shows **[Track]**.

Inserted text is shown underlined in colour; deleted text is shown struck through. In [Reveal Codes](Reveal-Codes) you see codes like:

```
[INS:Name 2026-11-12]new words[ins]   [DEL:Name 2026-11-12]old words[del]
```

The name is your Windows user name; the date is the date of the change.

Whether tracking is on is saved with the document.

### 2 Next Change

Jumps to the next change after the cursor (and from the end back to the first). If there are none, `* No changes *` is shown.

### 3 Accept and 4 Reject

Decides the change at the cursor:

- **Accept** an insertion: the text stays, the marking goes. Accept a deletion: the text is removed.
- **Reject** an insertion: the text is removed. Reject a deletion: the text comes back as normal text.

**With a Block on**, Accept and Reject decide all changes inside the Block at once.

After accepting or rejecting, the cursor moves to the next change, so you can go through a document by pressing **Alt-F5**, **7**, **3** (or **4**) repeatedly.

### 5 Accept All and 6 Reject All

Decide all changes in the document at once, after asking `Accept all changes?` or `Reject all changes?`. The status line tells you how many were decided. **Ctrl-Z** undoes this.

### 7 Compare with Disk Document

Compares the text on the screen with a document on disk and marks the differences as tracked changes: text that is new on the screen is marked as inserted, text that only exists in the disk document is marked as deleted.

1. Press **Alt-F5**, **7**, **7**.
2. Type the name of the other document at `Other Document:` (for example an older version) and press **Enter**.
3. Look through the differences with **2 Next Change**, then accept or reject them.

You can also compare with any saved version of the same document through **Ctrl-F5**, **6** Revisions, **3** Compare with current. See [Safety: Undo, Recovery and Revisions](Safety-Undo-Recovery-and-Revisions).

### Track changes and Word

- Changes made in Word with "Track Changes" on appear here as [INS] and [DEL] codes, with the author's name and date.
- Your changes appear in Word as ordinary Word tracked changes, which your client can accept or reject there.

### Redline and strikeout (the classic way)

The older WordPerfect 5.1 way of marking changes still works too: **Ctrl-F8**, **2** Appearance, **8** Redline and **9** Strikeout, and **Alt-F5**, **6** Generate, **1** Remove Redline Markings and Strikeout Text, or **2** Compare Screen and Disk Documents and Add Redline and Strikeout. See [Mark Text](Mark-Text).

## Comments (Ctrl-F5, 4)

A **comment** is a note in the document that is shown on the screen in a double-lined box but is **never printed**. Use it for notes to yourself, questions for the client, or reminders.

Press **Ctrl-F5**, **4** (Comment):

```
Comment: 1 Create; 2 Edit; 3 Convert to Text: 0
```

### 1 Create

1. Put the cursor where the comment belongs.
2. Press **Ctrl-F5**, **4**, **1**.
3. An editing screen titled *Document Comment* opens. Type the comment.
4. Press **F7** (Exit). The comment box appears in the text, and a **[Comment]** code is placed there.

### 2 Edit

Edits the nearest comment before the cursor (or, if there is none before it, the next one after it). Change the text and press **F7**.

### 3 Convert to Text

Turns the nearest comment into ordinary document text at its place. The [Comment] code disappears.

### Turning text into a comment

1. Mark the text as a Block (Alt-F4 or F12).
2. Press **Ctrl-F5**.
3. Answer `Create a comment?` with **Y**.

The blocked text is removed from the document and becomes a comment.

### Hiding comments

To hide the comment boxes on the screen, press **Shift-F1** (Setup), **2** (Display), **8** (Comments Display), **N**. The comments are still in the document; set it back to **Y** to see them again.

### Comments and Word

Comments are exchanged with Word: comments a client writes in Word appear here, and yours appear in Word.

*Note:* inside newspaper columns, comment boxes are not shown on the editing screen in this version (the comments are still there and visible in Reveal Codes).
