//! Preset pronti, costruiti sulla correzione del profilo cuffie scelto.
//!
//! BASE. La correzione misurata (AutoEq: filtri verso il target over-ear di
//! tipo Harman) arriva dal profilo scelto dall'utente (vedi `profiles.rs`):
//! l'app non incorpora nessuna misura. Il preamp NON è quello del file AutoEq:
//! lo ricalcola l'app (auto-preamp), con risultato equivalente.
//!
//! PRESET DI GUSTO. Partono dalla correzione misurata del profilo scelto (con
//! la variante selezionata) e aggiungono pochi filtri. Ogni numero ha un razionale
//! scritto accanto; i riferimenti usati sono:
//! - [OLIVE19] S. Olive, T. Welti, O. Khonsaripour, "Segmentation of Listeners
//!   Based on Their Preferred Headphone Sound Quality Profiles", AES paper 10156
//!   (2019): il 64% degli ascoltatori preferisce il target Harman, il 21% lo
//!   preferisce con 2-3 dB di bassi in meno, il 15% con 4-6 dB in più.
//!   https://www.aes.org/e-lib/browse.cfm?elib=20289
//! - [OLIVE15] S. Olive, "Factors that Influence Listeners' Preferred Bass and
//!   Treble Levels in Headphones" (2015): ascoltatori giovani e meno esperti
//!   preferiscono più bassi e più acuti.
//!   http://seanolive.blogspot.com/2015/11/factors-that-influence-listeners.html
//! - [HARMAN] Il target over-ear Harman 2018 contiene uno shelf di circa +6 dB a
//!   105 Hz, Q 0.7 (lo stesso usato da AutoEq per il parametro "bass boost"):
//!   i ritocchi dei bassi usano la stessa forma, quindi "+4 dB a 105 Hz"
//!   significa "4 dB di bassi in più del target Harman".
//! - [SII] ANSI S3.5-1997, Speech Intelligibility Index, pesi d'importanza per
//!   bande d'ottava: 250 Hz 0.0617, 500 Hz 0.1671, 1 kHz 0.2373, 2 kHz 0.2648,
//!   4 kHz 0.2142, 8 kHz 0.0549. Le ottave 1-4 kHz valgono il 72% dell'intelligibilità.
//! - [MASK] Diffusione del mascheramento verso l'alto ("upward spread of
//!   masking", B.C.J. Moore, An Introduction to the Psychology of Hearing):
//!   suoni gravi e forti mascherano le frequenze più alte, non il contrario.
//! - [LFE] Nei formati Dolby il canale LFE è limitato a 120 Hz (taglio netto) e
//!   riprodotto 10 dB più forte dei canali principali.
//!   https://en.wikipedia.org/wiki/Low-frequency_effects
//! - [ISO226] ISO 226:2003, curve isofoniche (Fletcher-Munson aggiornate). Le
//!   compensazioni di loudness sono calcolate dalla formula della norma e
//!   approssimate con due shelf: errore massimo 0,7 dB (verificato nei test).
//! - [BT] Bluetooth HFP con codec mSBC: audio vocale a 16 kHz, quindi banda
//!   utile fino a ~7-8 kHz (ITU-T G.722 "wideband" 50-7000 Hz).

use crate::eq::{EqProfile, Filter, FilterKind};
use crate::t;
use serde::Serialize;
use std::sync::Mutex;

const fn pk(freq: f64, gain: f64, q: f64) -> Filter {
    Filter::new(FilterKind::PK, freq, gain, q)
}
const fn lsc(freq: f64, gain: f64, q: f64) -> Filter {
    Filter::new(FilterKind::LSC, freq, gain, q)
}
const fn hsc(freq: f64, gain: f64, q: f64) -> Filter {
    Filter::new(FilterKind::HSC, freq, gain, q)
}

/// Curva di gusto da sommare alla correzione misurata. I testi sono chiavi del
/// catalogo (`preset.<id>.name`, `.desc`, `.why`) e si traducono al momento dell'uso.
struct Taste {
    id: &'static str,
    group: &'static str,
    filters: &'static [Filter],
}

const G_MOVIE: &str = "preset.group.movie";
const G_GAMING: &str = "preset.group.gaming";
const G_MUSIC: &str = "preset.group.music";
const G_VOICE: &str = "preset.group.voice";
const G_OTHER: &str = "preset.group.listening";

/// Razionale e fonti dei numeri (chiave `preset.<id>.why`) sono nel catalogo,
/// accanto a nome e descrizione.
const TASTES: &[Taste] = &[
    Taste { id: "film", group: G_MOVIE, filters: &[lsc(80.0, 5.0, 0.70), pk(250.0, -1.5, 1.0), pk(2000.0, 2.0, 0.70)] },
    Taste { id: "night", group: G_MOVIE, filters: &[lsc(105.0, -3.0, 0.70), pk(2000.0, 3.0, 0.70)] },
    Taste { id: "gaming_fps", group: G_GAMING, filters: &[lsc(105.0, -4.0, 0.70), pk(3500.0, 3.0, 0.90)] },
    Taste { id: "gaming", group: G_GAMING, filters: &[lsc(105.0, 4.0, 0.70), hsc(10000.0, 1.5, 0.70)] },
    Taste { id: "classical", group: G_MUSIC, filters: &[lsc(120.0, 5.5, 0.40), hsc(10000.0, 1.5, 1.0)] },
    Taste { id: "jazz", group: G_MUSIC, filters: &[lsc(105.0, -2.0, 0.70)] },
    Taste { id: "rock", group: G_MUSIC, filters: &[lsc(105.0, 4.0, 0.70), pk(3000.0, -1.5, 1.4)] },
    Taste { id: "edm", group: G_MUSIC, filters: &[lsc(60.0, 6.0, 0.70)] },
    Taste { id: "bass", group: G_MUSIC, filters: &[lsc(105.0, 6.0, 0.70)] },
    Taste { id: "loudness", group: G_OTHER, filters: &[lsc(120.0, 11.0, 0.40), hsc(10000.0, 3.5, 1.0)] },
    Taste { id: "voice", group: G_VOICE, filters: &[lsc(105.0, -4.0, 0.70), pk(2000.0, 2.5, 0.70), pk(6500.0, -1.5, 2.0)] },
    Taste { id: "calls", group: G_VOICE, filters: &[lsc(105.0, -6.0, 0.70), pk(2000.0, 3.0, 0.70), hsc(8000.0, -3.0, 0.70)] },
];

/// Correzione misurata su cui si costruiscono i preset.
#[derive(Debug, Clone, PartialEq)]
pub struct Base {
    /// Nome del modello.
    pub name: String,
    /// Etichetta della variante, solo se il modello ne ha più di una.
    pub variant: Option<String>,
    pub filters: Vec<Filter>,
    pub source: Option<String>,
}

impl Base {
    fn title(&self) -> String {
        match &self.variant {
            Some(v) => format!("{} ({v})", self.name),
            None => self.name.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BuiltinPreset {
    pub id: &'static str,
    /// Gruppo in cui mostrarlo nell'elenco (già tradotto).
    pub group: String,
    /// `true` se i filtri derivano interamente da misure reali.
    pub measured: bool,
    pub description: String,
    pub profile: EqProfile,
}

fn make(name: &str, source: Option<String>, parts: &[&[Filter]]) -> EqProfile {
    EqProfile {
        name: name.into(),
        filters: parts.iter().flat_map(|p| p.iter().copied()).collect(),
        source,
        ..Default::default()
    }
}

/// Id del preset che applica la sola correzione misurata.
pub const BASE_ID: &str = "base";
const FLAT_ID: &str = "flat";

/// Tutti i preset integrati, nella lingua corrente. Senza `base` (nessun profilo
/// scelto) restano i preset di gusto da soli e Piatto.
pub fn builtin(base: Option<&Base>) -> Vec<BuiltinPreset> {
    let mut out = Vec::new();
    if let Some(b) = base {
        out.push(BuiltinPreset {
            id: BASE_ID,
            group: t!("preset.group.measured"),
            measured: true,
            description: t!("preset.base.desc"),
            profile: make(&t!("preset.base.name", title = b.title()), b.source.clone(), &[&b.filters]),
        });
    }
    let measured: &[Filter] = base.map_or(&[], |b| &b.filters);
    out.extend(TASTES.iter().map(|taste| {
        let taste_name = crate::i18n::tr(&format!("preset.{}.name", taste.id), &[]);
        let rationale = crate::i18n::tr(&format!("preset.{}.why", taste.id), &[]);
        let (name, source) = match base {
            Some(b) => (
                t!("preset.taste.name", taste = taste_name, base = b.variant.as_deref().unwrap_or(&b.name)),
                t!(
                    "preset.taste.source",
                    measured = b.source.as_deref().unwrap_or(&b.name),
                    rationale = rationale
                ),
            ),
            None => (taste_name, t!("preset.taste.source_nobase", rationale = rationale)),
        };
        BuiltinPreset {
            id: taste.id,
            group: t!(taste.group),
            measured: false,
            description: crate::i18n::tr(&format!("preset.{}.desc", taste.id), &[]),
            profile: make(&name, Some(source), &[measured, taste.filters]),
        }
    }));
    out.push(BuiltinPreset {
        id: FLAT_ID,
        group: t!(G_OTHER),
        measured: false,
        description: t!("preset.flat.desc"),
        profile: EqProfile::default(),
    });
    out
}

/// Profilo di un preset integrato per id.
pub fn find(id: &str, base: Option<&Base>) -> Option<EqProfile> {
    builtin(base).into_iter().find(|p| p.id == id).map(|p| p.profile)
}

/// Chiave del preset predefinito: la correzione misurata se c'è un profilo
/// scelto, altrimenti Piatto.
pub fn default_key(has_base: bool) -> &'static str {
    if has_base {
        "b:base"
    } else {
        "b:flat"
    }
}

/// Profilo del preset predefinito.
pub fn default_profile(base: Option<&Base>) -> EqProfile {
    let id = &default_key(base.is_some())[2..];
    find(id, base).expect("il preset predefinito è tra quelli integrati")
}

/// Preset esclusi dal calcolo del preamp comune. "Ascolto a basso volume"
/// chiede da solo un preamp molto più basso degli altri: includerlo
/// abbasserebbe tutti i preset per uno che per definizione si usa a volume
/// basso. Escluso dal calcolo, usa comunque il proprio preamp (più negativo
/// del comune), quindi non va in clipping.
pub const UNIFORM_EXCLUDED: &[&str] = &["loudness"];

/// Preamp comune del "volume uniforme tra i preset": il più negativo tra i
/// preamp automatici dei preset integrati per la base scelta. Li considera
/// tutti perché una regola automatica può attivare qualunque preset integrato
/// (il menu del tray non ne elenca). Ricalcolato (circa 30 ms) solo quando
/// cambia la correzione di base, non a ogni anteprima.
pub fn common_preamp(base: Option<&Base>) -> f64 {
    static CACHE: Mutex<Option<(Vec<Filter>, f64)>> = Mutex::new(None);
    let key: Vec<Filter> = base.map(|b| b.filters.clone()).unwrap_or_default();
    if let Some((k, v)) = CACHE.lock().unwrap().as_ref() {
        if *k == key {
            return *v;
        }
    }
    let v = builtin(base)
        .iter()
        .filter(|p| !UNIFORM_EXCLUDED.contains(&p.id))
        .map(|p| p.profile.effective_preamp())
        .fold(0.0, f64::min);
    *CACHE.lock().unwrap() = Some((key, v));
    v
}

/// Profilo cuffie SINTETICO (numeri inventati, nessuna misura reale) per i
/// test di tutti i moduli.
#[cfg(test)]
pub mod fixtures {
    use super::*;

    pub fn base(variant: Option<&str>) -> Base {
        Base {
            name: "Acme Studio One".into(),
            variant: variant.map(Into::into),
            filters: vec![
                lsc(105.0, 1.0, 0.70),
                pk(270.0, -2.0, 0.40),
                pk(4400.0, 2.5, 3.50),
                pk(8000.0, 2.5, 1.00),
                hsc(10000.0, 0.2, 0.70),
            ],
            source: Some("profilo sintetico dei test".into()),
        }
    }

    /// Seconda misura dello stesso modello (per esempio "ANC spento").
    pub fn other_base(variant: Option<&str>) -> Base {
        Base { filters: vec![lsc(105.0, -1.5, 0.70), pk(3000.0, -2.0, 2.0), pk(6000.0, 4.0, 1.5)], ..base(variant) }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::{base, other_base};
    use super::*;
    use crate::eq::{auto_preamp, render_config, response_db};

    #[test]
    fn ids_are_unique_and_profiles_valid() {
        for b in [Some(base(None)), Some(base(Some("ANC on"))), None] {
            let all = builtin(b.as_ref());
            let mut ids: Vec<_> = all.iter().map(|p| p.id).collect();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), all.len());
            for p in &all {
                for f in &p.profile.filters {
                    assert_eq!(*f, f.sanitized(), "{}: filtro fuori range", p.id);
                }
                if p.id != "flat" {
                    assert!(p.profile.source.is_some(), "{}: fonte mancante", p.id);
                }
            }
            assert_eq!(all.iter().any(|p| p.id == BASE_ID), b.is_some());
        }
    }

    #[test]
    fn base_preset_is_the_chosen_correction() {
        let b = base(None);
        let p = find(BASE_ID, Some(&b)).unwrap();
        assert_eq!(p.name, "Correzione Acme Studio One");
        assert_eq!(p.filters, b.filters);
        assert_eq!(p.source, b.source);
        assert!(builtin(Some(&b)).iter().find(|p| p.id == BASE_ID).unwrap().measured);
        // Le varianti compaiono nei nomi.
        let b = base(Some("ANC on"));
        assert_eq!(find(BASE_ID, Some(&b)).unwrap().name, "Correzione Acme Studio One (ANC on)");
        assert!(find("film", Some(&b)).unwrap().name.ends_with("(ANC on)"));
        // Senza profilo: niente correzione, default Piatto.
        assert!(find(BASE_ID, None).is_none());
        assert_eq!(default_key(false), "b:flat");
        assert_eq!(default_profile(None), EqProfile::default());
        assert_eq!(default_key(true), "b:base");
        assert_eq!(default_profile(Some(&b)), find(BASE_ID, Some(&b)).unwrap());
    }

    #[test]
    fn every_preset_renders_with_device_scope() {
        for p in builtin(Some(&base(None))) {
            let cfg = render_config(&p.profile, "Acme Studio", false, None);
            assert!(cfg.contains("
Device: Acme Studio
"), "{}", p.id);
            assert_eq!(cfg.matches("Filter ").count(), p.profile.filters.len(), "{}", p.id);
        }
    }

    /// Ogni preset di gusto è la correzione del profilo scelto + la sua curva,
    /// ha un razionale e resta entro ritocchi moderati.
    #[test]
    fn taste_presets_sit_on_the_selected_correction() {
        for b in [base(None), other_base(Some("ANC off"))] {
            let n = b.filters.len();
            for t in TASTES {
                let p = find(t.id, Some(&b)).unwrap();
                assert_eq!(&p.filters[..n], &b.filters[..], "{}", t.id);
                assert_eq!(&p.filters[n..], t.filters, "{}", t.id);
                let why = crate::i18n::tr(&format!("preset.{}.why", t.id), &[]);
                assert!(why.len() > 40, "{}: razionale mancante", t.id);
                let max_boost = (0..200)
                    .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
                    .map(|f| response_db(t.filters, f))
                    .fold(f64::NEG_INFINITY, f64::max);
                assert!(max_boost <= 11.0, "{}: ritocco di {max_boost} dB", t.id);
                // Il preamp automatico tiene la curva complessiva sotto 0 dB.
                let pre = auto_preamp(&p.filters);
                assert!((-14.0..0.0).contains(&pre), "{}: preamp {pre}", t.id);
            }
        }
        // Senza profilo i preset di gusto sono la sola curva.
        let film = find("film", None).unwrap();
        assert_eq!(film.filters, TASTES.iter().find(|t| t.id == "film").unwrap().filters);
        assert!(find("inesistente", Some(&base(None))).is_none());
    }

    /// Le chiavi dei preset sono dinamiche (`preset.<id>.name/.desc/.why`) e lo
    /// scanner del catalogo non le vede: qui si controlla che esistano in ogni lingua.
    #[test]
    fn preset_texts_exist_in_every_language() {
        for lang in crate::i18n::LANGUAGES {
            let lang = crate::i18n::Lang::from_code(lang.code).unwrap();
            crate::i18n::with_lang(lang, || {
                let mut keys: Vec<String> = TASTES
                    .iter()
                    .flat_map(|t| ["name", "desc", "why"].map(|k| format!("preset.{}.{k}", t.id)))
                    .collect();
                keys.extend(TASTES.iter().map(|t| t.group.to_string()));
                keys.extend(["preset.base.name", "preset.base.desc", "preset.flat.name", "preset.flat.desc"].map(String::from));
                for key in keys {
                    let text = crate::i18n::tr(&key, &[]);
                    assert_ne!(text, key, "{}: chiave mancante in {}", key, lang.code());
                    assert!(!text.is_empty(), "{key}");
                }
            });
        }
    }

    /// Il preamp comune è il più negativo tra i preset integrati, esclusi
    /// quelli di `UNIFORM_EXCLUDED`, e segue la correzione scelta.
    #[test]
    fn common_preamp_rule() {
        let (a, b) = (base(None), other_base(None));
        for base in [Some(&a), Some(&b), None] {
            let all = builtin(base);
            let lowest = |excl: bool| {
                all.iter()
                    .filter(|p| !(excl && UNIFORM_EXCLUDED.contains(&p.id)))
                    .map(|p| auto_preamp(&p.profile.effective_filters()))
                    .fold(0.0, f64::min)
            };
            assert_eq!(common_preamp(base), lowest(true));
            // L'esclusione serve davvero: senza, il comune sarebbe più basso.
            assert!(lowest(false) < common_preamp(base) - 2.0);
        }
        // La cache non mescola le correzioni.
        assert_ne!(common_preamp(Some(&a)), common_preamp(Some(&b)));
        assert_eq!(common_preamp(Some(&a)), common_preamp(Some(&a)));
    }

    /// Con il preamp comune nessun preset integrato supera 0 dB; tutti usano il
    /// comune tranne gli esclusi, che tengono il proprio (più negativo).
    #[test]
    fn common_preamp_never_clips() {
        for b in [Some(base(None)), Some(other_base(None)), None] {
            let common = common_preamp(b.as_ref());
            for p in builtin(b.as_ref()) {
                let filters = p.profile.effective_filters();
                let pre = p.profile.preamp_with(Some(common));
                let mut freqs = crate::eq::log_freqs(4000);
                freqs.extend(filters.iter().map(|f| f.freq));
                let peak = freqs.iter().map(|&f| response_db(&filters, f)).fold(f64::NEG_INFINITY, f64::max);
                assert!(peak.max(0.0) + pre <= 1e-9, "{}: {:.2} dB", p.id, peak + pre);
                if UNIFORM_EXCLUDED.contains(&p.id) {
                    assert_eq!(pre, p.profile.effective_preamp(), "{}", p.id);
                    assert!(pre < common, "{}", p.id);
                } else {
                    assert_eq!(pre, common, "{}", p.id);
                }
            }
        }
    }

    /// ISO 226:2003, livello di pressione (dB SPL) di un tono puro a `phon`.
    fn iso226_spl(i: usize, phon: f64) -> f64 {
        const AF: [f64; 29] = [
            0.532, 0.506, 0.480, 0.455, 0.432, 0.409, 0.387, 0.367, 0.349, 0.330, 0.315, 0.301, 0.288,
            0.276, 0.267, 0.259, 0.253, 0.250, 0.246, 0.244, 0.243, 0.243, 0.243, 0.242, 0.242, 0.245,
            0.254, 0.271, 0.301,
        ];
        const LU: [f64; 29] = [
            -31.6, -27.2, -23.0, -19.1, -15.9, -13.0, -10.3, -8.1, -6.2, -4.5, -3.1, -2.0, -1.1, -0.4,
            0.0, 0.3, 0.5, 0.0, -2.7, -4.1, -1.0, 1.7, 2.5, 1.2, -2.1, -7.1, -11.2, -10.7, -3.1,
        ];
        const TF: [f64; 29] = [
            78.5, 68.7, 59.5, 51.1, 44.0, 37.5, 31.5, 26.5, 22.1, 17.9, 14.4, 11.4, 8.6, 6.2, 4.4, 3.0,
            2.2, 2.4, 3.5, 1.7, -1.3, -4.2, -6.0, -5.4, -1.5, 6.0, 12.6, 13.9, 12.3,
        ];
        let a = 4.47e-3 * (10f64.powf(0.025 * phon) - 1.15)
            + (0.4 * 10f64.powf((TF[i] + LU[i]) / 10.0 - 9.0)).powf(AF[i]);
        10.0 / AF[i] * a.log10() - LU[i] + 94.0
    }
    const ISO_FREQS: [f64; 29] = [
        20.0, 25.0, 31.5, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0,
        500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0,
        8000.0, 10000.0, 12500.0,
    ];

    /// I due preset di loudness approssimano la differenza tra le curve
    /// isofoniche ISO 226 (riferimento 80 phon) entro 0,7 dB.
    #[test]
    fn loudness_presets_follow_iso226() {
        assert!((iso226_spl(17, 40.0) - 40.0).abs() < 0.05); // 1 kHz: phon = dB SPL
        assert!((iso226_spl(7, 40.0) - 64.4).abs() < 0.1); // 100 Hz a 40 phon (tabella ISO)
        for (id, low) in [("loudness", 60.0), ("classical", 70.0)] {
            let t = TASTES.iter().find(|t| t.id == id).unwrap();
            let drop = 80.0 - low;
            for (i, &f) in ISO_FREQS.iter().enumerate() {
                let wanted = iso226_spl(i, low) - (iso226_spl(i, 80.0) - drop);
                let got = response_db(t.filters, f);
                assert!((got - wanted).abs() <= 0.7, "{id} a {f} Hz: {got:.2} invece di {wanted:.2}");
            }
        }
    }
}
