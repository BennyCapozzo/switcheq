# Data sources and licences / Fonti dei dati e licenze

> **The MIT licence of this repository does NOT cover third-party data.** / **I dati di terzi NON sono coperti
> dalla licenza MIT di questo repository.** Headphone measurements, and the equalizer settings computed from
> them, belong to their authors and carry their own terms. This is not legal advice; the research behind this page
> is in [research-licensing.md](research-licensing.md) (checked 2026-10-11, with the list of what could not be verified).

## What the repository contains / Cosa contiene il repository

| Item | Where | Third-party data? |
|---|---|---|
| Headphone profiles | nowhere | **None.** No measurement, no filter value and no list of example models ships in the repository or the binary. |
| Taste presets (`src-tauri/src/presets.rs`) | binary and repo | No measurement data. They are generic shelf/peak filters written for this project and **added to** the profile's correction. Their rationale cites published research (Olive et al., AES 2015 and 2019; ISO 226:2003; the Harman target as described in the literature), which is referenced, not copied. |
| Test fixtures | tests only | Made-up numbers. |

Rtings, Innerfidelity, Headphone.com and oratory1990 data are **never** included in the repository or the binary.
Rtings measurements are additionally **filtered out of the AutoEq search and refused at download** (see below).

## What is downloaded at runtime / Cosa si scarica a runtime

Only when the user picks a model (or presses search / refresh):

1. `results/INDEX.md` of AutoEq (about 850 KB), cached locally as `autoeq-index.tsv` (7 days).
2. One `ParametricEQ.txt` per chosen measurement (a few hundred bytes).

Rules applied by the app (`src-tauri/src/net.rs`, `profiles.rs`):

- host and path limited to `raw.githubusercontent.com/jaakkopasanen/AutoEq/`, at a **fixed commit**
  (`AUTOEQ_COMMIT`), never `master`;
- `User-Agent: SwitchEQ/<version>`;
- nothing is downloaded in the background or in bulk; never the whole tree;
- **Rtings is excluded**: entries whose measurement source is Rtings (the first folder of the AutoEq path, or the
  author named in `INDEX.md`, compared case-insensitively) are dropped when the index is read and again when the cache
  is read, so they never appear in a search; and `parametric_url` refuses such a path, so a download cannot be
  requested even by a forged request. The unit test
  `rtings_measurements_are_excluded_from_search_and_download` (`profiles.rs`) covers all three points. The list is the
  constant `EXCLUDED_SOURCES`;
- the chosen profile is copied into the user's local settings; after that no network is needed;
- the source is shown in the UI (under the preset: "Source: AutoEq (MIT) / `<measurement source>` — path — link at
  the pinned commit"), together with a notice that some sources (for example oratory1990) forbid commercial use.

## Sources / Fonti

| Source | Link | Licence / terms found | In repo or binary | Runtime download |
|---|---|---|---|---|
| **AutoEq** (code and results repository) | <https://github.com/jaakkopasanen/AutoEq> | MIT, Copyright (c) 2018-2022 Jaakko Pasanen. Cannot grant rights over third-party measurements. | Nothing copied | Yes, pinned commit |
| **Kuulokenurkka / Headphone Corner** | <https://kuulokenurkka.com/en/headphone-measurements/> | No explicit licence; describes itself as "open data collection" and invites using its measurements in AutoEq. Footer: "Copyright (c) 2026 Headphone Corner". Low risk, written confirmation still recommended. | **No** (the former example profile was removed) | Only if the user picks such a model |
| **oratory1990** | <https://www.reddit.com/r/oratory1990/wiki/index/list_of_presets/> | "Unlicensed commercial use is prohibited." Non-commercial use with credit. | No | Only if the user picks such a model; the UI warns about commercial use |
| **Rtings** | <https://www.rtings.com/company/terms-of-use> | Personal, private, non-commercial use only; forbids reproduction, derivative works and building databases without written consent. | **No** (the former `RTINGS_5128` data was removed) | **No**: excluded from search and download |
| **Innerfidelity / Stereophile** | <https://www.stereophile.com/> | "All rights reserved" (AVTech Media). | **No** | Only on explicit user choice |
| **Headphone.com (legacy)** | <https://headphones.com/> | Terms could not be read; treated as unknown. | **No** | Only on explicit user choice |
| **Crinacle** | <https://crinacle.com/graphs/headphones-legacy/> | No terms on reuse found (silence is not permission). | **No** | Only on explicit user choice |
| **Equalizer APO** (separate program) | <https://sourceforge.net/projects/equalizerapo/> | GPL v2 or later, Copyright Jonas Thedering. | **Not included** in the installer | Optional installer hook, version 1.4.2, SHA-256 pinned |

The measurement source of each correction is the first folder of its AutoEq path (for example
`oratory1990/over-ear/...`), and is what the UI shows.

## Responsibility of the user / Responsabilità dell'utente

When someone picks a model, the app downloads a file the user asked for from a public GitHub repository and
stores it on their own PC, with the source visible. Whoever redistributes SwitchEQ commercially, or bundles
downloaded profiles, must check the terms of the corresponding measurement source themselves.

## Changing the data / Modificare i dati

- **Exclude another source**: add its lower-case name to `EXCLUDED_SOURCES` in `profiles.rs` and extend the test.
- **Bundle a profile**: not supported today. Doing so would need a written permission from the measurement author,
  a place to keep the filters, and an entry here.
- **Update the AutoEq commit**: change `AUTOEQ_COMMIT`, `INDEX_URL` and `RESULTS_URL` in `profiles.rs` together
  (a test checks that they agree), then check that the index and a sample `ParametricEQ.txt` still exist at the new commit:
  `https://raw.githubusercontent.com/jaakkopasanen/AutoEq/<commit>/results/<path>`.

## Open points / Punti aperti

- Written confirmation from Headphone Corner and oratory1990 would be needed before bundling any of their data.
- Innerfidelity, Headphone.com and Crinacle entries are still listed by the search (only Rtings is filtered); their
  source and a licence warning are shown, and the user chooses explicitly.
- Whether the filter values themselves (a few dozen numbers) are protected by copyright or database rights is a
  legal question this project has not assessed.
