//! Lettura dei processi in esecuzione, della finestra in primo piano e
//! notifiche di Windows. La logica di decisione è in `autoswitch.rs`.
//!
//! Costo: un'istantanea ToolHelp (solo nomi e pid) a ogni giro; il percorso
//! completo viene chiesto a Windows solo per i processi nuovi e poi tenuto in
//! cache finché il pid resta vivo.

#[cfg(not(windows))]
use crate::autoswitch::Snapshot;

pub const APP_ID: &str = crate::brand::APP_ID;

#[cfg(windows)]
mod imp {
    use super::APP_ID;
    use crate::autoswitch::{Foreground, Proc, Snapshot};
    use std::collections::HashMap;
    use windows::core::{HSTRING, PCWSTR, PWSTR};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Win32::Foundation::{CloseHandle, RECT};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, GWL_STYLE, WS_CAPTION,
    };

    fn wide_to_string(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    fn image_path(pid: u32) -> Option<String> {
        // SAFETY: handle chiuso subito dopo l'uso; buffer e dimensione coerenti.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut size = buf.len() as u32;
            let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size);
            let _ = CloseHandle(h);
            r.ok()?;
            Some(String::from_utf16_lossy(&buf[..size as usize]).to_lowercase())
        }
    }

    /// (pid, pid padre, nome eseguibile in minuscolo) di tutti i processi.
    fn list() -> Vec<(u32, u32, String)> {
        let mut out = Vec::with_capacity(400);
        // SAFETY: l'istantanea viene chiusa a fine funzione; dwSize impostato come richiesto.
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
            let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            let mut ok = Process32FirstW(snap, &mut e).is_ok();
            while ok {
                out.push((e.th32ProcessID, e.th32ParentProcessID, wide_to_string(&e.szExeFile).to_lowercase()));
                ok = Process32NextW(snap, &mut e).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        out
    }

    fn foreground() -> Option<Foreground> {
        // SAFETY: sole letture su un handle di finestra; strutture inizializzate.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }
            let mut class = [0u16; 64];
            let n = GetClassNameW(hwnd, &mut class) as usize;
            let class = String::from_utf16_lossy(&class[..n]);
            // Desktop e barra delle applicazioni coprono lo schermo ma non sono app.
            let shell = matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd");
            let has_caption = (GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION.0) == WS_CAPTION.0;
            let mut rect = RECT::default();
            let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            let covers = GetWindowRect(hwnd, &mut rect).is_ok()
                && GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool()
                && rect.left <= mi.rcMonitor.left
                && rect.top <= mi.rcMonitor.top
                && rect.right >= mi.rcMonitor.right
                && rect.bottom >= mi.rcMonitor.bottom;
            // Una finestra massimizzata normale ha la barra del titolo: così non
            // la si scambia per un gioco borderless quando la taskbar è nascosta.
            Some(Foreground { pid, fullscreen: covers && !shell && !has_caption })
        }
    }

    #[derive(Default)]
    pub struct Scanner {
        /// pid → (nome, percorso) visti al giro precedente.
        cache: HashMap<u32, (String, Option<String>)>,
    }

    impl Scanner {
        pub fn snapshot(&mut self) -> Snapshot {
            let mut cache = HashMap::with_capacity(self.cache.len());
            let procs = list()
                .into_iter()
                .map(|(pid, parent, exe)| {
                    let path = match self.cache.remove(&pid) {
                        Some((name, path)) if name == exe => path,
                        _ => image_path(pid),
                    };
                    cache.insert(pid, (exe.clone(), path.clone()));
                    Proc { pid, parent, exe, path }
                })
                .collect();
            self.cache = cache;
            Snapshot { procs, foreground: foreground() }
        }
    }

    /// Nomi degli eseguibili dell'utente in esecuzione (per suggerire le regole).
    pub fn running_apps() -> Vec<String> {
        let mut names: Vec<String> = list()
            .into_iter()
            .filter(|(pid, _, _)| image_path(*pid).is_some_and(|p| !p.starts_with(r"c:\windows\")))
            .map(|(_, _, exe)| exe)
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Registra l'AppUserModelID (nome mostrato nelle notifiche) per l'utente
    /// corrente: serve alle app non pacchettizzate anche senza collegamento
    /// nel menu Start (es. in sviluppo).
    fn register_app_id() {
        let key = HSTRING::from(format!(r"Software\Classes\AppUserModelId\{APP_ID}"));
        let value: Vec<u16> = crate::brand::PRODUCT_NAME.encode_utf16().chain([0]).collect();
        // SAFETY: RegSetKeyValueW crea la chiave se manca; il buffer è una
        // stringa UTF-16 terminata di `value.len() * 2` byte.
        unsafe {
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                &HSTRING::from("DisplayName"),
                REG_SZ.0,
                Some(value.as_ptr().cast()),
                (value.len() * 2) as u32,
            );
        }
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
    }

    pub fn notify(title: &str, body: &str) -> Result<(), String> {
        static REGISTERED: std::sync::Once = std::sync::Once::new();
        REGISTERED.call_once(register_app_id);
        let xml = format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual>\
             <audio silent=\"true\"/></toast>",
            xml_escape(title),
            xml_escape(body)
        );
        let show = || -> windows::core::Result<()> {
            let doc = XmlDocument::new()?;
            doc.LoadXml(&HSTRING::from(xml))?;
            let toast = ToastNotification::CreateToastNotification(&doc)?;
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_ID))?.Show(&toast)
        };
        show().map_err(|e| e.message())
    }
}

#[cfg(windows)]
pub use imp::{notify, running_apps, Scanner};

#[cfg(not(windows))]
#[derive(Default)]
pub struct Scanner;
#[cfg(not(windows))]
impl Scanner {
    pub fn snapshot(&mut self) -> Snapshot {
        Snapshot::default()
    }
}
#[cfg(not(windows))]
pub fn running_apps() -> Vec<String> {
    Vec::new()
}
#[cfg(not(windows))]
pub fn notify(_: &str, _: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Istantanea reale: deve contenere questo processo con il suo percorso.
    #[test]
    fn snapshot_contains_current_process() {
        let mut sc = Scanner::default();
        let snap = sc.snapshot();
        let me = std::process::id();
        let p = snap.procs.iter().find(|p| p.pid == me).expect("processo corrente");
        assert!(p.path.as_deref().is_some_and(|x| x.ends_with(&p.exe)));
        // Secondo giro: i percorsi arrivano dalla cache.
        assert!(sc.snapshot().procs.iter().any(|p| p.pid == me && p.path.is_some()));
    }

    /// Prova a secco sul PC reale: stampa cosa verrebbe attivato adesso e
    /// quanto costa un giro di controllo. Non cambia niente.
    #[test]
    #[ignore = "dipende dai programmi aperti; stampa il risultato del rilevamento"]
    fn dry_run_on_this_pc() {
        use crate::autoswitch::{AutoConfig, Detector};
        let cfg = AutoConfig { enabled: true, ..Default::default() };
        let mut sc = Scanner::default();
        let t0 = std::time::Instant::now();
        let snap = sc.snapshot();
        let first = t0.elapsed();
        let t1 = std::time::Instant::now();
        let snap2 = sc.snapshot();
        let cached = t1.elapsed();
        println!("processi: {}, primo giro {first:?}, giro con cache {cached:?}", snap.procs.len());
        println!("primo piano: {:?}", snap2.foreground);
        let mut det = Detector::default();
        println!("preset desiderato: {:?}", det.desired(&snap2, &cfg));
        for p in &snap2.procs {
            let single = crate::autoswitch::Snapshot { procs: vec![p.clone()], foreground: None };
            let parent: Vec<_> = snap2.procs.iter().filter(|x| x.pid == p.parent).cloned().collect();
            let with_parent = crate::autoswitch::Snapshot { procs: [parent, vec![p.clone()]].concat(), foreground: None };
            let d = Detector::default().desired(&with_parent, &cfg).filter(|t| t.exe == p.exe);
            if d.is_some() || Detector::default().desired(&single, &cfg).is_some() {
                println!("  gioco: {} ({:?})", p.exe, p.path);
            }
        }
    }
}
