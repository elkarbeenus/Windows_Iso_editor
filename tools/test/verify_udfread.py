#!/usr/bin/env python3
"""Independent check of an image with libudfread (the UDF reader used by VLC/Kodi).
Prints a recursive listing with sizes + sha256 so two images can be compared.

usage: verify_udfread.py IMAGE [FILE_TO_DUMP]
"""
import ctypes, ctypes.util, hashlib, sys

lib = ctypes.CDLL(ctypes.util.find_library('udfread') or 'libudfread.so.0')

class Dirent(ctypes.Structure):
    _fields_ = [('d_type', ctypes.c_uint), ('d_name', ctypes.c_char_p)]

lib.udfread_init.restype = ctypes.c_void_p
lib.udfread_open.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
lib.udfread_opendir.restype = ctypes.c_void_p
lib.udfread_opendir.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
lib.udfread_readdir.restype = ctypes.POINTER(Dirent)
lib.udfread_readdir.argtypes = [ctypes.c_void_p, ctypes.POINTER(Dirent)]
lib.udfread_closedir.argtypes = [ctypes.c_void_p]
lib.udfread_file_open.restype = ctypes.c_void_p
lib.udfread_file_open.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
lib.udfread_file_size.restype = ctypes.c_int64
lib.udfread_file_size.argtypes = [ctypes.c_void_p]
lib.udfread_file_read.restype = ctypes.c_ssize_t
lib.udfread_file_read.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.c_size_t]
lib.udfread_file_close.argtypes = [ctypes.c_void_p]
lib.udfread_get_volume_id.restype = ctypes.c_char_p
lib.udfread_get_volume_id.argtypes = [ctypes.c_void_p]

UDF_DT_DIR = 1

def main():
    img = sys.argv[1].encode()
    u = lib.udfread_init()
    if lib.udfread_open(u, img) < 0:
        print('OPEN FAILED'); sys.exit(1)
    print('volume:', lib.udfread_get_volume_id(u).decode())

    def read(path):
        f = lib.udfread_file_open(u, path.encode())
        if not f:
            return None
        size = lib.udfread_file_size(f)
        buf = ctypes.create_string_buffer(size)
        got = 0
        while got < size:
            n = lib.udfread_file_read(f, ctypes.byref(buf, got), size - got)
            if n <= 0:
                break
            got += n
        lib.udfread_file_close(f)
        return buf.raw[:got], size

    def walk(path):
        d = lib.udfread_opendir(u, path.encode())
        if not d:
            print('OPENDIR FAILED', path); sys.exit(1)
        ent = Dirent()
        items = []
        while lib.udfread_readdir(d, ctypes.byref(ent)):
            name = ent.d_name.decode()
            if name in ('.', '..'):
                continue
            items.append((name, ent.d_type))
        lib.udfread_closedir(d)
        for name, t in sorted(items):
            p = (path.rstrip('/') + '/' + name)
            if t == UDF_DT_DIR:
                print('D', p)
                walk(p)
            else:
                r = read(p)
                if r is None:
                    print('READ FAILED', p); sys.exit(1)
                data, size = r
                ok = 'ok' if len(data) == size else 'SHORT'
                print('F', p, size, hashlib.sha256(data).hexdigest()[:16], ok)

    walk('/')
    if len(sys.argv) > 2:
        data, _ = read(sys.argv[2])
        sys.stdout.write(data.decode('utf-8', 'replace'))

main()
