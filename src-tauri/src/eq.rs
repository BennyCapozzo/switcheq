//! Modello dell'equalizzatore, calcolo della risposta in frequenza, preamp
//! automatico e generazione del file di configurazione di Equalizer APO.
//!
//! I filtri sono biquad secondo l'"Audio EQ Cookbook" di R. Bristow-Johnson,
//! lo stesso modello usato da Equalizer APO per `PK`, `LSC` e `HSC` con Q
//! esplicito: la curva disegnata e il preamp calcolato corrispondono quindi a
//! ciò che Equalizer APO applica davvero.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use std::fmt::Write as _;

/// Frequenza di campionamento usata per valutare la risposta. Le cuffie
/// Bluetooth su Windows lavorano a 44,1 o 48 kHz; tra 20 Hz e 20 kHz la
/// differenza sulla curva è trascurabile.
pub const SAMPLE_RATE: f64 = 48_000.0;

/// Centri delle 10 bande dell'EQ grafico (ottave ISO, come il FixedBandEQ di AutoEq).
pub const GRAPHIC_FREQS: [f64; 10] = [
    31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
/// Q delle bande grafiche: circa un'ottava di larghezza.
pub const GRAPHIC_Q: f64 = 1.41;

pub const MIN_FREQ: f64 = 10.0;
pub const MAX_FREQ: f64 = 22_000.0;
pub const MAX_GAIN: f64 = 24.0;
pub const MIN_Q: f64 = 0.1;
pub const MAX_Q: f64 = 20.0;
pub const MIN_PREAMP: f64 = -30.0;
pub const MAX_PREAMP: f64 = 10.0;

/// I nomi coincidono con la sintassi di Equalizer APO (e con il JSON dei preset).
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterKind {
    /// Peaking (campana).
    PK,
    /// Low shelf con frequenza centrale.
    LSC,
    /// High shelf con frequenza centrale.
    HSC,
}

impl FilterKind {
    fn apo_name(self) -> &'static str {
        match self {
            FilterKind::PK => "PK",
            FilterKind::LSC => "LSC",
            FilterKind::HSC => "HSC",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub kind: FilterKind,
    pub freq: f64,
    pub gain: f64,
    pub q: f64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Filter {
    pub const fn new(kind: FilterKind, freq: f64, gain: f64, q: f64) -> Self {
        Filter { kind, freq, gain, q, enabled: true }
    }

    /// Riporta i parametri entro limiti sensati (e sotto Nyquist).
    pub fn sanitized(self) -> Self {
        let fix = |v: f64, lo: f64, hi: f64, def: f64| if v.is_finite() { v.clamp(lo, hi) } else { def };
        Filter {
            kind: self.kind,
            freq: fix(self.freq, MIN_FREQ, MAX_FREQ, 1000.0),
            gain: fix(self.gain, -MAX_GAIN, MAX_GAIN, 0.0),
            q: fix(self.q, MIN_Q, MAX_Q, 0.707),
            enabled: self.enabled,
        }
    }
}

/// Profilo di equalizzazione: filtri parametrici + EQ grafico a 10 bande
/// sovrapposto (le due sezioni si sommano).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EqProfile {
    pub name: String,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub graphic: [f64; 10],
    #[serde(default = "default_true")]
    pub preamp_auto: bool,
    #[serde(default)]
    pub preamp_manual: f64,
    /// Fonte delle misure / descrizione (solo informativa).
    #[serde(default)]
    pub source: Option<String>,
}

impl Default for EqProfile {
    fn default() -> Self {
        EqProfile {
            name: crate::t!("preset.flat.name"),
            filters: Vec::new(),
            graphic: [0.0; 10],
            preamp_auto: true,
            preamp_manual: 0.0,
            source: None,
        }
    }
}

impl EqProfile {
    /// Filtri effettivamente attivi: parametrici abilitati + bande grafiche non nulle.
    pub fn effective_filters(&self) -> Vec<Filter> {
        let mut out: Vec<Filter> = self
            .filters
            .iter()
            .filter(|f| f.enabled && f.gain.abs() >= 0.05)
            .map(|f| f.sanitized())
            .collect();
        for (i, &g) in self.graphic.iter().enumerate() {
            if g.is_finite() && g.abs() >= 0.05 {
                out.push(Filter::new(FilterKind::PK, GRAPHIC_FREQS[i], g.clamp(-MAX_GAIN, MAX_GAIN), GRAPHIC_Q));
            }
        }
        out
    }

    /// Preamp da applicare: automatico (anti-clipping) o manuale.
    pub fn effective_preamp(&self) -> f64 {
        if self.preamp_auto {
            auto_preamp(&self.effective_filters())
        } else if self.preamp_manual.is_finite() {
            round1(self.preamp_manual.clamp(MIN_PREAMP, MAX_PREAMP))
        } else {
            0.0
        }
    }

    /// Preamp con il "volume uniforme tra i preset": `common` è il preamp
    /// comune (`None` se l'opzione è spenta). Un preamp automatico scende al
    /// valore comune ma non sale mai sopra il proprio, quindi la curva resta
    /// sotto 0 dB; il preamp manuale resta quello scelto dall'utente.
    pub fn preamp_with(&self, common: Option<f64>) -> f64 {
        let own = self.effective_preamp();
        match common {
            Some(c) if self.preamp_auto => own.min(c),
            _ => own,
        }
    }
}

/// Coefficienti biquad normalizzati (a0 = 1): [b0, b1, b2, a1, a2].
pub fn coefficients(f: &Filter, fs: f64) -> [f64; 5] {
    let a = 10f64.powf(f.gain / 40.0);
    let w0 = 2.0 * PI * f.freq / fs;
    let (sn, cs) = w0.sin_cos();
    let alpha = sn / (2.0 * f.q);
    let (b0, b1, b2, a0, a1, a2) = match f.kind {
        FilterKind::PK => (
            1.0 + alpha * a,
            -2.0 * cs,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cs,
            1.0 - alpha / a,
        ),
        FilterKind::LSC => {
            let s = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) - (a - 1.0) * cs + s),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cs),
                a * ((a + 1.0) - (a - 1.0) * cs - s),
                (a + 1.0) + (a - 1.0) * cs + s,
                -2.0 * ((a - 1.0) + (a + 1.0) * cs),
                (a + 1.0) + (a - 1.0) * cs - s,
            )
        }
        FilterKind::HSC => {
            let s = 2.0 * a.sqrt() * alpha;
            (
                a * ((a + 1.0) + (a - 1.0) * cs + s),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cs),
                a * ((a + 1.0) + (a - 1.0) * cs - s),
                (a + 1.0) - (a - 1.0) * cs + s,
                2.0 * ((a - 1.0) - (a + 1.0) * cs),
                (a + 1.0) - (a - 1.0) * cs - s,
            )
        }
    };
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

/// Guadagno (dB) di un singolo filtro alla frequenza `freq`.
pub fn filter_db(f: &Filter, freq: f64, fs: f64) -> f64 {
    let [b0, b1, b2, a1, a2] = coefficients(f, fs);
    let w = 2.0 * PI * freq / fs;
    let (s1, c1) = w.sin_cos();
    let (s2, c2) = (2.0 * w).sin_cos();
    let num = (b0 + b1 * c1 + b2 * c2).powi(2) + (b1 * s1 + b2 * s2).powi(2);
    let den = (1.0 + a1 * c1 + a2 * c2).powi(2) + (a1 * s1 + a2 * s2).powi(2);
    10.0 * (num / den).log10()
}

/// Risposta complessiva (dB, senza preamp) della catena di filtri.
pub fn response_db(filters: &[Filter], freq: f64) -> f64 {
    filters.iter().map(|f| filter_db(f, freq, SAMPLE_RATE)).sum()
}

/// `n` frequenze spaziate logaritmicamente tra 20 Hz e 20 kHz.
pub fn log_freqs(n: usize) -> Vec<f64> {
    let n = n.max(2);
    let (lo, hi) = (20f64.ln(), 20_000f64.ln());
    (0..n)
        .map(|i| (lo + (hi - lo) * i as f64 / (n - 1) as f64).exp())
        .collect()
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Preamp automatico: compensa il picco massimo della curva (20 Hz–20 kHz)
/// in modo che nessuna frequenza superi 0 dBFS. Arrotonda per eccesso al
/// decimo di dB (quindi verso valori più negativi): mai meno attenuazione
/// del necessario. Se la curva non ha guadagni positivi restituisce 0.
pub fn auto_preamp(filters: &[Filter]) -> f64 {
    if filters.is_empty() {
        return 0.0;
    }
    let mut freqs = log_freqs(1024);
    // I picchi stretti cadono sulle frequenze centrali: valutale esplicitamente.
    freqs.extend(filters.iter().map(|f| f.freq).filter(|f| (20.0..=20_000.0).contains(f)));
    let peak = freqs
        .iter()
        .map(|&f| response_db(filters, f))
        .fold(f64::NEG_INFINITY, f64::max);
    if peak <= 0.0 {
        0.0
    } else {
        // Il piccolo epsilon evita che errori di virgola mobile (es. 6.0000001)
        // spostino l'arrotondamento al decimo successivo.
        -((peak * 10.0 - 1e-6).ceil() / 10.0)
    }
}

/// Formatta un numero con al massimo `dec` decimali, sempre con il punto
/// (Equalizer APO non accetta la virgola) e senza zeri finali inutili.
fn num(v: f64, dec: usize) -> String {
    let s = format!("{v:.dec$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

/// Ripulisce il pattern della direttiva `Device:`: niente a capo o caratteri
/// di controllo (eviterebbero l'iniezione di comandi nel file), spazi
/// normalizzati. Se resta vuoto restituisce una stringa vuota: [`render_config`]
/// allora non scrive né `Device:` né filtri, così il file non si applica mai a
/// tutti i dispositivi.
pub fn sanitize_device_pattern(pattern: &str) -> String {
    let cleaned: String = pattern
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let parts: Vec<String> = cleaned
        .split(';')
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty() && !p.eq_ignore_ascii_case("all"))
        .collect();
    parts.join("; ")
}

fn comment_line(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

pub const CONFIG_HEADER: &str =
    "# Generated automatically: do not edit by hand, the app rewrites this file.";

/// Genera il contenuto del file incluso da `config.txt` di Equalizer APO.
/// `common` è il preamp comune del volume uniforme (`None` se spento): in
/// bypass resta applicato solo quello, così spegnere l'EQ non alza il volume.
/// Senza dispositivo (pattern vuoto) il file contiene solo commenti.
pub fn render_config(profile: &EqProfile, device_pattern: &str, bypass: bool, common: Option<f64>) -> String {
    let device = sanitize_device_pattern(device_pattern);
    let mut out = String::new();
    let _ = writeln!(out, "{CONFIG_HEADER}");
    if device.is_empty() {
        let _ = writeln!(out, "# No output device selected: no filter applied.");
        return out;
    }
    let _ = writeln!(out, "# Preset: {}", comment_line(&profile.name));
    if let Some(src) = profile.source.as_deref().filter(|s| !s.is_empty()) {
        let _ = writeln!(out, "# Source: {}", comment_line(src));
    }
    if bypass {
        let Some(c) = common else {
            let _ = writeln!(out, "# Bypass on: no filter applied.");
            return out;
        };
        let _ = writeln!(out, "# Bypass on: no filters, the common preamp stays (uniform volume).");
        let _ = writeln!(out, "Device: {device}");
        let _ = writeln!(out, "Preamp: {} dB", num(c.min(0.0), 1));
        return out;
    }
    let filters = profile.effective_filters();
    let _ = writeln!(out, "Device: {device}");
    let _ = writeln!(out, "Preamp: {} dB", num(profile.preamp_with(common), 1));
    for (i, f) in filters.iter().enumerate() {
        let _ = writeln!(
            out,
            "Filter {}: ON {} Fc {} Hz Gain {} dB Q {:.2}",
            i + 1,
            f.kind.apo_name(),
            num(f.freq, 1),
            num(f.gain, 1),
            f.q
        );
    }
    out
}

/// Restituisce `true` se `config.txt` contiene già (non commentata) la riga
/// `Include:` del nostro file.
pub fn has_include(config_txt: &str, file_name: &str) -> bool {
    config_txt.lines().any(|l| {
        let l = l.trim();
        let Some(rest) = l.strip_prefix("Include:").or_else(|| l.strip_prefix("include:")) else {
            return false;
        };
        rest.split(';').any(|p| p.trim().eq_ignore_ascii_case(file_name))
    })
}

/// Aggiunge la riga `Include:` in fondo a `config.txt` se manca. In fondo,
/// così la direttiva `Device:` del nostro file non può influenzare i comandi
/// già presenti dell'utente.
pub fn with_include(config_txt: &str, file_name: &str) -> Option<String> {
    if has_include(config_txt, file_name) {
        return None;
    }
    let mut out = config_txt.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str("\r\n");
    }
    let _ = write!(out, "# Headphone equalizer (managed by {})\r\n", crate::brand::PRODUCT_NAME);
    let _ = write!(out, "Include: {file_name}\r\n");
    Some(out)
}

/// Prefisso con cui l'app commenta le righe della configurazione dimostrativa
/// di Equalizer APO (riconoscibile a colpo d'occhio e da [`without_demo`]).
pub const DEMO_PREFIX: &str = "# [demo di Equalizer APO disattivata] ";

fn normalized(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase()
}

/// `GraphicEQ:` con almeno una banda e tutti i guadagni a 0.
fn is_flat_graphic_eq(line: &str) -> bool {
    let line = line.trim();
    let Some(rest) = line.get(..10).filter(|p| p.eq_ignore_ascii_case("GraphicEQ:")).map(|_| &line[10..]) else {
        return false;
    };
    let bands: Vec<_> = rest.split(';').map(str::trim).filter(|b| !b.is_empty()).collect();
    !bands.is_empty()
        && bands.iter().all(|b| {
            let parts: Vec<_> = b.split_whitespace().collect();
            parts.len() == 2 && parts[0].parse::<f64>().is_ok() && parts[1].parse::<f64>() == Ok(0.0)
        })
}

/// Commenta la configurazione dimostrativa che l'installer di Equalizer APO
/// mette in `config.txt` (`Preamp: -6 dB`, `Include: example.txt` con il suo
/// bass boost, `GraphicEQ` tutto a 0), che vale per tutti i dispositivi e si
/// somma al nostro file. Tocca solo le tre righe quando compaiono insieme e
/// consecutive, cioè la demo intatta: righe modificate dall'utente restano.
/// `None` se non c'è niente da cambiare (idempotente).
pub fn without_demo(config_txt: &str) -> Option<String> {
    let mut lines: Vec<String> = config_txt.split_inclusive('\n').map(str::to_string).collect();
    let mut changed = false;
    for i in 0..lines.len().saturating_sub(2) {
        if normalized(&lines[i]) == "preamp: -6 db"
            && normalized(&lines[i + 1]) == "include: example.txt"
            && is_flat_graphic_eq(&lines[i + 2])
        {
            for line in &mut lines[i..i + 3] {
                line.insert_str(0, DEMO_PREFIX);
            }
            changed = true;
        }
    }
    changed.then(|| lines.concat())
}

/// Importa un testo in formato Equalizer APO / AutoEq "ParametricEQ.txt"
/// (righe `Preamp:` e `Filter N: ON PK Fc ... Hz Gain ... dB Q ...`).
/// Il preamp letto viene ignorato a favore di quello automatico.
pub fn parse_apo_text(name: &str, text: &str) -> Result<EqProfile, String> {
    let mut filters = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        let Some((head, body)) = line.split_once(':') else { continue };
        if !head.trim().to_ascii_lowercase().starts_with("filter") {
            continue;
        }
        let tok: Vec<&str> = body.split_whitespace().collect();
        if tok.first().is_some_and(|t| t.eq_ignore_ascii_case("OFF")) {
            continue;
        }
        let kind = match tok.get(1).map(|t| t.to_ascii_uppercase()).as_deref() {
            Some("PK") | Some("PEQ") => FilterKind::PK,
            Some("LSC") => FilterKind::LSC,
            Some("HSC") => FilterKind::HSC,
            other => return Err(crate::t!("err.apo.line_type", line = n + 1, kind = other.unwrap_or("?"))),
        };
        let value = |key: &str| -> Result<f64, String> {
            let i = tok
                .iter()
                .position(|t| t.eq_ignore_ascii_case(key))
                .ok_or_else(|| crate::t!("err.apo.line_missing", line = n + 1, key = key))?;
            tok.get(i + 1)
                .and_then(|v| v.replace(',', ".").parse::<f64>().ok())
                .ok_or_else(|| crate::t!("err.apo.line_value", line = n + 1, key = key))
        };
        filters.push(Filter::new(kind, value("Fc")?, value("Gain")?, value("Q")?).sanitized());
    }
    if filters.is_empty() {
        return Err(crate::t!("err.apo.no_filters"));
    }
    Ok(EqProfile { name: name.into(), filters, source: Some(crate::t!("src.imported_apo")), ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_autoeq_parametric_file() {
        let txt = "Preamp: -6.8 dB\r\nFilter 1: ON LSC Fc 105 Hz Gain -1.5 dB Q 0.70\r\n\
                   Filter 2: ON PK Fc 4601 Hz Gain 5.3 dB Q 3.55\r\nFilter 3: OFF PK Fc 1 Hz Gain 1 dB Q 1\r\n";
        let p = parse_apo_text("X", txt).unwrap();
        assert_eq!(p.filters, vec![Filter::new(FilterKind::LSC, 105.0, -1.5, 0.7), pk(4601.0, 5.3, 3.55)]);
        assert!(parse_apo_text("X", "Preamp: -1 dB").is_err());
        assert!(parse_apo_text("X", "Filter 1: ON LP Fc 100 Hz").is_err());
    }

    fn pk(freq: f64, gain: f64, q: f64) -> Filter {
        Filter::new(FilterKind::PK, freq, gain, q)
    }

    fn profile(filters: Vec<Filter>) -> EqProfile {
        EqProfile { name: "Test".into(), filters, ..Default::default() }
    }

    #[test]
    fn peaking_gain_at_center_and_far_away() {
        let f = pk(1000.0, 6.0, 1.0);
        assert!((filter_db(&f, 1000.0, SAMPLE_RATE) - 6.0).abs() < 1e-6);
        assert!(filter_db(&f, 20.0, SAMPLE_RATE).abs() < 0.05);
        assert!(filter_db(&f, 20_000.0, SAMPLE_RATE).abs() < 0.3);
    }

    #[test]
    fn shelves_reach_full_gain_and_half_at_center() {
        let ls = Filter::new(FilterKind::LSC, 105.0, 6.0, 0.7);
        assert!((filter_db(&ls, 20.0, SAMPLE_RATE) - 6.0).abs() < 0.3);
        assert!((filter_db(&ls, 105.0, SAMPLE_RATE) - 3.0).abs() < 0.01);
        assert!(filter_db(&ls, 5000.0, SAMPLE_RATE).abs() < 0.05);

        let hs = Filter::new(FilterKind::HSC, 10_000.0, -4.0, 0.7);
        assert!((filter_db(&hs, 10_000.0, SAMPLE_RATE) + 2.0).abs() < 0.01);
        assert!(filter_db(&hs, 100.0, SAMPLE_RATE).abs() < 0.05);
    }

    #[test]
    fn auto_preamp_single_boost() {
        assert_eq!(auto_preamp(&[pk(1000.0, 6.0, 1.0)]), -6.0);
        assert_eq!(auto_preamp(&[pk(4601.0, 5.3, 3.55)]), -5.3);
    }

    #[test]
    fn auto_preamp_only_cuts_is_zero() {
        assert_eq!(auto_preamp(&[pk(1000.0, -6.0, 1.0), pk(100.0, -3.0, 0.7)]), 0.0);
        assert_eq!(auto_preamp(&[]), 0.0);
    }

    #[test]
    fn auto_preamp_overlapping_boosts_add_up() {
        let p = auto_preamp(&[pk(1000.0, 3.0, 1.0), pk(1000.0, 3.0, 1.0)]);
        assert_eq!(p, -6.0);
        // Due campane vicine si sommano parzialmente: serve più di 3 dB.
        let p = auto_preamp(&[pk(1000.0, 3.0, 1.0), pk(1400.0, 3.0, 1.0)]);
        assert!(p < -3.0 && p > -6.0, "preamp {p}");
    }

    #[test]
    fn auto_preamp_rounds_towards_more_attenuation() {
        assert_eq!(auto_preamp(&[pk(1000.0, 2.34, 1.0)]), -2.4);
    }

    #[test]
    fn auto_preamp_never_lets_curve_exceed_zero() {
        let filters = vec![
            Filter::new(FilterKind::LSC, 105.0, 6.0, 0.7),
            pk(60.0, 2.0, 1.5),
            Filter::new(FilterKind::HSC, 10_000.0, 3.0, 0.7),
        ];
        let pre = auto_preamp(&filters);
        for f in log_freqs(4000) {
            assert!(response_db(&filters, f) + pre <= 1e-9, "clip a {f} Hz");
        }
    }

    #[test]
    fn graphic_bands_are_included_in_preamp() {
        let mut p = profile(vec![]);
        p.graphic[5] = 4.0; // 1 kHz
        assert_eq!(p.effective_filters().len(), 1);
        assert_eq!(p.effective_preamp(), -4.0);
    }

    #[test]
    fn manual_preamp_is_used_and_clamped() {
        let mut p = profile(vec![pk(1000.0, 6.0, 1.0)]);
        p.preamp_auto = false;
        p.preamp_manual = -2.25;
        assert_eq!(p.effective_preamp(), -2.3);
        p.preamp_manual = -100.0;
        assert_eq!(p.effective_preamp(), MIN_PREAMP);
    }

    #[test]
    fn render_config_exact_output() {
        let mut p = profile(vec![
            Filter::new(FilterKind::LSC, 105.0, -1.5, 0.7),
            pk(4601.0, 5.3, 3.55),
            Filter { enabled: false, ..pk(683.0, 2.8, 1.94) },
            Filter::new(FilterKind::HSC, 10_000.0, 2.0, 0.7),
        ]);
        p.source = Some("Unit test".into());
        let cfg = render_config(&p, "Acme", false, None);
        let expected = format!(
            "{CONFIG_HEADER}\n# Preset: Test\n# Source: Unit test\n\
             Device: Acme\n\
             Preamp: -5.4 dB\n\
             Filter 1: ON LSC Fc 105 Hz Gain -1.5 dB Q 0.70\n\
             Filter 2: ON PK Fc 4601 Hz Gain 5.3 dB Q 3.55\n\
             Filter 3: ON HSC Fc 10000 Hz Gain 2 dB Q 0.70\n"
        );
        assert_eq!(cfg, expected);
    }

    #[test]
    fn render_config_uses_dot_decimals_and_graphic_bands() {
        let mut p = profile(vec![pk(52.5, 0.4, 3.67)]);
        p.graphic[0] = -1.2;
        let cfg = render_config(&p, "Acme", false, None);
        assert!(cfg.contains("Filter 1: ON PK Fc 52.5 Hz Gain 0.4 dB Q 3.67"));
        assert!(cfg.contains("Filter 2: ON PK Fc 31 Hz Gain -1.2 dB Q 1.41"));
        // Il taglio a 31 Hz riduce in parte il picco a 52,5 Hz: serve meno di 0,4 dB.
        let pre = p.effective_preamp();
        assert!(pre > -0.4 && pre < 0.0, "preamp {pre}");
        assert!(cfg.contains(&format!("Preamp: {pre} dB")));
        assert!(cfg.lines().filter(|l| !l.starts_with('#')).all(|l| !l.contains(',')));
    }

    #[test]
    fn render_config_bypass_has_no_commands() {
        let p = profile(vec![pk(1000.0, 6.0, 1.0)]);
        let cfg = render_config(&p, "Acme", true, None);
        assert!(cfg.lines().all(|l| l.starts_with('#')), "{cfg}");
    }

    #[test]
    fn common_preamp_lowers_auto_presets_only() {
        // Preset che chiede meno attenuazione del comune: scende al comune.
        let p = profile(vec![pk(1000.0, 3.0, 1.0)]);
        assert_eq!(p.preamp_with(Some(-6.9)), -6.9);
        assert_eq!(p.preamp_with(None), -3.0);
        // Preset che chiede più attenuazione: tiene la propria (niente clipping).
        let p = profile(vec![pk(1000.0, 11.0, 1.0)]);
        assert_eq!(p.preamp_with(Some(-6.9)), -11.0);
        // Piatto: prende il comune, così passare a Piatto non alza il volume.
        assert_eq!(EqProfile::default().preamp_with(Some(-6.9)), -6.9);
        // Preamp manuale: invariato.
        let mut p = profile(vec![pk(1000.0, 6.0, 1.0)]);
        p.preamp_auto = false;
        p.preamp_manual = -1.5;
        assert_eq!(p.preamp_with(Some(-6.9)), -1.5);
    }

    #[test]
    fn render_config_uses_common_preamp_also_for_flat_and_bypass() {
        let cfg = render_config(&EqProfile::default(), "Acme", false, Some(-6.9));
        assert!(cfg.contains("Device: Acme\nPreamp: -6.9 dB\n"), "{cfg}");
        let p = profile(vec![pk(1000.0, 6.0, 1.0)]);
        let cfg = render_config(&p, "Acme", true, Some(-6.9));
        assert!(cfg.ends_with("Device: Acme\nPreamp: -6.9 dB\n"), "{cfg}");
        assert!(!cfg.contains("Filter"));
    }

    #[test]
    fn render_config_flat_profile_has_zero_preamp() {
        let cfg = render_config(&EqProfile::default(), "Acme", false, None);
        assert!(cfg.contains("Device: Acme\nPreamp: 0 dB\n"));
        assert!(!cfg.contains("Filter"));
    }

    #[test]
    fn device_pattern_cannot_inject_commands_or_match_all() {
        assert_eq!(sanitize_device_pattern("Acme\nPreamp: 20 dB"), "Acme Preamp: 20 dB");
        assert_eq!(sanitize_device_pattern("  Acme   One ; Cuffie "), "Acme One; Cuffie");
        assert_eq!(sanitize_device_pattern(""), "");
        assert_eq!(sanitize_device_pattern("all"), "");
        let mut p = profile(vec![]);
        p.name = "Nome\nPreamp: 10 dB".into();
        let cfg = render_config(&p, "Acme", false, None);
        assert!(!cfg.lines().any(|l| l.starts_with("Preamp: 10")));
    }

    #[test]
    fn render_config_without_device_never_applies_to_anything() {
        let p = profile(vec![pk(1000.0, 6.0, 1.0)]);
        for pattern in ["", "  ", "all"] {
            for (bypass, common) in [(false, None), (false, Some(-6.0)), (true, Some(-6.0))] {
                let cfg = render_config(&p, pattern, bypass, common);
                assert!(cfg.lines().all(|l| l.starts_with('#')), "{cfg}");
            }
        }
    }

    #[test]
    fn sanitizes_out_of_range_filters() {
        let f = Filter::new(FilterKind::PK, 40_000.0, 99.0, 0.0).sanitized();
        assert_eq!((f.freq, f.gain, f.q), (MAX_FREQ, MAX_GAIN, MIN_Q));
        let f = Filter::new(FilterKind::PK, f64::NAN, 1.0, 1.0).sanitized();
        assert_eq!(f.freq, 1000.0);
    }

    #[test]
    fn include_is_appended_once() {
        let base = "Device: all\r\nPreamp: -2 dB\r\n";
        let updated = with_include(base, "switcheq.txt").unwrap();
        assert!(updated.starts_with(base));
        assert!(updated.ends_with("Include: switcheq.txt\r\n"));
        assert!(has_include(&updated, "switcheq.txt"));
        assert!(with_include(&updated, "switcheq.txt").is_none());
        // Senza a capo finale e con include commentato.
        let updated = with_include("Preamp: -2 dB\n# Include: switcheq.txt", "switcheq.txt").unwrap();
        assert!(updated.contains("switcheq.txt\r\n# Headphone"));
        assert!(has_include("Include: other.txt; SwitchEQ.TXT", "switcheq.txt"));
    }

    /// `config.txt` come lo lascia l'installer di Equalizer APO 1.4.2, con il
    /// nostro Include già aggiunto.
    const APO_DEMO: &str = "Preamp: -6 dB\r\nInclude: example.txt\r\n\
        GraphicEQ: 25 0; 40 0; 63 0; 100 0; 160 0; 250 0; 400 0; 630 0; 1000 0; 1600 0; 2500 0; 4000 0; 6300 0; 10000 0; 16000 0\r\n\
        # Headphone equalizer (managed by SwitchEQ)\r\nInclude: switcheq.txt\r\n";

    #[test]
    fn demo_is_commented_once_keeping_our_include() {
        let out = without_demo(APO_DEMO).unwrap();
        let lines: Vec<_> = out.split("\r\n").collect();
        assert_eq!(lines[0], format!("{DEMO_PREFIX}Preamp: -6 dB"));
        assert_eq!(lines[1], format!("{DEMO_PREFIX}Include: example.txt"));
        assert!(lines[2].starts_with(&format!("{DEMO_PREFIX}GraphicEQ: 25 0;")));
        assert_eq!(&lines[3..], ["# Headphone equalizer (managed by SwitchEQ)", "Include: switcheq.txt", ""]);
        assert!(has_include(&out, "switcheq.txt"));
        assert!(!has_include(&out, "example.txt"));
        // Idempotente: la seconda volta non cambia nulla.
        assert!(without_demo(&out).is_none());
    }

    #[test]
    fn customized_lines_are_left_alone() {
        for custom in [
            // Preamp cambiato dall'utente.
            "Preamp: -3 dB\nInclude: example.txt\nGraphicEQ: 25 0; 40 0\n",
            // GraphicEQ con valori diversi da 0.
            "Preamp: -6 dB\nInclude: example.txt\nGraphicEQ: 25 0; 40 2.5\n",
            // Righe della demo non consecutive o in un altro ordine.
            "Preamp: -6 dB\nDevice: Speakers\nInclude: example.txt\nGraphicEQ: 25 0\n",
            "Include: example.txt\nPreamp: -6 dB\nGraphicEQ: 25 0\n",
            // Include di un altro file.
            "Preamp: -6 dB\nInclude: mio.txt\nGraphicEQ: 25 0\n",
            "",
        ] {
            assert!(without_demo(custom).is_none(), "{custom:?}");
        }
        // Spazi e maiuscole diversi, senza a capo finale: è ancora la demo.
        let out = without_demo("preamp:  -6 dB\ninclude: Example.txt\ngraphiceq: 25 0;40 0;").unwrap();
        assert!(out.lines().all(|l| l.starts_with(DEMO_PREFIX)));
        assert!(!out.ends_with('\n'));
    }
}
