//! Avvio con Windows: voce con il nome del prodotto in `HKCU\...\Run`, solo per
//! l'utente corrente, con l'eseguibile tra virgolette e `--autostart`.
//!
//! Non usa tauri-plugin-autostart: nella modalità predefinita scrive in HKLM
//! (tutti gli utenti) appena l'app ha i privilegi di amministratore, cioè
//! sempre con il Controllo account utente disattivato. Così una build di
//! sviluppo registrata da `src-tauri\target\release` ripartiva al login al
//! posto di quella installata. Qui le build di sviluppo non si registrano mai
//! e la versione installata sposta su di sé le voci che puntano altrove.

use serde::Serialize;
use std::path::Path;

pub const VALUE_NAME: &str = crate::brand::STARTUP_VALUE;
/// Argomento della voce di avvio: l'app parte nascosta nel tray.
pub const ARG: &str = "--autostart";

/// Stato dell'avvio automatico mostrato nelle impostazioni.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Autostart {
    /// La voce dell'utente avvia proprio questo eseguibile ed è attiva.
    pub enabled: bool,
    /// Questo eseguibile è una build di sviluppo (non si registra all'avvio).
    pub dev_build: bool,
    /// Altre voci con lo stesso nome (in HKLM, verso un altro eseguibile o
    /// disattivate in Gestione attività).
    pub others: Vec<String>,
}

pub fn command_line(exe: &Path) -> String {
    format!("\"{}\" {ARG}", exe.display())
}

/// Eseguibile di una voce Run, con o senza virgolette (il plugin precedente
/// non le metteva, anche con spazi nel percorso).
pub fn entry_exe(cmd: &str) -> &str {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') {
        return rest.split('"').next().unwrap_or(rest);
    }
    match cmd.to_ascii_lowercase().find(".exe") {
        Some(i) => &cmd[..i + 4],
        None => cmd.split_whitespace().next().unwrap_or(cmd),
    }
}

fn same_exe(cmd: &str, exe: &Path) -> bool {
    entry_exe(cmd).eq_ignore_ascii_case(&exe.display().to_string())
}

/// Build di sviluppo: eseguibile dentro `src-tauri\target*\` (cargo / tauri build).
pub fn is_dev_build(exe: &Path) -> bool {
    let parts: Vec<String> = exe.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect();
    parts.windows(2).any(|w| w[0] == "src-tauri" && w[1].starts_with("target"))
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_ROUTINE_FLAGS,
        REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
    };

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Stato impostato da Gestione attività (primo byte dispari = disattivata).
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    fn get(root: HKEY, key: &str, flags: REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
        let (key, name) = (HSTRING::from(key), HSTRING::from(VALUE_NAME));
        let mut buf = vec![0u8; 4096];
        let mut size = buf.len() as u32;
        // SAFETY: buffer e dimensione sono validi per tutta la chiamata.
        let res = unsafe {
            RegGetValueW(
                root,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                flags,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if res.is_err() {
            return None;
        }
        buf.truncate(size as usize);
        Some(buf)
    }

    fn get_string(root: HKEY) -> Option<String> {
        let bytes = get(root, RUN, RRF_RT_REG_SZ)?;
        let wide: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&wide).trim_end_matches('\0').to_string())
    }

    fn disabled(root: HKEY) -> bool {
        get(root, APPROVED, RRF_RT_REG_BINARY).is_some_and(|b| b.first().is_some_and(|x| x & 1 == 1))
    }

    fn set_string(root: HKEY, value: &str) -> Result<(), String> {
        let wide: Vec<u16> = value.encode_utf16().chain([0]).collect();
        // SAFETY: RegSetKeyValueW crea la chiave se manca; il buffer è una
        // stringa UTF-16 terminata di `wide.len() * 2` byte.
        unsafe {
            RegSetKeyValueW(
                root,
                &HSTRING::from(RUN),
                &HSTRING::from(VALUE_NAME),
                REG_SZ.0,
                Some(wide.as_ptr().cast()),
                (wide.len() * 2) as u32,
            )
        }
        .ok()
        .map_err(|e| e.message())
    }

    fn delete(root: HKEY, key: &str) -> Result<(), String> {
        // SAFETY: chiave e nome sono stringhe terminate valide.
        let res = unsafe { RegDeleteKeyValueW(root, &HSTRING::from(key), &HSTRING::from(VALUE_NAME)) };
        if res == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        res.ok().map_err(|e| e.message())
    }

    pub fn status(exe: &Path) -> Autostart {
        let user = get_string(HKEY_CURRENT_USER);
        let user_off = disabled(HKEY_CURRENT_USER);
        let enabled = user.as_deref().is_some_and(|c| same_exe(c, exe)) && !user_off;
        let mut others = Vec::new();
        if let Some(c) = user.filter(|_| !enabled) {
            others.push(if user_off {
                crate::t!("startup.other_user_off", command = c)
            } else {
                crate::t!("startup.other_user", command = c)
            });
        }
        if let Some(c) = get_string(HKEY_LOCAL_MACHINE) {
            others.push(crate::t!("startup.other_machine", command = c));
        }
        Autostart { enabled, dev_build: is_dev_build(exe), others }
    }

    pub fn set(exe: &Path, on: bool) -> Result<(), String> {
        if on && is_dev_build(exe) {
            return Err(crate::t!("startup.dev_build"));
        }
        // La voce per tutti gli utenti lasciata dal plugin precedente va tolta in ogni caso.
        let machine = delete(HKEY_LOCAL_MACHINE, RUN).and_then(|_| delete(HKEY_LOCAL_MACHINE, APPROVED));
        if on {
            set_string(HKEY_CURRENT_USER, &command_line(exe))?;
            // Riattiva la voce se era stata disattivata in Gestione attività.
            delete(HKEY_CURRENT_USER, APPROVED)?;
        } else {
            delete(HKEY_CURRENT_USER, RUN)?;
        }
        machine.map_err(|e| crate::t!("err.startup.hklm", error = e))
    }

    pub fn repair(exe: &Path) -> Option<Result<String, String>> {
        if is_dev_build(exe) {
            return None;
        }
        let user = get_string(HKEY_CURRENT_USER);
        let machine = get_string(HKEY_LOCAL_MACHINE);
        let stale_user = user.as_deref().is_some_and(|c| !same_exe(c, exe));
        if machine.is_none() && !stale_user {
            return None;
        }
        let was_on = (machine.is_some() && !disabled(HKEY_LOCAL_MACHINE)) || (user.is_some() && !disabled(HKEY_CURRENT_USER));
        let old = machine.or(user).unwrap_or_default();
        Some(set(exe, was_on).map(|_| {
            if was_on {
                format!("avvio con Windows spostato da «{old}» a «{}»", command_line(exe))
            } else {
                format!("rimossa la voce di avvio disattivata «{old}»")
            }
        }))
    }
}

#[cfg(windows)]
pub use imp::{set, status};

/// All'avvio della versione installata: una voce con il nome del prodotto in HKLM o
/// verso un altro eseguibile (per esempio una build di sviluppo) viene
/// spostata su questo eseguibile in HKCU; se era disattivata in Gestione
/// attività viene solo tolta. `None` se non c'era niente da fare.
#[cfg(windows)]
pub fn repair(exe: &Path) -> Option<Result<String, String>> {
    imp::repair(exe)
}

#[cfg(not(windows))]
pub fn status(exe: &Path) -> Autostart {
    Autostart { dev_build: is_dev_build(exe), ..Default::default() }
}

#[cfg(not(windows))]
pub fn set(_: &Path, _: bool) -> Result<(), String> {
    Err(crate::t!("err.startup.windows_only"))
}

#[cfg(not(windows))]
pub fn repair(_: &Path) -> Option<Result<String, String>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn dev_builds_are_recognized() {
        for dev in [
            r"C:\Users\alice\work\src-tauri\target\release\switcheq.exe",
            r"C:\Users\alice\work\src-tauri\target-alt\release\switcheq.exe",
            r"C:\x\src-tauri\target\debug\switcheq.exe",
        ] {
            assert!(is_dev_build(Path::new(dev)), "{dev}");
        }
        for installed in [
            r"C:\Users\alice\AppData\Local\SwitchEQ\switcheq.exe",
            r"C:\Program Files\SwitchEQ\switcheq.exe",
            r"C:\target\switcheq.exe",
        ] {
            assert!(!is_dev_build(Path::new(installed)), "{installed}");
        }
    }

    #[test]
    fn run_entries_are_parsed_with_or_without_quotes() {
        let exe = PathBuf::from(r"C:\Users\alice\AppData\Local\SwitchEQ\switcheq.exe");
        let cmd = command_line(&exe);
        assert_eq!(cmd, r#""C:\Users\alice\AppData\Local\SwitchEQ\switcheq.exe" --autostart"#);
        assert_eq!(entry_exe(&cmd), exe.display().to_string());
        assert!(same_exe(&cmd, &exe));
        // Voce scritta dal plugin precedente: senza virgolette, con spazi nel percorso.
        let old = r"C:\Users\alice\work\src-tauri\target\release\switcheq.exe --autostart";
        assert_eq!(entry_exe(old), r"C:\Users\alice\work\src-tauri\target\release\switcheq.exe");
        assert!(!same_exe(old, &exe));
        assert!(same_exe(&cmd.to_uppercase(), &exe));
    }
}
