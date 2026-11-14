//! Builds the `autounattend.xml` that is placed in the root of the ISO.
//!
//! Safety design:
//!   * No disk partitioning, no product key, no edition choice, no account
//!     creation: setup stays fully interactive, exactly like the original ISO.
//!   * The debloat work runs inside the freshly installed Windows (specialize
//!     pass, as SYSTEM) – after all files are copied. Every single step is
//!     wrapped in try/catch and logged; the script always exits with code 0,
//!     so a failing step can never stop the installation.
//!   * A hard-coded protection list inside the script refuses to remove
//!     gaming, Store, runtime or security packages, even if requested.

use crate::catalog::{Action, ITEMS, PROTECTED};

pub const SCRIPT_DIR: &str = r"C:\Windows\Setup\Scripts\WinIsoEditor";
pub const APP_NAME: &str = "Windows ISO Editor";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub struct Generated {
    pub xml: String,
    pub specialize_ps1: String,
    pub firstlogon_ps1: String,
}

pub fn generate(selected: &[String], version: &str) -> Result<Generated, String> {
    for id in selected {
        if !ITEMS.iter().any(|i| i.id == id) {
            return Err(format!("unbekannte Option: {id}"));
        }
    }
    let chosen: Vec<_> = ITEMS.iter().filter(|i| selected.iter().any(|s| s == i.id)).collect();

    let mut appx: Vec<&str> = Vec::new();
    let mut caps: Vec<&str> = Vec::new();
    let mut feats: Vec<&str> = Vec::new();
    let mut system: Vec<(String, &str)> = Vec::new();
    let mut user: Vec<(String, &str)> = Vec::new();
    let mut winpe: Vec<&str> = Vec::new();

    for it in &chosen {
        for a in it.actions {
            match a {
                Action::Appx(v) => appx.extend_from_slice(v),
                Action::Capability(v) => caps.extend_from_slice(v),
                Action::Feature(v) => feats.extend_from_slice(v),
                Action::System(s) => system.push((it.id.to_string(), s)),
                Action::DefaultUser(s) => user.push((it.id.to_string(), s)),
                Action::WinPe(v) => winpe.extend_from_slice(v),
            }
        }
    }

    // ---------------------------------------------------------------- specialize.ps1
    let mut ps = String::new();
    ps.push_str(&format!(
        "# {APP_NAME} {version} - created by Dominic Lesina x Claude\n\
         # Runs once as SYSTEM during Windows setup (specialize pass).\n\
         # Every step is isolated: errors are logged, setup always continues.\n\
         $ErrorActionPreference = 'Continue'\n\
         $ProgressPreference = 'SilentlyContinue'\n\
         $dir = {dir}\n\
         $log = Join-Path $dir 'specialize.log'\n\
         function Log([string]$m) {{ try {{ Add-Content -LiteralPath $log -Value ('[{{0}}] {{1}}' -f (Get-Date -Format s), $m) }} catch {{}} }}\n\
         function Step([string]$name, [scriptblock]$sb) {{ try {{ & $sb; Log \"OK   $name\" }} catch {{ Log \"FAIL $name : $_\" }} }}\n\
         function RegSet([string]$path, [string]$name, $value, [string]$type = 'REG_DWORD') {{\n\
         \x20 & reg.exe add $path /v $name /t $type /d $value /f 2>&1 | Out-Null\n\
         \x20 if ($LASTEXITCODE -ne 0) {{ Log \"reg failed: $path\\$name\" }}\n\
         }}\n\
         $Protected = @({prot})\n\
         function IsProtected([string]$n) {{ foreach ($p in $Protected) {{ if ($n -like $p) {{ return $true }} }}; return $false }}\n\
         Log 'start'\n",
        dir = ps_quote(SCRIPT_DIR),
        prot = PROTECTED.iter().map(|p| ps_quote(p)).collect::<Vec<_>>().join(", ")
    ));

    if !appx.is_empty() {
        ps.push_str(&format!(
            "\n# ---- remove provisioned apps (only for new users; nothing else is touched)\n\
             Step 'apps' {{\n\
             \x20 $want = @({})\n\
             \x20 $prov = @(Get-AppxProvisionedPackage -Online)\n\
             \x20 foreach ($n in $want) {{\n\
             \x20   if (IsProtected $n) {{ Log \"skip protected $n\"; continue }}\n\
             \x20   foreach ($p in ($prov | Where-Object {{ $_.DisplayName -eq $n }})) {{\n\
             \x20     try {{ Remove-AppxProvisionedPackage -Online -PackageName $p.PackageName -ErrorAction Stop | Out-Null; Log \"removed $n\" }}\n\
             \x20     catch {{ Log \"could not remove $n : $_\" }}\n\
             \x20   }}\n\
             \x20 }}\n\
             }}\n",
            appx.iter().map(|p| ps_quote(p)).collect::<Vec<_>>().join(", ")
        ));
    }
    if !caps.is_empty() {
        ps.push_str(&format!(
            "\n# ---- remove legacy capabilities\n\
             Step 'capabilities' {{\n\
             \x20 $want = @({})\n\
             \x20 $all = @(Get-WindowsCapability -Online | Where-Object {{ $_.State -eq 'Installed' }})\n\
             \x20 foreach ($n in $want) {{\n\
             \x20   foreach ($c in ($all | Where-Object {{ $_.Name -like \"$n~*\" }})) {{\n\
             \x20     try {{ Remove-WindowsCapability -Online -Name $c.Name -ErrorAction Stop | Out-Null; Log \"removed capability $($c.Name)\" }}\n\
             \x20     catch {{ Log \"could not remove capability $($c.Name) : $_\" }}\n\
             \x20   }}\n\
             \x20 }}\n\
             }}\n",
            caps.iter().map(|p| ps_quote(p)).collect::<Vec<_>>().join(", ")
        ));
    }
    if !feats.is_empty() {
        ps.push_str(&format!(
            "\n# ---- disable optional features (only if present on this build)\n\
             Step 'features' {{\n\
             \x20 foreach ($n in @({})) {{\n\
             \x20   $f = Get-WindowsOptionalFeature -Online -FeatureName $n -ErrorAction SilentlyContinue\n\
             \x20   if ($f -and $f.State -eq 'Enabled') {{\n\
             \x20     try {{ Disable-WindowsOptionalFeature -Online -FeatureName $n -NoRestart -ErrorAction Stop | Out-Null; Log \"disabled feature $n\" }}\n\
             \x20     catch {{ Log \"could not disable feature $n : $_\" }}\n\
             \x20   }}\n\
             \x20 }}\n\
             }}\n",
            feats.iter().map(|p| ps_quote(p)).collect::<Vec<_>>().join(", ")
        ));
    }
    if !system.is_empty() {
        ps.push_str("\n# ---- system settings\n");
        for (id, code) in &system {
            ps.push_str(&format!("Step {} {{\n{}\n}}\n", ps_quote(id), indent(code)));
        }
    }
    if !user.is_empty() {
        ps.push_str(
            "\n# ---- settings for every user account created later (default profile)\n\
             $hive = Join-Path $env:SystemDrive 'Users\\Default\\NTUSER.DAT'\n\
             & reg.exe load 'HKU\\DefaultUser' $hive 2>&1 | Out-Null\n\
             if ($LASTEXITCODE -eq 0) {\n",
        );
        for (id, code) in &user {
            ps.push_str(&format!("  Step {} {{\n{}\n  }}\n", ps_quote(id), indent(&indent(code))));
        }
        ps.push_str(
            "  [GC]::Collect(); [GC]::WaitForPendingFinalizers(); Start-Sleep -Seconds 1\n\
             \x20 & reg.exe unload 'HKU\\DefaultUser' 2>&1 | Out-Null\n\
             \x20 if ($LASTEXITCODE -ne 0) { Start-Sleep -Seconds 3; [GC]::Collect(); & reg.exe unload 'HKU\\DefaultUser' 2>&1 | Out-Null }\n\
             } else { Log 'could not load default user hive' }\n",
        );
    }
    ps.push_str("\nLog 'done'\nexit 0\n");

    // ---------------------------------------------------------------- firstlogon.ps1
    // Second safety net for the first account: if any of the selected apps still got
    // installed for this user, remove it for this user only.
    let mut fl = String::new();
    fl.push_str(&format!(
        "# {APP_NAME} {version} - first logon clean-up (current user only)\n\
         $ErrorActionPreference = 'Continue'\n\
         $ProgressPreference = 'SilentlyContinue'\n\
         $log = Join-Path $env:TEMP 'WinIsoEditor-firstlogon.log'\n\
         function Log([string]$m) {{ try {{ Add-Content -LiteralPath $log -Value ('[{{0}}] {{1}}' -f (Get-Date -Format s), $m) }} catch {{}} }}\n\
         $Protected = @({prot})\n\
         function IsProtected([string]$n) {{ foreach ($p in $Protected) {{ if ($n -like $p) {{ return $true }} }}; return $false }}\n",
        prot = PROTECTED.iter().map(|p| ps_quote(p)).collect::<Vec<_>>().join(", ")
    ));
    if !appx.is_empty() {
        fl.push_str(&format!(
            "try {{\n\
             \x20 foreach ($n in @({})) {{\n\
             \x20   if (IsProtected $n) {{ continue }}\n\
             \x20   foreach ($p in @(Get-AppxPackage -Name $n -ErrorAction SilentlyContinue)) {{\n\
             \x20     try {{ Remove-AppxPackage -Package $p.PackageFullName -ErrorAction Stop; Log \"removed $n\" }} catch {{ Log \"could not remove $n : $_\" }}\n\
             \x20   }}\n\
             \x20 }}\n\
             }} catch {{ Log \"error: $_\" }}\n",
            appx.iter().map(|p| ps_quote(p)).collect::<Vec<_>>().join(", ")
        ));
    }
    fl.push_str("Log 'done'\nexit 0\n");

    // ---------------------------------------------------------------- XML
    let archs = ["amd64", "arm64"];
    let comp = |name: &str, arch: &str, inner: &str| -> String {
        format!(
            "    <component name=\"{name}\" processorArchitecture=\"{arch}\" publicKeyToken=\"31bf3856ad364e35\" language=\"neutral\" versionScope=\"nonSxS\">\n{inner}    </component>\n"
        )
    };

    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    x.push_str(&format!(
        "<!-- Created with {APP_NAME} {version} (Dominic Lesina x Claude). Setup stays interactive: no disk, key or account settings. -->\n"
    ));
    x.push_str("<unattend xmlns=\"urn:schemas-microsoft-com:unattend\" xmlns:wcm=\"http://schemas.microsoft.com/WMIConfig/2002/State\">\n");

    if !winpe.is_empty() {
        x.push_str("  <settings pass=\"windowsPE\">\n");
        for arch in archs {
            let mut inner = String::from("      <RunSynchronous>\n");
            for (i, c) in winpe.iter().enumerate() {
                inner.push_str(&format!(
                    "        <RunSynchronousCommand wcm:action=\"add\"><Order>{}</Order><Path>{}</Path></RunSynchronousCommand>\n",
                    i + 1,
                    xml_escape(c)
                ));
            }
            inner.push_str("      </RunSynchronous>\n");
            x.push_str(&comp("Microsoft-Windows-Setup", arch, &inner));
        }
        x.push_str("  </settings>\n");
    }

    // extract embedded scripts from the cached answer file, then run them
    let extract = "powershell.exe -NoProfile -Command \"$x=[xml](gc -LiteralPath ($env:windir+'\\Panther\\unattend.xml') -Raw);foreach($f in $x.unattend.Extensions.File){$p=$f.path;$null=md (Split-Path $p) -Force;sc -LiteralPath $p -Value $f.InnerText -Encoding UTF8}\"";
    let run_spec = format!("powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{SCRIPT_DIR}\\specialize.ps1\"");
    debug_assert!(extract.len() < 259 && run_spec.len() < 259);

    x.push_str("  <settings pass=\"specialize\">\n");
    for arch in archs {
        let inner = format!(
            "      <RunSynchronous>\n\
             \x20       <RunSynchronousCommand wcm:action=\"add\"><Order>1</Order><Path>{}</Path></RunSynchronousCommand>\n\
             \x20       <RunSynchronousCommand wcm:action=\"add\"><Order>2</Order><Path>{}</Path></RunSynchronousCommand>\n\
             \x20     </RunSynchronous>\n",
            xml_escape(extract),
            xml_escape(&run_spec)
        );
        x.push_str(&comp("Microsoft-Windows-Deployment", arch, &inner));
    }
    x.push_str("  </settings>\n");

    if !appx.is_empty() {
        let run_fl = format!("powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"{SCRIPT_DIR}\\firstlogon.ps1\"");
        x.push_str("  <settings pass=\"oobeSystem\">\n");
        for arch in archs {
            let inner = format!(
                "      <FirstLogonCommands>\n\
                 \x20       <SynchronousCommand wcm:action=\"add\"><Order>1</Order><CommandLine>{}</CommandLine></SynchronousCommand>\n\
                 \x20     </FirstLogonCommands>\n",
                xml_escape(&run_fl)
            );
            x.push_str(&comp("Microsoft-Windows-Shell-Setup", arch, &inner));
        }
        x.push_str("  </settings>\n");
    }

    x.push_str("  <Extensions xmlns=\"https://github.com/elkarbeenus/Windows_Iso_editor\">\n");
    for (name, body) in [("specialize.ps1", &ps), ("firstlogon.ps1", &fl)] {
        x.push_str(&format!(
            "    <File path=\"{SCRIPT_DIR}\\{name}\"><![CDATA[{}]]></File>\n",
            body.replace("]]>", "]] >")
        ));
    }
    x.push_str("  </Extensions>\n");
    x.push_str("</unattend>\n");

    Ok(Generated { xml: x, specialize_ps1: ps, firstlogon_ps1: fl })
}

fn indent(s: &str) -> String {
    s.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n")
}
