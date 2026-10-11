mod audio;
mod autoswitch;
mod bluetooth;
mod brand;
mod eq;
mod eqapo;
mod i18n;
mod media;
mod net;
mod presets;
mod procs;
mod profiles;
mod settings;
mod startup;

use bluetooth::Headset;
use eq::EqProfile;
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::path::PathBuf;
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, Wry};

const TRAY_ID: &str = "main";
const POLL_EVERY: Duration = Duration::from_secs(5);
/// Ogni quanti cicli di polling rileggere la batteria (12 × 5 s = 1 minuto).
const BATTERY_EVERY: u32 = 12;
/// Intervallo del controllo dei processi per l'attivazione automatica.
const AUTO_EVERY: Duration = Duration::from_secs(3);
/// Voci del tray per passare le cuffie a un altro dispositivo e riprenderle
/// (stesse diciture dei pulsanti nella finestra).
const RELEASE_KEY: &str = "hp.release";
const RECONNECT_KEY: &str = "hp.reconnect";

struct AppState {
    dir: PathBuf,
    settings: Mutex<Settings>,
    headsets: Mutex<Vec<Headset>>,
    bypass_item: Mutex<Option<CheckMenuItem<Wry>>>,
    auto_item: Mutex<Option<CheckMenuItem<Wry>>>,
    conn_item: Mutex<Option<MenuItem<Wry>>>,
    /// Voci del tray con testo fisso: (voce, chiave del catalogo).
    label_items: Mutex<Vec<(MenuItem<Wry>, &'static str)>>,
    /// App o gioco che ha attivato il preset corrente, se c'è.
    auto_status: Mutex<Option<AutoStatus>>,
    /// Percorso dell'eseguibile se è una build di sviluppo (`src-tauri\target*`).
    dev_build: Option<String>,
}

impl AppState {
    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    fn update<R>(&self, f: impl FnOnce(&mut Settings) -> R) -> Result<R, String> {
        let mut s = self.settings.lock().unwrap();
        let r = f(&mut s);
        s.save(&self.dir)?;
        Ok(r)
    }
}

#[derive(Serialize)]
struct InitState {
    version: &'static str,
    settings: Settings,
    apo: eqapo::ApoStatus,
    presets: Vec<presets::BuiltinPreset>,
    headsets: Vec<Headset>,
    auto_status: Option<AutoStatus>,
    dev_build: Option<String>,
    product: &'static str,
    /// Lingua effettiva dell'interfaccia (codice), anche con l'impostazione "auto".
    lang: &'static str,
}

/// Uscita audio proposta all'utente.
#[derive(Serialize)]
struct OutputInfo {
    id: String,
    name: String,
    /// Nome del dispositivo Bluetooth accoppiato, se l'uscita è Bluetooth.
    bluetooth: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct AutoStatus {
    exe: String,
    preset: String,
}

#[derive(Serialize)]
struct Curve {
    freqs: Vec<f64>,
    db: Vec<f64>,
    preamp: f64,
    /// Preamp comune del volume uniforme, `None` se l'opzione è spenta.
    common: Option<f64>,
}

#[derive(Serialize)]
struct ApplyResult {
    preamp: f64,
    /// File scritto, `None` se Equalizer APO non è installato.
    path: Option<String>,
}

#[derive(Deserialize)]
struct SettingsPatch {
    name_patterns: Option<Vec<String>>,
    apo_device: Option<String>,
    start_minimized: Option<bool>,
    default_on_connect: Option<bool>,
    uniform_volume: Option<bool>,
}

/// Testo di una voce del tray a partire dalla chiave del catalogo.
fn tray_label(key: &str) -> String {
    match key {
        "tray.show" => t!("tray.show", product = brand::PRODUCT_NAME),
        other => i18n::tr(other, &[]),
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

/// Scrive la config di Equalizer APO secondo le impostazioni correnti.
fn write_eq(s: &Settings) -> Result<ApplyResult, String> {
    let preamp = s.effective_preamp();
    let path = if eqapo::status().installed {
        Some(eqapo::apply(&s.profile, &s.apo_device, s.bypass, s.common_preamp())?)
    } else {
        None
    };
    Ok(ApplyResult { preamp, path })
}

fn sync_tray(app: &AppHandle) {
    let state = app.state::<AppState>();
    let s = state.settings();
    if let Some(item) = state.bypass_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(!s.bypass);
        let _ = item.set_text(t!("tray.eq_active"));
    }
    if let Some(item) = state.auto_item.lock().unwrap().as_ref() {
        let _ = item.set_checked(s.auto.enabled);
        let _ = item.set_text(t!("auto.title"));
    }
    for (item, key) in state.label_items.lock().unwrap().iter() {
        let _ = item.set_text(tray_label(key));
    }
    let headsets = state.headsets.lock().unwrap().clone();
    if let Some(item) = state.conn_item.lock().unwrap().as_ref() {
        let connected = headsets.iter().any(|h| h.connected);
        let _ = item.set_text(t!(if connected { RELEASE_KEY } else { RECONNECT_KEY }));
        let _ = item.set_enabled(!headsets.is_empty());
    }
    let status = match headsets.iter().find(|h| h.connected).or(headsets.first()) {
        None if s.patterns().is_empty() => match &s.output {
            Some(o) => t!("tray.status.output", name = o.name),
            None => t!("tray.status.no_output"),
        },
        None => t!("tray.status.not_found"),
        Some(h) if h.connected => match h.battery {
            Some(b) => t!("tray.status.connected_battery", name = h.name, battery = b),
            None => t!("tray.status.connected", name = h.name),
        },
        Some(h) => t!("tray.status.disconnected", name = h.name),
    };
    let auto = state.auto_status.lock().unwrap().clone();
    let eq = match (s.bypass, auto) {
        (true, _) => t!("tray.eq_bypass"),
        (false, Some(a)) => t!("tray.eq_auto", name = s.profile.name, exe = a.exe),
        (false, None) => t!("tray.eq", name = s.profile.name),
    };
    let title = if state.dev_build.is_some() {
        t!("tray.dev_build", product = brand::PRODUCT_NAME)
    } else {
        brand::PRODUCT_NAME.to_string()
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(format!("{title}\n{status}\n{eq}")));
    }
}

/// Mostra la finestra, creandola se serve. Quando è chiusa la webview viene
/// distrutta: nel tray resta solo il processo Rust (pochi MB) invece dei
/// ~170 MB dei processi WebView2.
fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let Some(cfg) = app.config().app.windows.iter().find(|w| w.label == "main") else { return };
    match tauri::WebviewWindowBuilder::from_config(app, cfg).and_then(|b| b.build()) {
        Ok(w) => {
            let _ = w.set_focus();
        }
        Err(e) => eprintln!("cannot create the window: {e}"),
    }
}

// ---------------------------------------------------------------- comandi

#[tauri::command]
fn get_state(state: State<AppState>) -> InitState {
    let settings = state.settings();
    InitState {
        version: env!("CARGO_PKG_VERSION"),
        presets: presets::builtin(settings.base().as_ref()),
        settings,
        apo: eqapo::status(),
        headsets: state.headsets.lock().unwrap().clone(),
        auto_status: state.auto_status.lock().unwrap().clone(),
        dev_build: state.dev_build.clone(),
        product: brand::PRODUCT_NAME,
        lang: i18n::current().code(),
    }
}

/// Cambia la lingua dell'interfaccia ("auto" o un codice): ricostruisce i testi
/// dei preset integrati, aggiorna il tray e riscrive il file di Equalizer APO.
#[tauri::command]
fn set_language(app: AppHandle, state: State<AppState>, language: String) -> Result<InitState, String> {
    let s = state.update(|s| {
        s.language = language;
        s.apply_language();
        s.clone()
    })?;
    let _ = write_eq(&s);
    sync_tray(&app);
    Ok(get_state(app.state::<AppState>()))
}

#[tauri::command]
fn get_presets(state: State<AppState>) -> Vec<presets::BuiltinPreset> {
    presets::builtin(state.settings().base().as_ref())
}

#[tauri::command]
fn autostart_status() -> Result<startup::Autostart, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(startup::status(&exe))
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<startup::Autostart, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    startup::set(&exe, enabled)?;
    Ok(startup::status(&exe))
}

#[tauri::command]
fn apo_status() -> eqapo::ApoStatus {
    eqapo::status()
}

/// Rilegge subito lo stato delle cuffie (bloccante) e lo propaga a tray e finestra.
fn rescan_headsets(app: &AppHandle) -> Result<Vec<Headset>, String> {
    let patterns = app.state::<AppState>().settings().patterns();
    let list = bluetooth::find(&patterns, true)?;
    *app.state::<AppState>().headsets.lock().unwrap() = list.clone();
    let _ = app.emit("headsets", &list);
    sync_tray(app);
    Ok(list)
}

#[tauri::command]
async fn refresh_headsets(app: AppHandle) -> Result<Vec<Headset>, String> {
    blocking(move || rescan_headsets(&app)).await?
}

#[tauri::command]
async fn headset_connect(app: AppHandle, connect: bool) -> Result<String, String> {
    let patterns = app.state::<AppState>().settings().patterns();
    let n = blocking(move || audio::set_connected(&patterns, connect)).await??;
    Ok(if connect {
        t!("conn.requested", n = n)
    } else {
        t!("conn.released", n = n)
    })
}

#[tauri::command]
async fn set_default_output(app: AppHandle) -> Result<String, String> {
    let s = app.state::<AppState>().settings();
    let patterns = s.patterns();
    match s.output {
        Some(o) if patterns.is_empty() => blocking(move || audio::set_default_by_id(&o.id)).await?,
        _ => blocking(move || audio::set_default(&patterns)).await?,
    }
}

#[tauri::command]
async fn audio_endpoints(app: AppHandle) -> Result<Vec<audio::Endpoint>, String> {
    let patterns = app.state::<AppState>().settings().patterns();
    blocking(move || audio::endpoints(&patterns)).await?
}

/// Uscite audio attive, con il dispositivo Bluetooth accoppiato a cui appartengono.
#[tauri::command]
async fn list_outputs() -> Result<Vec<OutputInfo>, String> {
    blocking(|| {
        let eps = audio::render_endpoints()?;
        let paired = bluetooth::paired_names();
        Ok(eps
            .into_iter()
            .map(|e| OutputInfo { bluetooth: bluetooth::paired_name_in(&e.name, &paired), id: e.id, name: e.name })
            .collect())
    })
    .await?
}

/// Sceglie l'uscita su cui agisce l'equalizzatore e riscrive la configurazione.
#[tauri::command]
async fn choose_output(app: AppHandle, id: String) -> Result<Settings, String> {
    let (output, bluetooth) = blocking(move || {
        let ep = audio::render_endpoints()?
            .into_iter()
            .find(|e| e.id == id)
            .ok_or_else(|| t!("err.output_not_found"))?;
        let bluetooth = bluetooth::paired_name_in(&ep.name, &bluetooth::paired_names());
        Ok::<_, String>((settings::OutputDevice { id: ep.id, name: ep.name }, bluetooth))
    })
    .await??;
    let state = app.state::<AppState>();
    let s = state.update(|s| {
        s.set_output(output, bluetooth);
        s.clone()
    })?;
    let rewrite = s.clone();
    let rescan = app.clone();
    blocking(move || {
        let r = write_eq(&rewrite);
        let _ = rescan_headsets(&rescan);
        r
    })
    .await??;
    sync_tray(&app);
    Ok(s)
}

/// Cerca un modello nell'indice di AutoEq (scaricato e messo in cache).
#[tauri::command]
async fn search_models(app: AppHandle, query: String, refresh: bool) -> Result<Vec<profiles::Candidate>, String> {
    let dir = app.state::<AppState>().dir.clone();
    blocking(move || {
        let index = profiles::load_index(&dir, refresh, net::get_text)?;
        Ok(profiles::search(&index, &query, profiles::SEARCH_LIMIT))
    })
    .await?
}

/// Applica un profilo appena ottenuto: salva e riscrive la configurazione.
async fn adopt_profile(app: &AppHandle, profile: profiles::HeadphoneProfile, variant: &str) -> Result<Settings, String> {
    let state = app.state::<AppState>();
    let s = state.update(|s| {
        s.set_headphone(Some(profile), variant);
        s.clone()
    })?;
    let rewrite = s.clone();
    blocking(move || write_eq(&rewrite)).await??;
    sync_tray(app);
    Ok(s)
}

/// Scarica le correzioni del risultato di ricerca scelto e lo imposta come modello.
#[tauri::command]
async fn choose_model(app: AppHandle, candidate: profiles::Candidate) -> Result<Settings, String> {
    let profile = blocking(move || profiles::build_from_candidate(&candidate, &net::get_text)).await??;
    adopt_profile(&app, profile, "").await
}

/// Imposta come modello un file ParametricEQ.txt (Equalizer APO / AutoEq).
#[tauri::command]
async fn choose_model_file(app: AppHandle, path: String) -> Result<Settings, String> {
    let profile = blocking(move || profiles::from_apo_file(&path)).await??;
    adopt_profile(&app, profile, "").await
}

/// Passa a un'altra variante di misura del modello (per esempio ANC spento).
#[tauri::command]
async fn choose_variant(app: AppHandle, variant: String) -> Result<Settings, String> {
    let state = app.state::<AppState>();
    let (s, changed) = state.update(|s| {
        let changed = s.set_variant(&variant);
        (s.clone(), changed)
    })?;
    if changed {
        let rewrite = s.clone();
        blocking(move || write_eq(&rewrite)).await??;
    }
    sync_tray(&app);
    Ok(s)
}

/// Toglie il modello: restano i preset di gusto senza correzione misurata.
#[tauri::command]
async fn clear_model(app: AppHandle) -> Result<Settings, String> {
    let state = app.state::<AppState>();
    let s = state.update(|s| {
        s.set_headphone(None, "");
        s.clone()
    })?;
    let rewrite = s.clone();
    blocking(move || write_eq(&rewrite)).await??;
    sync_tray(&app);
    Ok(s)
}

/// Chiude la procedura guidata (completata o saltata).
#[tauri::command]
fn finish_setup(state: State<AppState>) -> Result<Settings, String> {
    state.update(|s| {
        s.setup_done = true;
        s.clone()
    })
}

/// Apre solo destinazioni note (impostazioni Bluetooth, pagine di download).
#[tauri::command]
fn open_link(target: String) -> Result<(), String> {
    const ALLOWED: [&str; 3] = [
        "ms-settings:bluetooth",
        "https://sourceforge.net/projects/equalizerapo/",
        profiles::AUTOEQ_REPO,
    ];
    if !ALLOWED.iter().any(|a| target.starts_with(a)) {
        return Err(t!("err.link_not_allowed"));
    }
    std::process::Command::new("explorer.exe").arg(&target).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn eq_preview(state: State<AppState>, profile: EqProfile) -> Curve {
    let filters = profile.effective_filters();
    let freqs = eq::log_freqs(256);
    let db = freqs.iter().map(|&f| eq::response_db(&filters, f)).collect();
    let common = state.settings().common_preamp();
    Curve { freqs, db, preamp: profile.preamp_with(common), common }
}

#[tauri::command]
fn eq_apply(
    app: AppHandle,
    state: State<AppState>,
    profile: EqProfile,
    active_preset: Option<String>,
    bypass: bool,
) -> Result<ApplyResult, String> {
    let s = state.update(|s| {
        s.profile = profile;
        s.active_preset = active_preset;
        s.bypass = bypass;
        s.clone()
    })?;
    let r = write_eq(&s);
    sync_tray(&app);
    r
}

#[tauri::command]
fn update_settings(app: AppHandle, state: State<AppState>, patch: SettingsPatch) -> Result<Settings, String> {
    let (s, rewrite) = state.update(|s| {
        let old_device = s.apo_device.clone();
        if let Some(v) = patch.name_patterns {
            s.name_patterns = v.into_iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
        }
        if let Some(v) = patch.apo_device {
            s.apo_device = eq::sanitize_device_pattern(&v);
        }
        if let Some(v) = patch.start_minimized {
            s.start_minimized = v;
        }
        if let Some(v) = patch.default_on_connect {
            s.default_on_connect = v;
        }
        let mut profile_changed = false;
        if let Some(v) = patch.uniform_volume.filter(|v| *v != s.uniform_volume) {
            s.uniform_volume = v;
            profile_changed = true;
        }
        (s.clone(), old_device != s.apo_device || profile_changed)
    })?;
    if rewrite {
        write_eq(&s)?;
    }
    sync_tray(&app);
    Ok(s)
}

fn upsert_preset(list: &mut Vec<EqProfile>, mut p: EqProfile) {
    p.name = p.name.trim().chars().take(60).collect();
    if p.name.is_empty() {
        p.name = t!("preset.user_default");
    }
    match list.iter_mut().find(|x| x.name == p.name) {
        Some(x) => *x = p,
        None => list.push(p),
    }
}

#[tauri::command]
fn save_user_preset(state: State<AppState>, profile: EqProfile) -> Result<Vec<EqProfile>, String> {
    state.update(|s| {
        upsert_preset(&mut s.user_presets, profile);
        s.user_presets.clone()
    })
}

#[tauri::command]
fn delete_user_preset(state: State<AppState>, name: String) -> Result<Vec<EqProfile>, String> {
    state.update(|s| {
        s.user_presets.retain(|p| p.name != name);
        s.user_presets.clone()
    })
}

/// Importa un preset JSON (formato di esportazione dell'app) oppure un file di
/// testo Equalizer APO / AutoEq ParametricEQ.txt.
#[tauri::command]
fn import_preset(state: State<AppState>, path: String) -> Result<(EqProfile, Vec<EqProfile>), String> {
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 1_000_000 {
        return Err(t!("err.preset_too_large"));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let stem = std::path::Path::new(&path)
        .file_stem()
        .map(|s| s.to_string_lossy().replace(" ParametricEQ", ""))
        .unwrap_or_else(|| t!("preset.imported_default"));
    let mut profile = if text.trim_start().starts_with('{') {
        serde_json::from_str::<EqProfile>(&text).map_err(|e| t!("err.json_invalid", error = e))?
    } else {
        eq::parse_apo_text(&stem, &text)?
    };
    profile.filters = profile.filters.iter().map(|f| f.sanitized()).collect();
    if profile.filters.len() > 64 {
        return Err(t!("err.too_many_filters"));
    }
    let list = state.update(|s| {
        upsert_preset(&mut s.user_presets, profile.clone());
        s.user_presets.clone()
    })?;
    Ok((profile, list))
}

#[tauri::command]
fn export_preset(path: String, profile: EqProfile) -> Result<(), String> {
    if !path.to_ascii_lowercase().ends_with(".json") {
        return Err(t!("err.export_json"));
    }
    let json = serde_json::to_string_pretty(&profile).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_auto(app: AppHandle, state: State<AppState>, config: autoswitch::AutoConfig) -> Result<autoswitch::AutoConfig, String> {
    let c = state.update(|s| {
        s.auto = config.sanitized();
        s.auto.clone()
    })?;
    sync_tray(&app);
    Ok(c)
}

#[tauri::command]
async fn running_apps() -> Result<Vec<String>, String> {
    blocking(procs::running_apps).await
}

// ---------------------------------------------------------------- monitor

/// Controlla periodicamente lo stato delle cuffie e notifica il frontend solo
/// quando cambia qualcosa. Il costo è una query WinRT ogni 5 secondi.
fn monitor(app: AppHandle) {
    let mut tick: u32 = 0;
    loop {
        let state = app.state::<AppState>();
        let s = state.settings();
        let patterns = s.patterns();
        let prev = state.headsets.lock().unwrap().clone();
        if patterns.is_empty() {
            // Uscita cablata (o nessuna scelta): niente da cercare via Bluetooth.
            if !prev.is_empty() {
                state.headsets.lock().unwrap().clear();
                let _ = app.emit("headsets", Vec::<Headset>::new());
                sync_tray(&app);
            }
        } else if let Ok(mut list) = bluetooth::find(&patterns, false) {
            let changed_conn = list.len() != prev.len()
                || list.iter().any(|h| prev.iter().find(|p| p.id == h.id).map(|p| p.connected) != Some(h.connected));
            if changed_conn || tick % BATTERY_EVERY == 0 {
                if let Ok(full) = bluetooth::find(&patterns, true) {
                    list = full;
                }
            } else {
                for h in &mut list {
                    h.battery = prev.iter().find(|p| p.id == h.id).and_then(|p| p.battery);
                }
            }
            let newly_connected = list
                .iter()
                .any(|h| h.connected && !prev.iter().any(|p| p.id == h.id && p.connected));
            if list != prev {
                *state.headsets.lock().unwrap() = list.clone();
                let _ = app.emit("headsets", &list);
                sync_tray(&app);
            }
            // Al primo giro (tick 0) non cambiamo l'uscita: solo sulle nuove connessioni.
            if newly_connected && tick > 0 && s.default_on_connect {
                let app2 = app.clone();
                std::thread::spawn(move || {
                    // L'endpoint audio compare qualche secondo dopo la connessione Bluetooth.
                    for _ in 0..8 {
                        std::thread::sleep(Duration::from_secs(2));
                        if let Ok(name) = audio::set_default(&patterns) {
                            let _ = app2.emit("default-output", name);
                            break;
                        }
                    }
                });
            }
        }
        tick = tick.wrapping_add(1);
        std::thread::sleep(POLL_EVERY);
    }
}

/// Chiave di preset → profilo: "b:<id>" integrato, "u:<nome>" utente.
fn resolve_preset(key: &str, s: &Settings) -> Option<EqProfile> {
    if let Some(id) = key.strip_prefix("b:") {
        s.builtin_profile(id)
    } else if let Some(name) = key.strip_prefix("u:") {
        s.user_presets.iter().find(|p| p.name == name).cloned()
    } else {
        None
    }
}

/// Attivazione automatica dei preset: ogni 3 s confronta i processi con le
/// regole e le euristiche dei giochi, e segue film e musica tramite gli
/// eventi della sessione multimediale di Windows (che svegliano subito il
/// ciclo). A funzione spenta non legge processi né sessioni (resta solo
/// l'eventuale ripristino del preset precedente).
fn auto_monitor(app: AppHandle) {
    let mut scanner = procs::Scanner::default();
    let mut detector = autoswitch::Detector::default();
    let mut engine = autoswitch::Engine::default();
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let mut watcher: Option<media::Watcher> = None;
    let mut tracker = media::Tracker::default();
    let mut media_retry = Instant::now();
    loop {
        let _ = wake_rx.recv_timeout(AUTO_EVERY);
        while wake_rx.try_recv().is_ok() {}
        let state = app.state::<AppState>();
        let (cfg, has_base) = {
            let s = state.settings.lock().unwrap();
            (s.auto.clone(), s.headphone.is_some())
        };
        let media_on = cfg.enabled && cfg.media.enabled;
        if !media_on {
            watcher = None;
            tracker = media::Tracker::default();
        } else if watcher.is_none() && Instant::now() >= media_retry {
            match media::Watcher::new(wake_tx.clone()) {
                Ok(w) => watcher = Some(w),
                Err(e) => {
                    eprintln!("media sessions unavailable: {e}");
                    media_retry = Instant::now() + Duration::from_secs(60);
                }
            }
        }
        let target = if cfg.enabled {
            let snap = scanner.snapshot();
            let playing = watcher.as_mut().and_then(|w| {
                let sessions = w.sessions();
                // I titoli delle finestre servono solo per le sessioni dei browser.
                let browser_playing = sessions.iter().any(|s| s.playing && media::browser_exe(&s.aumid).is_some());
                let windows = if browser_playing { media::browser_windows(&snap.procs) } else { Vec::new() };
                media::playing_target(sessions, &windows, &cfg.media, has_base)
            });
            let media_target = if media_on { tracker.update(Instant::now(), playing) } else { None };
            autoswitch::pick(detector.candidates(&snap, &cfg), media_target)
        } else {
            scanner = procs::Scanner::default();
            None
        };
        let (change, settings, status) = {
            let mut s = state.settings.lock().unwrap();
            let change = engine.step(&mut s, target.as_ref(), resolve_preset);
            if change.is_some() {
                if let Err(e) = s.save(&state.dir) {
                    eprintln!("cannot save settings: {e}");
                }
            }
            let status = s
                .auto_restore
                .as_ref()
                .and(target.as_ref())
                .map(|t| AutoStatus { exe: t.exe.clone(), preset: s.profile.name.clone() });
            (change, s.clone(), status)
        };
        let status_changed = {
            let mut cur = state.auto_status.lock().unwrap();
            let changed = *cur != status;
            *cur = status.clone();
            changed
        };
        if status_changed {
            let _ = app.emit("auto-status", &status);
        }
        if let Some(change) = &change {
            let _ = write_eq(&settings);
            let _ = app.emit("settings", &settings);
            if cfg.notify {
                let body = match change {
                    autoswitch::Change::Applied { preset, exe } => t!("notify.applied", preset = preset, exe = exe),
                    autoswitch::Change::Restored { preset } => t!("notify.restored", preset = preset),
                };
                if let Err(e) = procs::notify(brand::PRODUCT_NAME, &body) {
                    eprintln!("notification not shown: {e}");
                }
            }
        }
        if change.is_some() || status_changed {
            sync_tray(&app);
        }
    }
}

/// Voce del tray "Rilascia cuffie" / "Riconnetti a questo PC": disconnette le
/// cuffie da questo PC così possono collegarsi a qualsiasi altro dispositivo,
/// o le riprende. Se il driver rifiuta, apre le impostazioni Bluetooth come i
/// pulsanti della finestra.
fn toggle_connection(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let connected = state.headsets.lock().unwrap().iter().any(|h| h.connected);
        let patterns = state.settings().patterns();
        match audio::set_connected(&patterns, !connected) {
            Ok(_) => {
                // Il cambio di stato Bluetooth arriva dopo qualche secondo.
                for _ in 0..4 {
                    std::thread::sleep(Duration::from_secs(2));
                    if rescan_headsets(&app).is_ok_and(|l| l.iter().any(|h| h.connected) != connected) {
                        break;
                    }
                }
            }
            Err(e) => {
                eprintln!("{e}");
                let _ = open_link("ms-settings:bluetooth".into());
            }
        }
    });
}

// ---------------------------------------------------------------- avvio

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let s = app.state::<AppState>().settings();
    let show = MenuItemBuilder::with_id("show", tray_label("tray.show")).build(app)?;
    let conn = MenuItemBuilder::with_id("conn", t!(RELEASE_KEY)).enabled(false).build(app)?;
    let eq_item = CheckMenuItemBuilder::with_id("eq", t!("tray.eq_active")).checked(!s.bypass).build(app)?;
    let auto_item = CheckMenuItemBuilder::with_id("auto", t!("auto.title")).checked(s.auto.enabled).build(app)?;
    let bt = MenuItemBuilder::with_id("bt", t!("tray.bluetooth")).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", t!("tray.quit")).build(app)?;
    let menu = MenuBuilder::new(app)
        .items(&[&show, &conn])
        .separator()
        .items(&[&eq_item, &auto_item, &bt])
        .separator()
        .item(&quit)
        .build()?;
    let state = app.state::<AppState>();
    *state.bypass_item.lock().unwrap() = Some(eq_item);
    *state.auto_item.lock().unwrap() = Some(auto_item);
    *state.conn_item.lock().unwrap() = Some(conn);
    *state.label_items.lock().unwrap() =
        vec![(show, "tray.show"), (bt, "tray.bluetooth"), (quit, "tray.quit")];

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(brand::PRODUCT_NAME)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "conn" => toggle_connection(app.clone()),
            "auto" => {
                let state = app.state::<AppState>();
                if let Ok(s) = state.update(|s| {
                    s.auto.enabled = !s.auto.enabled;
                    s.clone()
                }) {
                    let _ = app.emit("settings", &s);
                }
                sync_tray(app);
            }
            "bt" => {
                let _ = open_link("ms-settings:bluetooth".into());
            }
            "eq" => {
                let state = app.state::<AppState>();
                if let Ok(s) = state.update(|s| {
                    s.bypass = !s.bypass;
                    s.clone()
                }) {
                    let _ = write_eq(&s);
                    let _ = app.emit("bypass", s.bypass);
                }
                sync_tray(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Istanze di prova: cartella delle impostazioni alternativa.
            let dir = match std::env::var_os(brand::ENV_CONFIG_DIR).filter(|v| !v.is_empty()) {
                Some(d) => PathBuf::from(d),
                None => app.path().app_config_dir()?,
            };
            // Carica anche la lingua (`Settings::load` la imposta prima di costruire i preset).
            let settings = Settings::load(&dir);
            let hidden = settings.start_minimized && std::env::args().any(|a| a == startup::ARG);
            let exe = std::env::current_exe().ok();
            // La versione installata riprende la voce di avvio se punta altrove
            // (per esempio a una build di sviluppo) o è in HKLM.
            match exe.as_deref().and_then(startup::repair) {
                Some(Ok(done)) => eprintln!("{done}"),
                Some(Err(e)) => eprintln!("startup entry not repaired: {e}"),
                None => {}
            }
            let dev_build = exe.filter(|p| startup::is_dev_build(p)).map(|p| p.display().to_string());
            // Riallinea il file di Equalizer APO alle impostazioni salvate.
            let _ = write_eq(&settings);
            app.manage(AppState {
                dir,
                settings: Mutex::new(settings),
                headsets: Mutex::new(Vec::new()),
                bypass_item: Mutex::new(None),
                auto_item: Mutex::new(None),
                conn_item: Mutex::new(None),
                label_items: Mutex::new(Vec::new()),
                auto_status: Mutex::new(None),
                dev_build,
            });
            setup_tray(app)?;
            if !hidden {
                show_main(app.handle());
            }
            let handle = app.handle().clone();
            std::thread::Builder::new().name("headset-monitor".into()).spawn(move || monitor(handle))?;
            let handle = app.handle().clone();
            std::thread::Builder::new().name("auto-preset".into()).spawn(move || auto_monitor(handle))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_presets,
            set_language,
            autostart_status,
            set_autostart,
            set_auto,
            running_apps,
            apo_status,
            refresh_headsets,
            headset_connect,
            set_default_output,
            audio_endpoints,
            list_outputs,
            choose_output,
            search_models,
            choose_model,
            choose_model_file,
            choose_variant,
            clear_model,
            finish_setup,
            open_link,
            eq_preview,
            eq_apply,
            update_settings,
            save_user_preset,
            delete_user_preset,
            import_preset,
            export_preset,
        ])
        .build(tauri::generate_context!())
        .expect("error while starting the app")
        .run(|_app, event| {
            // La X chiude la finestra ma l'app resta nel tray finché non si sceglie
            // "Esci" (che chiama `exit(0)` e quindi ha un codice).
            if let RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_replaces_by_name_and_trims() {
        let mut list = Vec::new();
        upsert_preset(&mut list, EqProfile { name: " Mio ".into(), ..Default::default() });
        upsert_preset(&mut list, EqProfile { name: "Mio".into(), preamp_manual: -3.0, ..Default::default() });
        upsert_preset(&mut list, EqProfile { name: "".into(), ..Default::default() });
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].preamp_manual, -3.0);
        assert_eq!(list[1].name, "Preset utente");
    }

    /// Film e musica passano dallo stesso Engine dei giochi e, con il volume
    /// uniforme, ogni preset attivato resta al preamp comune: con un profilo
    /// scelto e senza.
    #[test]
    fn media_presets_keep_the_uniform_volume() {
        for with_profile in [true, false] {
            let mut s = if with_profile {
                let mut s = settings::test_support::with_headphone();
                s.active_preset = Some("b:flat".into());
                s.profile = EqProfile::default();
                s
            } else {
                Settings::default()
            };
            let common = presets::common_preamp(s.base().as_ref());
            assert_eq!(s.effective_preamp(), common, "Piatto");
            let cfg = media::MediaConfig::default();
            let mut e = autoswitch::Engine::default();
            let music_key = cfg.music_key(with_profile);
            assert_eq!(music_key, if with_profile { "b:base" } else { "b:flat" });
            for (aumid, want) in [
                ("Spotify.exe", music_key),
                ("4DF9E0F8.Netflix_mcm4njqhnhss8!Netflix.App", cfg.film_preset.clone()),
            ] {
                let session = media::Session { aumid: aumid.into(), playing: true, ..Default::default() };
                let t = media::playing_target(&[session], &[], &cfg, with_profile).unwrap();
                assert_eq!(t.preset, want);
                let before = s.active_preset.clone();
                let changed = e.step(&mut s, Some(&t), resolve_preset).is_some();
                assert_eq!(changed, before.as_deref() != Some(want.as_str()), "{aumid}");
                assert_eq!(s.active_preset.as_deref(), Some(want.as_str()));
                assert_eq!(s.effective_preamp(), common, "{aumid} (profilo: {with_profile})");
            }
            e.step(&mut s, None, resolve_preset);
            assert_eq!(s.active_preset.as_deref(), Some("b:flat"));
        }
    }

    /// Cambio di variante mentre la musica ha attivato la correzione misurata:
    /// la chiave resta "b:base" ma i filtri cambiano; non è una scelta manuale e
    /// alla fine torna il preset di prima, ricostruito sulla nuova variante.
    #[test]
    fn variant_switch_during_music_keeps_auto_preset_and_restore() {
        let mut s = settings::test_support::with_headphone();
        s.active_preset = Some("b:rock".into());
        s.profile = s.builtin_profile("rock").unwrap();
        let cfg = media::MediaConfig::default();
        let mut e = autoswitch::Engine::default();
        let music = || {
            let session = media::Session { aumid: "Spotify.exe".into(), playing: true, ..Default::default() };
            media::playing_target(&[session], &[], &cfg, true).unwrap()
        };
        assert!(e.step(&mut s, Some(&music()), resolve_preset).is_some());
        assert_eq!(s.active_preset.as_deref(), Some("b:base"));
        let anc_on = s.profile.clone();

        assert!(s.set_variant("anc_off"));
        assert_eq!(s.active_preset.as_deref(), Some("b:base"));
        assert_ne!(s.profile, anc_on, "i filtri seguono la nuova variante");
        assert_eq!(e.step(&mut s, Some(&music()), resolve_preset), None);
        assert!(s.auto_restore.is_some(), "non è una scelta manuale");

        assert!(matches!(e.step(&mut s, None, resolve_preset), Some(autoswitch::Change::Restored { .. })));
        assert_eq!(s.active_preset.as_deref(), Some("b:rock"));
        assert_eq!(s.profile, s.builtin_profile("rock").unwrap());
        assert!(s.profile.name.contains("ANC off"));
    }
}
