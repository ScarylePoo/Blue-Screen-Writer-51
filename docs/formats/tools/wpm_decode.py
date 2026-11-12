#!/usr/bin/env python3
"""wpm_decode.py - decoder for WordPerfect 5.1 (DOS) macro (.WPM), keyboard (.WPK)
and setup (WP{WP}.SET) files.  Clean-room; tables were extracted from WP.MRS
(the WP 5.1 macro resource file) and verified against the macro descriptions.

Usage:
  python3 -I wpm_decode.py FILE...                 # print decoded listing(s)
  python3 -I wpm_decode.py --all SRC_DIR OUT_DIR    # decode every .WPM/.WPK below
                                                   # SRC_DIR into OUT_DIR/<NAME>.txt
                                                   # and print a coverage report
See ../wpm_wpk_set.md for the format specification.
"""
import os, re, struct, sys, collections

# ---------------------------------------------------------------- name tables
# Macro programming commands, 0xFC00 + n.  Names from WP.MRS name table
# (16-byte Pascal strings at file offset 0x37C).  Display form is "{NAME}".
CMD_NAMES = {
    0xFC00: 'INVALID',
    0xFC01: 'ASSIGN',
    0xFC02: 'BELL',
    0xFC03: 'BREAK',
    0xFC04: 'CALL',
    0xFC05: 'CANCEL OFF',
    0xFC06: 'CANCEL ON',
    0xFC07: 'CASE',
    0xFC08: 'CASE CALL',
    0xFC09: 'CHAIN',
    0xFC0A: 'CHAR',
    0xFC0B: ';',
    0xFC0C: 'DISPLAY OFF',
    0xFC0D: 'DISPLAY ON',
    0xFC0E: 'ELSE',
    0xFC0F: 'END FOR',
    0xFC10: 'END IF',
    0xFC11: 'END WHILE',
    0xFC12: 'FOR',
    0xFC13: 'FOR EACH',
    0xFC14: 'GO',
    0xFC15: 'IF',
    0xFC16: 'LABEL',
    0xFC17: 'LOOK',
    0xFC18: 'NEST',
    0xFC19: 'NEXT',
    0xFC1A: 'SHELL MACRO',
    0xFC1B: 'ON CANCEL',
    0xFC1C: 'ON ERROR',
    0xFC1D: 'ON NOT FOUND',
    0xFC1E: 'PAUSE',
    0xFC1F: 'PROMPT',
    0xFC20: 'QUIT',
    0xFC21: 'RESTART',
    0xFC22: 'RETURN',
    0xFC23: 'RETURN CANCEL',
    0xFC24: 'RETURN ERROR',
    0xFC25: 'RETURN NOT FND',
    0xFC26: 'SPEED',
    0xFC27: 'STEP ON',
    0xFC28: 'TEXT',
    0xFC29: 'STATE',
    0xFC2A: 'WAIT',
    0xFC2B: 'WHILE',
    0xFC2C: 'Macro Commands',
    0xFC2D: 'STEP OFF',
    0xFC2E: 'ORIGINAL KEY',
    0xFC2F: 'IF EXISTS',
    0xFC30: 'MENU OFF',
    0xFC31: 'MENU ON',
    0xFC32: 'STATUS PROMPT',
    0xFC33: 'INPUT',
    0xFC34: 'VARIABLE',
    0xFC35: 'SYSTEM',
    0xFC36: 'MID',
    0xFC37: 'NTOK',
    0xFC38: 'KTON',
    0xFC39: 'LEN',
    0xFC3A: '~',
    0xFC3B: 'PAUSE KEY',
    0xFC3C: 'UNKNOWN',
    0xFC3D: 'OTHERWISE',
    0xFC3E: 'SHELL ASSIGN',
    0xFC3F: 'SHELL VARIABLE',
}
# The editor (and the WP.MRS command list) spells this one out in full:
CMD_DISPLAY = dict(CMD_NAMES)
CMD_DISPLAY[0xFC25] = 'RETURN NOT FOUND'

# Number of '~'-terminated arguments of each command.  -1 = variable list
# terminated by an empty argument (an extra '~').  CASE/CASE CALL: expr~ then
# (case~label~)* then ~ ;  FOR EACH: var~ then expr~* then ~.
CMD_ARGS = {0xFC01: 2, 0xFC04: 1, 0xFC07: -1, 0xFC08: -1, 0xFC09: 1, 0xFC0A: 2,
            0xFC0B: 1, 0xFC12: 4, 0xFC13: -1, 0xFC14: 1, 0xFC15: 1, 0xFC16: 1,
            0xFC17: 1, 0xFC18: 1, 0xFC1A: 1, 0xFC1B: 1, 0xFC1C: 1, 0xFC1D: 1,
            0xFC1F: 1, 0xFC26: 1, 0xFC28: 2, 0xFC2A: 1, 0xFC2B: 1, 0xFC2F: 1,
            0xFC32: 1, 0xFC33: 1, 0xFC34: 1, 0xFC35: 1, 0xFC36: 3, 0xFC37: 1,
            0xFC38: 1, 0xFC39: 1, 0xFC3B: 1, 0xFC3E: 2, 0xFC3F: 1}

# Keys / editing functions, 0x8000 + n.  Names from WP.MRS (offset 0x77C).
KEY_NAMES = {
    0x8000: 'Compose',
    0x8001: '^A',
    0x8002: '^B',
    0x8003: '^C',
    0x8004: '^D',
    0x8005: '^E',
    0x8006: '^F',
    0x8007: '^G',
    0x8008: 'Home',
    0x8009: 'Tab',
    0x800A: 'Enter',
    0x800B: 'Del to EOL',
    0x800C: 'Del to EOP',
    0x800D: '^M',
    0x800E: '^N',
    0x800F: '^O',
    0x8010: '^P',
    0x8011: '^Q',
    0x8012: '^R',
    0x8013: '^S',
    0x8014: '^T',
    0x8015: '^U',
    0x8016: '^V',
    0x8017: 'Up',
    0x8018: 'Right',
    0x8019: 'Left',
    0x801A: 'Down',
    0x801B: 'Esc',
    0x801C: '^\\',
    0x801D: '^]',
    0x801E: 'Keyboard',
    0x801F: '^_',
    0x8020: 'Cancel',
    0x8021: 'Search',
    0x8022: 'Help',
    0x8023: 'Indent',
    0x8024: 'List',
    0x8025: 'Bold',
    0x8026: 'Exit',
    0x8027: 'Underline',
    0x8028: 'End Field',
    0x8029: 'Save',
    0x802A: 'F11',
    0x802B: 'F12',
    0x802C: 'Setup',
    0x802D: 'Search Left',
    0x802E: 'Switch',
    0x802F: 'L/R Indent',
    0x8030: 'Date/Outline',
    0x8031: 'Center',
    0x8032: 'Print',
    0x8033: 'Format',
    0x8034: 'Merge Codes',
    0x8035: 'Retrieve',
    0x8036: 'Shft F11',
    0x8037: 'Shft F12',
    0x8038: 'Thesaurus',
    0x8039: 'Replace',
    0x803A: 'Reveal Codes',
    0x803B: 'Block',
    0x803C: 'Mark Text',
    0x803D: 'Flush Right',
    0x803E: 'Columns/Tables',
    0x803F: 'Style',
    0x8040: 'Graphics',
    0x8041: 'Macro',
    0x8042: 'Alt F11',
    0x8043: 'ALT F12',
    0x8044: 'Shell',
    0x8045: 'Spell',
    0x8046: 'Screen',
    0x8047: 'Move',
    0x8048: 'Text In/Out',
    0x8049: 'Tab Align',
    0x804A: 'Footnote',
    0x804B: 'Font',
    0x804C: 'Merge/Sort',
    0x804D: 'Macro Define',
    0x804E: 'Ctrl F11',
    0x804F: 'Ctrl F12',
    0x8050: 'Backspace',
    0x8051: 'Del',
    0x8052: 'Del Word',
    0x8053: 'Word Right',
    0x8054: 'Word Left',
    0x8055: 'End',
    0x8056: 'Home-Home-Left',
    0x8057: 'Invalid',
    0x8058: 'Goto',
    0x8059: 'Page Up',
    0x805A: 'Page Down',
    0x805B: 'Screen Down',
    0x805C: 'Screen Up',
    0x805D: 'Typeover',
    0x805E: 'Left Mar Rel',
    0x805F: 'HPg',
    0x8060: 'SHy',
    0x8061: '-',
    0x8062: ' ',
    0x8063: 'Para Up',
    0x8064: 'Para Down',
    0x8065: 'Item Left',
    0x8066: 'Item Right',
    0x8067: 'Item Up',
    0x8068: 'Item Down',
    0x8069: 'Alt Home',
    0x806A: 'Del Word (Row)',
    0x806B: 'Menu Bar',
    0x806C: 'Block Append',
    0x806D: 'Block Move',
    0x806E: 'Block Copy',
    0x806F: 'INVALID',
    0x8070: 'MOUSE MOTION',
    0x8071: 'MOUSE L BUTTON',
    0x8072: 'MOUSE M BUTTON',
    0x8073: 'MOUSE R BUTTON',
    0x8074: 'INVALID',
}

# WordPerfect character sets (high byte 1..12, low byte = index).  Only the
# entries verified against macro descriptions / well known are mapped; other
# characters are shown as [WP s,c].
WP_CHARSET = {}
for _i, _c in enumerate("ÁáÂâÄäÀàÅåÆæÇçÉéÊêËëÈèÍíÎîÏïÌìÑñÓóÔôÖöÒòÚúÛûÜüÙùŸÿÃãĐđØøÕõ"):
    WP_CHARSET[(1, 26 + _i)] = _c
WP_CHARSET[(1, 23)] = 'ß'
WP_CHARSET[(4, 0)] = '•'
WP_CHARSET[(4, 7)] = '¡'
WP_CHARSET[(4, 8)] = '¿'
# verified from DE_DOS.WPK (German AltGr / shifted keys) and DEG.WPM ("degree sign")
WP_CHARSET[(4, 6)] = '§'
WP_CHARSET[(4, 20)] = '²'
WP_CHARSET[(4, 21)] = '³'
WP_CHARSET[(6, 36)] = '°'
WP_CHARSET[(6, 37)] = 'µ'


def wpchar(s, c):
    return WP_CHARSET.get((s, c), '[WP %d,%d]' % (s, c))

# ------------------------------------------------------------- token render
def token_text(w):
    """Render one 16-bit macro word the way the WP 5.1 macro editor shows it."""
    if w == 0x0009:
        return '\t'              # editor formatting tab (ignored on playback)
    if w == 0x000A:
        return '\n'              # editor hard line break (ignored on playback)
    if w == 0x000D:
        return '\n'              # editor soft line break (ignored on playback)
    if 0x20 <= w <= 0x7E:
        return chr(w)
    if 0x0100 <= w <= 0x0CFF:
        return wpchar(w >> 8, w & 0xFF)
    if w in KEY_NAMES:
        return '{%s}' % KEY_NAMES[w]
    if w in CMD_DISPLAY:
        return '{%s}' % CMD_DISPLAY[w]
    if 0xFD00 <= w <= 0xFDFF:
        return '{KEY MACRO %d}' % (w & 0xFF)
    if 0xFE00 <= w <= 0xFEFF:
        c = w & 0xFF
        return '{ALT %s}' % (chr(c) if 0x20 < c < 0x7F else '%02X' % c)
    if 0xFF00 <= w <= 0xFF09:
        return '{VAR %d}' % (w & 0xFF)
    return None


def is_known(w):
    return token_text(w) is not None and w not in (0xFC00, 0xFC3C, 0x806F, 0x8074, 0x8057)


def render(words):
    out = []
    for w in words:
        t = token_text(w)
        out.append(t if t is not None else '{?%04X}' % w)
    return ''.join(out)

# --------------------------------------------------------- structural parse
def parse_structure(words):
    """Group the flat token stream into commands + '~' arguments, recursively.
    Returns (problems:list[str]).  Used to validate the argument model."""
    problems = []
    i = 0
    n = len(words)

    def parse_arg(i, depth):
        # read tokens until an unmatched '~' (0x7E); nested commands with
        # arguments consume their own tildes.
        while i < n:
            w = words[i]
            if w == 0x7E:
                return i + 1, True
            if w in CMD_ARGS:
                i = parse_cmd(i, depth + 1)
                continue
            i += 1
        return i, False

    def parse_cmd(i, depth):
        w = words[i]
        i += 1
        k = CMD_ARGS[w]
        if k >= 0:
            for a in range(k):
                i, ok = parse_arg(i, depth)
                if not ok:
                    problems.append('unterminated arg %d of {%s} (depth %d)' % (a + 1, CMD_NAMES[w], depth))
                    return i
        else:
            # first argument(s): CASE/CASE CALL expr; FOR EACH var
            i, ok = parse_arg(i, depth)
            while ok:
                j = i
                while j < n and words[j] in (0x09, 0x0A, 0x0D, 0xFC0B):
                    if words[j] == 0xFC0B:       # {;}comment~ between items
                        j, _ = parse_arg(j + 1, depth + 1)
                    else:
                        j += 1                   # editor formatting is ignored
                if j < n and words[j] == 0x7E:   # empty argument ends the list
                    return j + 1
                i, ok = parse_arg(i, depth)
            problems.append('unterminated list of {%s}' % CMD_NAMES[w])
        return i

    while i < n:
        if words[i] in CMD_ARGS:
            i = parse_cmd(i, 0)
        else:
            i += 1
    return problems

# ------------------------------------------------------------ WPC container
class WPCFile:
    def __init__(self, data):
        if data[:4] != b'\xffWPC':
            raise ValueError('not a WordPerfect Corp. file')
        self.data = data
        (self.body_off,) = struct.unpack_from('<I', data, 4)
        self.product, self.ftype, self.major, self.minor = data[8], data[9], data[10], data[11]
        (self.encryption,) = struct.unpack_from('<H', data, 12)
        self.packets = []      # (type, length, offset)
        pos = 16
        seen = set()
        while pos and pos < len(data) and pos not in seen:
            seen.add(pos)
            t, cnt, size, nxt = struct.unpack_from('<HHHI', data, pos)
            if t != 0xFFFB:
                break
            for k in range(1, cnt):
                pt, ln, off = struct.unpack_from('<HII', data, pos + 10 * k)
                if pt or ln or off:
                    self.packets.append((pt, ln, off))
            pos = nxt

    def packet(self, ptype):
        for t, ln, off in self.packets:
            if t == ptype:
                return self.data[off:off + ln]
        return None


def wp_string(b):
    """Decode a WP-document-encoded, NUL terminated string (descriptions).
    0xC0 c s 0xC0 = WP extended character (set s, index c)."""
    out = []
    i = 0
    while i < len(b) and b[i] != 0:
        if b[i] == 0xC0 and i + 3 < len(b) and b[i + 3] == 0xC0:
            out.append(wpchar(b[i + 2], b[i + 1]))
            i += 4
            continue
        out.append(chr(b[i]) if 0x20 <= b[i] < 0x7F else '\\x%02x' % b[i])
        i += 1
    return ''.join(out)


def words_of(b):
    """Little-endian u16 words up to (not including) the 0x0000 terminator."""
    w = []
    for i in range(0, len(b) - 1, 2):
        (x,) = struct.unpack_from('<H', b, i)
        if x == 0:
            break
        w.append(x)
    return w

# ---------------------------------------------------------------- keyboards
def _scan_names():
    n = {0x0F: 'Shift-Tab', 0x1A: 'Alt-[', 0x1B: 'Alt-]', 0x1C: 'Alt-Enter',
         0x27: 'Alt-;', 0x28: "Alt-'", 0x29: 'Alt-`', 0x2B: 'Alt-\\', 0x33: 'Alt-,',
         0x34: 'Alt-.', 0x35: 'Alt-/', 0x37: 'Alt-Keypad*', 0x47: 'Home', 0x48: 'Up',
         0x49: 'PgUp', 0x4A: 'Alt-Keypad-', 0x4B: 'Left', 0x4C: 'Keypad5', 0x4D: 'Right',
         0x4E: 'Alt-Keypad+', 0x4F: 'End', 0x50: 'Down', 0x51: 'PgDn', 0x52: 'Ins',
         0x53: 'Del', 0x72: 'Ctrl-PrtSc', 0x73: 'Ctrl-Left', 0x74: 'Ctrl-Right',
         0x75: 'Ctrl-End', 0x76: 'Ctrl-PgDn', 0x77: 'Ctrl-Home', 0x82: 'Alt--',
         0x83: 'Alt-=', 0x84: 'Ctrl-PgUp', 0x85: 'F11', 0x86: 'F12', 0x87: 'Shift-F11',
         0x88: 'Shift-F12', 0x89: 'Ctrl-F11', 0x8A: 'Ctrl-F12', 0x8B: 'Alt-F11',
         0x8C: 'Alt-F12', 0x8D: 'Ctrl-Up', 0x8E: 'Ctrl-Keypad-', 0x8F: 'Ctrl-Keypad5',
         0x90: 'Ctrl-Keypad+', 0x91: 'Ctrl-Down', 0x92: 'Ctrl-Ins', 0x93: 'Ctrl-Del',
         0x94: 'Ctrl-Tab', 0x95: 'Ctrl-Keypad/', 0x96: 'Ctrl-Keypad*', 0x97: 'Alt-Home',
         0x98: 'Alt-Up', 0x99: 'Alt-PgUp', 0x9B: 'Alt-Left', 0x9D: 'Alt-Right',
         0x9F: 'Alt-End', 0xA0: 'Alt-Down', 0xA1: 'Alt-PgDn', 0xA2: 'Alt-Ins',
         0xA3: 'Alt-Del', 0xA4: 'Alt-Keypad/', 0xA5: 'Alt-Tab', 0xA6: 'Alt-KeypadEnter'}
    for i, c in enumerate('QWERTYUIOP'):
        n[0x10 + i] = 'Alt-' + c
    for i, c in enumerate('ASDFGHJKL'):
        n[0x1E + i] = 'Alt-' + c
    for i, c in enumerate('ZXCVBNM'):
        n[0x2C + i] = 'Alt-' + c
    for i in range(10):
        n[0x3B + i] = 'F%d' % (i + 1)
        n[0x54 + i] = 'Shift-F%d' % (i + 1)
        n[0x5E + i] = 'Ctrl-F%d' % (i + 1)
        n[0x68 + i] = 'Alt-F%d' % (i + 1)
        n[0x78 + i] = 'Alt-%d' % ((i + 1) % 10)
    return n

SCAN_NAMES = _scan_names()


def ascii_key_name(c):
    if c < 0x20:
        return 'Ctrl-' + chr(c + 0x40)
    if c == 0x7F:
        return 'Ctrl-Backspace'
    return repr(chr(c))


def pair_key_name(a, s):
    if a == 0xE0:
        return 'Grey ' + SCAN_NAMES.get(s, 'scan %02X' % s)
    special = {(0x1B, 0x01): 'Esc', (0x08, 0x0E): 'Backspace', (0x09, 0x0F): 'Tab',
               (0x0A, 0x1C): 'Ctrl-Enter', (0x0D, 0x1C): 'Enter',
               (0x0A, 0xE0): 'Ctrl-KeypadEnter', (0x0D, 0xE0): 'KeypadEnter',
               (0x2A, 0x37): 'Keypad*', (0x2D, 0x4A): 'Keypad-', (0x2B, 0x4E): 'Keypad+',
               (0x2F, 0xE0): 'Keypad/'}
    return special.get((a, s), 'ascii %02X scan %02X' % (a, s))


def value_text(v):
    if v == 0xFFFF:
        return '(not a key)'
    if v == 0x0000:
        return '(unassigned)'
    if v == 0x7FFF:
        return '(0x7FFF special)'
    t = token_text(v)
    return t if t is not None else '{?%04X}' % v


def parse_keytable(p):
    """Return list of (byte_offset_in_table, key_label, value)."""
    out = []
    pos = 0
    for blk in range(2):           # two range tables: extended scans, ASCII
        first, last = p[pos], p[pos + 1]
        pos += 2
        for code in range(first, last + 1):
            (v,) = struct.unpack_from('<H', p, pos)
            name = SCAN_NAMES.get(code, 'ext scan %02X' % code) if blk == 0 else ascii_key_name(code)
            out.append((pos, name, v))
            pos += 2
    lists = ['E0-prefixed (grey) keys', 'ASCII+scan overrides', 'keypad, NumLock off',
             'keypad, NumLock on']
    li = 0
    while pos + 4 <= len(p):
        a, s = p[pos], p[pos + 1]
        (v,) = struct.unpack_from('<H', p, pos + 2)
        if a == 0xFF and s == 0xFF:
            li += 1
            pos += 4
            continue
        out.append((pos + 2, pair_key_name(a, s) + ' [%s]' % lists[min(li, 3)], v))
        pos += 4
    return out


def parse_wpk_macros(p):
    """Packet 2: u8 count, then {u8 number, u16 byte length, words...0000}."""
    res = {}
    if not p:
        return res
    i = 1
    for _ in range(p[0]):
        num, ln = struct.unpack_from('<BH', p, i)
        res[num] = words_of(p[i + 3:i + 3 + ln])
        i += 3 + ln
    return res


def parse_wpk_desc(p):
    """Packet 3: 45-byte records {u8 ascii, u8 scan, u16 table offset,
    u8 len (incl NUL), char[40] WP-encoded description}."""
    res = []
    if not p:
        return res
    for j in range(0, len(p) - 44, 45):
        a, s = p[j], p[j + 1]
        (off,) = struct.unpack_from('<H', p, j + 2)
        ln = p[j + 4]
        res.append((a, s, off, wp_string(p[j + 5:j + 5 + ln])))
    return res

# --------------------------------------------------------------- listings
def decode_wpm(path):
    f = WPCFile(open(path, 'rb').read())
    desc = wp_string(f.packet(1) or b'')
    words = words_of(f.data[f.body_off:])
    return f, desc, words


def listing_wpm(path):
    f, desc, words = decode_wpm(path)
    hdr = ('; file: %s\n; type: macro (product %d, file type %d, version %d.%d)\n; description: %s\n'
           % (os.path.basename(path), f.product, f.ftype, f.major, f.minor, desc))
    return hdr + '\n' + render(words) + '\n', {os.path.basename(path): words}


def listing_wpk(path):
    f = WPCFile(open(path, 'rb').read())
    table = f.packet(1)
    macros = parse_wpk_macros(f.packet(2))
    descs = parse_wpk_desc(f.packet(3))
    dmap = {d[2]: d[3] for d in descs}
    lines = ['; file: %s' % os.path.basename(path),
             '; type: keyboard definition (product %d, file type %d, version %d.%d)'
             % (f.product, f.ftype, f.major, f.minor), '',
             '== Key table (packet 1): physical key -> action ==']
    allwords = {}
    default = parse_keytable(DEFAULT_TABLE) if DEFAULT_TABLE else None
    dvals = {o: v for o, _, v in default} if default else {}
    for off, name, v in parse_keytable(table):
        mark = '' if dvals.get(off) == v else '   *changed from default ' + value_text(dvals.get(off, 0))
        d = dmap.get(off)
        lines.append('  [%03X] %-40s %s%s%s' % (off, name, value_text(v),
                                              ('   ; ' + d) if d else '', mark))
    lines += ['', '== Key descriptions (packet 3) ==']
    for a, s, off, d in descs:
        lines.append('  key %02X/%02X  table+%03X  %s' % (a, s, off, d))
    lines += ['', '== Keyboard macros (packet 2) ==']
    for num in sorted(macros):
        lines.append('-- {KEY MACRO %d} --' % num)
        lines.append(render(macros[num]))
        lines.append('')
        allwords['%s#KEY MACRO %d' % (os.path.basename(path), num)] = macros[num]
    return '\n'.join(lines) + '\n', allwords


DEFAULT_TABLE = None
# Built-in default key table (WP.MRS packet 2, identical to US.WPK packet 1)
DEFAULT_TABLE = bytes.fromhex(
    '0fa65e8051fe57fe45fe52fe54fe59fe55fe49fe4ffe50fe000000000000ffff41fe53fe44fe46fe47fe48fe4afe4bfe'
    '4cfe000000000000ffff00005afe58fe43fe56fe42fe4efe4dfe000000000000ffff0000ffffffffffff208021802280'
    '2380248025802680278028802980ffffffff088017805980ff7f198000001880000055801a805a805d8051802c802d80'
    '2e802f803080318032803380348035804480458046804780488049804a804b804c804d80388039803a803b803c803d80'
    '3e803f80408041803280548053800b800c80588001ff02ff03ff04ff05ff06ff07ff08ff09ff00ff61806b802cfc3a80'
    '3b80000000000000000000000000638000000000000064806e806d80000000000000698067800000ffff6580ffff6680'
    'ffff00006880000000000000000000000000017f0180028003800480058006800780088009800a800b800c800d800e80'
    '0f8010801180128013801480158016801780188019801a801b801c801d80ffff60802000210022002300240025002600'
    '2700280029002a002b002c002d002e002f0030003100320033003400350036003700380039003a003b003c003d003e00'
    '3f0040004100420043004400450046004700480049004a004b004c004d004e004f005000510052005300540055005600'
    '5700580059005a005b005c005d005e005f0060006100620063006400650066006700680069006a006b006c006d006e00'
    '6f0070007100720073007400750076007700780079007a007b007c007d007e005280e0470880e0481780e0495980e04b'
    '1980e04d1880e04f5580e0501a80e0515a80e0525d80e0535180e0735480e0745380e0750b80e0760c80e0775880e084'
    '2cfce08d6380e0916480e0926e80e0936d80ffff00001b011b80080e5080090f09800a1c5f800d1c0a800ae05f800de0'
    '0a80ffff00002a372a002d4a5c802b4e5b802fe02f00ffff00002a372a002d4a2d002b4e2b002fe02f00ffff0000'
)

# --------------------------------------------------------------- WP{WP}.SET
SET_PACKETS = {
    0x0005: 'registration / license number (ASCII, NUL padded)',
    0x0021: 'screen colour/attribute tables (2 x 155 bytes)',
    0x0024: 'screen colour/attribute tables (2 x 155 bytes)',
    0x0051: 'unknown (9 bytes)',
    0x0052: 'display / environment (graphics + text drivers, mouse, doc-summary text, units)',
    0x0053: 'keyboard layout file name (.WPK), empty = original keyboard',
    0x0054: 'currently selected printer (name, .PRS file, printer data)',
    0x0055: 'initial settings (date format string, ...)',
    0x0056: 'unknown (23 bytes)',
    0x0057: 'printer-related state (unknown)',
    0x0059: 'initial base font name of current printer',
    0x005A: 'empty in sample',
    0x005C: 'list of printer definitions (name + .PRS + data)',
    0x005D: 'font / character width table (printer)',
    0x005E: 'location of files (10 path fields)',
}
LOCATION_FIELDS = [(69, 'Printer files (inferred)'), (69, 'Thesaurus/Spell/Hyph main (inferred)'),
                   (69, 'Thesaurus/Spell/Hyph supplementary (inferred)'), (69, 'Backup files'),
                   (82, 'Style library filename'), (69, 'Keyboard/Macro files'),
                   (69, 'Graphic files'), (69, 'Documents'), (69, 'Style files directory'),
                   (69, 'unknown / unused')]


def listing_set(path):
    f = WPCFile(open(path, 'rb').read())
    lines = ['; file: %s  (product %d, file type 0x%02X, version %d.%d)'
             % (os.path.basename(path), f.product, f.ftype, f.major, f.minor)]
    for t, ln, off in f.packets:
        lines.append('packet 0x%04X len %4d @%5d  %s' % (t, ln, off, SET_PACKETS.get(t, '?')))
    p = f.packet(0x5E)
    if p:
        pos = 0
        for size, label in LOCATION_FIELDS:
            if pos + 2 > len(p):
                break
            (n,) = struct.unpack_from('<H', p, pos)
            lines.append('  location %-46s %s' % (label + ':', p[pos + 2:pos + 2 + n].decode('latin1')))
            pos += size
    for t, label in ((0x53, 'keyboard'), (0x54, 'printer'), (0x59, 'initial font')):
        p = f.packet(t)
        if p:
            lines.append('  %-14s %s' % (label + ':', p.split(b'\0')[0].decode('latin1')))
    p = f.packet(0x55)
    if p:
        lines.append('  date format:   %s' % p[3:].split(b'\0')[0].decode('latin1'))
    for t in (0x21, 0x24):
        p = f.packet(t)
        if not p:
            continue
        for tb in range(2):
            base = tb * 155
            cnt = p[base]
            ents = []
            for k in range(cnt):
                mask, col = struct.unpack_from('<IB', p, base + 1 + 5 * k)
                ents.append('%X:%02X' % (mask, col))
            lines.append('  colours pkt 0x%02X table %d (attrmask:PCattr): %s' % (t, tb, ' '.join(ents)))
    return '\n'.join(lines) + '\n'

# ------------------------------------------------------------------ main
def find_files(src):
    res = []
    for root, dirs, files in os.walk(src):
        for fn in files:
            if fn.upper().endswith(('.WPM', '.WPK')):
                res.append(os.path.join(root, fn))
    return sorted(res, key=lambda p: p.upper())


def run_all(src, outdir):
    os.makedirs(outdir, exist_ok=True)
    usage = collections.defaultdict(set)     # word -> set(file)
    unknown = collections.defaultdict(set)
    problems = {}
    nfiles = ntok = 0
    allf = find_files(src)
    cnt = collections.Counter(os.path.basename(p).upper() for p in allf)
    dups = {k for k, v in cnt.items() if v > 1}
    for p in allf:
        rel = os.path.relpath(p, src)
        try:
            text, wordsets = (listing_wpk if p.upper().endswith('.WPK') else listing_wpm)(p)
        except Exception as e:                       # pragma: no cover
            problems[rel] = ['FAILED: %r' % e]
            continue
        nfiles += 1
        base = os.path.basename(p)
        # root-level files keep their bare name; a same-named file in a
        # sub-directory gets the directory as prefix (e.g. WPM_SUP.WPM)
        name = rel.replace(os.sep, '_') if (base.upper() in dups and os.sep in rel) else base
        with open(os.path.join(outdir, name + '.txt'), 'w', encoding='utf-8') as fh:
            fh.write(text)
        for key, words in wordsets.items():
            ntok += len(words)
            for w in words:
                usage[w].add(rel)
                if not is_known(w):
                    unknown[w].add(rel)
            pr = parse_structure(words)
            if pr:
                problems[rel + ('' if key == base else ' ' + key.split('#')[1])] = pr
    return nfiles, ntok, usage, unknown, problems


def main(argv):
    if len(argv) >= 3 and argv[0] == '--all':
        nfiles, ntok, usage, unknown, problems = run_all(argv[1], argv[2])
        print('files decoded: %d, macro words: %d' % (nfiles, ntok))
        print('distinct word values: %d, unknown: %d' % (len(usage), len(unknown)))
        for w in sorted(unknown):
            print('  UNKNOWN %04X in %s' % (w, ', '.join(sorted(unknown[w]))))
        for f, pr in sorted(problems.items()):
            print('  STRUCTURE %s: %s' % (f, '; '.join(pr)))
        return
    for p in argv:
        if p.upper().endswith('.SET'):
            print(listing_set(p))
        elif p.upper().endswith('.WPK'):
            print(listing_wpk(p)[0])
        else:
            print(listing_wpm(p)[0])


if __name__ == '__main__':
    main(sys.argv[1:])
