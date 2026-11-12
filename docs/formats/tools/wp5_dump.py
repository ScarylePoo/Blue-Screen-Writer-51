#!/usr/bin/env python3
"""wp5_dump.py - walk a WordPerfect 5.0/5.1 document and print it Reveal-Codes style.

Reference implementation of docs/formats/wp5_document.md (read-only; no third-party deps).

usage:  python3 -I wp5_dump.py FILE [--prefix] [--hex] [--password PW] [--charsets TSV] [--stats]

  --prefix    also dump the prefix packets (summary, fonts, printer, styles, initial codes ...)
  --hex       append the raw bytes of every multi-byte function to its code
  --stats     print a histogram of codes and a list of unknown / inconsistent codes
  --password  decrypt a password-protected file (algorithm per libwpd WPXEncryption)

Output conventions: text is printed as Unicode; codes as [Name:args].  Codes that are
not shown by WordPerfect's Reveal Codes (formatter bookkeeping) are printed as {name}
so they can be distinguished.  [HRt] and [SRt] are followed by a newline.
"""
import os
import struct
import sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_TSV = os.path.join(HERE, '..', 'wp5_charsets.tsv')

FIXED_SIZE = [4, 9, 11, 3, 3, 5, 6, 7, 4, 5, 6, 6, 8, 10, 10, 12]  # 0xC0..0xCF, total bytes incl. both code bytes

ATTR_ON = ['Ext Large', 'Vry Large', 'Large', 'Small', 'Fine', 'Suprscpt', 'Subscpt', 'Outln',
           'Italc', 'Shadw', 'Redln', 'Dbl Und', 'Bold', 'StkOut', 'Und', 'Sm Cap']

SINGLE = {
    0x80: '{Temp-deleted}', 0x81: '[Rt Just On]', 0x82: '[Rt Just Off]', 0x83: '[C/A/Flrt]',
    0x84: '{0x84}', 0x85: '{Temp place saver}', 0x86: '[Center Pg]', 0x87: '[Col On]', 0x88: '[Col Off]',
    0x89: '{0x89}', 0x8A: '[W/O On]', 0x8B: '[W/O Off]', 0x8C: '[HRt-SPg]\n', 0x8D: '[Note Num]',
    0x8E: '[Box Num]', 0x8F: '[C/A/Flrt]', 0x90: '[DSRt]\n', 0x91: '[DSPg]\n', 0x92: '{EOP-deleted}',
    0x93: '[ISRt]', 0x94: '[ISRt]\n', 0x95: '[ISRt]\n', 0x96: '[Block]', 0x97: '{Block Off}',
    0x98: '{ToC Pg# placeholder}', 0x99: '[Dorm HRt]', 0x9A: '/', 0x9B: '[End Def]',
    0x9C: '{0x9C}', 0x9D: '{0x9D}', 0x9E: '[Hyph Off]', 0x9F: '[Hyph On]', 0xA0: '[ ]',
    0xA1: '+', 0xA2: 't', 0xA3: '=', 0xA4: 'T', 0xA5: '*', 0xA6: '!', 0xA7: '[Math On]',
    0xA8: '[Math Off]', 0xA9: '-', 0xAA: '-', 0xAB: '-', 0xAC: '[-soft]', 0xAD: '[-soft]\n',
    0xAE: '[-soft]\n', 0xAF: '[Col Off]', 0xB0: '[Col Off]', 0xB1: 'N', 0xB2: '[Outline Off]',
    0xB3: '[Rev Dir On]', 0xB4: '[Rev Dir Off]', 0xBD: '[Auto Code On]', 0xBE: '[Auto Code Off]',
    0xBF: '[Unknown]',
}
CONTROL = {0x02: '^B', 0x0A: '[HRt]\n', 0x0B: '[SPg]\n', 0x0C: '[HPg]\n', 0x0D: '[SRt]\n'}
MERGE_CTRL = {3: '^C', 4: '^D', 5: '^E', 6: '^F', 7: '^G', 14: '^N', 15: '^O', 16: '^P', 17: '^Q',
              18: '^R', 19: '^S', 20: '^T', 21: '^U', 22: '^V'}

MERGE = {32: 'ASSIGN', 33: 'BELL', 34: 'BREAK', 35: 'CALL', 36: 'CANCEL OFF', 37: 'CANCEL ON', 38: 'CASE',
         39: 'CASE CALL', 40: 'CHAIN MACRO', 41: 'CHAIN PRIMARY', 42: 'CHAIN SECONDARY', 43: 'CHAR',
         44: 'COMMENT', 45: 'CTON', 46: 'DATE', 47: 'DOCUMENT', 48: 'ELSE', 49: 'END FIELD', 50: 'END FOR',
         51: 'END IF', 52: 'END RECORD', 53: 'END WHILE', 54: 'FIELD', 55: 'FOR', 56: 'FOR EACH', 57: 'GO',
         58: 'IF', 59: 'IF BLANK', 60: 'IF EXISTS', 61: 'IF NOT BLANK', 62: 'KEYBOARD', 63: 'LABEL',
         64: 'LOCAL', 65: 'LOOK', 66: 'MID', 67: 'MRG CMND', 68: 'NEST MACRO', 69: 'NEST PRIMARY',
         70: 'NEST SECONDARY', 71: 'NEXT', 72: 'NEXT RECORD', 73: 'NTOC', 74: 'PROCESS', 75: 'ON CANCEL',
         76: 'ON ERROR', 77: 'PAGE OFF', 78: 'PAGE ON', 79: 'PRINT', 80: 'PROMPT', 81: 'QUIT', 82: 'RETURN',
         83: 'RETURN CANCEL', 84: 'RETURN ERROR', 85: 'REWRITE', 86: 'STEP OFF', 87: 'STEP ON',
         88: 'SUBST PRIMARY', 89: 'SUBST SECONDARY', 90: 'SYSTEM', 91: 'TEXT', 92: 'VARIABLE', 93: 'WAIT',
         94: 'WHILE', 95: 'STATUS PROMPT', 96: 'INPUT', 97: 'LEN', 98: 'FIELD NAMES', 99: 'END MRG CMNDS'}

PACKETS = {1: 'Document summary', 2: 'Fonts used (5.0)', 3: 'Document initial codes', 6: 'Document flags',
           7: 'Font name string pool', 8: 'Graphics images', 9: 'Form hash table', 12: 'Document printer',
           14: 'Supplemental dictionary', 15: 'Fonts used (5.1)', 16: 'DDE link / page count',
           17: 'Macro code', 19: 'Macro info', 21: 'OLE prefix', 0xFFFF: 'Deleted packet', 0: 'End of prefix'}


def u16(b, o):
    return struct.unpack_from('<H', b, o)[0]


def s16(b, o):
    return struct.unpack_from('<h', b, o)[0]


def u32(b, o):
    return struct.unpack_from('<I', b, o)[0]


def inch(w):
    """WPU (1/1200 in) -> WP-style inch string."""
    v = w / 1200.0
    s = ('%.3f' % v).rstrip('0').rstrip('.')
    return s + '"'


class Charsets:
    def __init__(self, path):
        self.map = {}
        if os.path.exists(path):
            for line in open(path, encoding='utf-8'):
                if line.startswith('#') or line.startswith('set\t'):
                    continue
                f = line.rstrip('\n').split('\t')
                if len(f) < 3:
                    continue
                self.map[(int(f[0]), int(f[1]))] = ''.join(chr(int(x[2:], 16)) for x in f[2].split())

    def get(self, cset, ch):
        r = self.map.get((cset, ch))
        if r is None:
            if cset == 0 and 0x20 <= ch < 0x7F:
                return chr(ch)
            return chr(0xE000 + cset * 256 + ch)  # lossless PUA fallback
        return r


def decrypt(data, password):
    pw = password.upper().encode('latin-1')
    base = len(pw) + 1
    out = bytearray(data)
    for i in range(16, len(data)):
        k = i - 16
        out[i] = data[i] ^ pw[k % len(pw)] ^ ((base + k) & 0xFF)
    return bytes(out)


def checksum(password):
    c = 0
    for ch in password.upper().encode('latin-1'):
        c = (((c >> 1) | (c << 15)) & 0xFFFF) ^ (ch << 8)
    return c


class Dumper:
    def __init__(self, data, cs, show_hex=False):
        self.d = data
        self.cs = cs
        self.hex = show_hex
        self.stats = Counter()
        self.problems = []
        self.font_names = {}    # offset -> name (packet 7)
        self.fonts = []         # packet 15/2 records -> name pool offset
        self.styles = {}

    # ---------------------------------------------------------------- strings
    def wpstr(self, b, stop_at_nul=True):
        """Decode a WP string (ASCII + 0xC0 extended chars, may contain functions)."""
        out, i = [], 0
        while i < len(b):
            c = b[i]
            if c == 0 and stop_at_nul:
                break
            if c == 0xC0 and i + 3 < len(b) and b[i + 3] == 0xC0:
                out.append(self.cs.get(b[i + 2], b[i + 1]))
                i += 4
                continue
            if 0x20 <= c < 0x7F:
                out.append(chr(c))
            elif c >= 0x80:
                return self.decode(b[:len(b) if not stop_at_nul else (b.find(b'\0') if b'\0' in b else len(b))])
            i += 1
        return ''.join(out)

    def cstr(self, b, o):
        e = b.find(b'\0', o)
        if e < 0:
            e = len(b)
        return self.wpstr(b[o:e]), e + 1

    # ---------------------------------------------------------------- body
    def decode(self, b, depth=0):
        out = []
        i, n = 0, len(b)
        while i < n:
            c = b[i]
            if 0x20 <= c <= 0x7E:
                j = i
                while j < n and 0x20 <= b[j] <= 0x7E:
                    j += 1
                out.append(b[i:j].decode('ascii'))
                self.stats['text'] += j - i
                i = j
                continue
            if c < 0x20:
                self.stats['ctl %02X' % c] += 1
                if c in CONTROL:
                    out.append(CONTROL[c])
                elif c in MERGE_CTRL:
                    out.append('[Mrg:%s]' % MERGE_CTRL[c])
                else:
                    out.append('{ctl %02X}' % c)
                    if c == 0:
                        self.problems.append('NUL byte in text at +%d' % i)
                i += 1
                continue
            if c == 0x7F:
                out.append('{7F}')
                self.problems.append('0x7F in text')
                i += 1
                continue
            if c < 0xC0:
                self.stats['sb %02X' % c] += 1
                out.append(SINGLE.get(c, '{sb %02X}' % c))
                if c not in SINGLE:
                    self.problems.append('reserved single-byte 0x%02X' % c)
                i += 1
                continue
            if c < 0xD0:
                size = FIXED_SIZE[c - 0xC0]
                if i + size > n or b[i + size - 1] != c:
                    self.problems.append('inconsistent fixed group 0x%02X at +%d' % (c, i))
                    out.append('{bad %02X}' % c)
                    i += 1
                    continue
                body = b[i + 1:i + size - 1]
                self.stats['fx %02X' % c] += 1
                s = self.fixed(c, body)
                if self.hex:
                    s += '<' + b[i:i + size].hex() + '>'
                out.append(s)
                i += size
                continue
            # variable-length group
            if i + 4 > n:
                self.problems.append('truncated variable group 0x%02X at +%d' % (c, i))
                break
            sub, ln = b[i + 1], u16(b, i + 2)
            end = i + 4 + ln
            if end > n or ln < 4 or b[end - 1] != c or b[end - 2] != sub or u16(b, end - 4) != ln:
                self.problems.append('inconsistent variable group 0x%02X/%d at +%d' % (c, sub, i))
                out.append('{bad %02X}' % c)
                i += 1
                continue
            data = b[i + 4:end - 4]
            self.stats['var %02X/%02X' % (c, sub)] += 1
            try:
                s = self.var(c, sub, data, depth)
            except Exception as e:  # keep walking on malformed contents
                self.problems.append('error decoding 0x%02X/%d: %s' % (c, sub, e))
                s = '{%02X/%d ?}' % (c, sub)
            if self.hex:
                s += '<' + b[i:end].hex() + '>'
            out.append(s)
            i = end
        return ''.join(out)

    # ---------------------------------------------------------------- fixed
    def fixed(self, c, p):
        if c == 0xC0:
            return self.cs.get(p[1], p[0])
        if c == 0xC1:
            f = p[0]
            kind = f >> 6
            hard = not (f & 0x02)
            dot = 'Dot ' if f & 0x10 else ''
            if kind == 2:
                name = '←Mar Rel'
            elif kind == 3:
                name = 'Cntr Tab' if f & 0x08 else 'Center'
            elif kind == 1:
                name = 'Flsh Rgt' if f & 0x20 else 'Dec Tab'
            else:
                name = 'Rgt Tab' if f & 0x08 else 'Tab'
            if name.endswith('Tab'):
                name = dot + name
                if hard:
                    name = name.upper()
            return '[%s]' % name
        if c == 0xC2:
            return '[→Indent←]' if p[0] & 1 else '[→Indent]'
        if c == 0xC3:
            return '[%s]' % (ATTR_ON[p[0]] if p[0] < 16 else 'Attr %d' % p[0])
        if c == 0xC4:
            return '[%s]' % (ATTR_ON[p[0]].lower() if p[0] < 16 else 'attr %d' % p[0])
        if c == 0xC5:
            return '[Block Pro:%s]' % ('Off' if p[0] & 1 else 'On')
        if c == 0xC6:
            return '{End Indent}'
        if c == 0xC7:
            return '{Hyph Disp Char}'
        self.problems.append('reserved fixed group 0x%02X' % c)
        return '{fx %02X}' % c

    # ---------------------------------------------------------------- variable
    def font_name(self, num):
        if num < len(self.fonts):
            return self.font_names.get(self.fonts[num], '?font%d' % num)
        return '?font%d' % num

    def var(self, c, s, d, depth):
        sub = lambda o: self.decode(d[o:], depth + 1)
        if c == 0xD0:
            if s == 0:
                return '[Ln Height:%s]' % ('Auto' if u16(d, 2) == 0 else inch(u16(d, 2)))
            if s == 1:
                return '[L/R Mar:%s,%s]' % (inch(u16(d, 4)), inch(u16(d, 6)))
            if s == 2:
                v = u16(d, 2)
                return '[Ln Spacing:%s]' % (('%.2f' % ((v >> 8) + (v & 0xFF) / 256.0)).rstrip('0').rstrip('.'))
            if s == 3:
                return '[HZone:%d%%,%d%%]' % (round(d[2] * 100 / 256), round(d[3] * 100 / 256))
            if s == 4:
                pos = [u16(d, 100 + 2 * k) for k in range(40)]
                types = d[180:200]
                rel = u16(d, 202) if len(d) >= 204 else 0xFFFF
                stops = []
                for k, v in enumerate(pos):
                    if v == 0xFFFF:
                        break
                    t = (types[k // 2] >> (4 if k % 2 == 0 else 0)) & 0x0F
                    vv = v - rel if rel != 0xFFFF else v
                    stops.append(inch(vv if vv < 0x8000 else vv - 0x10000) + 'LCRD'[t & 3] + ('.' if t & 4 else ''))
                return '[Tab Set:%s%s]' % ('Rel; ' if rel != 0xFFFF else 'Abs; ', ','.join(stops))
            if s == 5:
                return '[T/B Mar:%s,%s]' % (inch(u16(d, 4)), inch(u16(d, 6)))
            if s == 6:
                return '[Just:%s]' % {0: 'Left', 1: 'Full', 2: 'Center', 3: 'Right', 4: 'Decimal'}.get(d[1], d[1])
            if s == 7:
                v = d[1]
                names = [(1, 'PgNum'), (2, 'PgNum BC'), (4, 'HA'), (8, 'HB'), (16, 'FA'), (32, 'FB')]
                return '[Suppress:%s]' % ','.join(nm for bit, nm in names if v & bit)
            if s == 8:
                pos = ['None', 'Top Left', 'Top Center', 'Top Right', 'Alternating Top', 'Bottom Left',
                       'Bottom Center', 'Bottom Right', 'Alternating Bottom']
                return '[Pg Numbering:%s]' % (pos[d[3]] if d[3] < 9 else d[3])
            if s == 0x0B:
                ln, wd = u16(d, 95), u16(d, 97)
                tl = d[100]
                tname = self.wpstr(d[101:101 + min(tl, 41)])
                return '[Paper Sz/Typ:%s x %s,%s]' % (inch(wd), inch(ln), tname or 'type %d' % d[99])
        if c == 0xD1:
            if s == 0:
                return '[Color:%d,%d,%d]' % (d[3], d[4], d[5])
            if s == 1:
                fn = d[25]
                pt = ''
                if len(d) >= 30:
                    pt = ' %gpt' % (u16(d, 28) / 50.0)
                return '[Font:%s%s]' % (self.font_name(fn), pt)
            return '{Font grp %d}' % s
        if c == 0xD2:
            names = {0: '[Math Def]', 2: '[Par Num Def]', 3: '[Ftn Opt]', 4: '[End Opt]', 5: '[Fig Opt]',
                     6: '[Tbl Opt]', 7: '[Txt Opt]', 8: '[Usr Opt]', 9: '[Equ Opt]', 0x0E: '[Link End]',
                     0x0F: '[DDE Link]', 0x10: '[DDE Link End]', 0x11: '[Border Opt]'}
            if s == 1:
                v = d[97]
                ncol = v & 0x1F
                kind = 'Parallel w/Block Pro' if v & 0x80 else ('Parallel' if v & 0x40 else 'Newspaper')
                m = [inch(u16(d, 98 + 2 * k)) for k in range(2 * ncol)]
                return '[Col Def:%s;%d;%s]' % (kind, ncol, ','.join(m))
            if s == 0x0B:
                oldcols = u16(d, 2)
                base = 24 + 5 * oldcols
                flags, ncols = d[base], u16(d, base + 2)
                widths = [inch(u16(d, base + 24 + 2 * k)) for k in range(ncols)]
                pos = ['Left', 'Right', 'Center', 'Full', 'Set'][flags & 7] if (flags & 7) < 5 else '?'
                return '[Tbl Def:%s;%d,%s]' % (pos, ncols, ','.join(widths))
            if s == 0x0D:
                return '[Link:%s]' % self.wpstr(d[17:17 + d[16]])
            return names.get(s, '{Def grp %d}' % s)
        if c == 0xD3:
            if s == 0:
                return '[Decml/Algn Char:%s,%s]' % (self.charw(u16(d, 4)), self.charw(u16(d, 6)))
            if s == 1:
                v = d[1]
                return '[Undrln:%s]' % ','.join(x for bit, x in ((1, 'Spaces'), (2, 'Tabs')) if v & bit)
            if s in (2, 3):
                return '[New %s Num:%d]' % ('Ftn' if s == 2 else 'End', u16(d, 2))
            if s == 4:
                v = u16(d, 2)
                roman = (v & 0x8000) or (len(d) > 5 and d[5] == 1)
                return '[Pg Num:%s]' % (self.roman(v & 0x7FFF) if roman else v & 0x7FFF)
            if s == 5:
                v = d[5]
                return '[Ln Num:%s]' % ('On' if v & 0x80 else 'Off')
            if s == 6:
                f, v = d[0], s16(d, 3)
                if f & 1:
                    return '[AdvTo%s:%s]' % ('Pos' if f & 2 else 'Ln', inch(v))
                if f & 2:
                    return '[Adv%s:%s]' % ('Rgt' if v >= 0 else 'Lft', inch(abs(v)))
                return '[Adv%s:%s]' % ('Dn' if v >= 0 else 'Up', inch(abs(v)))
            if s == 7:
                return '[Force:%s]' % ('Odd' if d[2] else 'Even')
            if s == 8:
                return '[Bline:%s]' % ('On' if d[1] else 'Off')
            if s == 0x0A:
                return '[Wrd/Ltr Spacing:%d%%,%d%%]' % (u16(d, 4), u16(d, 6))
            if s == 0x0B:
                return '[Just Lim:%d,%d]' % (u16(d, 4), u16(d, 6))
            if 0x0C <= s <= 0x10:
                v = u16(d, 2)
                return '[New %s Num:%d%s]' % (['Fig', 'Tbl', 'Txt', 'Usr', 'Equ'][s - 0x0C], v >> 5,
                                              ('.%d' % (v & 31)) if v & 31 else '')
            if s == 0x11:
                return '[Lang:%s]' % d[2:4].decode('ascii', 'replace')
            if s == 0x12:
                return '[Pg Num Style:%s]' % self.wpstr(d[30:60])
            if s == 0x13:
                return '[Direction:%d]' % d[1]
            return '{Set grp %d}' % s
        if c == 0xD4:
            if s == 4:
                return '{Hidden text:%s}' % sub(2)
            return '{Fmt grp %d}' % s   # formatter bookkeeping, not shown in Reveal Codes
        if c == 0xD5:
            nm = ['Header A', 'Header B', 'Footer A', 'Footer B'][s] if s < 4 else 'HF %d' % s
            occ = d[7]
            occs = {0: 'Discontinue', 1: 'Every page', 2: 'Odd pages', 4: 'Even pages'}.get(occ, 'occ %d' % occ)
            txt = sub(18) if len(d) > 18 and occ else ''
            return '[%s:%s;%s]' % (nm, occs, txt)
        if c == 0xD6:
            flags, num = d[0], u16(d, 1)
            ref = chr(num) if flags & 0x80 else str(num)
            if s == 0:
                start = 4 + 2 * (d[3] + 1) + 9
                return '[Footnote:%s;%s]' % (ref, sub(start))
            return '[Endnote:%s;%s]' % (ref, sub(7))
        if c == 0xD7:
            if s in (0, 1):
                kind = {0: 'ToC', 2: 'List'}.get(d[0] >> 4, '?')
                return '[%s:%s,%d]' % ('Mark' if s == 0 else 'End Mark', kind, (d[0] & 15) + 1)
            if s == 2:
                kind = {0: 'ToC', 1: 'Index', 2: 'List', 3: 'ToA'}.get(d[0] >> 4, '?')
                return '[Def Mark:%s,%d]' % (kind, (d[0] & 15) + (0 if kind == 'Index' else 1))
            if s == 3:
                parts = d.split(b'\0', 1)
                return '[Index:%s%s]' % (self.wpstr(parts[0]), (';' + self.wpstr(parts[1])) if len(parts) > 1 and parts[1] else '')
            if s == 4:
                parts = d[1:].split(b'\0', 1)
                return '[ToA:%d;%s;%s]' % (d[0], self.wpstr(parts[0]), self.wpstr(parts[1]) if len(parts) > 1 else '')
            if s == 5:
                return '[Endnote Placement]'
            if s == 6:
                return '{Save Pg Info}'
            if s == 7:
                tag, e = self.cstr(d, 1)
                return '[Ref(%s):%s]' % (tag, ['Pg', 'Par', 'Ftn', 'End', 'Fig', 'Tbl', 'Txt', 'Usr', 'Equ'][d[0]] if d[0] < 9 else d[0])
            if s == 8:
                return '[Target(%s)]' % self.cstr(d, 0)[0]
            if s in (9, 10, 11, 12):
                return '[%s:%s]' % ({9: 'Subdoc', 10: 'Subdoc Start', 11: 'Subdoc End', 12: 'VAX path'}[s],
                                    self.cstr(d, 3)[0])
        if c == 0xD8:
            if s == 0:
                return '[Date:%s]' % self.wpstr(d)
            if s == 1:
                lvl = d[0] & 0x7F
                return '[Par Num:%s]' % (('%d' % (lvl + 1)) if d[0] & 0x80 else 'Auto')
            if s == 2:
                return '[Ovrstk:%s]' % sub(2)
            if s == 3:
                return '[Insert Pg Num:%s]' % self.wpstr(d)
        if c == 0xD9:
            if s == 0:
                return '[Ptr Cmnd:%s%s]' % ('File=' if d[0] else '', self.wpstr(d[1:]))
            if s == 1:
                return '[Cndl EOP:%d]' % d[0]
            if s == 2:
                return '[Comment]{%s}' % sub(3)
            if s == 3:
                return '[Kern:%s]' % ('On' if d[1] else 'Off')
            if s == 4:
                return '[Outline On]'
            if s == 5:
                return '[Leading Adj:%s,%s]' % (inch(s16(d, 4)), inch(s16(d, 6)))
            if s == 6:
                return '{Graphics kern %d}' % u16(d, 0)
            if s == 7:
                return '{Hidden(prod %d type %d)}' % (d[0], d[1])
            if s == 8:
                return '{Macro}'
        if c == 0xDA:
            kinds = ['Figure', 'Tbl Box', 'Text Box', 'Usr Box', 'Equation', 'HLine', 'VLine']
            kind = kinds[s] if s < 7 else 'Box %d' % s
            if s in (5, 6):
                return '[%s:%s,%s]' % (kind, inch(u16(d, 4)), inch(u16(d, 6)))
            num = u16(d, 0)
            numtxt = '%d%s' % (num >> 5, ('.%d' % (num & 31)) if num & 31 else '')
            fmt = d[48]
            fname = self.cstr(d, 49)[0]
            content = ''
            if len(d) >= 117:
                caplen = u16(d, 115)
                cap = self.decode(d[117:117 + caplen], depth + 1)
                rest = d[117 + caplen:]
                if fmt in (8, 16, 17) and rest:
                    content = self.decode(rest, depth + 1)
                elif fmt == 0x80:
                    content = '<WPG image #%d in packet 8>' % u16(d, 109)
                return '[%s:%s;%s;%s]%s' % (kind, numtxt, fname, cap, ('{' + content + '}') if content else '')
            return '[%s:%s;%s]' % (kind, numtxt, fname)
        if c == 0xDB:
            if s == 0:
                return '[Style On:%s]' % self.cstr(d, 12)[0]
            if s == 1:
                return '[Style Off:%s]' % self.cstr(d, 18)[0]
            if s == 2:
                return '[Open Style:%s]' % self.cstr(d, 3)[0]
            if s == 3:
                return '{End Style Def}'
        if c == 0xDC:
            if s == 0:
                return '[Cell]'
            if s == 1:
                return '[Row]'
            if s == 2:
                return '[Tbl Off]'
        if c == 0xDD:
            if s in (1, 3):
                return '[Row]' + ('[HPg]' if s == 3 else '')
            if s == 2:
                return '[Tbl Off]'
        if c == 0xDE:
            return '[Mrg:%s]' % MERGE.get(s, 'cmd %d' % s)
        if c == 0xDF:
            ln = u16(d, 6)
            return '{Equation:%s}' % self.wpstr(d[8:8 + ln], stop_at_nul=False)
        if c == 0xE1:
            return '{Tag grp %d}' % s
        if c == 0xFE:
            return '[Unknown]'
        self.problems.append('unknown variable group 0x%02X/%d' % (c, s))
        return '{%02X/%d}' % (c, s)

    def charw(self, w):
        return self.cs.get(w >> 8, w & 0xFF) if w else ''

    @staticmethod
    def roman(n):
        out = ''
        for v, r in ((1000, 'm'), (900, 'cm'), (500, 'd'), (400, 'cd'), (100, 'c'), (90, 'xc'), (50, 'l'),
                     (40, 'xl'), (10, 'x'), (9, 'ix'), (5, 'v'), (4, 'iv'), (1, 'i')):
            while n >= v:
                out += r
                n -= v
        return out

    # ---------------------------------------------------------------- prefix
    def read_prefix(self, verbose):
        d = self.d
        packets = []
        pos, seen = 16, set()
        while pos and pos not in seen and pos + 10 <= len(d):
            seen.add(pos)
            typ, nidx, bsize, nxt = u16(d, pos), u16(d, pos + 2), u16(d, pos + 4), u32(d, pos + 6)
            if typ != 0xFFFB:
                self.problems.append('bad index block header at %d' % pos)
                break
            for k in range(1, nidx):
                o = pos + 10 * k
                pt, ln, off = u16(d, o), u32(d, o + 2), u32(d, o + 6)
                if pt in (0,):
                    continue
                packets.append((pt, ln, off))
            pos = nxt
        for pt, ln, off in packets:
            if pt == 7:
                p = off
                while p < off + ln:
                    e = d.find(b'\0', p, off + ln)
                    if e < 0:
                        e = off + ln
                    self.font_names[p - off] = self.wpstr(d[p:e])
                    p = e + 1
            if pt in (15, 2):
                rs = font_record_size(pt, ln)
                self.fonts = [u16(d, off + rs * k + 18) for k in range(ln // rs)]
        if not verbose:
            return packets
        print('== prefix packets')
        for pt, ln, off in packets:
            name = PACKETS.get(pt, 'Style' if 0x100 <= pt <= 0x1FF else 'PS table' if 0x200 <= pt <= 0x2FF else '?')
            print('  type %5d (0x%04X) %-26s len %6d at %d' % (pt, pt, name, ln, off))
            blob = d[off:off + ln]
            if pt == 1:
                fields, p = [], 0
                while p < len(blob) and len(fields) < 9:
                    s, p = self.cstr(blob, p)
                    fields.append(s.strip())
                labels = ['created', 'descr. name', 'descr. type', 'subject', 'author', 'typist', 'abstract', 'account', 'keywords']
                for lab, val in zip(labels, fields):
                    if val:
                        print('      %-12s %s' % (lab, val[:70]))
            elif pt == 7:
                for o, nm in sorted(self.font_names.items()):
                    print('      +%-4d %s' % (o, nm))
            elif pt in (2, 15):
                rs = font_record_size(pt, ln)
                for k in range(ln // rs):
                    r = off + rs * k
                    pts = u16(d, r + (22 if pt == 2 else 47)) / 50.0
                    print('      font %d: %s  %.4gpt' % (k, self.font_names.get(u16(d, r + 18), '?'), pts))
            elif pt == 12:
                print('      printer: %r file %r' % (self.wpstr(blob[0:37]), self.wpstr(blob[37:50])))
            elif pt == 8:
                blob = d[off:]   # index length is always 2; image data follows (see spec)
                cnt = u16(blob, 0)
                print('      %d image(s): sizes %s' % (cnt, [u32(blob, 2 + 4 * k) for k in range(cnt)]))
            elif pt == 3:
                print('      codes: ' + self.decode(blob).replace('\n', '\\n'))
            elif 0x100 <= pt <= 0x1FF and ln >= 88:
                name = self.wpstr(blob[0:21]); typ = blob[21]; desc = self.wpstr(blob[22:77])
                bs, es, xs = u16(blob, 78), u16(blob, 82), u16(blob, 86)
                beg = self.decode(blob[88:88 + bs]); end = self.decode(blob[88 + bs:88 + bs + es])
                kind = 'Paired' if typ & 1 else 'Open'
                print('      style %r (%s, enter=%d) %r' % (name, kind, typ >> 6, desc))
                print('        on : ' + beg.replace('\n', '\\n'))
                if typ & 1:
                    print('        off: ' + end.replace('\n', '\\n'))
        return packets


def font_record_size(pt, ln):
    """Packet 15 records are 86 bytes.  Packet 2 (WP5.0) is documented as 86 but real files use 82."""
    if pt == 2 and (ln % 86 or ln % 82 == 0) and ln % 82 == 0:
        return 82
    return 86


def main(argv):
    args = [a for a in argv[1:] if not a.startswith('--')]
    opts = [a for a in argv[1:] if a.startswith('--')]
    pw = None
    tsv = DEFAULT_TSV
    for k, a in enumerate(argv):
        if a == '--password':
            pw = argv[k + 1]
            args.remove(pw)
        if a == '--charsets':
            tsv = argv[k + 1]
            args.remove(tsv)
    if not args:
        print(__doc__)
        return 2
    data = open(args[0], 'rb').read()
    if data[:4] != b'\xffWPC':
        print('not a WordPerfect 5.x/6.x file (no FF "WPC" signature)')
        return 1
    docoff, prod, ftype, major, minor, enc = u32(data, 4), data[8], data[9], data[10], data[11], u16(data, 12)
    print('== header: doc offset %d, product %d, file type %d, version %d.%d, encryption 0x%04X, size %d'
          % (docoff, prod, ftype, major, minor, enc, len(data)))
    if major != 0:
        print('major version %d: not a 5.x file (WP6+ uses major 2)' % major)
        return 1
    if enc:
        if not pw:
            print('document is password protected (use --password)')
            return 1
        if checksum(pw) != enc:
            print('wrong password (checksum 0x%04X)' % checksum(pw))
            return 1
        data = decrypt(data, pw)
    dm = Dumper(data, Charsets(tsv), '--hex' in opts)
    dm.read_prefix('--prefix' in opts)
    print('== body')
    sys.stdout.write(dm.decode(data[docoff:]))
    print('\n== end')
    if '--stats' in opts:
        print('== code histogram')
        for k, v in sorted(dm.stats.items()):
            print('  %-14s %d' % (k, v))
    print('== problems: %d' % len(dm.problems))
    for p in dm.problems[:50]:
        print('  ' + p)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
