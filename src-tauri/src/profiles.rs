//! Profili cuffie: la correzione di un modello (con eventuali varianti di
//! misura, per esempio ANC attivo / spento) e le sorgenti da cui ottenerla.
//!
//! Sorgenti:
//! - indice del repository AutoEq (`results/INDEX.md`, scaricato a runtime e
//!   conservato in cache come elenco compatto) e relativo `ParametricEQ.txt`;
//! - file `ParametricEQ.txt` importato a mano (vedi `lib.rs`).
//!
//! Nessun dato misurato è incorporato nel binario. Le misure di Rtings sono
//! escluse da ricerca e download (vedi `is_excluded_source`).
//!
//! Il profilo scelto viene copiato in `settings.json`: dopo la scelta l'app
//! funziona anche senza rete.

use crate::eq::{parse_apo_text, Filter};
use crate::presets::Base;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Commit di AutoEq da cui si scarica: i percorsi in `results/` cambiano nel
/// tempo, un commit fissato li rende stabili.
pub const AUTOEQ_COMMIT: &str = "7ae0f56d53074872b028649617a22bbb4232feb7";
pub const INDEX_URL: &str =
    "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/7ae0f56d53074872b028649617a22bbb4232feb7/results/INDEX.md";
const RESULTS_URL: &str =
    "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/7ae0f56d53074872b028649617a22bbb4232feb7/results";
pub const AUTOEQ_REPO: &str = "https://github.com/jaakkopasanen/AutoEq";
const INDEX_CACHE: &str = "autoeq-index.tsv";
const INDEX_TTL: Duration = Duration::from_secs(7 * 24 * 3600);
/// Risultati massimi restituiti da una ricerca.
pub const SEARCH_LIMIT: usize = 40;
const MAX_FILTERS: usize = 64;

/// Fonti di misura escluse (minuscolo): i loro termini d'uso vietano riuso e
/// database derivati senza consenso scritto. Valgono per ricerca e download.
const EXCLUDED_SOURCES: [&str; 1] = ["rtings"];

/// `true` se la fonte (cartella AutoEq o autore dell'indice) è esclusa.
pub fn is_excluded_source(source: &str) -> bool {
    let source = source.to_lowercase();
    EXCLUDED_SOURCES.iter().any(|x| source.contains(x))
}

// ------------------------------------------------------------------ modello

/// Una misura/modalità di un modello (per esempio "ANC attivo").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Variant {
    pub id: String,
    pub label: String,
    pub filters: Vec<Filter>,
    /// Da dove vengono i filtri (mostrato come fonte dei preset).
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeadphoneProfile {
    pub name: String,
    pub variants: Vec<Variant>,
}

impl HeadphoneProfile {
    /// Variante per id; se non esiste (o l'id è vuoto) la prima.
    pub fn variant(&self, id: &str) -> Option<&Variant> {
        self.variants.iter().find(|v| v.id == id).or_else(|| self.variants.first())
    }

    /// Base su cui costruire i preset. L'etichetta della variante compare nei
    /// nomi solo se il modello ne ha più di una.
    pub fn base(&self, variant: &str) -> Option<Base> {
        let v = self.variant(variant)?;
        Some(Base {
            name: self.name.clone(),
            variant: (self.variants.len() > 1).then(|| v.label.clone()),
            filters: v.filters.clone(),
            source: v.source.clone(),
        })
    }

    /// Controlla un profilo ricevuto dal frontend o letto dalle impostazioni.
    pub fn sanitized(mut self) -> Result<Self, String> {
        self.name = self.name.trim().chars().take(80).collect();
        if self.name.is_empty() {
            return Err(crate::t!("err.profile.no_name"));
        }
        if self.variants.is_empty() || self.variants.len() > 16 {
            return Err(crate::t!("err.profile.variants"));
        }
        for (i, v) in self.variants.iter_mut().enumerate() {
            if v.filters.is_empty() || v.filters.len() > MAX_FILTERS {
                return Err(crate::t!("err.profile.variant_filters", label = v.label));
            }
            v.filters = v.filters.iter().map(|f| f.sanitized()).collect();
            v.label = v.label.trim().chars().take(60).collect();
            if v.id.trim().is_empty() {
                v.id = format!("v{}", i + 1);
            }
        }
        Ok(self)
    }
}

// --------------------------------------------------------- indice di AutoEq

/// Una voce dell'indice: `results/<source>/<form>/<folder>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexEntry {
    pub source: String,
    pub form: String,
    pub folder: String,
}

impl IndexEntry {
    pub fn path(&self) -> String {
        format!("{}/{}/{}", self.source, self.form, self.folder)
    }
}

fn valid_segment(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.chars().any(|c| c.is_control() || c == '/' || c == '\\')
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// URL del `ParametricEQ.txt` di un percorso `source/form/folder`.
pub fn parametric_url(path: &str) -> Result<String, String> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() != 3 || !parts.iter().all(|p| valid_segment(p)) {
        return Err(crate::t!("err.profile.bad_path", path = path));
    }
    if is_excluded_source(parts[0]) {
        return Err(crate::t!("err.profile.excluded_source", source = parts[0]));
    }
    let folder = percent_encode(parts[2]);
    Ok(format!(
        "{RESULTS_URL}/{}/{}/{folder}/{folder}%20ParametricEQ.txt",
        percent_encode(parts[0]),
        percent_encode(parts[1])
    ))
}

/// Legge `INDEX.md`. Le righe sono `- [Nome](./source/form/Nome%20codificato) by FONTE[ on RIG]`;
/// i percorsi contengono parentesi non codificate, quindi il percorso termina
/// all'ultima occorrenza di `) by `.
pub fn parse_index(md: &str) -> Vec<IndexEntry> {
    md.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("- [")?;
            let (_, rest) = rest.split_once("](")?;
            let end = rest.rfind(") by ")?;
            if is_excluded_source(&rest[end + 5..]) {
                return None;
            }
            let path = rest[..end].strip_prefix("./")?;
            let mut parts = path.splitn(3, '/').map(percent_decode);
            let (source, form, folder) = (parts.next()?, parts.next()?, parts.next()?);
            let entry = IndexEntry { source, form, folder };
            [&entry.source, &entry.form, &entry.folder]
                .iter()
                .all(|s| valid_segment(s) && !s.contains('\t'))
                .then_some(entry)
                .filter(|e| !is_excluded_source(&e.source))
        })
        .collect()
}

/// Prima riga della cache: la cache di un altro commit non si riusa.
fn cache_header() -> String {
    format!("# AutoEq {AUTOEQ_COMMIT}
")
}

fn to_cache(entries: &[IndexEntry]) -> String {
    let mut out = String::with_capacity(entries.len() * 64 + 64);
    out.push_str(&cache_header());
    for e in entries {
        out.push_str(&e.source);
        out.push('\t');
        out.push_str(&e.form);
        out.push('\t');
        out.push_str(&e.folder);
        out.push('\n');
    }
    out
}

/// Legge la cache; scarta comunque le fonti escluse, anche se una cache
/// scritta in precedenza le contenesse.
fn from_cache(text: &str) -> impl Iterator<Item = IndexEntry> + '_ {
    text.lines()
        .filter_map(|l| {
            let mut p = l.splitn(3, '\t');
            Some(IndexEntry { source: p.next()?.into(), form: p.next()?.into(), folder: p.next()?.into() })
        })
        .filter(|e| !is_excluded_source(&e.source))
}

/// Indice in cache (rinnovato dopo una settimana). Se il download fallisce si
/// usa la cache scaduta; se non c'è nemmeno quella, errore.
pub fn load_index(dir: &Path, force: bool, fetch: impl Fn(&str) -> Result<String, String>) -> Result<String, String> {
    let file = dir.join(INDEX_CACHE);
    let cached = fs::read_to_string(&file).ok().filter(|t| t.starts_with(&cache_header()));
    let fresh = fs::metadata(&file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < INDEX_TTL);
    if let (Some(text), true, false) = (&cached, fresh, force) {
        return Ok(text.clone());
    }
    let downloaded = fetch(INDEX_URL).and_then(|md| {
        let entries = parse_index(&md);
        if entries.is_empty() {
            Err(crate::t!("err.profile.index_empty"))
        } else {
            Ok(to_cache(&entries))
        }
    });
    match downloaded {
        Ok(text) => {
            let _ = fs::create_dir_all(dir);
            let tmp = dir.join("autoeq-index.tmp");
            if fs::write(&tmp, &text).is_ok() {
                let _ = fs::rename(&tmp, &file);
            }
            Ok(text)
        }
        Err(e) => cached.ok_or_else(|| {
            format!(
                "Impossibile scaricare l'indice di AutoEq ({e}). Controlla la connessione, \
                 oppure importa un file ParametricEQ.txt."
            )
        }),
    }
}

// ------------------------------------------------------------------ ricerca

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateVariant {
    pub label: String,
    pub path: String,
}

/// Risultato di una ricerca: un modello e le sue misure (stessa fonte e stesso rig).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub name: String,
    pub source: String,
    pub form: String,
    pub variants: Vec<CandidateVariant>,
}

/// "Modello (ANC on)" → ("Modello", "ANC on").
fn split_suffix(folder: &str) -> (&str, Option<&str>) {
    if let Some(open) = folder.rfind(" (").filter(|_| folder.ends_with(')')) {
        let label = &folder[open + 2..folder.len() - 1];
        if open > 0 && !label.is_empty() {
            return (&folder[..open], Some(label));
        }
    }
    (folder, None)
}

/// Cerca nell'indice in cache (tutte le parole, senza distinguere maiuscole).
/// Le voci della stessa fonte e dello stesso rig che differiscono solo dal
/// suffisso tra parentesi diventano varianti dello stesso modello.
pub fn search(cache: &str, query: &str, limit: usize) -> Vec<Candidate> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let key = |e: &IndexEntry| (e.source.clone(), e.form.clone(), split_suffix(&e.folder).0.to_string());
    let mut keys: Vec<(String, String, String)> = Vec::new();
    for e in from_cache(cache) {
        let name = e.folder.to_lowercase();
        if words.iter().all(|w| name.contains(w)) {
            let k = key(&e);
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    let mut out: Vec<Candidate> = keys
        .iter()
        .map(|(source, form, base)| Candidate {
            name: base.clone(),
            source: source.clone(),
            form: form.clone(),
            variants: Vec::new(),
        })
        .collect();
    for e in from_cache(cache) {
        let k = key(&e);
        if let Some(i) = keys.iter().position(|x| *x == k) {
            let label = split_suffix(&e.folder).1.map(str::to_string);
            out[i].variants.push(CandidateVariant { label: label.unwrap_or_default(), path: e.path() });
        }
    }
    for c in &mut out {
        if c.variants.len() == 1 {
            // Un'unica misura: il suffisso fa parte del nome.
            let only = &mut c.variants[0];
            c.name = only.path.rsplit('/').next().unwrap_or(&c.name).to_string();
            only.label.clear();
        } else {
            for v in c.variants.iter_mut().filter(|v| v.label.is_empty()) {
                v.label = crate::t!("variant.standard");
            }
        }
    }
    let first = words[0].as_str();
    out.sort_by_key(|c| (!c.name.to_lowercase().starts_with(first), c.name.len(), c.source.clone(), c.form.clone()));
    out.truncate(limit);
    out
}

// --------------------------------------------------------------- download

fn slug(label: &str, i: usize) -> String {
    let s: String = label
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if s.is_empty() {
        format!("v{}", i + 1)
    } else {
        s
    }
}

fn credit(path: &str) -> String {
    let encoded: Vec<String> = path.split('/').map(percent_encode).collect();
    let origin = path.split('/').next().unwrap_or_default();
    format!("AutoEq (MIT) / {origin} — «{path}» — {AUTOEQ_REPO}/tree/{AUTOEQ_COMMIT}/results/{}", encoded.join("/"))
}

/// Scarica e legge il `ParametricEQ.txt` di un percorso AutoEq.
pub fn fetch_filters(path: &str, fetch: &impl Fn(&str) -> Result<String, String>) -> Result<Vec<Filter>, String> {
    let text = fetch(&parametric_url(path)?)?;
    let p = parse_apo_text(path, &text)?;
    if p.filters.len() > MAX_FILTERS {
        return Err(crate::t!("err.profile.too_many_filters"));
    }
    Ok(p.filters)
}

/// Costruisce un profilo da un risultato di ricerca scaricando le sue misure.
pub fn build_from_candidate(
    c: &Candidate,
    fetch: &impl Fn(&str) -> Result<String, String>,
) -> Result<HeadphoneProfile, String> {
    let mut variants = Vec::new();
    for (i, v) in c.variants.iter().enumerate() {
        let label = if v.label.is_empty() { c.form.clone() } else { v.label.clone() };
        variants.push(Variant {
            id: slug(&label, i),
            label,
            filters: fetch_filters(&v.path, fetch)?,
            source: Some(credit(&v.path)),
        });
    }
    HeadphoneProfile { name: c.name.clone(), variants }.sanitized()
}

/// Profilo (una sola variante) da un file ParametricEQ.txt importato a mano.
pub fn from_apo_file(path: &str) -> Result<HeadphoneProfile, String> {
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > 1_000_000 {
        return Err(crate::t!("err.profile.file_too_large"));
    }
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let stem = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().replace(" ParametricEQ", ""))
        .unwrap_or_default();
    let name = if stem.trim().is_empty() { "Importato".to_string() } else { stem };
    let p = parse_apo_text(&name, &text)?;
    HeadphoneProfile {
        name: name.clone(),
        variants: vec![Variant {
            id: "file".into(),
            label: name,
            filters: p.filters,
            source: Some(crate::t!("src.imported_file", path = path)),
        }],
    }
    .sanitized()
}

// -------------------------------------------------------------------- test

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eq::FilterKind;

    /// Righe nel formato reale di INDEX.md, con nomi di fantasia.
    const INDEX: &str = "\
# Index of the results\n\
\n\
- [Acme Studio One](./Oratory1990/over-ear/Acme%20Studio%20One) by Oratory1990\n\
- [Acme Studio One (ANC on)](./Labo/over-ear/Acme%20Studio%20One%20(ANC%20on)) by Labo\n\
- [Acme Studio One (ANC off, neutral)](./Labo/over-ear/Acme%20Studio%20One%20(ANC%20off,%20neutral)) by Labo\n\
- [Acme Studio One (neutral)](./Labo/over-ear/Acme%20Studio%20One%20(neutral)) by Labo\n\
- [Acme Pods & Co](./Rigs/Bruel%20&%20Kjaer%205128%20over-ear/Acme%20Pods%20&%20Co) by Rigs on Bruel & Kjaer 5128\n\
- [Beta Buds](./Labo/in-ear/Beta%20Buds) by Labo\n\
- [Evil](./Labo/over-ear/..) by Labo\n\
- [Troppo corta](./Labo/over-ear) by Labo\n\
- not a list entry\n";

    #[test]
    fn index_lines_with_unencoded_parentheses_are_parsed() {
        let v = parse_index(INDEX);
        assert_eq!(v.len(), 6, "{v:?}");
        assert_eq!(v[1], IndexEntry { source: "Labo".into(), form: "over-ear".into(), folder: "Acme Studio One (ANC on)".into() });
        assert_eq!(v[2].folder, "Acme Studio One (ANC off, neutral)");
        assert_eq!(v[3].folder, "Acme Studio One (neutral)");
        assert_eq!(v[3].path(), "Labo/over-ear/Acme Studio One (neutral)");
        // Il rig fa parte della forma, il nome contiene "&".
        assert_eq!((v[4].form.as_str(), v[4].folder.as_str()), ("Bruel & Kjaer 5128 over-ear", "Acme Pods & Co"));
        assert_eq!((v[5].form.as_str(), v[5].folder.as_str()), ("in-ear", "Beta Buds"));
        assert_eq!(v[0].source, "Oratory1990");
        assert!(v.iter().all(|e| e.folder != ".."), "i percorsi con .. si scartano");
    }

    #[test]
    fn urls_encode_every_segment() {
        assert_eq!(
            parametric_url("Labo/over-ear/Acme Studio One (ANC on)").unwrap(),
            "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/7ae0f56d53074872b028649617a22bbb4232feb7/results/Labo/over-ear/\
             Acme%20Studio%20One%20%28ANC%20on%29/Acme%20Studio%20One%20%28ANC%20on%29%20ParametricEQ.txt"
        );
        let u = parametric_url("Rigs/Bruel & Kjaer 5128 over-ear/Acme Pods & Co").unwrap();
        assert!(u.contains("Bruel%20%26%20Kjaer%205128%20over-ear/Acme%20Pods%20%26%20Co/"), "{u}");
        for bad in ["a/b", "a/b/c/d", "../b/c", "a/./c", "a//c", "a/b/c\\d", "a/b/\n"] {
            assert!(parametric_url(bad).is_err(), "{bad}");
        }
        assert_eq!(percent_decode("Acme%20Pods%20%26%20Co%2"), "Acme Pods & Co%2");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn search_groups_variants_of_the_same_model() {
        let cache = to_cache(&parse_index(INDEX));
        let r = search(&cache, "acme studio", 10);
        assert_eq!(r.len(), 2, "{r:?}");
        let single = r.iter().find(|c| c.source == "Oratory1990").unwrap();
        assert_eq!(single.name, "Acme Studio One");
        assert_eq!(single.variants.len(), 1);
        assert_eq!(single.variants[0].label, "");
        let labo = r.iter().find(|c| c.source == "Labo").unwrap();
        assert_eq!(labo.name, "Acme Studio One");
        let labels: Vec<_> = labo.variants.iter().map(|v| v.label.as_str()).collect();
        assert_eq!(labels, ["ANC on", "ANC off, neutral", "neutral"]);

        // La ricerca trova anche i fratelli che non contengono la parola cercata.
        let r = search(&cache, "ANC on", 10);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].variants.len(), 3);

        // Maiuscole, più parole, rig con "&", limite, query vuota o senza risultati.
        assert_eq!(search(&cache, "POD co", 10)[0].name, "Acme Pods & Co");
        assert_eq!(search(&cache, "acme", 1).len(), 1);
        assert!(search(&cache, "   ", 10).is_empty());
        assert!(search(&cache, "inesistente", 10).is_empty());
    }

    #[test]
    fn lone_suffixed_entry_keeps_its_full_name() {
        let cache = to_cache(&[IndexEntry { source: "S".into(), form: "f".into(), folder: "Foo Bar (2020)".into() }]);
        let r = search(&cache, "foo", 5);
        assert_eq!(r[0].name, "Foo Bar (2020)");
        assert_eq!(r[0].variants[0].label, "");
    }

    #[test]
    fn index_cache_is_used_while_fresh_and_when_offline() {
        let dir = std::env::temp_dir().join(format!("switcheq-idx-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let calls = std::cell::Cell::new(0);
        let ok = |_: &str| {
            calls.set(calls.get() + 1);
            Ok(INDEX.to_string())
        };
        let offline = |_: &str| Err::<String, _>("rete assente".to_string());

        // Prima volta, senza cache e senza rete: errore comprensibile.
        let e = load_index(&dir, false, offline).unwrap_err();
        assert!(e.contains("rete assente") && e.contains("ParametricEQ.txt"), "{e}");

        let text = load_index(&dir, false, ok).unwrap();
        assert_eq!(text.lines().count(), 7, "intestazione + 6 voci");
        assert_eq!(calls.get(), 1);
        // Cache fresca: nessun download. Con `force` si riscarica.
        load_index(&dir, false, ok).unwrap();
        assert_eq!(calls.get(), 1);
        load_index(&dir, true, ok).unwrap();
        assert_eq!(calls.get(), 2);
        // Con `force` ma senza rete resta la cache.
        assert_eq!(load_index(&dir, true, offline).unwrap(), text);
        // Un indice vuoto non sostituisce quello buono.
        assert_eq!(load_index(&dir, true, |_: &str| Ok("niente".to_string())).unwrap(), text);
        // La cache di un altro commit non si riusa, nemmeno offline.
        fs::write(dir.join(INDEX_CACHE), "# AutoEq altro
S	f	X
").unwrap();
        assert!(load_index(&dir, false, offline).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn downloads_are_pinned_to_one_autoeq_commit() {
        assert_eq!(AUTOEQ_COMMIT.len(), 40);
        assert!(AUTOEQ_COMMIT.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(INDEX_URL.contains(&format!("/AutoEq/{AUTOEQ_COMMIT}/results/")));
        assert!(RESULTS_URL.contains(&format!("/AutoEq/{AUTOEQ_COMMIT}/results")));
        assert!(!INDEX_URL.contains("master") && !RESULTS_URL.contains("master"));
        let c = credit("Labo/over-ear/Acme Studio One (neutral)");
        assert!(c.contains(&format!("/tree/{AUTOEQ_COMMIT}/")));
        assert!(c.starts_with("AutoEq (MIT) / Labo"), "{c}");
    }

    /// Download finto con numeri di fantasia: nessun dato misurato.
    fn fake(url: &str) -> Result<String, String> {
        if url.contains("Missing") {
            return Err("HTTP 404".into());
        }
        Ok("Preamp: -2.0 dB\nFilter 1: ON LSC Fc 105 Hz Gain 1.5 dB Q 0.70\nFilter 2: ON PK Fc 3000 Hz Gain -2.0 dB Q 2.00\n".into())
    }

    #[test]
    fn candidate_becomes_a_profile_with_variants_and_credit() {
        let cache = to_cache(&parse_index(INDEX));
        let c = &search(&cache, "studio one anc", 5)[0];
        let p = build_from_candidate(c, &fake).unwrap();
        assert_eq!(p.name, "Acme Studio One");
        let ids: Vec<_> = p.variants.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["anc_on", "anc_off_neutral", "neutral"]);
        assert_eq!(p.variants[0].filters[0], Filter::new(FilterKind::LSC, 105.0, 1.5, 0.7));
        assert!(p.variants[0].source.as_deref().unwrap().contains("jaakkopasanen/AutoEq"));

        // Con più varianti l'etichetta compare nei nomi dei preset.
        let b = p.base("anc_off_neutral").unwrap();
        assert_eq!((b.name.as_str(), b.variant.as_deref()), ("Acme Studio One", Some("ANC off, neutral")));
        // Id sconosciuto: la prima variante.
        assert_eq!(p.base("boh").unwrap().variant.as_deref(), Some("ANC on"));

        // Un solo modello senza varianti: nessuna etichetta.
        let single = build_from_candidate(&search(&cache, "beta", 5)[0], &fake).unwrap();
        assert_eq!(single.base("").unwrap().variant, None);
    }

    #[test]
    fn download_errors_are_reported() {
        let c = Candidate {
            name: "X".into(),
            source: "S".into(),
            form: "f".into(),
            variants: vec![CandidateVariant { label: String::new(), path: "S/f/Missing".into() }],
        };
        assert_eq!(build_from_candidate(&c, &fake).unwrap_err(), "HTTP 404");
        let garbage = |_: &str| Ok::<_, String>("<html>404</html>".to_string());
        assert!(fetch_filters("S/f/X", &garbage).is_err());
    }

    #[test]
    fn manual_file_becomes_a_single_variant_profile() {
        let dir = std::env::temp_dir().join(format!("switcheq-file-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Acme Studio One ParametricEQ.txt");
        fs::write(&file, "Preamp: -3.1 dB\nFilter 1: ON PK Fc 100 Hz Gain 2.0 dB Q 1.0\n").unwrap();
        let p = from_apo_file(file.to_str().unwrap()).unwrap();
        assert_eq!(p.name, "Acme Studio One");
        assert_eq!(p.variants.len(), 1);
        assert_eq!(p.variants[0].filters.len(), 1);
        assert_eq!(p.base("").unwrap().variant, None, "una sola variante: nessuna etichetta nei nomi");
        fs::write(&file, "niente filtri").unwrap();
        assert!(from_apo_file(file.to_str().unwrap()).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rtings_measurements_are_excluded_from_search_and_download() {
        let md = "\
- [Acme Studio One](./Rtings/over-ear/Acme%20Studio%20One) by Rtings\n\
- [Acme Studio One](./rtings/over-ear/Acme%20Studio%20One) by rtings\n\
- [Acme Studio One](./Labo/over-ear/Acme%20Studio%20One) by Rtings\n\
- [Acme Studio One](./Rigs/Bruel%20&%20Kjaer%205128%20over-ear/Acme%20Studio%20One) by Rigs on Bruel & Kjaer 5128\n";
        let entries = parse_index(md);
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].source, "Rigs");

        // Una cache scritta prima del filtro non rende di nuovo visibili le voci.
        let old_cache = format!("{}Rtings\tover-ear\tAcme Studio One\nRigs\tover-ear\tAcme Studio Two\n", cache_header());
        let found = search(&old_cache, "acme", 10);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].source, "Rigs");

        // Il download rifiuta il percorso anche se arriva dal frontend, senza toccare la rete.
        for path in ["Rtings/over-ear/Acme Studio One", "rtings/in-ear/X", "RTINGS/over-ear/X"] {
            assert!(parametric_url(path).is_err(), "{path}");
        }
        let never = |_: &str| Err::<String, _>("non deve scaricare".to_string());
        let forged = Candidate {
            name: "Acme".into(),
            source: "Rtings".into(),
            form: "over-ear".into(),
            variants: vec![CandidateVariant { label: String::new(), path: "Rtings/over-ear/Acme Studio One".into() }],
        };
        let e = build_from_candidate(&forged, &never).unwrap_err();
        assert!(e.contains("Rtings"), "{e}");
        assert!(fetch_filters("Rtings/over-ear/Acme Studio One", &never).is_err());

        assert!(is_excluded_source("Rtings") && !is_excluded_source("Oratory1990") && !is_excluded_source("Rigs"));
    }

    #[test]
    fn profile_validation() {
        let good = Variant { id: String::new(), label: " A ".into(), filters: vec![Filter::new(FilterKind::PK, 1e9, 99.0, 1.0)], source: None };
        let p = HeadphoneProfile { name: " N ".into(), variants: vec![good.clone()] }.sanitized().unwrap();
        assert_eq!((p.name.as_str(), p.variants[0].id.as_str(), p.variants[0].label.as_str()), ("N", "v1", "A"));
        assert_eq!(p.variants[0].filters[0], p.variants[0].filters[0].sanitized());
        assert!(HeadphoneProfile { name: " ".into(), variants: vec![good.clone()] }.sanitized().is_err());
        assert!(HeadphoneProfile { name: "N".into(), variants: vec![] }.sanitized().is_err());
        let empty = Variant { filters: vec![], ..good };
        assert!(HeadphoneProfile { name: "N".into(), variants: vec![empty] }.sanitized().is_err());
    }
}
