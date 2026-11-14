#!/usr/bin/env bash
# Builds/collects mkfs.fat and wimlib-imagex for the AppImage (USB stick feature).
# usage: bundle_usb_tools.sh <AppDir/usr/bin>
set -euo pipefail
DEST="$(readlink -f "$1")"
LIBDIR="$(readlink -f "$DEST/../lib")"
W="$(mktemp -d)"
cd "$W"
DOSFS_VER=4.2
WIMLIB_VER=1.14.4

echo "== mkfs.fat (dosfstools $DOSFS_VER, static)"
curl -fsSL -o dosfstools.tar.gz "https://github.com/dosfstools/dosfstools/releases/download/v${DOSFS_VER}/dosfstools-${DOSFS_VER}.tar.gz"
tar xzf dosfstools.tar.gz
( cd "dosfstools-${DOSFS_VER}" && ./configure -q LDFLAGS=-static >/dev/null && make -s -j"$(nproc)" >/dev/null )
install -m755 "dosfstools-${DOSFS_VER}/src/mkfs.fat" "$DEST/mkfs.fat"
strip "$DEST/mkfs.fat"
"$DEST/mkfs.fat" --help 2>&1 | head -1 || true

echo "== wimlib-imagex"
OK=0
if curl -fsSL -o wimlib.tar.gz "https://wimlib.net/downloads/wimlib-${WIMLIB_VER}.tar.gz"; then
  tar xzf wimlib.tar.gz
  if ( cd "wimlib-${WIMLIB_VER}" && ./configure -q --without-ntfs-3g --without-fuse --disable-shared --enable-static >/dev/null && make -s -j"$(nproc)" >/dev/null ); then
    bin="wimlib-${WIMLIB_VER}/wimlib-imagex"
    [ -f "wimlib-${WIMLIB_VER}/.libs/wimlib-imagex" ] && bin="wimlib-${WIMLIB_VER}/.libs/wimlib-imagex"
    install -m755 "$bin" "$DEST/wimlib-imagex"
    strip "$DEST/wimlib-imagex"
    OK=1
  fi
fi
if [ "$OK" != 1 ]; then
  echo "   source build failed -> distro wimlib-imagex + its libraries"
  sudo apt-get install -y -qq wimtools patchelf >/dev/null
  install -m755 "$(command -v wimlib-imagex)" "$DEST/wimlib-imagex"
  mkdir -p "$LIBDIR/wimlib"
  ldd "$DEST/wimlib-imagex" | awk '/=> \//{print $3}' | while read -r so; do
    case "$(basename "$so")" in libc.so*|libm.so*|libpthread.so*|libdl.so*|librt.so*|libresolv.so*|ld-linux*) continue ;; esac
    cp -L "$so" "$LIBDIR/wimlib/"
  done
  patchelf --set-rpath '$ORIGIN/../lib/wimlib' "$DEST/wimlib-imagex"
fi
ldd "$DEST/wimlib-imagex" || true
"$DEST/wimlib-imagex" --version | head -1
