# Changelog

## v0.0.4

**Behoben**
- SteamOS/Linux: USB-Stick-Erstellung brach mit „mount: … libmount.so.1: version `MOUNT_2_42' not found“ ab. Ursache: Der Root-Helfer gab den Bibliothekspfad des AppImage an Systemprogramme weiter, `mount` lud dadurch die alte `libmount` aus dem AppImage. Die Umgebung wird jetzt vor jedem Systemaufruf bereinigt.
- Neuer Build-Test: Das fertige AppImage erstellt selbst (wie über pkexec) einen Windows-Stick inklusive aufgeteilter install.wim und prüft, dass keine AppImage-Bibliotheken an Systemprogramme durchsickern.

**Hinweis:** Diese Version gibt es nur als AppImage (Linux/SteamOS). Windows-Builds folgen später wieder. Wer v0.0.3 hat, bekommt das Update direkt in der App über „Jetzt aktualisieren“.

**Download**
- `Windows-ISO-Editor_0.0.4_x86_64.AppImage` – Linux / SteamOS

## v0.0.3

**Neu: Bootfähige USB-Sticks direkt in der App** – zwei Programme in einem
- Neue Startseite: **„Windows-ISO anpassen“** oder **„Bootfähigen USB-Stick erstellen“**
- Nach dem Erstellen einer ISO: Button **„Direkt auf USB-Stick schreiben“**
- **Windows 10/11** (x64 & ARM64): Stick wie beim Media Creation Tool (MBR, FAT32, UEFI); eine `install.wim` über 4 GB wird automatisch aufgeteilt (Linux: wimlib, Windows: DISM)
- **SteamOS-Recovery** (`.img`, `.img.bz2`) und **Linux-Distributionen** (Hybrid-ISOs) werden 1:1 geschrieben; unter Linux auch `.gz`, `.xz`, `.zst`, `.zip`
- **Jeder Stick wird geprüft:** Nach dem Schreiben wird alles vom Stick zurückgelesen und verglichen; defekte oder gefälschte Sticks fallen sofort auf
- Sicherheitssperren: Nur USB-/Wechseldatenträger, niemals das Systemlaufwerk; vor dem Löschen kommt eine Bestätigung
- SteamOS/Linux: Root-Rechte per Passwortabfrage (pkexec); Windows: einmalige Administrator-Abfrage

**Neu: Automatische Updates in der App**
- Beim Start prüft die App die GitHub-Releases; ist eine neuere Version da, erscheint oben ein Banner mit **„Jetzt aktualisieren“**
- Ein Klick: herunterladen, per SHA-256 prüfen, App ersetzen, neu starten – Einstellungen bleiben erhalten (AppImage, portable exe; installierte Version startet den neuen Installer)
- Abschaltbar für Offline-Nutzung mit `WIE_NO_UPDATE_CHECK=1`

**Installationsfehler beim Partitionieren untersucht**
- Die Antwortdatei enthält keine Laufwerkseinstellungen. Neue automatische Tests belegen: Windows **und** der Linux-Kernel lesen die bearbeitete ISO Datei für Datei identisch zum Original
- Tipp bei „Es konnte keine neue Partition erstellt werden“: im Setup Shift+F10 → `diskpart` → `select disk 0` → `clean` → `convert gpt`, dann den nicht zugewiesenen Speicher wählen und direkt „Weiter“

**Downloads**
- `Windows-ISO-Editor_0.0.3_x86_64.AppImage` – Linux / SteamOS
- `Windows-ISO-Editor_0.0.3_x64-setup.exe` – Windows-Installer
- `Windows-ISO-Editor_0.0.3_x64-portable.exe` – Windows ohne Installation

## v0.0.2

**Behoben**
- SteamOS/Linux: Die Oberfläche blieb grau. Ursache: Das AppImage brachte zu alte `libwayland`-Bibliotheken mit, mit denen der Grafiktreiber von SteamOS den Web-Renderer abstürzen ließ. Sie werden jetzt entfernt, die App nutzt die Systembibliotheken.
- Zusätzlicher Sicherheitsmodus: Lädt die Oberfläche einmal nicht, startet die App beim nächsten Mal automatisch mit Software-Rendering (manuell erzwingbar mit `WIE_SAFE_MODE=1`).
- Build prüft jetzt automatisch, dass die Oberfläche startet und gezeichnet wird (Linux und Windows).

**Downloads**
- `Windows-ISO-Editor_0.0.2_x86_64.AppImage` – Linux / SteamOS
- `Windows-ISO-Editor_0.0.2_x64-setup.exe` – Windows-Installer
- `Windows-ISO-Editor_0.0.2_x64-portable.exe` – Windows ohne Installation

## v0.0.1

Erste Testversion – *first test release*.

**Neu**
- Windows-11-ISO (x64 & ARM64) auswählen oder per Drag & Drop hineinziehen; Editionen, Architektur und Build werden angezeigt
- 42 Optionen in 6 Bereichen: KI & Überwachung (Copilot, Recall, Click to Do, Cortana, KI in Paint/Editor), Datenschutz, Werbung & Vorschläge, vorinstallierte Apps, Installation, Komfort
- Drei Voreinstellungen: **Sanft**, **Empfohlen**, **Maximal** – danach frei anpassbar; jede Karte zeigt „Was passiert genau?“
- Feste Schutzliste: Xbox/Game Bar/Gaming Services, Bluetooth, Treiber, Store, Windows Update, Windows-Sicherheit, Runtimes und Edge werden nie entfernt
- Sicheres Verfahren: Die Original-ISO wird bitgenau kopiert und nur um eine `autounattend.xml` ergänzt – Bootdateien und `install.wim` bleiben unverändert; danach automatische Prüfung
- Installation bleibt interaktiv (keine Laufwerks-, Schlüssel- oder Kontovorgaben); jeder Aufräumschritt ist einzeln abgesichert und protokolliert
- Optional: lokales Konto ohne Internet, Hardware-Prüfung umgehen, automatische Verschlüsselung aus, Schnellstart aus (Dual-Boot mit SteamOS)
- „Nur autounattend.xml speichern“ für vorhandene USB-Sticks
- Deutsch & Englisch, Info-Fenster: *Erstellt von Dominic Lesina × Claude*

**Downloads**
- `Windows-ISO-Editor_0.0.1_x86_64.AppImage` – Linux / SteamOS (ausführbar machen und starten)
- `Windows-ISO-Editor_0.0.1_x64-setup.exe` – Windows-Installer
- `Windows-ISO-Editor_0.0.1_x64-portable.exe` – Windows ohne Installation
