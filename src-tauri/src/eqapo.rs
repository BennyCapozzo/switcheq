//! Rilevamento di Equalizer APO e scrittura della configurazione.
//!
//! L'app scrive `<config>\<brand::APO_FILE>` e aggiunge (una sola volta, in fondo)
//! la riga `Include:` corrispondente a `<config>\config.txt`, dopo averne fatto un backup.
//! Commenta anche la configurazione dimostrativa dell'installer di Equalizer
//! APO (vedi [`eq::without_demo`]), con un backup a parte.
//! Equalizer APO rilegge i file automaticamente quando cambiano.

use crate::brand;
use crate::eq::{self, EqProfile};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const OUR_FILE: &str = brand::APO_FILE;
pub const DOWNLOAD_URL: &str = "https://sourceforge.net/projects/equalizerapo/";
const DEFAULT_DIR: &str = r"C:\Program Files\EqualizerAPO";

#[derive(Debug, Clone, Serialize)]
pub struct ApoStatus {
    pub installed: bool,
    pub install_path: Option<String>,
    pub config_path: Option<String>,
    /// `config.txt` contiene già la riga Include del nostro file.
    pub included: bool,
    pub download_url: &'static str,
}

#[cfg(windows)]
fn reg_string(value: &str) -> Option<String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY};

    let key = HSTRING::from(r"SOFTWARE\EqualizerAPO");
    let name = HSTRING::from(value);
    let mut buf = [0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: buffer e dimensione sono validi; RegGetValueW termina la stringa.
    let res = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if res.is_err() {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    let s = String::from_utf16_lossy(&buf[..len.min(buf.len())]);
    let s = s.trim_end_matches('\0').trim().to_string();
    (!s.is_empty()).then_some(s)
}

#[cfg(not(windows))]
fn reg_string(_value: &str) -> Option<String> {
    None
}

fn locate() -> (Option<PathBuf>, Option<PathBuf>) {
    // Istanze di prova: una cartella config alternativa al posto di quella reale.
    if let Some(dir) = std::env::var_os(brand::ENV_APO_CONFIG).map(PathBuf::from).filter(|p| p.is_dir()) {
        return (Some(dir.clone()), Some(dir));
    }
    let install = reg_string("InstallPath")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(DEFAULT_DIR)))
        .filter(|p| p.join("config").is_dir() || p.join("Editor.exe").is_file());
    let config = reg_string("ConfigPath")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| install.as_ref().map(|p| p.join("config")).filter(|p| p.is_dir()));
    (install, config)
}

pub fn status() -> ApoStatus {
    let (install, config) = locate();
    let included = config
        .as_ref()
        .and_then(|c| fs::read_to_string(c.join("config.txt")).ok())
        .is_some_and(|txt| eq::has_include(&txt, OUR_FILE));
    ApoStatus {
        installed: install.is_some() && config.is_some(),
        install_path: install.map(|p| p.display().to_string()),
        config_path: config.map(|p| p.display().to_string()),
        included,
        download_url: DOWNLOAD_URL,
    }
}

fn io_err(action: &str, path: &Path, e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        crate::t!("err.apo.denied", action = action, path = path.display())
    } else {
        crate::t!("err.apo.io", action = action, path = path.display(), error = e)
    }
}

/// Scrive il file solo se il contenuto è cambiato (evita ricaricamenti inutili
/// di Equalizer APO).
fn write_if_changed(path: &Path, content: &str) -> Result<(), String> {
    if fs::read_to_string(path).is_ok_and(|old| old == content) {
        return Ok(());
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content).map_err(|e| io_err(&crate::t!("err.apo.act_write"), &tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| io_err(&crate::t!("err.apo.act_replace"), path, e))
}

fn ensure_include(config_dir: &Path) -> Result<(), String> {
    let main = config_dir.join("config.txt");
    let current = match fs::read_to_string(&main) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(io_err(&crate::t!("err.apo.act_read"), &main, e)),
    };
    let backup = |name: &str| -> Result<(), String> {
        let path = config_dir.join(name);
        if current.is_empty() || path.exists() {
            return Ok(());
        }
        fs::write(&path, &current).map_err(|e| io_err(&crate::t!("err.apo.act_backup"), &path, e))
    };
    let mut text = current.clone();
    if let Some(updated) = eq::without_demo(&text) {
        backup(brand::APO_DEMO_BACKUP)?;
        text = updated;
    }
    if let Some(updated) = eq::with_include(&text, OUR_FILE) {
        backup(brand::APO_BACKUP)?;
        text = updated;
    }
    if text != current {
        fs::write(&main, text).map_err(|e| io_err(&crate::t!("err.apo.act_write"), &main, e))?;
    }
    Ok(())
}

/// Applica il profilo: genera il nostro file, assicura l'Include in `config.txt`
/// e disattiva la demo di Equalizer APO.
/// `common` è il preamp comune del volume uniforme (`None` se spento).
/// Senza dispositivo scelto non tocca `config.txt`: al più riduce a soli
/// commenti il nostro file, se esiste già.
pub fn apply(profile: &EqProfile, device_pattern: &str, bypass: bool, common: Option<f64>) -> Result<String, String> {
    let (_, config) = locate();
    let dir = config.ok_or_else(|| crate::t!("err.apo.not_installed"))?;
    let content = eq::render_config(profile, device_pattern, bypass, common);
    let path = dir.join(OUR_FILE);
    if eq::sanitize_device_pattern(device_pattern).is_empty() {
        if path.exists() {
            write_if_changed(&path, &content)?;
        }
        return Ok(path.display().to_string());
    }
    write_if_changed(&path, &content)?;
    ensure_include(&dir)?;
    Ok(path.display().to_string())
}
