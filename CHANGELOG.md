# Changelog

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
