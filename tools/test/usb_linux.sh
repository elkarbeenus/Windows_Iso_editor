#!/usr/bin/env bash
# CI integration test (Linux, needs sudo): kernel UDF reads our patched ISO,
# and the USB worker creates correct sticks on loop devices.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CLI="$ROOT/core/target/debug/examples/wie-cli"
T="$(mktemp -d)"
cd "$T"
echo "== workdir $T"

# --- real WIM so the split path can be tested end to end
mkdir -p wimsrc/sub
head -c 3000000 /dev/urandom > wimsrc/a.bin
head -c 1500000 /dev/urandom > wimsrc/sub/b.bin
echo hello > wimsrc/readme.txt
wimlib-imagex capture wimsrc test.wim "Test" --compress=none >/dev/null
python3 "$ROOT/tools/test/gen_udf.py" in.iso --wim test.wim
"$CLI" build in.iso out.iso recommended
"$CLI" xml recommended > expected.xml

hash_tree() { (cd "$1" && find . -type f -print0 | sort -z | xargs -0 sha256sum); }

# --- 1) Linux kernel UDF driver reads the patched ISO (WinUSB-SteamOS, Ventoy, ... rely on it)
mkdir iso_o iso_p
sudo mount -o loop,ro -t udf in.iso iso_o
sudo mount -o loop,ro -t udf out.iso iso_p
hash_tree iso_o > h_o.txt
hash_tree iso_p | grep -v -i "autounattend.xml" > h_p.txt
diff h_o.txt h_p.txt
cmp iso_p/autounattend.xml expected.xml
echo "OK kernel UDF: all $(wc -l < h_o.txt) original files identical, autounattend.xml present"

# --- 2) Windows stick on a loop device (no split)
truncate -s 1200M stick.img
L=$(sudo losetup -P -f --show stick.img)
echo "== loop $L"
sudo "$CLI" usb-worker --image out.iso --device "$L" --mode windows --force-any --lang en | tee w1.log | tail -3
grep -q '"t":"done"' w1.log
mkdir usb
sudo mount "${L}p1" usb
[ "$(sudo blkid -o value -s TYPE "${L}p1")" = "vfat" ]
hash_tree usb | sort -k2 > h_u.txt
hash_tree iso_p | sort -k2 > h_i.txt
diff h_i.txt h_u.txt
sudo umount usb
echo "OK stick (no split): identical to ISO"

# --- 3) Windows stick with split install.wim (thresholds lowered for the test)
sudo env WIE_SPLIT_THRESHOLD=100000 WIE_SPLIT_MB=1 "$CLI" usb-worker --image out.iso --device "$L" --mode windows --force-any --lang en | tee w2.log | tail -3
grep -q '"t":"done"' w2.log
sudo mount "${L}p1" usb
ls -la usb/sources
[ ! -e usb/sources/install.wim ]
[ -e usb/sources/install.swm ] && [ -e usb/sources/install2.swm ]
mkdir applied
sudo wimlib-imagex apply usb/sources/install.swm 1 applied --ref="usb/sources/install*.swm" >/dev/null
diff -r wimsrc applied
sudo umount usb
echo "OK stick (split): install.swm parts apply to the original content"

# --- 4) raw images (plain + bz2)
head -c 30000000 /dev/urandom > raw.img
sudo "$CLI" usb-worker --image raw.img --device "$L" --mode raw --force-any | tail -1 | grep -q '"t":"done"'
sudo cmp -n 30000000 raw.img "$L"
bzip2 -k raw.img
sudo dd if=/dev/zero of="$L" bs=1M count=40 status=none
sudo "$CLI" usb-worker --image raw.img.bz2 --device "$L" --mode raw --force-any | tail -1 | grep -q '"t":"done"'
sudo cmp -n 30000000 raw.img "$L"
echo "OK raw + bz2"

# --- 5) safety: a non-USB device is refused without --force-any
if sudo "$CLI" usb-worker --image raw.img --device "$L" --mode raw | grep -q '"t":"done"'; then echo "safety check failed"; exit 1; fi
echo "OK safety check"

sudo losetup -d "$L"
sudo umount iso_o iso_p
echo "ALL LINUX USB TESTS PASSED"
