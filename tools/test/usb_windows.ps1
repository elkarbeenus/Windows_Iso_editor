# CI integration test (Windows runner, admin):
#  1) Windows itself mounts our patched ISO (UDF driver) and sees identical files
#  2) the USB worker builds a correct stick on a virtual disk (VHDX), incl. DISM split
#  3) raw image write
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
$cli = "$root\core\target\debug\examples\wie-cli.exe"
$t = Join-Path $env:RUNNER_TEMP "wie"
New-Item -ItemType Directory -Force $t | Out-Null
Set-Location $t

# .NET only: drives mounted during this session are unknown to PowerShell's providers
function Hashes([string]$dir) {
  $sha = [System.Security.Cryptography.SHA256]::Create()
  [System.IO.Directory]::EnumerateFiles($dir, '*', [System.IO.SearchOption]::AllDirectories) | ForEach-Object {
    $rel = $_.Substring($dir.Length).TrimStart('\').ToLower()
    $fs = [System.IO.File]::OpenRead($_)
    try { $h = [BitConverter]::ToString($sha.ComputeHash($fs)).Replace('-', '') } finally { $fs.Dispose() }
    "$h $rel"
  } | Sort-Object { $_.Substring(65) }
}
function Letter($img) { ((Get-DiskImage -ImagePath $img) | Get-Volume).DriveLetter }

# real WIM via DISM (for the split test)
New-Item -ItemType Directory -Force wimsrc\sub | Out-Null
$rnd = New-Object byte[] 3000000; (New-Object Random 7).NextBytes($rnd); [IO.File]::WriteAllBytes("$t\wimsrc\a.bin", $rnd)
$rnd2 = New-Object byte[] 1500000; (New-Object Random 9).NextBytes($rnd2); [IO.File]::WriteAllBytes("$t\wimsrc\sub\b.bin", $rnd2)
dism.exe /Capture-Image /ImageFile:"$t\test.wim" /CaptureDir:"$t\wimsrc" /Name:Test /Compress:none | Out-Null
if ($LASTEXITCODE -ne 0) { throw "dism capture failed" }

# Fixture ISO authored by Windows itself (IMAPI2, UDF 1.02) with a Windows-setup-like layout
$src = "$t\isosrc"
foreach ($d in 'boot', 'efi\boot', 'efi\microsoft\boot', 'sources', 'support') { New-Item -ItemType Directory -Force "$src\$d" | Out-Null }
function RandFile($path, $size, $seed) { $b = New-Object byte[] $size; (New-Object Random $seed).NextBytes($b); [IO.File]::WriteAllBytes($path, $b) }
RandFile "$src\bootmgr" 9000 1; RandFile "$src\bootmgr.efi" 7000 2; RandFile "$src\setup.exe" 20000 3
RandFile "$src\boot\etfsboot.com" 4096 4; RandFile "$src\boot\bcd" 12000 5
RandFile "$src\efi\boot\bootx64.efi" 5000 6; RandFile "$src\efi\microsoft\boot\efisys.bin" 6144 7
RandFile "$src\sources\boot.wim" 50000 8; RandFile "$src\sources\setup.exe" 3000 9
Copy-Item "$t\test.wim" "$src\sources\install.wim"
Set-Content "$src\autorun.inf" "[AutoRun]`r`nopen=setup.exe" -Encoding ascii
Set-Content "$src\support\readme.txt" "hello" -Encoding ascii

Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Runtime.InteropServices.ComTypes;
public static class IsoWriter {
  public static void Save(object comStream, string path) {
    IStream s = (IStream)comStream;
    byte[] buf = new byte[1024 * 1024];
    IntPtr read = System.Runtime.InteropServices.Marshal.AllocHGlobal(sizeof(int));
    try {
      using (FileStream fs = File.Create(path)) {
        while (true) {
          s.Read(buf, buf.Length, read);
          int n = System.Runtime.InteropServices.Marshal.ReadInt32(read);
          if (n <= 0) break;
          fs.Write(buf, 0, n);
        }
      }
    } finally { System.Runtime.InteropServices.Marshal.FreeHGlobal(read); }
  }
}
'@
$fsi = New-Object -ComObject IMAPI2FS.MsftFileSystemImage
$fsi.ChooseImageDefaultsForMediaType(13)   # DVD+R DL sized media
$fsi.FileSystemsToCreate = 4               # UDF
$fsi.UDFRevision = 0x102                   # UDF 1.02 like Microsoft's Windows ISOs
$fsi.VolumeName = 'WIE_TEST_X64'
$fsi.Root.AddTree($src, $false)
$img = $fsi.CreateResultImage()
[IsoWriter]::Save($img.ImageStream, "$t\in.iso")
"fixture ISO: $((Get-Item "$t\in.iso").Length) bytes"
& $cli inspect "$t\in.iso"
if ($LASTEXITCODE -ne 0) { throw "inspect of IMAPI ISO failed" }
& $cli build "$t\in.iso" "$t\out.iso" recommended
if ($LASTEXITCODE -ne 0) { throw "build failed" }
& $cli xml recommended | Set-Content -NoNewline -Encoding ascii "$t\expected.xml"

# ---- 1) Windows UDF driver
Mount-DiskImage -ImagePath "$t\in.iso" | Out-Null
Mount-DiskImage -ImagePath "$t\out.iso" | Out-Null
Start-Sleep 2
$lo = Letter "$t\in.iso"; $lp = Letter "$t\out.iso"
"original ${lo}:  patched ${lp}:"
$ho = Hashes "${lo}:\"
$hp = Hashes "${lp}:\" | Where-Object { $_ -notmatch 'autounattend\.xml$' }
if (Compare-Object $ho $hp) { Compare-Object $ho $hp | Format-Table | Out-String | Write-Host; throw "files differ under Windows" }
if (-not [System.IO.File]::Exists("${lp}:\autounattend.xml")) { throw "autounattend.xml not visible to Windows" }
$a = [IO.File]::ReadAllBytes("${lp}:\autounattend.xml"); $b = [IO.File]::ReadAllBytes("$t\expected.xml")
if ([Convert]::ToBase64String($a) -ne [Convert]::ToBase64String($b)) { throw "autounattend.xml content differs" }
"OK Windows UDF: $($ho.Count) original files identical, autounattend.xml readable"
$isoHashes = Hashes "${lp}:\"
Dismount-DiskImage -ImagePath "$t\in.iso" | Out-Null

# ---- 2) virtual USB stick (VHDX)
$vhd = "$t\stick.vhdx"
@("create vdisk file=`"$vhd`" maximum=1500 type=expandable", "attach vdisk", "exit") | Set-Content -Encoding ascii "$t\dp.txt"
diskpart /s "$t\dp.txt" | Out-Null
Start-Sleep 2
$disk = Get-Disk | Where-Object { $_.Location -like "*stick.vhdx*" }
$n = $disk.Number
"virtual disk number $n ($($disk.BusType))"

function RunWorker([string[]]$extra) {
  $out = & $cli usb-worker --image "$t\out.iso" --device $n --force-any --lang en @extra
  $out | Select-Object -Last 4 | Write-Host
  if (-not ($out -match '"t":"done"')) { throw "worker failed: $($out -join "`n")" }
}

RunWorker @('--mode', 'windows')
$l = (Get-Partition -DiskNumber $n -PartitionNumber 1).DriveLetter
$hu = Hashes "${l}:\"
if (Compare-Object $isoHashes $hu) { Compare-Object $isoHashes $hu | Format-Table | Out-String | Write-Host; throw "stick differs from ISO" }
if ((Get-Volume -DriveLetter $l).FileSystem -ne 'FAT32') { throw "not FAT32" }
"OK stick (no split)"

$env:WIE_SPLIT_THRESHOLD = '100000'; $env:WIE_SPLIT_MB = '1'
RunWorker @('--mode', 'windows')
Remove-Item Env:WIE_SPLIT_THRESHOLD, Env:WIE_SPLIT_MB
$l = (Get-Partition -DiskNumber $n -PartitionNumber 1).DriveLetter
[System.IO.Directory]::GetFiles("${l}:\sources") | Write-Host
if ([System.IO.File]::Exists("${l}:\sources\install.wim")) { throw "install.wim should be split" }
if (-not [System.IO.File]::Exists("${l}:\sources\install.swm")) { throw "install.swm missing" }
dism.exe /Get-WimInfo /WimFile:"${l}:\sources\install.swm" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "swm not readable by DISM" }
"OK stick (split with DISM)"

# ---- 3) raw image
$raw = New-Object byte[] 20000000; (New-Object Random 3).NextBytes($raw); [IO.File]::WriteAllBytes("$t\raw.img", $raw)
$out = & $cli usb-worker --image "$t\raw.img" --device $n --mode raw --force-any --lang en
$out | Select-Object -Last 2 | Write-Host
if (-not ($out -match '"t":"done"')) { throw "raw write failed" }
"OK raw"

# safety: refused without --force-any (virtual disk is not USB)
$out = & $cli usb-worker --image "$t\raw.img" --device $n --mode raw --lang en
if ($out -match '"t":"done"') { throw "safety check failed" }
"OK safety check"

@("select vdisk file=`"$vhd`"", "detach vdisk", "exit") | Set-Content -Encoding ascii "$t\dp2.txt"
diskpart /s "$t\dp2.txt" | Out-Null
Dismount-DiskImage -ImagePath "$t\out.iso" | Out-Null
"ALL WINDOWS TESTS PASSED"
