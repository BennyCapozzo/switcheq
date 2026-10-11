# Third-party notices

SwitchEQ's own code is MIT-licensed (see [LICENSE](LICENSE)). This file lists the third-party
software it uses or interacts with, and the notices their licences require. Third-party **data**
(headphone measurements) is described separately in [docs/data-sources.md](docs/data-sources.md) and is
**not** covered by SwitchEQ's MIT licence.

Avvisi di terze parti (IT): il codice di SwitchEQ è sotto licenza MIT. Qui sono elencati il software di
terzi usato e gli avvisi richiesti dalle loro licenze; i dati di misura di terzi sono descritti in
`docs/data-sources.md` e **non** sono coperti dalla MIT di SwitchEQ.

## AutoEq

- Project: <https://github.com/jaakkopasanen/AutoEq>
- Licence: MIT. Copyright (c) 2018-2022 Jaakko Pasanen.
- Use: SwitchEQ does **not** bundle any AutoEq file. At the user's request it downloads the results index
  (`results/INDEX.md`) and a single `ParametricEQ.txt` per chosen model from a fixed AutoEq commit
  (`profiles::AUTOEQ_COMMIT` in `src-tauri/src/profiles.rs`), caches them locally and shows the source in the UI.

The AutoEq licence text, reproduced because SwitchEQ names, links to and downloads from the project:

```
MIT License

Copyright (c) 2018-2022 Jaakko Pasanen

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

(Original: <https://github.com/jaakkopasanen/AutoEq/blob/master/LICENSE>.)

The measurements behind the AutoEq results belong to their authors and carry their own terms; the MIT licence
of the AutoEq repository cannot grant rights over them. See [docs/data-sources.md](docs/data-sources.md).

## Equalizer APO (not distributed)

- Project: <https://sourceforge.net/projects/equalizerapo/> — Copyright Jonas Thedering.
- Licence: GNU General Public License, version 2 or (at your option) any later version.
- SwitchEQ is a separate program: it only writes a text file in Equalizer APO's `config` folder
  (`switcheq.txt`, referenced from `config.txt` with an `Include:` line) and reads its install path from the
  registry. **Equalizer APO is not included in the SwitchEQ installer.** The optional installer hook
  (`src-tauri/windows/hooks.nsh`) can, after asking the user, download version 1.4.2 from SourceForge, check
  its SHA-256 and launch it; that installer is not code-signed. Source code of Equalizer APO:
  <https://sourceforge.net/projects/equalizerapo/files/1.4.2/>.

## Tauri and Rust crates

SwitchEQ is built with [Tauri 2](https://tauri.app/) (MIT or Apache-2.0, Copyright the Tauri Programme within
The Commons Conservancy) and statically links the Rust crates below. Summary of the 252 crates in the Windows
dependency graph (`cargo metadata --filter-platform x86_64-pc-windows-msvc`, checked 2026-10-11):

| Licence expression (SPDX) | Crates |
|---|---|
| MIT OR Apache-2.0 (and spelling variants) | about 160 |
| MIT | 45 |
| Unicode-3.0 | 18 |
| Unlicense OR MIT | 10 |
| MPL-2.0 | 5 (`cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext`, `selectors`) |
| Apache-2.0 | 1 (`tao`) |
| BSD-3-Clause | 2 (`alloc-no-stdlib`, `alloc-stdlib`) |
| Zlib | 2 (`foldhash`, `zlib-rs`) |
| other combinations (MIT-0, CC0-1.0, 0BSD, ...) | the rest, always with an MIT or Apache-2.0 option |

Direct dependencies: `tauri`, `tauri-plugin-dialog`, `tauri-plugin-single-instance`, `serde`, `serde_json`,
`tauri-build`, `windows`, `windows-core`, `windows-collections`. The five MPL-2.0 crates are used unmodified;
their source is available from crates.io. Each crate's copyright notice and licence text are in its package on
crates.io. To regenerate the complete list with texts, run
[`cargo about`](https://github.com/EmbarkStudios/cargo-about) or `cargo license` in `src-tauri/`.

## Frontend

The interface is plain TypeScript (no UI framework), bundled with [Vite](https://vite.dev/) (MIT). The only
runtime packages are `@tauri-apps/api` and `@tauri-apps/plugin-dialog` (MIT or Apache-2.0). `typescript`
(Apache-2.0) and `@tauri-apps/cli` (MIT or Apache-2.0) are build-time tools and are not shipped.

## Microsoft WebView2

The window uses the system WebView2 runtime, which the installer downloads from Microsoft if missing
(`webviewInstallMode: downloadBootstrapper`). It is governed by Microsoft's own licence terms and is not part of
this repository.

## Icons

The application icons in `src-tauri/icons/` are a stylised pair of headphones; they contain no third-party logo
or trademark.

## Names

Product and brand names that appear in the documentation, the preset descriptions or the app detection lists
(for example streaming services, game launchers or music players) are trademarks of their owners and are used only to
identify them. SwitchEQ is not affiliated with, sponsored by or endorsed by AutoEq, Equalizer APO, any headphone
maker or any measurement source.
