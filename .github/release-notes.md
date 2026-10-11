## SwitchEQ __VERSION__

Headphone equalizer for Windows: AutoEq corrections applied through Equalizer APO, with
Bluetooth status and automatic presets for games, movies and music.

### Requirements

- Windows 10 or 11, 64-bit.
- [Equalizer APO](https://sourceforge.net/projects/equalizerapo/) does the actual filtering. It is
  not bundled: at the end of the setup the installer asks (default answer: No) whether to download
  version 1.4.2 from SourceForge, checks its SHA-256 and only then runs it. You can also install it
  yourself.
- The WebView2 runtime (included in Windows 11; the installer fetches Microsoft's bootstrapper if
  it is missing).

### Install

1. Download `__EXE__` from the assets below. The installer works for the current user and needs no
   administrator rights.
2. Verify the download (see below), then run it.
3. In Equalizer APO's Device Selector, tick your headphones' playback device.

### Unsigned installer and Windows SmartScreen

This installer is **not code-signed**. Windows SmartScreen may show "Windows protected your PC" the
first time. If you trust where you got the file and the hash below matches, choose *More info*,
then *Run anyway*. Some antivirus programs may also warn about unsigned installers.

### Verify the SHA-256

Expected SHA-256 of `__EXE__`:

```text
__SHA256__
```

The same value is in `__EXE__.sha256`. In PowerShell, from the folder with the download:

```powershell
(Get-FileHash .\__EXE__ -Algorithm SHA256).Hash
```

or with `certutil -hashfile __EXE__ SHA256`. The result must equal the value above (the case does
not matter). If it differs, do not run the file.

### More information

See the [README](https://github.com/BennyCapozzo/switcheq#readme) for features, usage, privacy and
known limitations.
