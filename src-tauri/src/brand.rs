//! Nome del prodotto e identificatori derivati, in un unico punto.
//!
//! Per cambiare nome si modificano `PRODUCT_NAME` e `APP_ID` qui, `productName`, `identifier` e il
//! titolo della finestra in `tauri.conf.json` (un test verifica che coincidano)
//! e il nome del pacchetto in `Cargo.toml`. L'identificativo deve restare
//! diverso da quello di qualsiasi altra app, così impostazioni, istanza unica,
//! cartella di installazione e voce di avvio non entrano mai in conflitto.

/// Nome mostrato all'utente (finestra, tray, installer, notifiche).
pub const PRODUCT_NAME: &str = "SwitchEQ";

/// Identificativo Tauri: cartella delle impostazioni, istanza unica, AUMID.
pub const APP_ID: &str = "it.switcheq.app";

/// Voce di avvio in `HKCU\...\Run`.
pub const STARTUP_VALUE: &str = PRODUCT_NAME;

/// File scritto nella cartella `config` di Equalizer APO e incluso da `config.txt`.
pub const APO_FILE: &str = "switcheq.txt";

/// Suffisso dei backup di `config.txt` accanto al file originale.
pub const APO_BACKUP: &str = "config.txt.switcheq-backup";
pub const APO_DEMO_BACKUP: &str = "config.txt.switcheq-demo-backup";

/// Variabile d'ambiente: cartella `config` di Equalizer APO alternativa. Serve
/// ai test e alle istanze di prova per non toccare l'installazione reale.
pub const ENV_APO_CONFIG: &str = "SWITCHEQ_APO_CONFIG";

/// Variabile d'ambiente: cartella delle impostazioni alternativa.
pub const ENV_CONFIG_DIR: &str = "SWITCHEQ_CONFIG_DIR";

/// `User-Agent` delle richieste di rete: `SwitchEQ/<versione>`.
pub fn user_agent() -> String {
    format!("{PRODUCT_NAME}/{}", env!("CARGO_PKG_VERSION"))
}

/// Nomi (in minuscolo) degli eseguibili di questa app, mai scambiati per un gioco.
pub const EXE_NAMES: [&str; 1] = ["switcheq.exe"];

#[cfg(test)]
mod tests {
    use super::*;

    /// `tauri.conf.json` e `Cargo.toml` devono dire lo stesso nome di questo file.
    #[test]
    fn config_files_match_the_constants() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["productName"], PRODUCT_NAME);
        assert_eq!(conf["identifier"], APP_ID);
        assert_eq!(conf["app"]["windows"][0]["title"], PRODUCT_NAME);
        assert!(EXE_NAMES.contains(&format!("{}.exe", env!("CARGO_PKG_NAME")).as_str()));
    }

    #[test]
    fn apo_file_is_a_plain_txt_name() {
        assert!(APO_FILE.ends_with(".txt") && !APO_FILE.contains(' '));
    }
}
