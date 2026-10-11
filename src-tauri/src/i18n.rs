//! Traduzioni dell'interfaccia (menu del tray, notifiche, errori, preset).
//!
//! Le stringhe stanno in `src/i18n/<codice>.json`, condivise con il frontend:
//! un oggetto piatto `chiave → testo`, con segnaposto `{nome}`. Nessuna libreria:
//! i file sono incorporati nel binario con `include_str!` e letti una sola volta.
//!
//! Per aggiungere una lingua: un file JSON con le stesse chiavi, una riga in
//! [`LANGUAGES`] e una in `src/i18n.ts` (vedi README). I test verificano che
//! nessuna chiave manchi e che nessun testo sia vuoto.

use std::collections::HashMap;
#[cfg(not(test))]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

pub struct Language {
    /// Codice mostrato nelle impostazioni e usato come nome del file.
    pub code: &'static str,
    /// Identificatore primario della lingua di Windows (`LANGID & 0x3ff`).
    pub windows_primary: u16,
    json: &'static str,
}

pub const LANGUAGES: &[Language] = &[
    Language { code: "it", windows_primary: 0x10, json: include_str!("../../src/i18n/it.json") },
    Language { code: "en", windows_primary: 0x09, json: include_str!("../../src/i18n/en.json") },
];

/// Lingua usata quando quella di Windows non è tra [`LANGUAGES`] e per le chiavi mancanti.
pub const DEFAULT_CODE: &str = "en";
/// Valore dell'impostazione "segui la lingua di Windows".
pub const AUTO: &str = "auto";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lang(usize);

impl Lang {
    pub fn from_code(code: &str) -> Option<Lang> {
        LANGUAGES.iter().position(|l| l.code == code).map(Lang)
    }

    pub fn code(self) -> &'static str {
        LANGUAGES[self.0].code
    }

    fn fallback() -> Lang {
        Lang::from_code(DEFAULT_CODE).expect("la lingua di ripiego è tra quelle disponibili")
    }
}

/// Lingua dell'interfaccia di Windows, se è tra quelle disponibili.
pub fn system() -> Lang {
    // Nei test la lingua di sistema è fissa (italiano), così non dipendono dal PC.
    if cfg!(test) {
        return Lang::from_code("it").unwrap();
    }
    #[cfg(windows)]
    {
        // SAFETY: funzione senza argomenti che legge solo la lingua dell'utente.
        let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        let primary = id & 0x3ff;
        if let Some(i) = LANGUAGES.iter().position(|l| l.windows_primary == primary) {
            return Lang(i);
        }
    }
    Lang::fallback()
}

/// Traduce l'impostazione salvata: `"auto"` (o un codice sconosciuto) segue Windows.
pub fn resolve(setting: &str) -> Lang {
    Lang::from_code(setting).unwrap_or_else(system)
}

/// Codice valido da salvare nelle impostazioni: `"auto"` oppure una lingua nota.
pub fn normalize_setting(setting: &str) -> String {
    if Lang::from_code(setting).is_some() { setting.to_string() } else { AUTO.to_string() }
}

#[cfg(not(test))]
static CURRENT: AtomicUsize = AtomicUsize::new(usize::MAX);

#[cfg(not(test))]
pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        usize::MAX => Lang::fallback(),
        i => Lang(i),
    }
}

#[cfg(not(test))]
pub fn set_current(lang: Lang) {
    CURRENT.store(lang.0, Ordering::Relaxed);
}

// Nei test ogni thread ha la propria lingua (italiano di base): i test paralleli non si disturbano.
#[cfg(test)]
thread_local! {
    static OVERRIDE: std::cell::Cell<Option<Lang>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub fn current() -> Lang {
    OVERRIDE.with(|o| o.get()).unwrap_or_else(|| Lang::from_code("it").unwrap())
}

#[cfg(test)]
pub fn set_current(lang: Lang) {
    OVERRIDE.with(|o| o.set(Some(lang)));
}

/// Esegue `f` con la lingua indicata, poi ripristina quella di prima.
#[cfg(test)]
pub fn with_lang<R>(lang: Lang, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Lang>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|o| o.set(self.0));
        }
    }
    let _restore = Restore(OVERRIDE.with(|o| o.replace(Some(lang))));
    f()
}

type Table = HashMap<String, String>;

fn tables() -> &'static Vec<Table> {
    static TABLES: OnceLock<Vec<Table>> = OnceLock::new();
    TABLES.get_or_init(|| {
        LANGUAGES
            .iter()
            .map(|l| serde_json::from_str(l.json).unwrap_or_else(|e| panic!("{}.json non valido: {e}", l.code)))
            .collect()
    })
}

fn lookup(lang: Lang, key: &str) -> Option<&'static str> {
    tables()[lang.0].get(key).map(String::as_str)
}

/// Testo di `key` nella lingua corrente, con i segnaposto `{nome}` sostituiti.
/// Una chiave mancante ripiega sull'inglese e, in ultimo, sulla chiave stessa.
pub fn tr(key: &str, args: &[(&str, String)]) -> String {
    let mut text = lookup(current(), key).or_else(|| lookup(Lang::fallback(), key)).unwrap_or(key).to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// `t!("chiave")` oppure `t!("chiave", nome = valore, ...)`.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::tr($key, &[])
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr($key, &[$((stringify!($name), ($value).to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
    }

    fn placeholders(text: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let Some(len) = rest[start..].find('}') else { break };
            out.insert(rest[start + 1..start + len].to_string());
            rest = &rest[start + len + 1..];
        }
        out
    }

    #[test]
    fn every_language_has_the_same_keys_and_no_empty_text() {
        let reference = &tables()[Lang::fallback().0];
        for (l, table) in LANGUAGES.iter().zip(tables()) {
            let missing: Vec<_> = reference.keys().filter(|k| !table.contains_key(*k)).collect();
            let extra: Vec<_> = table.keys().filter(|k| !reference.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{}: chiavi mancanti {missing:?}", l.code);
            assert!(extra.is_empty(), "{}: chiavi in più {extra:?}", l.code);
            for (key, text) in table {
                assert!(!text.trim().is_empty(), "{}: testo vuoto per «{key}»", l.code);
                assert_eq!(placeholders(text), placeholders(&reference[key]), "{}: segnaposto diversi in «{key}»", l.code);
            }
            assert!(table.contains_key("lang.name"), "{}: manca lang.name", l.code);
        }
    }

    /// Una chiave per riga e nessun duplicato (un oggetto JSON ripetuto ne conserverebbe una sola).
    #[test]
    fn json_files_have_one_unique_key_per_line() {
        for (l, table) in LANGUAGES.iter().zip(tables()) {
            let entries = l.json.lines().filter(|line| line.trim_start().starts_with('"')).count();
            assert_eq!(entries, table.len(), "{}: chiavi duplicate o su più righe", l.code);
        }
    }

    /// Cerca `marker"chiave"` nei sorgenti: la chiave deve esistere in ogni lingua.
    fn used_keys(text: &str, marker: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (at, _) in text.match_indices(marker) {
            let before = text[..at].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$') {
                continue;
            }
            let rest = &text[at + marker.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
            }
        }
        out
    }

    #[test]
    fn keys_used_by_the_code_exist() {
        let mut keys = BTreeSet::new();
        let src = root().join("src-tauri").join("src");
        for entry in fs::read_dir(&src).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rs") && path.file_name().is_some_and(|n| n != "i18n.rs") {
                let text = fs::read_to_string(&path).unwrap();
                keys.extend(used_keys(&text, "t!(\""));
                keys.extend(used_keys(&text, "tr(\""));
                keys.extend(used_keys(&text, "label_key: \""));
            }
        }
        for name in ["main.ts", "api.ts"] {
            let text = fs::read_to_string(root().join("src").join(name)).unwrap();
            keys.extend(used_keys(&text, "t(\""));
            keys.extend(used_keys(&text, "labelKey: \""));
        }
        let html = fs::read_to_string(root().join("index.html")).unwrap();
        for attr in ["data-i18n=\"", "data-i18n-title=\"", "data-i18n-placeholder=\"", "data-i18n-aria=\""] {
            keys.extend(used_keys(&html, attr));
        }
        assert!(keys.len() > 50, "scansione dei sorgenti non riuscita ({} chiavi)", keys.len());
        for (l, table) in LANGUAGES.iter().zip(tables()) {
            let missing: Vec<_> = keys.iter().filter(|k| !table.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{}: chiavi usate ma non definite {missing:?}", l.code);
        }
    }

    #[test]
    fn placeholders_are_replaced_and_missing_keys_fall_back() {
        with_lang(Lang::from_code("en").unwrap(), || {
            assert_eq!(tr("lang.name", &[]), "English");
            assert_eq!(tr("chiave.inesistente", &[]), "chiave.inesistente");
        });
        with_lang(Lang::from_code("it").unwrap(), || assert_eq!(tr("lang.name", &[]), "Italiano"));
        assert_eq!(placeholders("a {x} b {y} {x}"), BTreeSet::from(["x".to_string(), "y".to_string()]));
    }

    #[test]
    fn settings_resolve_to_known_languages() {
        assert_eq!(resolve("it").code(), "it");
        assert_eq!(resolve("en").code(), "en");
        assert!(LANGUAGES.iter().any(|l| l.code == resolve("auto").code()));
        assert_eq!(normalize_setting("it"), "it");
        assert_eq!(normalize_setting("xx"), "auto");
        assert_eq!(normalize_setting(""), "auto");
    }
}
