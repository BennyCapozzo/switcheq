# Licenze, termini e nomi: rendere il repo open source e generico

Ricerca del 2026-10-11. Nessun codice dell'app è stato modificato. Le citazioni sono testuali
(in inglese) e vengono dai file/pagine originali letti con `curl` o, dove il sito blocca i
client automatici, da copie su web.archive.org (indicato caso per caso).

> Questa **non è una consulenza legale**. Dove i testi sono ambigui lo dico, e l'elenco di
> ciò che non ho potuto verificare è in fondo.

> **Aggiornamento successivo alla ricerca.** Il profilo di esempio con misure Kuulokenurkka è stato rimosso: nel repository e nel binario non c'è alcun dato di profilo. Le misure Rtings sono escluse dalla ricerca e rifiutate al download (`EXCLUDED_SOURCES` in `src-tauri/src/profiles.rs`, con test). Il resto del documento è lo storico della ricerca e non è stato riscritto.

## 0. Riepilogo

| Tema | Conclusione |
|---|---|
| Codice AutoEq | MIT: riusabile, con avviso di copyright e testo della licenza. |
| File `results/` di AutoEq | Nessuna licenza propria: ricadono sotto la MIT del repo, **ma la MIT di Pasanen non può concedere diritti sulle misure altrui**. Il rischio dipende dalla fonte (tabella al §2). |
| Rtings | Termini d'uso vietano riproduzione, opere derivate e creazione di database senza consenso scritto. **Fonte da evitare** (oggi usata dal preset `RTINGS_5128`). |
| oratory1990 | "Unlicensed commercial use is prohibited." Uso non commerciale ok con credito. |
| Kuulokenurkka | Nessuna licenza esplicita, ma dichiara "open data collection" e invita a usare le misure in AutoEq. Rischio basso, da confermare per iscritto. |
| Crinacle / Headphone.com / Innerfidelity | Crinacle: nessun termine sui dati. Innerfidelity: "All rights reserved". Headphone.com: non verificato. |
| Equalizer APO | GPL v2 o successiva. **Non incorporarlo nell'installer**: scaricarlo da SourceForge (versione e SHA-256 fissati). L'installer è non firmato e lancia comunque una GUI anche con `/S`. |
| Licenza del repo | **MIT** per il codice, dati di terzi tenuti separati con avvisi di attribuzione. |
| Nome | **SwitchEQ** (scelto): nessun conflitto esatto su GitHub, Microsoft Store, SourceForge, PyPI/crates e domini; ma esiste `EQ Switch` (`matejarh/EQ-Switch`), stessa nicchia e nome molto simile, e il marchio non è verificabile con gli strumenti a disposizione. "Auto Equalizer" sconsigliato. |

## 1. AutoEq: codice e risultati

Repository: <https://github.com/jaakkopasanen/AutoEq> (branch `master`, ultimo push 2025-07-20;
metadati API GitHub: `"license": {"key": "mit", "spdx_id": "MIT"}`).

Testo del `LICENSE` (<https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/LICENSE>):

> MIT License
> Copyright (c) 2018-2022 Jaakko Pasanen
> Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:
> The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

Fatti verificati:

- Tra le voci dell'albero git lette (52.581, risposta troncata dall'API GitHub) c'è **un solo** file di licenza: `LICENSE` in radice.
  Né `results/`, né `measurements/` hanno licenze o avvisi propri, e il README di una cartella risultato
  (es. `results/Kuulokenurkka/over-ear/Dyson OnTrac (ANC off, neutral)/README.md`) contiene solo le tabelle dei filtri.
- Il README di AutoEq elenca le fonti delle misure ("oratory1990, crinacle, Innerfidelity, Rtings and legacy
  headphone.com measurements") senza dichiarare i loro termini, e dice dei risultati precalcolati:
  "Pre-computed equalizer settings in results, although these should not be used by normal users since
  autoeq.app exists".
- Le misure Kuulokenurkka e Rtings sono in `measurements/Kuulokenurkka` e `measurements/Rtings`; i risultati in
  `results/Kuulokenurkka/over-ear/Dyson OnTrac (...)` e `results/Rtings/Bruel & Kjaer 5128 over-ear/Dyson OnTrac`.

**Si possono includere in un'app open source?** Il codice sì, senza dubbio. I file `ParametricEQ.txt` sono
numeri (preamp + ~10 filtri) calcolati da un algoritmo MIT su misure di terzi. La MIT copre quello che
Pasanen possiede; non può rilicenziare il diritto d'autore o i termini contrattuali dei proprietari delle
misure. Tuttavia dieci frequenze/guadagni/Q sono dati fattuali con scarsa protezione autoriale. Il rischio
reale è quindi **contrattuale** (termini d'uso del sito, es. Rtings) più che di copyright, e va valutato per fonte.

**Download a runtime da `raw.githubusercontent.com`?** Nessun divieto specifico. Le regole generali di GitHub
(<https://docs.github.com/en/site-policy/acceptable-use-policies/github-acceptable-use-policies>)
vietano "using our servers for any form of excessive automated bulk activity, to place undue burden on our
servers through automated means"; il download di un singolo file per cuffia su richiesta dell'utente non rientra.
Pratica consigliata: richiesta solo su azione dell'utente, `User-Agent` identificativo, cache locale, mai
scaricare l'albero intero. Rischi tecnici: i percorsi delle cartelle in `results/` contengono nomi lunghi e variabili (es. `Dyson OnTrac (ANC off, neutral)`) e possono essere riorganizzati, quindi **fissare un commit** invece
di `master`, e prevedere errore pulito se il file non c'è.

**Attribuzione.** La MIT richiede di mantenere l'avviso di copyright e il testo del permesso "in all copies or
substantial portions". Se copiamo i file o ne incorporiamo i valori nel repo, includere in
`THIRD_PARTY_NOTICES` il copyright "Copyright (c) 2018-2022 Jaakko Pasanen" e il testo MIT, il link
all'esatta cartella `results/...` e il commit. In più, credito all'autore della misura (oratory1990 lo chiede;
vedi §2). Con il solo download a runtime non copiamo nulla nel repo, ma mostrare fonte e licenza nell'UI resta
la forma corretta.

## 2. Termini delle misure originali

| Fonte | Termini trovati (citazione) | Può includere nel repo | Può scaricare | Attribuzione | Rischi |
|---|---|---|---|---|---|
| **Rtings** | <https://www.rtings.com/company/terms-of-use> ("Effective September 2016"), art. 3.2: "...for User's personal private and non-commercial use only, provided that User does not: (a) download or print any Material in a systematic or regular manner to create a database... (c) modify, translate, reverse engineer, reproduce, decompile, disassemble or create derivative works of any of the Material; (d) ...copy, commercially exploit or give or transfer any rights in the Material in any form, to any person or entity without RTINGS prior written consent." Art. 10: "any reproduction, total or partial, of the contents of RTINGS is totally forbidden." | **No** (senza consenso scritto) | Non tramite il nostro prodotto |—| Alto. Il preset `RTINGS_5128` copia 1:1 filtri derivati da loro misure. Chiedere permesso a Rtings oppure rimuovere. |
| **oratory1990** | Wiki reddit, copia Wayback del 2025 (<https://web.archive.org/web/2025/https://www.reddit.com/r/oratory1990/wiki/index/list_of_presets/>, pagina con "latest update: 22.10.2025"): "Unlicensed commercial use is prohibited." Pagina indice: "The measurement source is noted in the PDF at the bottom left. Send me a PM if you think I've forgotten to credit the original measurement source." FAQ: i file su GitHub sono "created and maintained by u/jaakkopasanen... has very little to do with me", e per i preset di oratory "I also listen to the headphones and fine-tune the settings by ear". | Solo per progetto non commerciale, con credito | Sì, non commerciale | Citare oratory1990 e la fonte originale della misura | Medio-alto se l'app diventa commerciale; i suoi preset *originali* (PDF/Reddit) sono distinti dai risultati AutoEq e non vanno copiati. |
| **Kuulokenurkka** ("Headphone Corner", <https://kuulokenurkka.com/en/headphone-measurements/>, **non** `.fi`) | Nessuna licenza. Testo: "In the spirit of open data collection, the site also features many measurements made by other people." e "You can also use Headphone Corner's measurements in AutoEQ, as they are now available in the application's database." Piè di pagina: "Copyright © 2026 Headphone Corner". | Probabile sì, ma senza licenza scritta | Sì (via AutoEq) | Citare "Headphone Corner / Kuulokenurkka" e il link corretto | Basso. Una email di conferma scritta rimuoverebbe l'ambiguità. Il dominio `kuulokenurkka.fi` citato in `src-tauri/src/presets.rs` **non esiste** (NXDOMAIN, controllato via DoH Cloudflare): correggere in `.com`. |
| **Crinacle** (<https://crinacle.com/graphs/headphones-legacy/>) | Pagina legacy (Wayback): "Measurements in this database are raw and do not conform to any rigid industry standard." Nessun termine su riuso/redistribuzione; solo informativa privacy e pubblicità CafeMedia. | Non chiaro | Non chiaro | Citare comunque | Medio (silenzio ≠ permesso). Evitare finché non c'è un'autorizzazione. |
| **Headphone.com (legacy)** | Non sono riuscito a leggere i termini (la pagina non era raggiungibile né in archivio). | Non verificato | Non verificato | — | Sconosciuto: escludere. |
| **Innerfidelity / Stereophile** | Pagina InnerFidelity Headphone Measurements su stereophile.com (Wayback): "© 2024 Stereophile, AVTech Media Americas Inc., USA, All rights reserved". Termini AVTech (<http://www.avtechmediausa.com/terms.html>, Wayback): "the site may not be copied, reproduced, republished, uploaded, transmitted, modified, indexed, cataloged, mirrored or distributed in any way, in whole or in part, without the express prior written consent of AVTech Media Americas" (salvo licenze aperte o fair use USA). | **No** | No | — | Alto per redistribuzione; le misure sono datasheet protetti. |

Nota su "filtri derivati": nessuna di queste fonti dice se un EQ calcolato da una misura sia "derivato".
Rtings lo vieta in generale ("derivative works"), oratory1990 vieta l'uso commerciale non autorizzato; negli altri
casi il silenzio non equivale a licenza.

Il modo più pulito per un progetto generico è **non distribuire preset di terzi**, e fare così: (1) l'utente
sceglie la cuffia e l'app scarica da AutoEq il singolo file al momento (fonte e licenza mostrate nell'UI); (2) gli
unici preset inclusi nel repo sono quelli ricavati da fonti con permesso scritto o da misure proprie.

## 3. Equalizer APO

Versione verificata: **1.4.2** (release su SourceForge del 2025-03-21).
Sorgenti letti via `git clone https://git.code.sf.net/p/equalizerapo/code` (commit
`bbfcc3e`, 2025-11-28).

**Licenza.** `License.txt` è il testo della GPL v2. Intestazione dei sorgenti:
"This program is free software; you can redistribute it and/or modify it under the terms of the GNU General
Public License as published by the Free Software Foundation; either version 2 of the License, or (at your
option) any later version." Pagina progetto SourceForge: "GNU General Public License version 2.0 (GPLv2)".
Copyright: "Jonas Thedering".

**Includere l'installer nel nostro NSIS o scaricarlo?** Raccomandato: **scaricarlo**.

- Ridistribuire il suo `.exe` (anche annidato) significa distribuire un binario GPL: servono licenza, avviso di
  copyright e, per GPLv2 §3, i sorgenti corrispondenti (o offerta scritta valida 3 anni). Il nostro codice non
  verrebbe "contagiato" se resta un programma separato (parla con APO solo via file `config.txt`/`Include:` e registro),
  ma gli obblighi di distribuzione sarebbero a nostro carico e l'installer cambierebbe a ogni release.
- Scaricarlo da SourceForge non è una distribuzione da parte nostra. URL ufficiale:
  `https://sourceforge.net/projects/equalizerapo/files/1.4.2/EqualizerAPO-x64-1.4.2.exe/download` (redirect a un
  mirror, 11.980.366 byte). **SHA-256 calcolato qui:** `7403be7427bbe1936a40dded082829b6e217fc4f5990fee5cba501f0ae055afa`.
  Fissare versione e hash e rifiutare file diversi. Il download usa un redirect verso mirror variabili: l'hash è
  l'unica garanzia di integrità.
- Rischio: il file **non è firmato** (`Get-AuthenticodeSignature` → `NotSigned`), quindi SmartScreen/antivirus
  possono avvisare l'utente; non lo possiamo risolvere.
- Se invece lo si volesse includere: obbligo di licenza GPLv2 mostrata, avviso "Equalizer APO © Jonas Thedering,
  GPLv2+", link ai sorgenti (`EqualizerAPO-src-1.4.2.zip` su SourceForge) e nessun cambiamento che neghi i diritti
  GPL. Le librerie dentro l'installer (`Setup.nsi`: FFTW, libsndfile, Qt6, runtime MSVC) hanno licenze proprie che non ho
  verificato file per file.

**Installazione silenziosa.** `Setup.nsi` è un installer NSIS (`RequestExecutionLevel admin`), quindi il flag
standard NSIS `/S` esiste (e `/D=` per la cartella), ma:

- `Setup.nsi` non gestisce una modalità silenziosa propria (nessun riferimento a "silent" nello script) e la sezione
  d'installazione termina con `ExecWait '"$INSTDIR\DeviceSelector.exe" /i' $0`.
- In `DeviceSelector/main.cpp` solo `/u` è trattato come comando senza interfaccia; con `/i` (e senza argomenti) si
  apre il dialogo Qt per scegliere i dispositivi, seguito da `DeviceTestDialog` e da un `QMessageBox` ("This dialog can be
  reopened anytime..."). **Quindi anche con `/S` compare una GUI interattiva** e serve comunque la scelta del dispositivo
  (il "Configurator" delle vecchie versioni si chiama ora Device Selector).
- In base all'esito di `DeviceSelector.exe /i` lo script può impostare `SetRebootFlag`; il nostro wrapper deve
  gestire un eventuale riavvio richiesto.
- Non ho eseguito l'installer (modificherebbe registro e servizi audio del PC dell'utente): il comportamento
  descritto è dedotto dal sorgente, non provato.

**Registrazione programmatica del dispositivo.** Possibile, ma è esattamente ciò che fa `DeviceAPOInfo::install()`:

- Chiavi (tutte in `HKLM`, quindi servono privilegi di amministratore):
  - `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render\{guid-dispositivo}\FxProperties`
    (valori `{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1/2/5/6/7`: LFX/GFX/SFX/MFX/EFX) con i GUID degli APO
    di Equalizer APO: pre-mix `{EACD2258-FCAC-4FF4-B36D-419E924A6D79}`, post-mix `{EC1CC9CE-FAED-4822-828A-82A81A6F018F}`;
  - `HKLM\SOFTWARE\EqualizerAPO\Child APOs\{guid}` (APO originale del dispositivo, `Version`=`2`, ecc.);
  - `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Audio\DisableProtectedAudioDG` = 1 (DWORD);
  - registrazione COM dell'APO (`regsvr32 /s EqualizerAPO.dll`, già fatta dall'installer).
- La modalità si sceglie (LFX+GFX / SFX+MFX / SFX+EFX): il Readme 1.4.2 dice che su Windows 8.1+ SFX/EFX è il
  default e su Windows 11 con Bluetooth si usa SFX/MFX perché EFX non funziona.
- Se `FxProperties` esiste già, il programma salva un backup `.reg` dei valori originali; se mancano permessi, **prende
  la proprietà della chiave e la rende scrivibile** (`takeOwnership` / `makeWritable`). Poi riavvia il servizio audio
  (`ServiceHelper::restartService`), che interrompe l'audio per qualche secondo.
- Rischi di scrivere queste chiavi da soli: un errore può lasciare il dispositivo senza audio o con driver/enhancement
  non ripristinati; aggiornamenti di Windows o dei driver possono riscrivere `FxProperties` (il codice prevede
  per questo un flag `DisableAutomaticAdjustment` per dispositivo); duplicazione di logica con una versione futura di
  Equalizer APO. **Raccomandazione:** non reimplementare; lasciare all'utente il Device Selector ufficiale e limitarsi a
  verificare l'installazione leggendo `HKLM\SOFTWARE\EqualizerAPO\InstallPath` (cosa che l'app già fa).

## 4. Licenza consigliata per il repo

**MIT** per il codice dell'app.

- Compatibile con le dipendenze principali: Tauri è MIT/Apache-2.0 (non verificato qui file per file), AutoEq è MIT.
- Equalizer APO (GPL) resta un programma separato scaricato dall'utente: nessun obbligo copyleft sul nostro codice.
- Struttura: `LICENSE` (MIT), `THIRD_PARTY_NOTICES.md` (AutoEq, Tauri e crate, font/icone), `docs/data-sources.md` con per
  ogni preset fonte, link e licenza, e una nota esplicita che i dati di terzi **non** sono coperti dalla MIT del repo.
- Alternativa: Apache-2.0 (brevetti esplicitati), meno semplice per un'app piccola. Evitare GPL: renderebbe più
  difficile riusare il codice e non serve per Equalizer APO.
- Il nome "Dyson" e "OnTrac" sono marchi altrui: usarli solo in modo descrittivo ("compatibile con...") nei nomi
  delle cuffie, non nel nome del prodotto o nell'icona. (Questo punto è una mia valutazione, non l'ho verificato con una ricerca marchi.)

## 5. Nome: SwitchEQ

Nome scelto dall'utente. Verifiche eseguite il 2026-10-11, con esito per ciascuna fonte.

| Fonte | Come ho cercato | Esito |
|---|---|---|
| GitHub, repository | API search per nome (`switcheq`, `SwitchEQ`, `switch-eq`, `switch_eq`) e ricerca libera `switcheq` | **0 repository** con nome "switcheq"/"SwitchEQ" e 0 risultati nella ricerca libera. Esistono nomi simili, vedi sotto. |
| GitHub, utenti/organizzazioni | API search users `switcheq` | **0 risultati**. Il lookup diretto di `github.com/SwitchEQ` è fallito per rete: non conclusivo. |
| Microsoft Store | API `apps.microsoft.com/api/products/search`, query "SwitchEQ", "Switch EQ", "EQ Switch" (primi 20 risultati ciascuna) | **Nessun titolo "SwitchEQ"** né simile. Risultati fuzzy non pertinenti (FxSound, Boom 3D, Audio Device Switcher, Samsung Smart Switch...). Limite: solo i primi 20 per query. |
| SourceForge | Ricerca web limitata a `sourceforge.net`; accesso diretto a ricerca e a `/projects/switcheq/` | **Nessun progetto "SwitchEQ" tra i risultati.** L'accesso diretto è bloccato (HTTP 403 ai client automatici), quindi non posso escludere un progetto non indicizzato. Progetto vicino per funzione: **Mega Switcher** (interfaccia per Equalizer APO). |
| Siti di software audio / store | Ricerca web generica e limitata a alternativeto, softpedia, KVR, Plugin Boutique, Product Hunt, App Store, Google Play, Chocolatey, npm | **Nessun prodotto "SwitchEQ"**. Solo omonimi parziali di altri settori (Switchee, Switch Energy...). Limite: motore di ricerca generico, non l'indice interno di ciascun sito. |
| Registri di pacchetti | PyPI e crates.io (API): `switcheq` | **404, nome libero.** npm, Chocolatey e winget: **non verificati** (rete/403). |
| Domini | DoH Cloudflare, record NS: `switcheq` `.com .app .io .net .org .dev .eu .it` | **Tutti NXDOMAIN, cioè non registrati** al momento del controllo. |
| Marchi USPTO / EUIPO / TMview / WIPO | Provate le interfacce ufficiali e aggregatori (Justia, Trademarkia, uspto.report) | **Ricerca ufficiale NON eseguibile**: sono applicazioni web JavaScript senza API aperta raggiungibile, e gli aggregatori rispondono 403. Fatta solo una ricerca web indiretta: **nessun risultato "SWITCHEQ"** in USPTO/EUIPO. Non è una ricerca di anteriorità. |

### Conflitti e vicinanze trovati

1. **`matejarh/EQ-Switch` ("EQ Switch")**: è il conflitto più rilevante. App Windows (C++) che gestisce profili
   Equalizer APO con un clic, quindi **stessa nicchia e stesso utente**. Creata 2025-08-01, ultima release v1.3.1
   (2025-12-10), 2 stelle, licenza MIT dichiarata nel README (l'API GitHub non rileva un file di licenza). Il nome
   differisce per spazio e ordine delle parole ("EQ Switch" contro "SwitchEQ"), ma a voce e in ricerca si somigliano molto.
   Non ho trovato rivendicazioni di marchio.
2. **Mega Switcher** (SourceForge, interfaccia per Equalizer APO con preset da AutoEq/oratory1990): non omonimo,
   ma prodotto concorrente con la stessa parola "Switcher".
3. Altri repo per Equalizer APO con nomi simili e poche stelle: `Lentica/Equalizer-APO-Profile-Switcher`,
   `MrLixm/eqap0-switcher`, `KaxyCode/ABI-EQ-AutoSwitch`, `smyhlin/SmartEqualizerPresetsSwitcher`. Tutti descrittivi.
   Significa che "switch" + "EQ" è un'idea ovvia in questo settore: **il nome è poco distintivo come marchio**.
4. **Marchi con prefisso "SWITCH"**: il segno "SWITCH" è molto affollato (SWITCH, SWITCHDOCK e SWITCHWERK
   risultano in USPTO tramite ricerca web). Non ho potuto leggere classi e titolari (403). Un titolare con più marchi
   "SWITCH…" nelle classi software o audio potrebbe opporsi a un composto "SWITCH+EQ". Rischio non quantificabile senza
   una ricerca ufficiale.
5. **Nintendo Switch**: nessun conflitto diretto con "SwitchEQ", ma meglio evitare grafica o descrizioni che richiamino
   la console. (Valutazione mia, non verificata.)

### Giudizio su SwitchEQ

- **Nessun conflitto esatto** trovato su GitHub, Microsoft Store, SourceForge, registri PyPI/crates e domini.
- **Distintività bassa e vicinanza a "EQ Switch"** (stessa funzione, stessa piattaforma): più rischio di confusione e di
  ricerche che mischiano i due progetti che rischio legale concreto.
- **Marchio: non verificato.** Nessuna evidenza di registrazioni, ma nessuna ricerca di anteriorità ufficiale.
- Dopo la scelta conviene riservare subito `switcheq.com` (o `.app`), l'organizzazione GitHub `SwitchEQ`, i pacchetti
  `switcheq` su crates.io/PyPI/npm e il nome nel Microsoft Store (liberi al momento del controllo, tranne npm non
  verificato).

### Alternative (controllo precedente, non ripetuto)

| Nome | Conflitti trovati | Giudizio |
|---|---|---|
| Auto Equalizer | Confondibile con **AutoEq** (`jaakkopasanen/AutoEq`, `autoeq.app`) e con app generiche; `autoequalizer.com` in vendita. | Sconsigliato |
| TuneDock | Repo omonimi (`SDINAHET/TuneDock`, `commaster101/tuneDock`, `gramme-linkcom/TuneDock`, `theworker02/tunedock`); `tunedock.com` in vendita. | Medio |
| HeadTune | `tungcorn/headtune-cli`, `C-algaro/HeadTunes`; `headtune.com` in vendita. | Basso-medio |

## 6. Raccomandazione finale

1. **Licenza**: MIT per il codice + `THIRD_PARTY_NOTICES.md`; dati di terzi esclusi dalla MIT e documentati.
2. **Rtings**: rimuovere il preset `RTINGS_5128` dal repo pubblico (o ottenere consenso scritto da Rtings).
   Stesso trattamento per qualsiasi dato Innerfidelity/Stereophile e Headphone.com.
3. **Preset inclusi**: tenere solo quelli con fonte a rischio basso (Kuulokenurkka, dopo conferma per iscritto) o
   calcolati da misure proprie; correggere in `presets.rs` il link `kuulokenurkka.fi` → `kuulokenurkka.com` e citare
   "Headphone Corner".
4. **Cuffie generiche**: scaricare a runtime, su azione dell'utente, il singolo `ParametricEQ.txt` da AutoEq (commit
   fissato), con cache, `User-Agent` e fonte/licenza visibili. Mostrare l'avviso che alcune fonti (oratory1990)
   escludono l'uso commerciale.
5. **Equalizer APO**: non incorporarlo. Scaricare 1.4.2 da SourceForge con SHA-256 fissato, avvisare che l'installer
   è non firmato e che si apre comunque il Device Selector; non scrivere il registro MMDevices da soli.
6. **Nome**: SwitchEQ non ha conflitti esatti, ma è vicino a `EQ Switch` (`matejarh/EQ-Switch`) e poco distintivo.
   Prima di investire nel marchio (logo, dominio, store) fare una ricerca ufficiale EUIPO/TMview e USPTO, e valutare di
   scrivere all'autore di EQ Switch per evitare confusione. Evitare "Auto Equalizer". Riservare subito dominio,
   organizzazione GitHub e pacchetti.
7. Prima della pubblicazione, scrivere (via email/Reddit) a **Headphone Corner** e a **Rtings** (e opzionalmente a
   **oratory1990**) chiedendo conferma d'uso: una risposta scritta vale più di qualsiasi interpretazione.

## 7. Non verificato

- Termini d'uso di **Headphone.com / Headphones.com** (pagine non raggiungibili) e di **Crinacle** oltre alla pagina
  legacy dei grafici (nessun testo di termini trovato, ma non ho letto un eventuale documento separato).
- Il testo esatto dei datasheet PDF Innerfidelity (la dicitura "Source Interlink Media" proviene da un risultato di
  ricerca, non da un file letto).
- **Kuulokenurkka**: nessuna licenza esplicita (un permesso scritto non è stato richiesto).
- **Comportamento reale di `/S`** dell'installer Equalizer APO: dedotto dal sorgente, non eseguito.
- Licenze delle librerie contenute nell'installer (FFTW, libsndfile, Qt6, runtime MSVC) e di Tauri/crate di questo
  repo: non lette file per file.
- Pagina reddit (oratory1990): letta solo da copie Wayback del 2025 (non dal sito live, che blocca i client
  automatici); il testo "latest update: 22.10.2025" può essere cambiato.
- Esistenza di limiti di frequenza specifici su `raw.githubusercontent.com` per client non autenticati.
- **Marchi registrati (USPTO, EUIPO/TMview, WIPO) per "SwitchEQ" e per i marchi "SWITCH…"**: le banche dati ufficiali non
  sono interrogabili senza browser e gli aggregatori bloccano i client automatici. Esito indiretto (ricerca web):
  nessun "SWITCHEQ". Non è una ricerca di anteriorità.
- **SourceForge**: ricerca diretta bloccata (403); controllo solo tramite motore di ricerca.
- npm, Chocolatey, winget, Steam, F-Droid, altri app store per "SwitchEQ"; esistenza di `github.com/SwitchEQ` (lookup
  diretto fallito, solo la ricerca utenti dà 0).
- Licenza di `matejarh/EQ-Switch`: il README dichiara MIT, l'API GitHub non trova un file di licenza.
- La questione se i valori dei filtri (poche decine di numeri) siano protetti da diritto d'autore o diritto sui
  generis dei database: è un parere giuridico, non l'ho valutata.
