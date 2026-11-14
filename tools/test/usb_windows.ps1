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

function Hashes($dir) {
  Get-ChildItem -LiteralPath $dir -Recurse -File | ForEach-Object {
    $rel = $_.FullName.Substring($dir.Length).TrimStart('\').ToLower()
    "{0} {1}" -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash, $rel
  } | Sort-Object { $_.Substring(65) }
}
function Letter($img) { ((Get-DiskImage -ImagePath $img) | Get-Volume).DriveLetter }

# real WIM via DISM (for the split test)
New-Item -ItemType Directory -Force wimsrc\sub | Out-Null
$rnd = New-Object byte[] 3000000; (New-Object Random 7).NextBytes($rnd); [IO.File]::WriteAllBytes("$t\wimsrc\a.bin", $rnd)
$rnd2 = New-Object byte[] 1500000; (New-Object Random 9).NextBytes($rnd2); [IO.File]::WriteAllBytes("$t\wimsrc\sub\b.bin", $rnd2)
dism.exe /Capture-Image /ImageFile:"$t\test.wim" /CaptureDir:"$t\wimsrc" /Name:Test /Compress:none | Out-Null
if ($LASTEXITCODE -ne 0) { throw "dism capture failed" }

python "$root\tools\test\gen_udf.py" "$t\in.iso" --wim "$t\test.wim"
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
if (-not (Test-Path "${lp}:\autounattend.xml")) { throw "autounattend.xml not visible to Windows" }
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
Get-ChildItem "${l}:\sources" | Format-Table Name, Length | Out-String | Write-Host
if (Test-Path "${l}:\sources\install.wim") { throw "install.wim should be split" }
if (-not (Test-Path "${l}:\sources\install.swm")) { throw "install.swm missing" }
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
