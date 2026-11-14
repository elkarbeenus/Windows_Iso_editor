#!/usr/bin/env python3
"""Walks all UDF descriptors of an image and validates tag checksum, CRC and
tag location (independent re-implementation, used in CI).  usage: check_tags.py IMAGE"""
import struct, sys

S = 2048
d = open(sys.argv[1], 'rb').read()
n = len(d) // S
errors = []

def crc(b):
    c = 0
    for x in b:
        c ^= x << 8
        for _ in range(8):
            c = ((c << 1) ^ 0x1021) & 0xFFFF if c & 0x8000 else (c << 1) & 0xFFFF
    return c

def check(buf, where, expect_loc, expect_id=None):
    ident, ver, cs, _, ser, dcrc, clen, loc = struct.unpack_from('<HHBBHHHI', buf, 0)
    if expect_id is not None and ident != expect_id:
        errors.append(f'{where}: tag id {ident} != {expect_id}'); return
    if (sum(buf[0:4]) + sum(buf[5:16])) & 0xFF != cs:
        errors.append(f'{where}: bad checksum')
    if crc(buf[16:16 + clen]) != dcrc:
        errors.append(f'{where}: bad CRC')
    if loc != expect_loc:
        errors.append(f'{where}: tag location {loc} != {expect_loc}')
    return ident

def sec(i):
    return d[i * S:(i + 1) * S]

avdps = [i for i in (256, n - 1, n - 257) if 0 <= i < n and struct.unpack_from('<H', sec(i))[0] == 2]
for a in avdps:
    check(sec(a), f'AVDP@{a}', a, 2)
if 256 not in avdps:
    errors.append('no AVDP at 256')
if n - 1 not in avdps:
    print('note: no AVDP at last sector')
av = sec(256)
ml, mloc, rl, rloc = struct.unpack_from('<IIII', av, 16)
part = None
for base, ln in ((mloc, ml), (rloc, rl)):
    for i in range(ln // S):
        b = sec(base + i)
        tid = check(b, f'VDS@{base+i}', base + i)
        if tid == 5:
            part = struct.unpack_from('<II', b, 188)
        if tid == 6:
            lvd = b
        if tid == 8:
            break
pstart, plen = part
fsd_lbn = struct.unpack_from('<I', lvd, 252)[0]
il, iloc = struct.unpack_from('<II', lvd, 432)
check(sec(iloc), 'LVID', iloc, 9)
fsd = sec(pstart + fsd_lbn)
check(fsd, 'FSD', fsd_lbn, 256)
root = struct.unpack_from('<I', fsd, 404)[0]
if pstart + plen > n:
    errors.append(f'partition ({pstart}+{plen}) exceeds image ({n})')
pvd = sec(16)
if pvd[1:6] == b'CD001' and struct.unpack_from('<I', pvd, 80)[0] != n:
    errors.append('ISO9660 volume space size mismatch')

def extents(fe):
    flags = struct.unpack_from('<H', fe, 34)[0] & 7
    lea, lad = struct.unpack_from('<II', fe, 168)
    ads = fe[176 + lea:176 + lea + lad]
    out = []
    step = 16 if flags == 1 else 8
    for o in range(0, len(ads), step):
        ln, pos = struct.unpack_from('<II', ads, o)
        if ln & 0x3FFFFFFF:
            out.append((pos, ln & 0x3FFFFFFF))
    return out

count = [0, 0]
def walk(lbn, path):
    fe = sec(pstart + lbn)
    check(fe, f'FE {path}', lbn, 261)
    for pos, ln in extents(fe):
        if pos + (ln + S - 1) // S > plen:
            errors.append(f'{path}: extent outside partition')
    if fe[27] != 4:
        count[0] += 1
        return
    count[1] += 1
    data = b''.join(d[(pstart + p) * S:(pstart + p) * S + ln] for p, ln in extents(fe))
    size = struct.unpack_from('<Q', fe, 56)[0]
    data = data[:size]
    o = 0
    ext = extents(fe)
    while o < len(data):
        blk = ext[0][0] + o // S
        if struct.unpack_from('<H', data, o)[0] != 257:
            errors.append(f'{path}: garbage in directory at {o}'); break
        l_fi, = struct.unpack_from('<B', data, o + 19)
        l_iu, = struct.unpack_from('<H', data, o + 36)
        total = (38 + l_iu + l_fi + 3) // 4 * 4
        check(data[o:o + total], f'FID {path}@{o}', blk, 257)
        chars = data[o + 18]
        name = data[o + 38 + l_iu + 1:o + 38 + l_iu + l_fi].decode('latin-1')
        icb = struct.unpack_from('<I', data, o + 24)[0]
        if not chars & 0x08:
            walk(icb, path + '/' + name)
        o += total

walk(root, '')
print(f'files={count[0]} dirs={count[1]} errors={len(errors)}')
for e in errors:
    print('ERROR', e)
sys.exit(1 if errors else 0)
