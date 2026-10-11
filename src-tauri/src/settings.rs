//! Impostazioni persistenti (JSON nella cartella di configurazione dell'app).

use crate::autoswitch::{AutoConfig, AutoRestore};
use crate::eq::{sanitize_device_pattern, EqProfile};
use crate::presets::{self, Base};
use crate::profiles::HeadphoneProfile;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Versione corrente del formato delle impostazioni.
pub const SETTINGS_VERSION: u32 = 1;

/// Dispositivo di uscita audio scelto dall'utente (endpoint di riproduzione).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputDevice {
    /// Id dell'endpoint Core Audio (`{0.0.0.00000000}.{guid}`).
    pub id: String,
    /// Nome mostrato da Windows.
    pub name: String,
}

/// `{guid}` finale dell'id di un endpoint (`{0.0.0.00000000}.{guid}`): è quello
/// che Equalizer APO confronta con la direttiva `Device:`.
pub fn endpoint_guid(id: &str) -> Option<String> {
    let guid = id.rsplit('.').next()?;
    (guid.starts_with('{') && guid.ends_with('}') && guid.len() > 2).then(|| guid.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    /// Prima configurazione guidata completata (o saltata).
    pub setup_done: bool,
    /// Uscita audio su cui agisce l'equalizzatore.
    pub output: Option<OutputDevice>,
    /// Nomi Bluetooth (sottostringhe, case-insensitive) delle cuffie: abilitano
    /// stato, batteria e "Rilascia cuffie". Vuoto per cuffie cablate.
    pub name_patterns: Vec<String>,
    /// Pattern della direttiva `Device:` di Equalizer APO. Vuoto: nessun
    /// dispositivo scelto, quindi il file non si applica a nulla.
    pub apo_device: String,
    /// All'avvio automatico con Windows resta nascosta nel tray.
    pub start_minimized: bool,
    /// Imposta le cuffie come uscita audio predefinita quando si connettono.
    pub default_on_connect: bool,
    pub bypass: bool,
    pub profile: EqProfile,
    /// Preset di partenza: "b:<id>" (integrato) o "u:<nome>" (utente); `None` se modificato.
    pub active_preset: Option<String>,
    pub user_presets: Vec<EqProfile>,
    /// Modello di cuffie scelto, con le sue correzioni (copiate qui: l'app
    /// funziona anche offline).
    pub headphone: Option<HeadphoneProfile>,
    /// Variante di misura in uso (per esempio ANC attivo / spento).
    pub variant: String,
    /// Volume uniforme tra i preset: tutti i preset con preamp automatico
    /// usano lo stesso preamp comune (vedi `presets::common_preamp`).
    pub uniform_volume: bool,
    /// Attivazione automatica dei preset per app e giochi.
    pub auto: AutoConfig,
    /// Stato da ripristinare quando si chiude l'app che ha attivato un preset.
    pub auto_restore: Option<AutoRestore>,
    /// Lingua dell'interfaccia: "auto" (segue Windows) o un codice di `i18n::LANGUAGES`.
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: SETTINGS_VERSION,
            setup_done: false,
            output: None,
            name_patterns: Vec::new(),
            apo_device: String::new(),
            start_minimized: true,
            default_on_connect: false,
            bypass: false,
            profile: presets::default_profile(None),
            active_preset: Some(presets::default_key(false).into()),
            user_presets: Vec::new(),
            headphone: None,
            variant: String::new(),
            uniform_volume: true,
            auto: AutoConfig::default(),
            auto_restore: None,
            language: crate::i18n::AUTO.into(),
        }
    }
}

impl Settings {
    pub fn file(dir: &Path) -> PathBuf {
        dir.join("settings.json")
    }

    pub fn load(dir: &Path) -> Self {
        let mut s: Settings = fs::read_to_string(Self::file(dir))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        s.normalize();
        s
    }

    /// Rende coerente un file letto da disco (modificabile a mano): profilo
    /// valido, variante esistente, pattern del dispositivo ripulito.
    fn normalize(&mut self) {
        self.headphone = self.headphone.take().and_then(|h| h.sanitized().ok());
        self.variant = self
            .headphone
            .as_ref()
            .and_then(|h| h.variant(&self.variant))
            .map(|v| v.id.clone())
            .unwrap_or_default();
        self.apo_device = sanitize_device_pattern(&self.apo_device);
        self.apply_language();
    }

    /// Imposta la lingua corrente dalle impostazioni e ricostruisce i testi dei
    /// preset integrati (nome del profilo attivo e di quello da ripristinare).
    pub fn apply_language(&mut self) {
        self.language = crate::i18n::normalize_setting(&self.language);
        crate::i18n::set_current(crate::i18n::resolve(&self.language));
        self.refresh_builtin();
    }

    pub fn save(&self, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = dir.join("settings.json.tmp");
        fs::write(&tmp, json).map_err(|e| e.to_string())?;
        fs::rename(&tmp, Self::file(dir)).map_err(|e| e.to_string())
    }

    /// Correzione misurata del profilo scelto (con la variante in uso).
    pub fn base(&self) -> Option<Base> {
        self.headphone.as_ref().and_then(|h| h.base(&self.variant))
    }

    /// Chiave del preset predefinito (correzione misurata, o Piatto senza profilo).
    pub fn default_key(&self) -> &'static str {
        presets::default_key(self.headphone.is_some())
    }

    /// Profilo di un preset integrato sulla correzione corrente.
    pub fn builtin_profile(&self, id: &str) -> Option<EqProfile> {
        presets::find(id, self.base().as_ref())
    }

    /// Dopo un cambio di correzione: il preset integrato attivo (e quello da
    /// ripristinare) viene ricostruito sulla nuova base. Se non esiste più
    /// (la correzione è sparita) si passa al preset predefinito.
    fn refresh_builtin(&mut self) {
        let base = self.base();
        let default = (self.default_key(), presets::default_profile(base.as_ref()));
        let rebuild = |key: &Option<String>| -> Option<Option<EqProfile>> {
            let id = key.as_deref()?.strip_prefix("b:")?;
            Some(presets::find(id, base.as_ref()))
        };
        match rebuild(&self.active_preset) {
            Some(Some(p)) => self.profile = p,
            Some(None) => {
                self.active_preset = Some(default.0.into());
                self.profile = default.1.clone();
            }
            None => {}
        }
        if let Some(r) = self.auto_restore.as_mut() {
            match rebuild(&r.active_preset) {
                Some(Some(p)) => r.profile = p,
                Some(None) => {
                    r.active_preset = Some(default.0.into());
                    r.profile = default.1;
                }
                None => {}
            }
        }
    }

    /// Sceglie (o toglie) il modello di cuffie e la variante. Se il preset
    /// attivo era la correzione misurata e il nuovo profilo non ce l'ha, torna
    /// a Piatto.
    pub fn set_headphone(&mut self, headphone: Option<HeadphoneProfile>, variant: &str) {
        // Prima scelta del modello: da Piatto (il predefinito senza profilo) si
        // passa alla correzione misurata, che diventa il nuovo predefinito.
        let from_default_flat = self.headphone.is_none() && self.active_preset.as_deref() == Some("b:flat");
        self.headphone = headphone;
        self.variant = self.headphone.as_ref().and_then(|h| h.variant(variant)).map(|v| v.id.clone()).unwrap_or_default();
        if from_default_flat && self.headphone.is_some() {
            self.active_preset = Some(self.default_key().into());
        }
        self.refresh_builtin();
    }

    /// Sceglie l'uscita audio. `bluetooth` è il nome del dispositivo Bluetooth
    /// accoppiato a cui appartiene l'endpoint (abilita stato, batteria e
    /// "Rilascia cuffie", e vale per l'endpoint stereo e per il vivavoce);
    /// `None` per le uscite cablate, identificate dal GUID dell'endpoint.
    pub fn set_output(&mut self, output: OutputDevice, bluetooth: Option<String>) {
        match bluetooth {
            Some(name) => {
                self.apo_device = sanitize_device_pattern(&name);
                self.name_patterns = vec![name];
            }
            None => {
                self.apo_device = sanitize_device_pattern(&endpoint_guid(&output.id).unwrap_or_else(|| output.name.clone()));
                self.name_patterns.clear();
            }
        }
        self.output = Some(output);
    }

    /// Cambia la variante di misura. Il preset integrato attivo segue la nuova
    /// correzione (anche se è quello da ripristinare o applicato in automatico:
    /// la chiave è sempre "b:base"). `true` se è cambiato qualcosa da scrivere.
    pub fn set_variant(&mut self, id: &str) -> bool {
        let Some(v) = self.headphone.as_ref().and_then(|h| h.variants.iter().find(|v| v.id == id)) else {
            return false;
        };
        if v.id == self.variant {
            return false;
        }
        self.variant = v.id.clone();
        self.refresh_builtin();
        true
    }

    /// Preamp comune da applicare, `None` se il volume uniforme è spento.
    pub fn common_preamp(&self) -> Option<f64> {
        self.uniform_volume.then(|| presets::common_preamp(self.base().as_ref()))
    }

    /// Preamp effettivamente scritto in Equalizer APO per il profilo attivo
    /// (in bypass resta solo il preamp comune, se attivo).
    pub fn effective_preamp(&self) -> f64 {
        if self.bypass {
            self.common_preamp().unwrap_or(0.0)
        } else {
            self.profile.preamp_with(self.common_preamp())
        }
    }

    /// Nomi Bluetooth da cercare; vuoto per le cuffie cablate.
    pub fn patterns(&self) -> Vec<String> {
        self.name_patterns.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    }
}

#[cfg(test)]
pub mod test_support {
    use super::*;
    use crate::presets::fixtures;
    use crate::profiles::Variant;

    /// Profilo sintetico con due varianti (ANC attivo / spento).
    pub fn headphone() -> HeadphoneProfile {
        let v = |id: &str, label: &str, b: Base| Variant { id: id.into(), label: label.into(), filters: b.filters, source: b.source };
        HeadphoneProfile {
            name: "Acme Studio One".into(),
            variants: vec![v("anc_on", "ANC on", fixtures::base(None)), v("anc_off", "ANC off", fixtures::other_base(None))],
        }
    }

    /// Impostazioni con il profilo sintetico e la sua correzione attiva.
    pub fn with_headphone() -> Settings {
        let mut s = Settings::default();
        s.set_headphone(Some(headphone()), "anc_on");
        s.active_preset = Some("b:base".into());
        s.profile = s.builtin_profile("base").unwrap();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{headphone, with_headphone};
    use super::*;
    use crate::presets::fixtures;

    #[test]
    fn new_install_has_no_device_and_no_profile() {
        let s = Settings::default();
        assert!(s.output.is_none() && s.headphone.is_none() && !s.setup_done);
        assert_eq!(s.apo_device, "");
        assert!(s.patterns().is_empty(), "niente cuffie Bluetooth predefinite");
        assert_eq!(s.active_preset.as_deref(), Some("b:flat"));
        assert_eq!(s.profile, EqProfile::default());
        assert_eq!(s.base(), None);
        // File assente o illeggibile: stesse impostazioni di una nuova installazione.
        let dir = std::env::temp_dir().join(format!("switcheq-test-{}", std::process::id()));
        assert_eq!(Settings::load(&dir).active_preset.as_deref(), Some("b:flat"));
    }

    #[test]
    fn output_choice_sets_bluetooth_patterns_or_endpoint_guid() {
        let out = |id: &str, name: &str| OutputDevice { id: id.into(), name: name.into() };
        let mut s = Settings::default();
        s.set_output(out("{0.0.0.00000000}.{aaaa-1111}", "Cuffie (Acme One Stereo)"), Some("Acme One".into()));
        assert_eq!((s.apo_device.as_str(), s.patterns()), ("Acme One", vec!["Acme One".to_string()]));
        // Uscita cablata: nessun Bluetooth, il file APO vale solo per quell'endpoint.
        s.set_output(out("{0.0.0.00000000}.{bbbb-2222}", "Cuffie (USB Audio)"), None);
        assert_eq!(s.apo_device, "{bbbb-2222}");
        assert!(s.patterns().is_empty());
        assert_eq!(s.output.as_ref().map(|o| o.name.as_str()), Some("Cuffie (USB Audio)"));
        // Id senza GUID: ripiega sul nome mostrato.
        s.set_output(out("strano", "Cuffie; Preamp: 20"), None);
        assert_eq!(s.apo_device, "Cuffie; Preamp: 20");
        assert_eq!(endpoint_guid("{0.0.0.00000000}.{cc}").as_deref(), Some("{cc}"));
        assert_eq!(endpoint_guid("xx"), None);
    }

    #[test]
    fn load_repairs_a_hand_edited_file() {
        let dir = std::env::temp_dir().join(format!("switcheq-load-{}", std::process::id()));
        let mut s = with_headphone();
        s.variant = "non_esiste".into();
        s.apo_device = "all".into();
        s.headphone.as_mut().unwrap().variants[0].filters.clear(); // profilo non valido
        s.save(&dir).unwrap();
        let back = Settings::load(&dir);
        assert!(back.headphone.is_none());
        assert_eq!((back.variant.as_str(), back.apo_device.as_str()), ("", ""));
        assert_eq!(back.active_preset.as_deref(), Some("b:flat"), "la correzione non c'è più");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn partial_json_falls_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"apo_device":"Acme"}"#).unwrap();
        assert_eq!(s.apo_device, "Acme");
        assert!(s.uniform_volume);
        let s = Settings { name_patterns: vec![" ".into(), " Acme ".into()], ..Default::default() };
        assert_eq!(s.patterns(), vec!["Acme".to_string()]);
    }

    #[test]
    fn choosing_a_profile_selects_the_first_variant_and_roundtrips() {
        let mut s = Settings::default();
        s.set_headphone(Some(headphone()), "");
        assert_eq!(s.variant, "anc_on");
        assert_eq!(s.base().unwrap().variant.as_deref(), Some("ANC on"));
        assert_eq!(s.default_key(), "b:base");
        // Da Piatto (predefinito senza profilo) alla correzione misurata.
        assert_eq!(s.active_preset.as_deref(), Some("b:base"));
        assert_eq!(s.profile, s.builtin_profile("base").unwrap());
        s.set_headphone(Some(headphone()), "anc_off");
        assert_eq!(s.variant, "anc_off");
        s.set_headphone(Some(headphone()), "inesistente");
        assert_eq!(s.variant, "anc_on", "variante sconosciuta: la prima");
        // Il profilo vive nel file delle impostazioni: nessuna rete per riaprirlo.
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.headphone, s.headphone);
        assert_eq!(back.base(), s.base());
    }

    #[test]
    fn uniform_volume_applies_common_preamp_but_keeps_manual_user_presets() {
        let mut s = with_headphone();
        let common = presets::common_preamp(s.base().as_ref());
        assert_eq!(s.effective_preamp(), common);
        s.profile = EqProfile::default(); // Piatto
        assert_eq!(s.effective_preamp(), common);
        s.bypass = true;
        assert_eq!(s.effective_preamp(), common);
        s.bypass = false;
        s.uniform_volume = false;
        assert_eq!(s.effective_preamp(), 0.0);
        assert_eq!(s.common_preamp(), None);

        // Preset utente con preamp manuale: invariato, con l'opzione accesa o spenta.
        let user = EqProfile { name: "Mio".into(), preamp_auto: false, preamp_manual: -2.0, ..Default::default() };
        for uniform in [true, false] {
            let s = Settings { profile: user.clone(), uniform_volume: uniform, ..Default::default() };
            assert_eq!(s.effective_preamp(), -2.0);
        }
    }

    #[test]
    fn variant_switch_rebuilds_the_active_builtin_preset() {
        let mut s = with_headphone();
        let anc_on = s.profile.clone();
        assert!(s.set_variant("anc_off"));
        assert_eq!(s.active_preset.as_deref(), Some("b:base"));
        assert_ne!(s.profile, anc_on);
        assert_eq!(s.profile, s.builtin_profile("base").unwrap());
        assert!(s.profile.name.contains("ANC off"));
        assert!(!s.set_variant("anc_off"), "stessa variante: nulla da fare");
        assert!(!s.set_variant("boh"));

        // Gli altri preset integrati restano, ricostruiti sulla nuova variante.
        let mut s = with_headphone();
        s.active_preset = Some("b:rock".into());
        s.profile = s.builtin_profile("rock").unwrap();
        assert!(s.set_variant("anc_off"));
        assert_eq!(s.active_preset.as_deref(), Some("b:rock"));
        assert_eq!(s.profile, s.builtin_profile("rock").unwrap());
        assert!(s.profile.name.contains("ANC off"));

        // Preset utente e profilo modificato a mano: invariati.
        let user = EqProfile { name: "Mio".into(), ..Default::default() };
        let mut s = with_headphone();
        s.active_preset = Some("u:Mio".into());
        s.profile = user.clone();
        assert!(s.set_variant("anc_off"));
        assert_eq!((s.active_preset.as_deref(), &s.profile), (Some("u:Mio"), &user));

        // Anche il preset da ripristinare e quello applicato in automatico
        // seguono la variante: la chiave è sempre la stessa.
        let mut s = with_headphone();
        let restore = AutoRestore { profile: s.profile.clone(), active_preset: Some("b:base".into()), applied: "b:base".into() };
        s.auto_restore = Some(restore);
        s.set_variant("anc_off");
        let r = s.auto_restore.as_ref().unwrap();
        assert_eq!((r.active_preset.as_deref(), r.applied.as_str()), (Some("b:base"), "b:base"));
        assert_eq!(r.profile, s.builtin_profile("base").unwrap());
    }

    #[test]
    fn removing_the_profile_falls_back_to_flat() {
        let mut s = with_headphone();
        s.auto_restore = Some(AutoRestore { profile: s.profile.clone(), active_preset: Some("b:base".into()), applied: "b:film".into() });
        s.set_headphone(None, "");
        assert_eq!(s.active_preset.as_deref(), Some("b:flat"));
        assert_eq!(s.profile, EqProfile::default());
        assert_eq!(s.auto_restore.as_ref().unwrap().active_preset.as_deref(), Some("b:flat"));
        assert_eq!(s.variant, "");
        // Preset di gusto: restano, senza correzione misurata.
        let mut s = with_headphone();
        s.active_preset = Some("b:film".into());
        s.profile = s.builtin_profile("film").unwrap();
        s.set_headphone(None, "");
        assert_eq!(s.active_preset.as_deref(), Some("b:film"));
        assert_eq!(s.profile.filters, s.builtin_profile("film").unwrap().filters);
    }

    /// Il cambio lingua ricostruisce nome e fonte del preset integrato attivo e di
    /// quello da ripristinare, e un codice sconosciuto torna ad "auto".
    #[test]
    fn language_change_rebuilds_builtin_preset_texts() {
        let mut s = Settings::default();
        s.set_headphone(Some(test_support::headphone()), "");
        s.active_preset = Some("b:film".into());
        s.auto_restore = Some(AutoRestore { profile: s.profile.clone(), active_preset: Some("b:film".into()), applied: "b:gaming".into() });
        crate::i18n::with_lang(crate::i18n::Lang::from_code("it").unwrap(), || {
            s.language = "it".into();
            s.apply_language();
            assert!(s.profile.name.contains("Film"), "{}", s.profile.name);
            s.language = "en".into();
            s.apply_language();
            assert!(s.profile.name.contains("Movies"), "{}", s.profile.name);
            assert_eq!(s.auto_restore.as_ref().unwrap().profile.name, s.profile.name);
            s.language = "xx".into();
            s.apply_language();
            assert_eq!(s.language, "auto");
        });
    }

    /// Un altro modello con la sua correzione: i preset di gusto passano sui suoi filtri.
    #[test]
    fn switching_model_moves_the_taste_presets() {
        let mut s = with_headphone();
        s.active_preset = Some("b:rock".into());
        s.profile = s.builtin_profile("rock").unwrap();
        let mut other = headphone();
        other.name = "Beta Buds".into();
        other.variants.truncate(1);
        other.variants[0].filters = fixtures::other_base(None).filters;
        s.set_headphone(Some(other), "");
        assert_eq!(s.profile.name, "Rock / Metal (Beta Buds)");
        let n = fixtures::other_base(None).filters.len();
        assert_eq!(&s.profile.filters[..n], &fixtures::other_base(None).filters[..]);
    }
}
