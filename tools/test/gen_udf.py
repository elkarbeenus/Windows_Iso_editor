#!/usr/bin/env python3
"""Generates a small UDF 1.02 bridge image laid out like a Microsoft Windows ISO
(ISO9660 stub + UDF with boot/, efi/, sources/ ...). Test fixture only.

usage: gen_udf.py OUT.iso [--long-ad] [--trailing-avdp]
"""
import struct, sys, os, random

S = 2048

def crc_itu(data):
    crc = 0
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) if crc & 0x8000 else (crc << 1)
            crc &= 0xFFFF
    return crc

def tag(buf, ident, loc, crclen, ver=2):
    struct.pack_into('<HHBBHHHI', buf, 0, ident, ver, 0, 0, 1, 0, 0, loc)
    struct.pack_into('<HH', buf, 8, crc_itu(buf[16:16 + crclen]), crclen)
    buf[4] = sum(buf[0:4]) + sum(buf[5:16]) & 0xFF
    buf[4] = (sum(buf[0:4]) + sum(buf[5:16])) & 0xFF

def regid(ident, suffix=b''):
    b = bytearray(32)
    b[1:1 + len(ident)] = ident
    b[24:24 + len(suffix)] = suffix
    return bytes(b)

def dstring(s, n):
    b = bytearray(n)
    enc = b'\x08' + s.encode('latin-1')
    b[:len(enc)] = enc
    b[n - 1] = len(enc)
    return bytes(b)

def ts():
    return struct.pack('<HhBBBBBBBB', 0x1000 | 60, 2024, 5, 1, 12, 0, 0, 0, 0, 0)

class Img:
    def __init__(self, long_ad):
        self.long_ad = long_ad
        self.part_start = 261
        self.blocks = {}   # lbn -> bytes
        self.next = 0
        self.uid = 16
        self.nfiles = 0
        self.ndirs = 0

    def alloc(self, n):
        l = self.next
        self.next += n
        return l

    def fe(self, lbn, ftype, size, extents, uid, links=1):
        b = bytearray(S)
        struct.pack_into('<IHHHBB', b, 16, 0, 4, 0, 1, 0, ftype)
        struct.pack_into('<H', b, 34, 1 if self.long_ad else 0)
        struct.pack_into('<IIIHBBI', b, 36, 0xFFFFFFFF, 0xFFFFFFFF, 0x14A5 if ftype == 5 else 0x1CE7, links, 0, 0, 0)
        nblocks = (size + S - 1) // S
        struct.pack_into('<QQ', b, 56, size, nblocks)
        b[72:84] = ts(); b[84:96] = ts(); b[96:108] = ts()
        struct.pack_into('<I', b, 108, 1)
        b[128:160] = regid(b'*Microsoft CDIMAGE UDF')
        struct.pack_into('<Q', b, 160, uid)
        ads = bytearray()
        for (l, ln) in extents:
            if self.long_ad:
                ads += struct.pack('<IIH6x', ln, l, 0)
            else:
                ads += struct.pack('<II', ln, l)
        struct.pack_into('<II', b, 168, 0, len(ads))
        b[176:176 + len(ads)] = ads
        tag(b, 261, lbn, 176 + len(ads) - 16)
        self.blocks[lbn] = bytes(b)

    def fid(self, name, fe_lbn, chars, uid, block):
        nm = b'' if name is None else b'\x08' + name.encode('latin-1')
        l_iu = 0
        total = (38 + l_iu + len(nm) + 3) // 4 * 4
        b = bytearray(total)
        struct.pack_into('<HBB', b, 16, 1, chars, len(nm))
        struct.pack_into('<IIH', b, 20, S, fe_lbn, 0)
        struct.pack_into('<I', b, 32, uid & 0xFFFFFFFF)
        struct.pack_into('<H', b, 36, l_iu)
        b[38:38 + len(nm)] = nm
        tag(b, 257, block, 38 + l_iu + len(nm) - 16)
        return b

    def add_file(self, data):
        fe_lbn = self.alloc(1)
        n = max(1, (len(data) + S - 1) // S)
        d_lbn = self.alloc(n)
        # split into extents of max 1 GiB-ish (here: 3 blocks) to test multi-extent reading
        exts = []
        off = 0
        lb = d_lbn
        chunk = 3 * S
        while off < len(data) or not exts:
            ln = min(chunk, len(data) - off)
            exts.append((lb, ln))
            off += ln
            lb += (ln + S - 1) // S
            if ln == 0:
                break
        uid = self.uid; self.uid += 1
        self.fe(fe_lbn, 5, len(data), exts, uid)
        pad = data + b'\0' * (n * S - len(data))
        for i in range(n):
            self.blocks[d_lbn + i] = pad[i * S:(i + 1) * S]
        self.nfiles += 1
        return fe_lbn, uid

    def add_dir(self, tree, parent_lbn, parent_uid, is_root=False):
        fe_lbn = self.alloc(1)
        uid = 0 if is_root else self.uid
        if not is_root:
            self.uid += 1
        if parent_lbn is None:
            parent_lbn, parent_uid = fe_lbn, uid
        children = []
        for name, val in tree.items():
            if isinstance(val, dict):
                children.append((name, True) + self.add_dir(val, fe_lbn, uid))
            else:
                children.append((name, False) + self.add_file(val))
        # directory data: size known after building fids; place in fresh blocks
        fids = [self.fid(None, parent_lbn, 0x0A, parent_uid, 0)]
        for name, isdir, lbn, cuid in children:
            fids.append(self.fid(name, lbn, 0x02 if isdir else 0, cuid, 0))
        size = sum(len(f) for f in fids)
        nb = max(1, (size + S - 1) // S)
        d_lbn = self.alloc(nb)
        data = bytearray()
        for f in fids:
            blk = d_lbn + len(data) // S
            f = bytearray(f)
            crclen = struct.unpack_from('<H', f, 10)[0]
            tag(f, 257, blk, crclen)
            data += f
        data += b'\0' * (nb * S - len(data))
        for i in range(nb):
            self.blocks[d_lbn + i] = bytes(data[i * S:(i + 1) * S])
        links = 1 + sum(1 for c in children if c[1])
        self.fe(fe_lbn, 4, size, [(d_lbn, size)], uid, links)
        self.ndirs += 1
        return fe_lbn, uid

def wim(editions):
    xml = '<WIM><TOTALBYTES>1</TOTALBYTES>'
    for i, (name, arch, build) in enumerate(editions, 1):
        xml += f'<IMAGE INDEX="{i}"><WINDOWS><ARCH>{arch}</ARCH><VERSION><BUILD>{build}</BUILD></VERSION></WINDOWS><NAME>{name}</NAME><DISPLAYNAME>{name}</DISPLAYNAME></IMAGE>'
    xml += '</WIM>'
    xb = b'\xff\xfe' + xml.encode('utf-16-le')
    h = bytearray(208)
    h[0:8] = b'MSWIM\0\0\0'
    struct.pack_into('<I', h, 8, 208)
    off = 208 + 5000
    h[72:79] = len(xb).to_bytes(7, 'little')
    struct.pack_into('<QQ', h, 80, off, len(xb))
    body = bytearray(h) + bytes(random.Random(1).getrandbits(8) for _ in range(5000)) + xb + b'\x11' * 7000
    return bytes(body)

def main():
    out = sys.argv[1]
    long_ad = '--long-ad' in sys.argv
    trailing = '--trailing-avdp' in sys.argv
    real_wim = None
    if '--wim' in sys.argv:
        real_wim = open(sys.argv[sys.argv.index('--wim') + 1], 'rb').read()
    rnd = random.Random(42)
    rb = lambda n: bytes(rnd.getrandbits(8) for _ in range(n))
    tree = {
        'autorun.inf': b'[AutoRun.Amd64]\r\nopen=setup.exe\r\n',
        'boot': {'etfsboot.com': rb(4096), 'bcd': rb(12000)},
        'bootmgr': rb(9000),
        'bootmgr.efi': rb(7000),
        'efi': {'boot': {'bootx64.efi': rb(5000)}, 'microsoft': {'boot': {'efisys.bin': rb(S * 3)}}},
        'setup.exe': rb(20000),
        'sources': {'boot.wim': rb(50000), 'install.wim': real_wim or wim([('Windows 11 Home', 9, '26100'), ('Windows 11 Pro', 9, '26100')]), 'setup.exe': rb(3000)},
        'support': {'readme.txt': b'hello'},
    }
    img = Img(long_ad)
    fsd_lbn = img.alloc(1)
    fsd_td = img.alloc(1)
    root_lbn, _ = img.add_dir(tree, None, 0, is_root=True)

    # FSD
    b = bytearray(S)
    b[16:28] = ts()
    struct.pack_into('<HHIIII', b, 28, 3, 3, 1, 1, 0, 0)
    b[48:112] = b'\x00' + b'OSTA Compressed Unicode'.ljust(63, b'\0')
    b[112:240] = dstring('CCSA_X64FRE_EN-US_DV9', 128)
    b[240:304] = b'\x00' + b'OSTA Compressed Unicode'.ljust(63, b'\0')
    b[304:336] = dstring('CCSA_X64FRE_EN-US_DV9', 32)
    struct.pack_into('<IIH', b, 400, S, root_lbn, 0)
    b[416:448] = regid(b'*OSTA UDF Compliant', b'\x02\x01\x03')
    tag(b, 256, fsd_lbn, 512 - 16)
    img.blocks[fsd_lbn] = bytes(b)
    b = bytearray(S); tag(b, 8, fsd_td, 0); img.blocks[fsd_td] = bytes(b)

    part_len = img.next
    total = img.part_start + part_len + (1 if trailing else 0)
    disk = bytearray(total * S)
    def put(sector, data):
        disk[sector * S: sector * S + len(data)] = data
    # ISO9660 stub
    pvd = bytearray(S); pvd[0] = 1; pvd[1:6] = b'CD001'; pvd[6] = 1
    pvd[40:72] = b'CCSA_X64FRE_EN-US_DV9'.ljust(32)
    struct.pack_into('<I', pvd, 80, total); struct.pack_into('>I', pvd, 84, total)
    put(16, pvd)
    t = bytearray(S); t[0] = 255; t[1:6] = b'CD001'; t[6] = 1; put(17, t)
    for i, idn in enumerate([b'BEA01', b'NSR02', b'TEA01']):
        v = bytearray(S); v[1:6] = idn; v[6] = 1; put(18 + i, v)
    def vds(base):
        seq = 0
        # PVD
        b = bytearray(S)
        struct.pack_into('<II', b, 16, seq, 0)
        b[24:56] = dstring('CCSA_X64FRE_EN-US_DV9', 32)
        struct.pack_into('<HHHHII', b, 56, 1, 1, 2, 2, 1, 1)
        tag(b, 1, base, 512 - 16); put(base, b)
        # PD
        b = bytearray(S)
        struct.pack_into('<IHH', b, 16, 1, 1, 0)
        b[24:56] = regid(b'+NSR02')
        struct.pack_into('<III', b, 184, 1, img.part_start, part_len)
        tag(b, 5, base + 1, 512 - 16); put(base + 1, b)
        # LVD
        b = bytearray(S)
        struct.pack_into('<I', b, 16, 2)
        b[20:84] = b'\x00' + b'OSTA Compressed Unicode'.ljust(63, b'\0')
        b[84:212] = dstring('CCSA_X64FRE_EN-US_DV9', 128)
        struct.pack_into('<I', b, 212, S)
        b[216:248] = regid(b'*OSTA UDF Compliant', b'\x02\x01\x03')
        struct.pack_into('<IIH', b, 248, S, fsd_lbn, 0)
        struct.pack_into('<II', b, 264, 6, 1)
        struct.pack_into('<II', b, 432, S * 2, 64)
        struct.pack_into('<BBHH', b, 440, 1, 6, 1, 0)
        tag(b, 6, base + 2, 446 - 16); put(base + 2, b)
        # USD
        b = bytearray(S); struct.pack_into('<II', b, 16, 3, 0); tag(b, 7, base + 3, 24 - 16); put(base + 3, b)
        # TD
        b = bytearray(S); tag(b, 8, base + 4, 0); put(base + 4, b)
    vds(32); vds(48)
    # LVID
    b = bytearray(S)
    b[16:28] = ts()
    struct.pack_into('<I', b, 28, 1)
    struct.pack_into('<Q', b, 40, img.uid)
    struct.pack_into('<II', b, 72, 1, 46)
    struct.pack_into('<II', b, 80, 0, part_len)
    b[88:120] = regid(b'*Microsoft CDIMAGE UDF')
    struct.pack_into('<IIHHH', b, 120, img.nfiles, img.ndirs, 0x102, 0x102, 0x102)
    tag(b, 9, 64, 80 + 8 + 46 - 16); put(64, b)
    b = bytearray(S); tag(b, 8, 65, 0); put(65, b)
    # AVDP
    def avdp(sec):
        b = bytearray(S)
        struct.pack_into('<IIII', b, 16, 16 * S, 32, 16 * S, 48)
        tag(b, 2, sec, 512 - 16); put(sec, b)
    avdp(256)
    if trailing:
        avdp(total - 1)
    for lbn, data in img.blocks.items():
        put(img.part_start + lbn, data)
    with open(out, 'wb') as f:
        f.write(disk)

if __name__ == '__main__':
    main()
