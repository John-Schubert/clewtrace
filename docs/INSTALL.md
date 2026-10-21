<!-- Install guidance for CT-PLT-001 installers. Decisions: ADR-0001 (John, 2026-10-21). -->
# Installing Clewtrace

Download the installer for your system and `SHA256SUMS.txt` from the GitHub release.
Clewtrace needs no network connection to install or run (CT-SEC-001).

## Check the download

Compare the installer's SHA-256 with its line in `SHA256SUMS.txt`:

- Linux or macOS: `sha256sum -c SHA256SUMS.txt --ignore-missing` (macOS: `shasum -a 256 -c`)
- Windows PowerShell: `Get-FileHash .\Clewtrace_<version>_x64_en-US.msi -Algorithm SHA256`

Installers are not code-signed yet. Code signing is required before the first business
pilot. Until then Windows SmartScreen and macOS Gatekeeper show a warning.

## Windows 10 and 11 (MSI)

Clewtrace uses the Microsoft Edge WebView2 runtime. The MSI does not include it and never
downloads it, so the install works offline.

- Windows 11 and updated Windows 10 already have WebView2. Run the MSI.
- **Air-gapped or un-updated Windows 10:** install WebView2 first.
  1. On a connected machine, download Microsoft's **Evergreen Standalone Installer** for
     WebView2 (x64) from https://developer.microsoft.com/microsoft-edge/webview2/. This is
     the offline runtime package, not the small bootstrapper.
  2. Carry it to the target machine through your normal media transfer process and run it
     as an administrator. It needs no network connection.
  3. Run the Clewtrace MSI.

To check whether WebView2 is installed, look in Settings > Apps for "Microsoft Edge
WebView2 Runtime".

If SmartScreen warns about the unsigned MSI, choose "More info", then "Run anyway".

## macOS 13 and newer (DMG)

Open the DMG and drag Clewtrace to Applications. The first time, if Gatekeeper blocks the
unsigned app, open System Settings > Privacy & Security and choose "Open Anyway".

## Linux (deb or rpm)

The deb and rpm use your distribution's own WebKitGTK 4.1 and GTK 3 packages, which the
package manager installs as dependencies. On an offline machine, those packages must be
available from a local mirror or installed first.

- Debian or Ubuntu: `sudo apt install ./Clewtrace_<version>_amd64.deb`
- Fedora or RHEL: `sudo dnf install ./Clewtrace-<version>-1.x86_64.rpm`

There is no AppImage. It would bundle GTK and WebKitGTK, which are LGPL, and exceed the
20 MB installer budget (ADR-0001, ADR-0003).

## License notices

`THIRD-PARTY-NOTICES.txt` is installed next to the program on Windows, in
`Clewtrace.app/Contents/Resources` on macOS, and in `/usr/lib/Clewtrace` on Linux. It is
also attached to each release.
