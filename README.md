# SwitchEQ

A small Windows tray app that equalizes **any headphones**, Bluetooth or wired. It downloads the
correction for your model from the [AutoEq](https://github.com/jaakkopasanen/AutoEq) project, adds
optional taste presets (movies, games, music, voice...), switches preset automatically when a game
or a movie starts, and applies everything through
[Equalizer APO](https://sourceforge.net/projects/equalizerapo/).

> **Riassunto in italiano.** SwitchEQ è un'app per Windows 10/11, leggera e nel tray, che equalizza
> qualsiasi cuffia (Bluetooth o cablata): cerchi il modello, l'app scarica la correzione da AutoEq e
> la applica con Equalizer APO. Include preset di gusto con razionale documentato, volume uniforme
> tra i preset, cambio automatico per giochi, film e musica, stato Bluetooth, rilascio/riconnessione
> delle cuffie, avvio con Windows e interfaccia in italiano e inglese. Nel programma non c'è alcuna
> misura: i profili si scaricano solo su richiesta dell'utente da AutoEq (le misure di Rtings sono
> escluse). Nessuna telemetria. Equalizer APO non è incluso: l'installer può scaricarlo (versione
> 1.4.2, SHA-256 verificato) solo dopo conferma. L'installer non è firmato, quindi SmartScreen può
> avvisare. Codice sotto licenza MIT; i dati di terzi hanno condizioni proprie
> ([`docs/data-sources.md`](docs/data-sources.md)). Le sezioni qui sotto sono in inglese.

**Contents:** [Goal](#goal) · [Features](#features) · [How it works](#how-it-works) ·
[Installation](#installation) · [Usage](#usage) · [Taste presets](#taste-presets) ·
[Automatic presets](#automatic-presets) · [Known limitations](#known-limitations) ·
[Privacy](#privacy) · [Building from source](#building-from-source) ·
[Contributing](#contributing) · [Licences and data sources](#licences-and-data-sources)

## Goal

Headphone equalization from measurements is well understood, but applying it on Windows normally
means editing Equalizer APO text files by hand and switching presets manually. SwitchEQ aims to make
that a two-minute job for any headphones and then stay out of the way:

- low footprint: one small native process in the tray, no background service, no UI framework, no
  polling when it is not needed;
- no measurement data inside the program: corrections are fetched from AutoEq only when you ask;
- an honest, documented rationale for every taste preset, instead of "magic" curves;
- no telemetry and no account.

SwitchEQ is not a sound engine. It does not process audio itself and it does not do spatial audio:
the filtering is done by Equalizer APO.

## Features

- **AutoEq profiles for any headphones.** Search the AutoEq index by model name, pick a result and the
  app downloads its `ParametricEQ.txt`. Models with several measurements (for example ANC on/off or a
  maker's EQ modes) become *variants* you can switch between. You can also import your own
  `ParametricEQ.txt`.
- **Taste presets** built on top of the chosen correction: Movies / Cinema, TV series / Night
  dialogue, Competitive gaming, Immersive gaming, Classical, Jazz, Rock / Metal, Electronic / Hip-hop, Bass
  boost, Low-volume listening, Voice / Podcast and Calls. Each has its filters and sources listed
  [below](#taste-presets).
- **Uniform volume across presets** (on by default): every preset shares one preamp, so switching
  preset does not make the volume jump. See [Uniform volume](#uniform-volume).
- **Automatic switching** for games, movies and music: a game starts and the gaming preset is applied,
  the video player or streaming site starts and the Movies preset is applied, music plays and the
  Music preset is applied; when it ends the previous preset returns. Per-executable and per-app
  rules are available. See [Automatic presets](#automatic-presets).
- **Equalizer**: 10-band graphic EQ (+-12 dB), parametric editor (peaking, low/high shelf), response
  curve computed with the same biquad filters as Equalizer APO, automatic anti-clipping preamp or a
  manual one, bypass, user presets.
- **Bluetooth** (when the chosen output is Bluetooth): connection state, battery level when Windows
  exposes it, **Release headphones / Reconnect to this PC** (to hand the headphones to another
  device and take them back), default output device.
- **Tray icon**, **start with Windows** (current user only) and a single running instance.
- **Languages**: Italian and English. The app follows the Windows language and can be switched in
  Settings; window, tray, notifications, errors and preset texts all change immediately.

## How it works

SwitchEQ is a [Tauri 2](https://tauri.app/) app: a Rust backend and a plain TypeScript frontend
(Vite, no UI framework) shown in the system WebView2. When the window is closed only the Rust process
stays alive, with the tray icon.

```text
 AutoEq (GitHub, pinned commit)            Windows
        |  index + one ParametricEQ.txt        |  media sessions, process list,
        v                                      |  Bluetooth / Core Audio devices
   +-------------+   settings.json      +------v------+
   |  SwitchEQ   |--------------------->|   Rust app  |
   |  (this app) |   switcheq.txt       +------+------+
   +-------------+--------------------------->|
                          Equalizer APO <-----+  (config\config.txt includes switcheq.txt)
                                  |
                          audio device effect (APO) on the selected headphones
```

1. **Profile.** The chosen profile (name, variants, filters, source) is copied into `settings.json`,
   so after the first download the app works offline.
2. **Preset.** The active preset is the profile correction plus, for taste presets, a few documented
   filters. Filters and preamp are computed in `src-tauri/src/eq.rs` and `presets.rs`.
3. **Equalizer APO.** The app finds Equalizer APO from the registry (`HKLM\SOFTWARE\EqualizerAPO`) or
   in `C:\Program Files\EqualizerAPO` and writes `config\switcheq.txt`, for example:

   ```text
   # Generated by SwitchEQ: do not edit by hand, the file is rewritten by the app.
   # Preset: Correction for Acme Studio One
   # Source: AutoEq "Labo/over-ear/Acme Studio One" - https://github.com/jaakkopasanen/AutoEq/...
   Device: Acme Studio One
   Preamp: -6.9 dB
   Filter 1: ON LSC Fc 105 Hz Gain -0.5 dB Q 0.70
   ...
   ```

   The `Device:` directive restricts the filters to the chosen output (for Bluetooth headphones the
   pairing name, which matches both the stereo and the hands-free endpoint; for wired headphones the
   endpoint GUID). With no device chosen the file contains only comments: it is never applied to *all*
   devices. In bypass only comments remain or, with uniform volume, `Device:` and the common preamp.
4. **Include line.** The app adds, once, the line `Include: switcheq.txt` at the end of
   `config\config.txt` (backup first: `config.txt.switcheq-backup`). It also comments out the
   demonstration configuration that the Equalizer APO installer writes (`Preamp: -6 dB`,
   `Include: example.txt`, a zero `GraphicEQ`) only when those three lines appear together and
   consecutively, with a backup `config.txt.switcheq-demo-backup`; to restore it, remove the prefix.
5. **Game detection.** A thread looks at the process list every 3 seconds, only while automatic
   presets are on (ToolHelp snapshot, full path only for new processes, foreground window and
   full-screen check). See [Automatic presets](#automatic-presets).
6. **Movies and music.** Detection uses the Windows media sessions
   (`GlobalSystemMediaTransportControlsSessionManager`) and is event driven: there is no periodic
   reading.
7. **Files.** Settings and user presets live in `%APPDATA%\it.switcheq.app\settings.json`, together
   with the cached AutoEq index (`autoeq-index.tsv`). The environment variable `SWITCHEQ_CONFIG_DIR`
   overrides that folder and `SWITCHEQ_APO_CONFIG` overrides Equalizer APO's `config` folder (used by
   tests and trial instances so they never touch a real installation).

Source layout:

```text
index.html, src/          frontend: Vite + vanilla TypeScript; src/i18n/*.json = translations
src-tauri/
  src/brand.rs            product name, identifier and file names (one place)
  src/profiles.rs         headphone profile, AutoEq index, search, import, source exclusion (+ tests)
  src/net.rs              text download from the single allowed host
  src/settings.rs         JSON settings, chosen output, profile, taste presets (+ tests)
  src/presets.rs          taste presets built on the profile (+ tests)
  src/eq.rs               biquad filters, preamp, Equalizer APO config text (+ tests)
  src/eqapo.rs            Equalizer APO detection and file writing
  src/audio.rs            Core Audio: endpoints, connection (KS), default output
  src/bluetooth.rs        WinRT: paired devices, connection, battery
  src/autoswitch.rs, media.rs, procs.rs   automatic presets, movies/music, processes
  src/startup.rs          start with Windows (HKCU\...\Run)
  src/i18n.rs             Rust-side translations (tray, notifications, errors, presets) (+ tests)
  src/lib.rs              commands, tray, background monitors
  windows/hooks.nsh       installer hook: optional Equalizer APO download
```

## Installation

**Requirements:** Windows 10 or 11 (64-bit), the WebView2 runtime (included in Windows 11; the
installer downloads Microsoft's bootstrapper if it is missing) and
[Equalizer APO](https://sourceforge.net/projects/equalizerapo/).

1. Download `SwitchEQ_<version>_x64-setup.exe` from the
   [Releases page](https://github.com/BennyCapozzo/switcheq/releases). Installers are built by GitHub
   Actions from a version tag; each release has a `.sha256` file next to the installer and the
   expected hash in its notes. [Verify the download](#verify-the-download), then run it. It installs
   for the current user only, without administrator rights. Installer languages: English and
   Italian.
2. **Equalizer APO is not included** (it is a separate GPL v2+ program). If it is not installed, at
   the end of the setup the installer **asks** (default answer: No) whether to download it. If you
   agree it:
   - downloads version **1.4.2** from SourceForge (about 12 MB);
   - checks its **SHA-256** against the value pinned in
     [`src-tauri/windows/hooks.nsh`](src-tauri/windows/hooks.nsh) and **discards the file without
     running it** if the hash differs;
   - warns that Equalizer APO's own setup is **not code-signed** (SmartScreen or your antivirus may
     warn), needs administrator rights (UAC) and may ask for a restart;
   - launches that setup, which finishes by opening Equalizer APO's official **Device Selector**.

   The question is skipped in silent (`/S`), passive (`/P`) and update modes, and when Equalizer APO is
   already installed. To disable the hook, remove `installerHooks` from `tauri.conf.json`. You can also
   install Equalizer APO yourself at any time.
3. **Choose the device in Equalizer APO.** In the Device Selector (or later in *Configurator.exe* from
   the Equalizer APO folder) tick **your headphones' playback device** and apply. SwitchEQ never
   writes the `MMDevices` registry keys: enabling the effect on a device is left to Equalizer APO.
   If your headphones appear as two devices (stereo and "hands-free"), select at least the stereo one;
   for calls, also the hands-free one. A restart may be needed.
4. Start SwitchEQ. If Equalizer APO is missing the app says so, with the steps and a download button.

The installer is **not code-signed**, so Windows SmartScreen may show "Windows protected your PC"
the first time; choose *More info*, then *Run anyway* only if you trust where you got the file.

### Verify the download

Compare the installer's SHA-256 with the value in the release notes (or in the `.sha256` file next to
the installer). In PowerShell, from the folder with the download:

```powershell
Get-FileHash .\SwitchEQ_<version>_x64-setup.exe -Algorithm SHA256
```

(or `certutil -hashfile SwitchEQ_<version>_x64-setup.exe SHA256`). The hash must be identical (the
case does not matter); if it is not, do not run the file. This detects a corrupted or altered
download, but it is not a signature: it is only as trustworthy as the page you read the expected value
from.

## Usage

On first run a three-step wizard asks for:

1. **Audio output device**: the list of Windows playback endpoints (Bluetooth or wired). This chooses
   the `Device:` line used for Equalizer APO.
2. **Headphone model**: type the model and press *Search*. The result shows the measurement source
   and the rig it comes from. Choose it and the app downloads that correction. You can also import a
   `ParametricEQ.txt` file, or skip this step and choose a model later. The *Refresh index* button
   re-downloads the AutoEq list; *About AutoEq* opens the project page.
3. **Default preset**: the chosen model's correction. Taste presets are added on top of it.

Everything can be changed later from the *Headphones* card ("Change model"). Without a model the Flat preset remains and
the taste presets still work (without the measured correction). The app also offers:

- **Equalizer** card: pick a preset from the list (the tray menu shows the active one), tweak it in the
  graphic or parametric editor, save your own presets, import `ParametricEQ.txt` files.
- **Settings**: language, start with Windows, stay hidden in the tray on automatic start, make the
  headphones the default output when they connect, uniform volume, the Bluetooth names to recognise.
- **Automatic presets for apps and games** card: games, per-executable rules, movie and music detection.
- **Headphones** card, for Bluetooth outputs: connection state and battery; *Release headphones* / *Reconnect to this PC*.

AutoEq and Equalizer APO are different things: AutoEq provides the **profiles** (filter values) and
Equalizer APO is the **engine** that applies them. SwitchEQ is the glue between the two.

## Taste presets

Each taste preset is **the chosen model's correction plus a few filters** with a documented rationale
(in [`presets.rs`](src-tauri/src/presets.rs) every number has its source next to it). None of them is
a measurement. If you change the model variant (for example ANC), the base changes and the active
presets are recomputed.

References: **[Olive 2019]** S. Olive et al., AES paper 10156 (64% of listeners prefer the Harman
target, 21% prefer 2-3 dB less bass, 15% prefer 4-6 dB more); **[Olive 2015]** *Factors that Influence
Listeners' Preferred Bass and Treble Levels in Headphones*; **[Harman]** the 2018 over-ear target
(about +6 dB shelf at 105 Hz, Q 0.7); **[SII]** ANSI S3.5-1997 (the 1-4 kHz octaves carry 72% of speech
intelligibility); **[Masking]** upward spread of masking (Moore); **[LFE]** the Dolby LFE channel,
limited to 120 Hz, +10 dB; **[ISO 226]** equal-loudness contours (compensations are computed with the
standard's formula and approximated with two shelves, checked by a test); **[BT]** HFP with mSBC: 16 kHz
audio, useful band about 7-8 kHz.

| Preset | Filters added to the correction | Rationale |
|---|---|---|
| **Movies / Cinema** | LSC 80 Hz +5 (Q 0.7); PK 250 Hz -1.5 (Q 1); PK 2 kHz +2 (Q 0.7) | Sub-bass for LFE and explosions [LFE]; +5 dB is the centre of the "more bass" preference [Olive 2019]; -1.5 dB at 250 Hz against muddiness [Masking]; dialogue [SII]. The cinema X-curve is not applied (it is meant for theatres) |
| **TV series / Night dialogue** | LSC 105 Hz -3; PK 2 kHz +3 (Q 0.7) | Reduced bass [Olive 2019], voice forward [SII]. An EQ does not compress dynamics: use the player's or TV's night mode for that |
| **Competitive gaming / FPS** | LSC 105 Hz -4; PK 3.5 kHz +3 (Q 0.9) | Less bass so it does not mask the rest [Harman, Masking]; more energy where footsteps and transients live |
| **Immersive gaming** | LSC 105 Hz +4; HSC 10 kHz +1.5 | "More bass" and "more treble" preference of young gamers [Olive 2019, 2015]. This is not HRTF spatialization |
| **Classical / Orchestral** | LSC 120 Hz +5.5 (Q 0.4); HSC 10 kHz +1.5 (Q 1) | ISO 226 compensation from 80 to 70 phon |
| **Jazz / Acoustic** | LSC 105 Hz -2 | Cautious end of the "less bass" group [Olive 2019] |
| **Rock / Metal** | LSC 105 Hz +4; PK 3 kHz -1.5 (Q 1.4) | More kick and bass; 3-4 kHz is the most sensitive region [ISO 226] |
| **Electronic / Hip-hop** | LSC 60 Hz +6 | High end of the "more bass" preference, moved to 60 Hz (808, synth bass) |
| **Bass boost** | LSC 105 Hz +6 | Shape of the Harman shelf |
| **Low-volume listening** | LSC 120 Hz +11 (Q 0.4); HSC 10 kHz +3.5 (Q 1) | ISO 226 loudness from 80 to 60 phon |
| **Voice / Podcast** | LSC 105 Hz -4; PK 2 kHz +2.5 (Q 0.7); PK 6.5 kHz -1.5 (Q 2) | Less proximity effect; voice [SII]; softer sibilants |
| **Calls / Meetings** | LSC 105 Hz -6; PK 2 kHz +3 (Q 0.7); HSC 8 kHz -3 | Below 100 Hz a call has only noise [Harman]; above 7-8 kHz only hiss [BT] |

### Uniform volume

Every preset has a different automatic preamp, so without precautions the perceived volume changes by
a few dB at every switch (also automatically when a game starts). With the option on, all presets use
the **same preamp**: the most negative among the built-in presets built on the chosen profile (the
`auto_preamp` function in `eq.rs`), so none exceeds 0 dBFS. Windows' volume is never touched.

- **Low-volume listening** is excluded from the calculation (alone it would lower all the others by
  several dB) and keeps its own, more negative preamp.
- **Flat** and **bypass** use the common preamp: turning the EQ off does not raise the volume.
- **User presets**: a manual preamp is left alone; an automatic one equals the common preamp, or its
  own if it needs more attenuation.
- **Trade-off**: everything sounds a little quieter; compensate with the volume of individual apps.

## Automatic presets

**A process counts as a game** if it lives in a launcher library (`\steamapps\common\`, `\Epic Games\`,
`\GOG Galaxy\Games\`, `\EA Games\`, `\Ubisoft Game Launcher\games\`, `\XboxGames\`, `\Riot Games\`), was
started by a launcher (Steam, Epic, Battle.net, EA app, Ubisoft Connect, GOG Galaxy, Riot Client, Xbox
/ Store games), is a child of a game, is (optionally) a full-screen foreground app, or has a user rule
"Treat as a game". Browsers, video players, Spotify, Discord, OBS, the Windows shell, installers,
updaters and anti-cheat are never games; the **Ignore** rule excludes any other program.

**User rules** map `executable -> preset`, with suggestions from the running apps. **Priority**:
foreground app first, then the rules in list order, then games. With a movie or music playing:
game > movie/music > per-executable rule > manual choice.

**Restore**: on first activation the app saves the preset in use in `settings.json`; when no candidate
is left it restores it. If you pick another preset by hand while an automatic one is active, your
choice wins. A Windows notification on change is optional.

**Movies and music** use the Windows media sessions and are event driven. Music: YouTube (always, videos
too), YouTube Music, Spotify, Apple Music, TIDAL, Deezer, Amazon Music. Movies: Netflix, Disney+, Prime
Video, VLC, Windows Media Player and Movies & TV (Music if the file is audio only). In a browser the tab
title is used; unrecognised browser sessions do not change preset. Hysteresis: the preset changes after
2 s of continuous playback and, when paused, returns after 10 s. User rules `app or site -> Music /
Movies / Ignore / preset` win over the built-in recognition. For music the default preset is the chosen
model's correction, for movies it is Movies / Cinema.

## Known limitations

- **The installer is not code-signed.** SmartScreen and some antivirus programs may warn about both the
  SwitchEQ installer and Equalizer APO's own (also unsigned) setup.
- **Not verified**: the optional Equalizer APO download in the installer has been built but not run
  end-to-end in automated tests; game detection is heuristic; **VLC 3** usually does not publish a media
  session (use a per-executable rule for `vlc.exe`); the app identifiers (AUMID) of Chrome, Edge and
  Store apps (Netflix, Disney+, Prime Video...) were not verified on every setup. English translations
  have not been reviewed by a native speaker.
- **Release / Reconnect headphones** (Bluetooth only): WinRT has no API to reconnect an already paired
  Bluetooth Classic audio device. The app uses the kernel-streaming property `KSPROPSETID_BtAudio`
  (`ONESHOT_RECONNECT` / `ONESHOT_DISCONNECT`) of the Microsoft Bluetooth driver; with third-party
  drivers it opens `ms-settings:bluetooth` instead. It acts only on this PC.
- **Battery** comes from `DEVPKEY_Bluetooth_Battery`, set only if the headphones report the level over
  HFP; otherwise "Battery n/a". No vendor-specific protocol is used.
- **Default output** uses the undocumented `IPolicyConfig` COM interface, which may stop working in a
  future Windows version.
- **Equalizer APO must be enabled on the headphones' endpoint** with its own Device Selector or
  Configurator. The `Device:` directive searches for the words in the device name, the connection name
  and the GUID, so if you rename the device in Windows, choose the output again.
- **Headphones with several measurements** (ANC, maker EQ modes): each variant is a different
  measurement; choose the one matching how you use the headphones.
- **Performance**: with the window closed the app idles at roughly 35 MB (one process, measured on one
  machine); with the window open WebView2 adds several processes and a few hundred MB.
- **Network**: searching and downloading a profile needs the internet; offline, the cached index and any
  profile already chosen keep working.
- **Measurements are third-party data** with their own terms; see [Licences](#licences-and-data-sources).
  The taste presets are generic filters and are not a substitute for listening.

## Privacy

Checked in the code (`src-tauri/src/net.rs`, `lib.rs`, `Cargo.toml`):

- **No telemetry, analytics, crash reporting, accounts or automatic updater.** None of that code or those
  dependencies exists in the app.
- **The only network access the app makes** is downloading text from
  `raw.githubusercontent.com/jaakkopasanen/AutoEq/<pinned commit>/`: the results index (about 850 KB,
  cached for 7 days) and one `ParametricEQ.txt` per measurement you choose, only when you search, refresh
  or choose a model. The URL allow-list is closed in `net.rs`; the request carries only the
  `User-Agent: SwitchEQ/<version>`, and uses the system proxy and certificates.
- **Links** (AutoEq project page, Equalizer APO's SourceForge page, the Bluetooth settings page) are
  opened in your normal browser or Settings only when you click, and only for a short allow-list.
- **The installer** may contact SourceForge to download Equalizer APO if you say yes, and Microsoft's
  servers to fetch the WebView2 bootstrapper if the runtime is missing.
- **What is stored locally**: your settings, chosen profile, user presets and rules (including the
  executable names you add) in `%APPDATA%\it.switcheq.app\settings.json`, the AutoEq index cache, and
  the `switcheq.txt` file in Equalizer APO's config folder. The process list, media sessions and
  Bluetooth information are read locally to apply your rules and are never sent anywhere.

## Building from source

Requirements: Rust stable with the MSVC toolchain, Node.js 20 or newer, and WebView2 (present in
Windows 11).

If `npm install` fails with "node is not recognized", or `tauri build` with "cargo metadata ... program
not found" (typical when Node is managed by nvm for Windows), put Node and Cargo at the front of `PATH`
for that session:

```powershell
$env:Path = "C:\nvm4w\nodejs;$env:USERPROFILE\.cargo\bin;" + $env:Path
```

(use the folder where your Node is installed instead of `C:\nvm4w\nodejs`). Then:

```powershell
npm install
npm run tauri dev        # development with hot reload
npm run tauri build      # NSIS installer in src-tauri\target\release\bundle\nsis
cd src-tauri
cargo test
cargo clippy --all-targets
```

To try a build **without touching an existing installation** use a separate target directory
(`$env:CARGO_TARGET_DIR = "target-alt"`, which `tauri build` resolves inside `src-tauri`) and set
`SWITCHEQ_CONFIG_DIR` (settings folder) and `SWITCHEQ_APO_CONFIG` (Equalizer APO config folder) to
temporary folders. Never start a development build with `--autostart`, and do not run the installer on
a machine where you do not want it installed.

The product name and identifiers (Tauri identifier `it.switcheq.app`, `switcheq.txt`, startup entry)
are defined in [`src-tauri/src/brand.rs`](src-tauri/src/brand.rs) together with `tauri.conf.json` and
`package.json`; a test checks that they agree.

## Contributing

Issues and pull requests are welcome. Please run `cargo test` and `cargo clippy --all-targets` in
`src-tauri` and `npm run build` before sending a change. New behaviour should come with a test; test
fixtures must use made-up numbers, never real measurements.

### Releasing

Releases are built by [`.github/workflows/release.yml`](.github/workflows/release.yml). Make the
version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json` agree, merge to
`main`, then push a matching tag (`git tag v0.1.0 && git push origin v0.1.0`). The workflow builds the
unsigned NSIS installer and creates a **draft** GitHub Release with the installer, its `.sha256` file
and the notes from [`.github/release-notes.md`](.github/release-notes.md); review it and publish it by
hand. [`ci.yml`](.github/workflows/ci.yml) runs the tests, clippy and the frontend build on every push
and pull request.

### Languages and translations

The interface is available in Italian and English. At start it follows the Windows language (any
language other than Italian falls back to English); you can change it in Settings -> Language and the
choice applies at once to the window, tray menu, notifications, error messages and the name, description
and rationale of the presets.

All texts live in `src/i18n/<code>.json`: a flat `key -> text` object with `{name}` placeholders. The
same file serves the frontend (`src/i18n.ts`) and the Rust backend (`src-tauri/src/i18n.rs`, embedded in
the binary with `include_str!`, macro `t!("key", name = value)`). Preset keys are
`preset.<id>.name`, `.desc` and `.why` (the rationale with numbers and sources: translate the words and
keep numbers and citations unchanged).

To add a language (example: `fr`):

1. Copy `src/i18n/en.json` to `src/i18n/fr.json` and translate the values. Keys and `{name}` placeholders
   stay identical; the `lang.name` key is the language's own name.
2. In `src/i18n.ts` add `import fr from "./i18n/fr.json";` and `fr` to `TABLES`.
3. In `src-tauri/src/i18n.rs` add a row to `LANGUAGES`
   (`Language { code: "fr", windows_primary: 0x0c, json: include_str!("../../src/i18n/fr.json") }`; the
   number is the primary language identifier of Windows, `LANGID & 0x3ff`).
4. Run `cargo test` inside `src-tauri`: the tests fail if a key is missing in a language, a placeholder
   does not match, a text is empty, or the code uses a key that does not exist (preset keys included).

Comments in the generated Equalizer APO file and the logs stay in English.

## Licences and data sources

- **Code:** [MIT](LICENSE). Notices for the dependencies, AutoEq and Equalizer APO are in
  [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).
- **Third-party data is not covered by the MIT licence.** AutoEq is MIT, but the measurements behind its
  corrections belong to their authors and carry their own terms (for example, oratory1990 forbids
  commercial use). SwitchEQ ships **no** measurement, filter value or list of example models. Profiles
  are downloaded to your PC only when you ask, with the source shown in the interface.
- **Rtings is excluded.** Rtings' terms of use forbid reproduction, derivative works and databases
  without written consent, so entries measured by Rtings are filtered out of the search and refused at
  download (`EXCLUDED_SOURCES` in `src-tauri/src/profiles.rs`, covered by a unit test). No Rtings data,
  nor any Rtings-based preset, is in the repository or in the binary.
- **Sources, links and terms:** [`docs/data-sources.md`](docs/data-sources.md). The research behind them,
  with what could not be verified: [`docs/research-licensing.md`](docs/research-licensing.md) (written in
  Italian).
- **Equalizer APO** is an independent project (GPL v2 or later) installed separately; it is not
  distributed with SwitchEQ.
- Names of products and services that appear in the documentation or detection lists belong to their
  owners and are used only to identify them. SwitchEQ is not affiliated with any of them.

This is not legal advice. Anyone who redistributes SwitchEQ commercially, or bundles downloaded
profiles, must check the terms of the corresponding measurement source.
