"""Builds ja-reading.knd, the Japanese reading dictionary Kuro Nesos downloads once to read
kanji in lyrics (see src/jdict.rs), from the IPADIC sources (mecab-ipadic-2.7.0).

    python tools/make_ja_dict.py <mecab-ipadic dir> <out file>

The IPADIC sources are in https://github.com/taku910/mecab (directory mecab-ipadic,
EUC-JP encoded). Their license (NAIST, see COPYING there) requires keeping the copyright
notice, which is embedded at the end of the output file.

File layout (little endian):
    header   magic "KNJD", version u32, entries u32, lsize u16, rsize u16,
             index_off u32, data_off u32, matrix_off u32, notice_off u32, total u32,
             unknown-word table: 8 x (left u16, right u16, cost i16)
    index    entries x u32 (offset of each entry in the data section), sorted by surface
    data     per entry: left u16, right u16, cost i16, flags u8, surface_len u8,
             reading_len u8, surface (UTF-8), reading (UTF-8 katakana; empty with KANA_SAME)
    matrix   rsize x lsize i16 connection costs: cost(prev.right, next.left)
    notice   IPADIC copyright notice (UTF-8)
"""
import os
import struct
import sys

ATTACH, PARTICLE, KANA_SAME = 1, 2, 4
# unknown-word classes, in the order src/jdict.rs uses them
CLASSES = ['KANJI', 'HIRAGANA', 'KATAKANA', 'ALPHA', 'NUMERIC', 'SYMBOL', 'SPACE', 'DEFAULT']


def katakana(s):
    return ''.join(chr(ord(c) + 0x60) if 0x3041 <= ord(c) <= 0x3096 else c for c in s)


def flags_for(pos1, pos2):
    if pos1 == '助動詞' or (pos1 == '助詞' and pos2 == '接続助詞') or (pos1 in ('動詞', '名詞', '形容詞') and pos2 == '接尾'):
        return ATTACH
    if pos1 == '助詞':
        return PARTICLE
    return 0


def main(src, out):
    rows = []
    for name in sorted(os.listdir(src)):
        if not name.endswith('.csv'):
            continue
        with open(os.path.join(src, name), encoding='euc_jp') as f:
            for line in f:
                c = line.rstrip('\n').split(',')
                surface, left, right, cost = c[0], int(c[1]), int(c[2]), int(c[3])
                reading = c[11] if len(c) > 11 and c[11] != '*' else ''
                flags = flags_for(c[4], c[5])
                if not reading or reading == katakana(surface):
                    flags |= KANA_SAME
                    reading = ''
                sb, rb = surface.encode('utf-8'), reading.encode('utf-8')
                if len(sb) > 255 or len(rb) > 255:
                    continue
                rows.append((sb, cost, left, right, flags, rb))
    rows.sort(key=lambda r: (r[0], r[1]))

    with open(os.path.join(src, 'matrix.def'), encoding='ascii') as f:
        rsize, lsize = map(int, f.readline().split())
        matrix = [0] * (rsize * lsize)
        for line in f:
            a, b, v = line.split()
            matrix[int(a) * lsize + int(b)] = int(v)

    unk = {}
    with open(os.path.join(src, 'unk.def'), encoding='euc_jp') as f:
        for line in f:
            c = line.split(',')
            unk.setdefault(c[0], (int(c[1]), int(c[2]), int(c[3])))
    notice = open(os.path.join(src, 'COPYING'), encoding='euc_jp', errors='replace').read().encode('utf-8')

    data = bytearray()
    index = []
    for sb, cost, left, right, flags, rb in rows:
        index.append(len(data))
        data += struct.pack('<HHhBBB', left, right, max(-32768, min(32767, cost)), flags, len(sb), len(rb)) + sb + rb

    head_len = 4 + 4 + 4 + 2 + 2 + 5 * 4 + len(CLASSES) * 6
    index_off = head_len
    data_off = index_off + 4 * len(index)
    matrix_off = data_off + len(data)
    notice_off = matrix_off + 2 * len(matrix)
    total = notice_off + len(notice)
    with open(out, 'wb') as f:
        f.write(b'KNJD' + struct.pack('<IIHH', 1, len(rows), lsize, rsize))
        f.write(struct.pack('<5I', index_off, data_off, matrix_off, notice_off, total))
        for cls in CLASSES:
            f.write(struct.pack('<HHh', *unk.get(cls, unk['DEFAULT'])))
        f.write(struct.pack(f'<{len(index)}I', *index))
        f.write(data)
        f.write(struct.pack(f'<{len(matrix)}h', *matrix))
        f.write(notice)
    print(f'{len(rows)} entries, matrix {rsize}x{lsize}, {total / 1048576:.1f} MB -> {out}')


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
