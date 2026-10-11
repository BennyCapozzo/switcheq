//! Rilevamento di film e musica dalla sessione multimediale di Windows
//! (`Windows.Media.Control`, la stessa del riquadro del volume e dei tasti
//! multimediali).
//!
//! La classificazione e l'isteresi sono pure e testabili. La lettura delle
//! sessioni è in `imp` ed è guidata dagli eventi di Windows: le sessioni si
//! rileggono solo quando una cambia (avvio, pausa, nuovo brano o video), mai a
//! intervalli fissi. I titoli delle finestre del browser si leggono solo
//! mentre una sessione del browser è in riproduzione.
//!
//! Cosa espongono i browser. Firefox, verificato con una prova reale: AUMID
//! esadecimale di 16 cifre ("308046B0AF4A39CB"), titolo e artista presi dai
//! metadati Media Session della pagina (YouTube: titolo del video e nome del
//! canale), PlaybackType "Music" anche per i video, nessun indirizzo del sito.
//! Chrome ed Edge (Chromium) non erano installati sul PC di prova: dal codice
//! di Chromium ci si aspetta AUMID "Chrome" / "MSEdge" e gli stessi metadati.
//! Il sito si riconosce quindi dal titolo della finestra del browser, che
//! contiene il titolo della scheda attiva ("… - YouTube - Google Chrome").

use crate::autoswitch::{Target, IGNORE};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Valore di `MediaRule::preset`: usa il preset scelto per la musica.
pub const MUSIC: &str = "music";
/// Valore di `MediaRule::preset`: usa il preset scelto per i film.
pub const FILM: &str = "film";
/// Valore di `MediaConfig::music_preset`: la correzione misurata del profilo
/// scelto (verso il target Harman, preferito dal 64% degli ascoltatori
/// [Olive 2019, vedi presets.rs]); senza profilo, Piatto.
pub const BASE_PRESET: &str = "base";
const MAX_RULES: usize = 200;

/// Per cambiare preset la riproduzione deve durare almeno questo tempo:
/// evita cambi per un'anteprima o un salto di brano.
pub const START_DELAY: Duration = Duration::from_secs(2);
/// In pausa o a riproduzione finita il preset resta per questo tempo, poi
/// torna quello di prima: una pausa breve o il passaggio tra due brani non
/// fanno cambiare il suono.
pub const STOP_GRACE: Duration = Duration::from_secs(10);

/// Abbinamento scelto dall'utente: testo da cercare → preset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRule {
    /// Testo in minuscolo cercato nell'AUMID dell'app, nel titolo e
    /// nell'artista della sessione e nel titolo della finestra del browser
    /// (es. "twitch", "spotify", "raiplay").
    pub pattern: String,
    /// [`MUSIC`], [`FILM`], [`IGNORE`] o una chiave di preset "b:<id>" / "u:<nome>".
    pub preset: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaConfig {
    /// Attiva il rilevamento (vale solo con i preset automatici accesi).
    pub enabled: bool,
    /// Preset per la musica, oppure [`BASE_PRESET`].
    pub music_preset: String,
    /// Preset per film e serie.
    pub film_preset: String,
    /// Regole dell'utente: hanno la precedenza sui riconoscimenti integrati.
    pub rules: Vec<MediaRule>,
}

impl Default for MediaConfig {
    fn default() -> Self {
        MediaConfig { enabled: true, music_preset: BASE_PRESET.into(), film_preset: "b:film".into(), rules: Vec::new() }
    }
}

impl MediaConfig {
    /// Ripulisce la configurazione arrivata dal frontend: testi in
    /// minuscolo, niente duplicati (vince la prima regola), preset validi.
    pub fn sanitized(mut self) -> Self {
        let mut seen = std::collections::HashSet::new();
        self.rules = self
            .rules
            .into_iter()
            .filter_map(|r| {
                let pattern = r.pattern.trim().to_lowercase();
                let preset = r.preset.trim().to_string();
                let valid = !pattern.is_empty() && pattern.chars().count() <= 120 && !pattern.chars().any(char::is_control);
                (valid && !preset.is_empty() && seen.insert(pattern.clone())).then_some(MediaRule { pattern, preset })
            })
            .take(MAX_RULES)
            .collect();
        let d = MediaConfig::default();
        let fix = |v: String, def: String| {
            let v = v.trim().to_string();
            if v.is_empty() || [MUSIC, FILM, IGNORE].contains(&v.as_str()) { def } else { v }
        };
        self.music_preset = fix(self.music_preset, d.music_preset);
        self.film_preset = fix(self.film_preset, d.film_preset);
        if self.film_preset == BASE_PRESET {
            self.film_preset = MediaConfig::default().film_preset;
        }
        self
    }

    /// Chiave del preset per la musica, con [`BASE_PRESET`] risolto.
    pub fn music_key(&self, has_base: bool) -> String {
        if self.music_preset == BASE_PRESET {
            crate::presets::default_key(has_base).into()
        } else {
            self.music_preset.clone()
        }
    }
}

// ---------------------------------------------------------------- sessioni

/// `PlaybackType` dichiarato dall'app (spesso assente nei browser).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Unknown,
    Music,
    Video,
    Image,
}

/// Una sessione multimediale di Windows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Session {
    /// SourceAppUserModelId, es. "Spotify.exe", "MSEdge",
    /// "4DF9E0F8.Netflix_mcm4njqhnhss8!Netflix.App".
    pub aumid: String,
    pub playing: bool,
    pub kind: Kind,
    pub title: String,
    pub artist: String,
    /// Sessione corrente secondo Windows (quella del riquadro del volume).
    pub current: bool,
}

/// Finestra principale di un browser: eseguibile in minuscolo e titolo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub exe: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Music,
    Film,
}

/// App o sito riconosciuto senza regole dell'utente.
struct Known {
    name: &'static str,
    /// `None`: lettore video (Film, ma Musica se l'app dichiara un brano audio).
    category: Option<Category>,
    /// Parti dell'AUMID (minuscolo) delle app desktop, Store e delle PWA.
    apps: &'static [&'static str],
    /// Nome del sito come parte a sé del titolo ("… - YouTube - Google Chrome").
    sites: &'static [&'static str],
    /// Domini, nel caso compaiano nei metadati.
    domains: &'static [&'static str],
}

const M: Option<Category> = Some(Category::Music);
const F: Option<Category> = Some(Category::Film);

/// L'ordine conta: YouTube Music prima di YouTube.
const KNOWN: &[Known] = &[
    Known {
        name: "YouTube Music",
        category: M,
        // PWA ufficiale (id dell'estensione Chromium) e client desktop non ufficiali.
        apps: &["cinhimbnkkaeohfgghhklpknlkffjgod", "youtube-music", "youtubemusic"],
        sites: &["youtube music"],
        domains: &["music.youtube.com"],
    },
    Known {
        name: "YouTube",
        category: M,
        apps: &["agimnkijcaahngcdmfeangaknmldooml"],
        sites: &["youtube"],
        domains: &["youtube.com", "youtu.be"],
    },
    Known { name: "Spotify", category: M, apps: &["spotify"], sites: &["spotify"], domains: &["open.spotify.com"] },
    Known {
        name: "Apple Music",
        category: M,
        apps: &["applemusic", "apple music"],
        sites: &["apple music"],
        domains: &["music.apple.com"],
    },
    // Il pacchetto Store di TIDAL si chiama ancora "WiMPMusic".
    Known { name: "TIDAL", category: M, apps: &["tidal", "wimpmusic"], sites: &["tidal"], domains: &["tidal.com"] },
    Known { name: "Deezer", category: M, apps: &["deezer"], sites: &["deezer"], domains: &["deezer.com"] },
    Known {
        name: "Amazon Music",
        category: M,
        apps: &["amazonmusic", "amazon music"],
        sites: &["amazon music"],
        domains: &["music.amazon."],
    },
    Known { name: "Netflix", category: F, apps: &["netflix"], sites: &["netflix"], domains: &["netflix.com"] },
    Known {
        name: "Disney+",
        category: F,
        apps: &["disney"],
        sites: &["disney+", "disney plus"],
        domains: &["disneyplus.com"],
    },
    Known {
        name: "Prime Video",
        category: F,
        apps: &["primevideo", "amazonvideo"],
        sites: &["prime video", "amazon prime video"],
        domains: &["primevideo.com"],
    },
    Known { name: "VLC", category: None, apps: &["vlc"], sites: &[], domains: &[] },
    // Il nuovo Lettore multimediale di Windows è il pacchetto "ZuneMusic"; il
    // vecchio Windows Media Player espone "MediaPlayer32".
    Known {
        name: "Lettore multimediale",
        category: None,
        apps: &["microsoft.zunemusic", "mediaplayer32", "wmplayer"],
        sites: &[],
        domains: &[],
    },
    Known { name: "Film e TV", category: None, apps: &["microsoft.zunevideo"], sites: &[], domains: &[] },
];

/// Eseguibile del browser a cui appartiene la sessione, se è un browser
/// (anche le PWA, che hanno AUMID "Chrome._crx_<id>" / "MSEdge._crx_<id>").
pub fn browser_exe(aumid: &str) -> Option<&'static str> {
    let a = aumid.to_lowercase();
    let is = |name: &str| a == name || a.starts_with(&format!("{name}."));
    if is("chrome") {
        Some("chrome.exe")
    } else if is("msedge") {
        Some("msedge.exe")
    } else if is("brave") {
        Some("brave.exe")
    } else if is("vivaldi") {
        Some("vivaldi.exe")
    } else if is("opera") {
        Some("opera.exe")
    } else if a.contains("firefox") || (a.len() == 16 && a.chars().all(|c| c.is_ascii_hexdigit())) {
        // Firefox registra un AUMID ricavato dalla cartella di installazione,
        // es. "308046B0AF4A39CB".
        Some("firefox.exe")
    } else {
        None
    }
}

/// Parti di un titolo separate da " - ", " | ", ": " ecc., in minuscolo.
fn segments(text: &str) -> Vec<String> {
    let mut t = text.to_lowercase();
    for sep in [" - ", " – ", " — ", " | ", ": ", " · ", " • "] {
        t = t.replace(sep, "\u{1}");
    }
    t.split('\u{1}').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// `seg` è il nome del sito; accetta anche il "YouTube e altre 3 pagine" di Edge.
fn segment_is(seg: &str, site: &str) -> bool {
    seg == site || seg.strip_prefix(site).is_some_and(|rest| rest.starts_with(' ') && rest.chars().any(|c| c.is_ascii_digit()))
}

fn known_site(text: &str) -> Option<&'static Known> {
    let lower = text.to_lowercase();
    let segs = segments(text);
    KNOWN.iter().find(|k| {
        k.domains.iter().any(|d| lower.contains(d)) || k.sites.iter().any(|s| segs.iter().any(|seg| segment_is(seg, s)))
    })
}

/// Esito della classificazione, prima di scegliere il preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Category(Category),
    Preset(String),
    Ignore,
}

/// Cosa è stato riconosciuto e da cosa (nome mostrato nel tooltip del tray).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub verdict: Verdict,
    pub label: String,
}

fn rule_hit(r: &MediaRule) -> Hit {
    let verdict = match r.preset.as_str() {
        MUSIC => Verdict::Category(Category::Music),
        FILM => Verdict::Category(Category::Film),
        IGNORE => Verdict::Ignore,
        p => Verdict::Preset(p.into()),
    };
    Hit { verdict, label: r.pattern.clone() }
}

fn known_hit(k: &Known, kind: Kind) -> Hit {
    let cat = k.category.unwrap_or(if kind == Kind::Music { Category::Music } else { Category::Film });
    Hit { verdict: Verdict::Category(cat), label: k.name.into() }
}

/// Classifica una sessione. Per i browser usa i metadati e il titolo della
/// finestra che contiene il titolo della sessione; se nessuna finestra lo
/// contiene (es. Netflix intitola la scheda solo "Netflix"), usa le finestre
/// di quel browser solo se tutte indicano la stessa cosa. Sessioni del
/// browser non riconosciute: `None`.
pub fn classify(s: &Session, windows: &[Window], cfg: &MediaConfig) -> Option<Hit> {
    let aumid = s.aumid.to_lowercase();
    let browser = browser_exe(&aumid);
    let mut strong = vec![aumid.clone(), s.title.to_lowercase(), s.artist.to_lowercase()];
    let mut weak = Vec::new();
    if let Some(exe) = browser {
        let title = s.title.trim().to_lowercase();
        let (matching, others): (Vec<String>, Vec<String>) = windows
            .iter()
            .filter(|w| w.exe == exe)
            .map(|w| w.title.to_lowercase())
            .partition(|t| title.chars().count() >= 3 && t.contains(&title));
        if matching.is_empty() {
            weak = others;
        } else {
            strong.extend(matching);
        }
    }
    let rule = |texts: &[String]| cfg.rules.iter().find(|r| texts.iter().any(|t| t.contains(&r.pattern))).map(rule_hit);
    if let Some(hit) = rule(&strong) {
        return Some(hit);
    }
    if let Some(k) = KNOWN.iter().find(|k| k.apps.iter().any(|a| aumid.contains(a))) {
        return Some(known_hit(k, s.kind));
    }
    browser?;
    if let Some(k) = strong[1..].iter().find_map(|t| known_site(t)) {
        return Some(known_hit(k, s.kind));
    }
    let hits: Vec<Hit> = weak
        .iter()
        .filter_map(|t| rule(std::slice::from_ref(t)).or_else(|| known_site(t).map(|k| known_hit(k, s.kind))))
        .collect();
    let first = hits.first()?;
    hits.iter().all(|h| h.verdict == first.verdict).then(|| first.clone())
}

/// Preset desiderato per le sessioni in riproduzione: la corrente per prima,
/// poi le altre nell'ordine di Windows. `None` se nulla di riconosciuto suona.
pub fn playing_target(sessions: &[Session], windows: &[Window], cfg: &MediaConfig, has_base: bool) -> Option<Target> {
    let mut playing: Vec<&Session> = sessions.iter().filter(|s| s.playing).collect();
    playing.sort_by_key(|s| !s.current);
    playing.into_iter().find_map(|s| {
        let hit = classify(s, windows, cfg)?;
        let preset = match hit.verdict {
            Verdict::Ignore => return None,
            Verdict::Category(Category::Music) => cfg.music_key(has_base),
            Verdict::Category(Category::Film) => cfg.film_preset.clone(),
            Verdict::Preset(p) => p,
        };
        Some(Target { preset, exe: hit.label, game: false })
    })
}

/// Isteresi tra la sessione che suona e il preset da tenere attivo.
#[derive(Debug, Default)]
pub struct Tracker {
    active: Option<Target>,
    pending: Option<(Target, Instant)>,
    last_playing: Option<Instant>,
}

impl Tracker {
    /// `playing` è il risultato di [`playing_target`] in questo momento.
    pub fn update(&mut self, now: Instant, playing: Option<Target>) -> Option<Target> {
        match playing {
            Some(t) => {
                self.last_playing = Some(now);
                if self.active.as_ref() == Some(&t) {
                    self.pending = None;
                } else {
                    let since = match &self.pending {
                        Some((p, since)) if *p == t => *since,
                        _ => {
                            self.pending = Some((t.clone(), now));
                            now
                        }
                    };
                    if now.duration_since(since) >= START_DELAY {
                        self.active = Some(t);
                        self.pending = None;
                    }
                }
            }
            None => {
                self.pending = None;
                if self.last_playing.map_or(true, |t| now.duration_since(t) >= STOP_GRACE) {
                    self.active = None;
                }
            }
        }
        self.active.clone()
    }
}

#[cfg(windows)]
pub use imp::{browser_windows, Watcher};

#[cfg(windows)]
mod imp {
    use super::{Kind, Session, Window};
    use crate::autoswitch::Proc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::Arc;
    use windows::core::{Result, BOOL};
    use windows::Foundation::TypedEventHandler;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as GSession, GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Media::MediaPlaybackType;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible};

    /// Browser di cui leggere i titoli delle finestre.
    const BROWSERS: &[&str] = &["chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "vivaldi.exe", "opera.exe"];

    /// Segue le sessioni multimediali tramite eventi. Ogni evento segna le
    /// sessioni da rileggere e sveglia il thread dei preset automatici.
    pub struct Watcher {
        manager: Manager,
        tokens: [i64; 2],
        subs: Vec<(GSession, i64, i64)>,
        dirty: Arc<AtomicBool>,
        resubscribe: Arc<AtomicBool>,
        wake: Sender<()>,
        cache: Vec<Session>,
    }

    fn notifier(dirty: &Arc<AtomicBool>, resub: Option<&Arc<AtomicBool>>, wake: &Sender<()>) -> impl Fn() + Send + 'static {
        let (dirty, resub, wake) = (dirty.clone(), resub.cloned(), wake.clone());
        move || {
            if let Some(r) = &resub {
                r.store(true, Ordering::SeqCst);
            }
            dirty.store(true, Ordering::SeqCst);
            let _ = wake.send(());
        }
    }

    impl Watcher {
        pub fn new(wake: Sender<()>) -> Result<Self> {
            // SAFETY: inizializza COM (MTA) per questo thread; ripetuta è innocua.
            let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
            let manager = Manager::RequestAsync()?.join()?;
            let dirty = Arc::new(AtomicBool::new(true));
            let resubscribe = Arc::new(AtomicBool::new(true));
            let on_sessions = notifier(&dirty, Some(&resubscribe), &wake);
            let on_current = notifier(&dirty, None, &wake);
            let t1 = manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
                on_sessions();
                Ok(())
            }))?;
            let t2 = manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
                on_current();
                Ok(())
            }))?;
            Ok(Watcher { manager, tokens: [t1, t2], subs: Vec::new(), dirty, resubscribe, wake, cache: Vec::new() })
        }

        fn unsubscribe(&mut self) {
            for (s, a, b) in self.subs.drain(..) {
                let _ = s.RemovePlaybackInfoChanged(a);
                let _ = s.RemoveMediaPropertiesChanged(b);
            }
        }

        fn subscribe(&mut self) -> Result<()> {
            self.unsubscribe();
            for s in self.manager.GetSessions()? {
                let on_play = notifier(&self.dirty, None, &self.wake);
                let on_props = notifier(&self.dirty, None, &self.wake);
                let a = s.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                    on_play();
                    Ok(())
                }))?;
                let b = s.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    on_props();
                    Ok(())
                }))?;
                self.subs.push((s, a, b));
            }
            Ok(())
        }

        /// Sessioni attuali; le rilegge da Windows solo dopo un evento.
        pub fn sessions(&mut self) -> &[Session] {
            if self.resubscribe.swap(false, Ordering::SeqCst) {
                if let Err(e) = self.subscribe() {
                    eprintln!("sessioni multimediali non lette: {}", e.message());
                }
            }
            if self.dirty.swap(false, Ordering::SeqCst) {
                let current = self.manager.GetCurrentSession().and_then(|s| s.SourceAppUserModelId()).map(|a| a.to_string()).ok();
                self.cache = self.subs.iter().filter_map(|(s, _, _)| read(s, current.as_deref())).collect();
            }
            &self.cache
        }
    }

    impl Drop for Watcher {
        fn drop(&mut self) {
            self.unsubscribe();
            let _ = self.manager.RemoveSessionsChanged(self.tokens[0]);
            let _ = self.manager.RemoveCurrentSessionChanged(self.tokens[1]);
        }
    }

    fn kind(t: MediaPlaybackType) -> Kind {
        match t {
            MediaPlaybackType::Music => Kind::Music,
            MediaPlaybackType::Video => Kind::Video,
            MediaPlaybackType::Image => Kind::Image,
            _ => Kind::Unknown,
        }
    }

    fn read(s: &GSession, current: Option<&str>) -> Option<Session> {
        let aumid = s.SourceAppUserModelId().ok()?.to_string();
        let info = s.GetPlaybackInfo().ok()?;
        let playing = info.PlaybackStatus().is_ok_and(|st| st == Status::Playing);
        let mut k = info.PlaybackType().and_then(|r| r.Value()).map(kind).unwrap_or_default();
        let (mut title, mut artist) = (String::new(), String::new());
        if let Ok(p) = s.TryGetMediaPropertiesAsync().and_then(|op| op.join()) {
            title = p.Title().map(|t| t.to_string()).unwrap_or_default();
            artist = p.Artist().map(|t| t.to_string()).unwrap_or_default();
            if k == Kind::Unknown {
                k = p.PlaybackType().and_then(|r| r.Value()).map(kind).unwrap_or_default();
            }
        }
        Some(Session { current: current == Some(aumid.as_str()), aumid, playing, kind: k, title, artist })
    }

    /// Titoli delle finestre visibili dei browser in esecuzione.
    pub fn browser_windows(procs: &[Proc]) -> Vec<Window> {
        let exes: HashMap<u32, &str> =
            procs.iter().filter(|p| BROWSERS.contains(&p.exe.as_str())).map(|p| (p.pid, p.exe.as_str())).collect();
        let mut found: Vec<(u32, String)> = Vec::new();
        if exes.is_empty() {
            return Vec::new();
        }
        unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
            // SAFETY: lparam punta al Vec di `browser_windows`, vivo per tutta EnumWindows.
            let out = unsafe { &mut *(lparam.0 as *mut Vec<(u32, String)>) };
            // SAFETY: sole letture su un handle di finestra valido durante l'enumerazione.
            unsafe {
                if IsWindowVisible(hwnd).as_bool() {
                    let mut buf = [0u16; 512];
                    let n = GetWindowTextW(hwnd, &mut buf) as usize;
                    if n > 0 {
                        let mut pid = 0u32;
                        GetWindowThreadProcessId(hwnd, Some(&mut pid));
                        out.push((pid, String::from_utf16_lossy(&buf[..n])));
                    }
                }
            }
            BOOL(1)
        }
        // SAFETY: il callback usa il puntatore solo durante la chiamata.
        let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut found as *mut _ as isize)) };
        found
            .into_iter()
            .filter_map(|(pid, title)| exes.get(&pid).map(|exe| Window { exe: (*exe).into(), title }))
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Prova reale in sola lettura: stampa le sessioni multimediali e i
        /// titoli delle finestre dei browser. `cargo test -- --ignored --nocapture media`
        #[test]
        #[ignore]
        fn dump_current_sessions() {
            let (tx, _rx) = std::sync::mpsc::channel();
            let mut w = Watcher::new(tx).expect("gestore sessioni non disponibile");
            let procs = crate::procs::Scanner::default().snapshot().procs;
            let windows = browser_windows(&procs);
            for win in &windows {
                println!("finestra {}: {:?}", win.exe, win.title);
            }
            for s in w.sessions() {
                println!("{s:?}");
                println!("  -> {:?}", super::super::classify(s, &windows, &Default::default()));
            }
        }

        /// Per misurare la memoria: 10 s senza gestore, poi 30 s con il gestore
        /// attivo e in ascolto degli eventi (memoria privata letta dall'esterno).
        #[test]
        #[ignore]
        fn hold_watcher_for_measurement() {
            use std::time::{Duration, Instant};
            println!("pid {}", std::process::id());
            std::thread::sleep(Duration::from_secs(10));
            let (tx, rx) = std::sync::mpsc::channel();
            let mut w = Watcher::new(tx).expect("gestore sessioni non disponibile");
            let _ = w.sessions();
            let _ = browser_windows(&crate::procs::Scanner::default().snapshot().procs);
            println!("gestore attivo");
            let (start, mut events) = (Instant::now(), 0);
            while start.elapsed() < Duration::from_secs(30) {
                if rx.recv_timeout(Duration::from_secs(1)).is_ok() {
                    events += 1;
                    let _ = w.sessions();
                }
            }
            println!("eventi ricevuti: {events}");
        }
    }
}

#[cfg(not(windows))]
pub struct Watcher;

#[cfg(not(windows))]
impl Watcher {
    pub fn new(_wake: std::sync::mpsc::Sender<()>) -> Result<Self, String> {
        Err(crate::t!("err.unsupported"))
    }
    pub fn sessions(&mut self) -> &[Session] {
        &[]
    }
}

#[cfg(not(windows))]
pub fn browser_windows(_procs: &[crate::autoswitch::Proc]) -> Vec<Window> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(aumid: &str, title: &str) -> Session {
        Session { aumid: aumid.into(), playing: true, title: title.into(), ..Default::default() }
    }

    fn win(exe: &str, title: &str) -> Window {
        Window { exe: exe.into(), title: title.into() }
    }

    fn category(s: &Session, windows: &[Window]) -> Option<(Category, String)> {
        match classify(s, windows, &MediaConfig::default())? {
            Hit { verdict: Verdict::Category(c), label } => Some((c, label)),
            other => panic!("{other:?}"),
        }
    }

    fn music(label: &str) -> Option<(Category, String)> {
        Some((Category::Music, label.into()))
    }

    fn film(label: &str) -> Option<(Category, String)> {
        Some((Category::Film, label.into()))
    }

    /// App desktop e Store riconosciute dall'AUMID (nomi dei pacchetti).
    #[test]
    fn apps_by_aumid() {
        for (aumid, want) in [
            ("Spotify.exe", music("Spotify")),
            ("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", music("Spotify")),
            ("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App", music("Apple Music")),
            ("WiMPMusic.27241E05630EA_kn85bz84x7te4!TIDAL", music("TIDAL")),
            ("Deezer.62021768415AF_q7m17pa7q8kj0!Deezer.Music", music("Deezer")),
            ("AmazonMobileLLC.AmazonMusic_kc6t79cpj4tp0!AmazonMobileLLC.AmazonMusic", music("Amazon Music")),
            ("Chrome._crx_cinhimbnkkaeohfgghhklpknlkffjgod", music("YouTube Music")),
            ("MSEdge._crx_agimnkijcaahngcdmfeangaknmldooml", music("YouTube")),
            ("4DF9E0F8.Netflix_mcm4njqhnhss8!Netflix.App", film("Netflix")),
            ("Disney.37853FC22B2CE_6rarf9sa4v8jt!App", film("Disney+")),
            ("AmazonVideo.PrimeVideo_pwbj9vvecjh7j!App", film("Prime Video")),
            ("VideoLAN.VLC", film("VLC")),
            ("vlc.exe", film("VLC")),
            ("Microsoft.ZuneVideo_8wekyb3d8bbwe!Microsoft.ZuneVideo", film("Film e TV")),
            ("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic", film("Lettore multimediale")),
            ("Discord.exe", None),
            ("CAF9E577.Plex_aam28m9va5cke!App", None),
        ] {
            assert_eq!(category(&session(aumid, "Qualcosa"), &[]), want, "{aumid}");
        }
        // I lettori video con un brano audio passano a Musica.
        let mp3 = Session { kind: Kind::Music, ..session("Microsoft.ZuneMusic_8wekyb3d8bbwe!Microsoft.ZuneMusic", "Brano") };
        assert_eq!(category(&mp3, &[]), music("Lettore multimediale"));
        let vlc = Session { kind: Kind::Music, ..session("VideoLAN.VLC", "Brano") };
        assert_eq!(category(&vlc, &[]), music("VLC"));
        // YouTube resta Musica anche se il browser dichiara un video.
        let yt = Session { kind: Kind::Video, ..session("Chrome", "Video") };
        assert_eq!(category(&yt, &[win("chrome.exe", "Video - YouTube - Google Chrome")]), music("YouTube"));
        // Un'app non browser con "Netflix" nel titolo non conta.
        assert_eq!(category(&session("CAF9E577.Plex_x!App", "Netflix doc"), &[]), None);
    }

    #[test]
    fn browsers_are_recognized() {
        assert_eq!(browser_exe("Chrome"), Some("chrome.exe"));
        assert_eq!(browser_exe("MSEdge"), Some("msedge.exe"));
        assert_eq!(browser_exe("308046B0AF4A39CB"), Some("firefox.exe"));
        assert_eq!(browser_exe("Chrome._crx_abc"), Some("chrome.exe"));
        assert_eq!(browser_exe("Spotify.exe"), None);
        assert_eq!(browser_exe("ChromeCast.exe"), None);
    }

    /// Siti nel browser: dal titolo della finestra che contiene il titolo della
    /// sessione, con i formati di Chrome, Edge e Firefox.
    #[test]
    fn sites_by_window_title() {
        let other = win("chrome.exe", "Posta in arrivo - Gmail - Google Chrome");
        for (aumid, title, window, want) in [
            ("Chrome", "Lo-fi beats", win("chrome.exe", "Lo-fi beats - YouTube - Google Chrome"), music("YouTube")),
            ("Chrome", "Lo-fi beats", win("chrome.exe", "(3) Lo-fi beats - YouTube - Google Chrome"), music("YouTube")),
            (
                "MSEdge",
                "Video X",
                win("msedge.exe", "Video X - YouTube e altre 2 pagine - Personale - Microsoft\u{200b} Edge"),
                music("YouTube"),
            ),
            ("MSEdge", "Video X", win("msedge.exe", "Video X - YouTube and 2 more pages - Work - Microsoft Edge"), music("YouTube")),
            ("308046B0AF4A39CB", "Video X", win("firefox.exe", "Video X - YouTube — Mozilla Firefox"), music("YouTube")),
            ("Chrome", "Song", win("chrome.exe", "Song - YouTube Music - Google Chrome"), music("YouTube Music")),
            ("Chrome", "Loki", win("chrome.exe", "Watch Loki | Disney+ - Google Chrome"), film("Disney+")),
            ("MSEdge", "The Boys", win("msedge.exe", "Prime Video: The Boys - Microsoft Edge"), film("Prime Video")),
            ("Chrome", "Brano", win("chrome.exe", "Brano - Artista - Deezer - Google Chrome"), music("Deezer")),
            // Un trailer di Netflix su YouTube è YouTube, quindi Musica.
            ("Chrome", "Netflix trailer", win("chrome.exe", "Netflix trailer - YouTube - Google Chrome"), music("YouTube")),
            // Siti non riconosciuti: nessun cambio.
            ("MSEdge", "Corto", win("msedge.exe", "Corto - Vimeo - Microsoft Edge"), None),
            ("Chrome", "Tidal wave", win("chrome.exe", "Tidal wave - Vimeo - Google Chrome"), None),
        ] {
            assert_eq!(category(&session(aumid, title), &[other.clone(), window.clone()]), want, "{}", window.title);
        }
        // Le finestre di un altro browser non contano.
        assert_eq!(category(&session("MSEdge", "Lo-fi"), &[win("chrome.exe", "Lo-fi - YouTube - Google Chrome")]), None);
    }

    /// Netflix intitola la scheda solo "Netflix": se nessuna finestra contiene
    /// il titolo della sessione decidono le finestre del browser, ma solo se
    /// tutte indicano la stessa cosa.
    #[test]
    fn unmatched_title_uses_unanimous_windows() {
        let s = session("Chrome", "Stranger Things");
        let netflix = win("chrome.exe", "Netflix - Google Chrome");
        let mail = win("chrome.exe", "Posta in arrivo - Gmail - Google Chrome");
        let yt = win("chrome.exe", "Altro - YouTube - Google Chrome");
        assert_eq!(category(&s, &[netflix.clone(), mail]), film("Netflix"));
        assert_eq!(category(&s, &[netflix, yt]), None);
        assert_eq!(category(&s, &[]), None);
        // Formato letto davvero da Firefox (prova del 10/10/2026, testi anonimizzati):
        // AUMID esadecimale, PlaybackType "Music" anche per un video, artista =
        // canale, finestra intitolata solo "YouTube — Mozilla Firefox".
        let ff = Session { kind: Kind::Music, artist: "Nome del canale".into(), ..session("308046B0AF4A39CB", "Titolo del video") };
        assert_eq!(category(&ff, &[win("firefox.exe", "YouTube — Mozilla Firefox")]), music("YouTube"));
        // Dominio nei metadati (pagine senza Media Session).
        let s = Session { artist: "www.netflix.com".into(), ..session("Chrome", "Netflix") };
        assert_eq!(category(&s, &[]), film("Netflix"));
    }

    #[test]
    fn user_rules_win_over_builtins() {
        let cfg = MediaConfig {
            rules: vec![
                MediaRule { pattern: "twitch".into(), preset: FILM.into() },
                MediaRule { pattern: "youtube".into(), preset: IGNORE.into() },
                MediaRule { pattern: "spotify".into(), preset: "b:edm".into() },
                MediaRule { pattern: "discord".into(), preset: MUSIC.into() },
            ],
            ..Default::default()
        }
        .sanitized();
        let tw = session("Chrome", "Streamer");
        let windows = [win("chrome.exe", "Streamer - Twitch - Google Chrome"), win("chrome.exe", "Video - YouTube - Google Chrome")];
        let t = playing_target(&[tw], &windows, &cfg, true).unwrap();
        assert_eq!((t.preset.as_str(), t.exe.as_str(), t.game), ("b:film", "twitch", false));
        // "Ignora" su YouTube: nessun preset.
        assert_eq!(playing_target(&[session("Chrome", "Video")], &windows, &cfg, true), None);
        assert_eq!(playing_target(&[session("Spotify.exe", "x")], &[], &cfg, true).unwrap().preset, "b:edm");
        assert_eq!(playing_target(&[session("Discord.exe", "x")], &[], &cfg, false).unwrap().preset, "b:flat");
    }

    #[test]
    fn playing_target_prefers_current_and_ignores_paused() {
        let cfg = MediaConfig::default();
        let spotify = session("Spotify.exe", "Brano");
        let netflix = Session { current: true, ..session("4DF9E0F8.Netflix_mcm4njqhnhss8!Netflix.App", "Serie") };
        let t = playing_target(&[spotify.clone(), netflix.clone()], &[], &cfg, true).unwrap();
        assert_eq!((t.preset.as_str(), t.exe.as_str()), ("b:film", "Netflix"));
        let paused = Session { playing: false, ..netflix };
        let t = playing_target(&[spotify.clone(), paused.clone()], &[], &cfg, true).unwrap();
        assert_eq!((t.preset.as_str(), t.exe.as_str()), ("b:base", "Spotify"));
        assert_eq!(playing_target(&[spotify], &[], &cfg, false).unwrap().preset, "b:flat");
        assert_eq!(playing_target(&[paused], &[], &cfg, true), None);
        // Sessione del browser non riconosciuta come corrente: conta quella dopo.
        let unknown = Session { current: true, ..session("MSEdge", "Corto") };
        let deezer = session("Deezer.62021768415AF_q7m17pa7q8kj0!Deezer.Music", "Brano");
        assert_eq!(playing_target(&[unknown, deezer], &[], &cfg, true).unwrap().exe, "Deezer");
    }

    #[test]
    fn tracker_hysteresis() {
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_secs(s);
        let music = Target { preset: "b:base".into(), exe: "Spotify".into(), game: false };
        let film = Target { preset: "b:film".into(), exe: "Netflix".into(), game: false };
        let mut tr = Tracker::default();
        // Serve riproduzione continua per START_DELAY.
        assert_eq!(tr.update(at(0), Some(music.clone())), None);
        assert_eq!(tr.update(at(1), Some(music.clone())), None);
        assert_eq!(tr.update(at(2), Some(music.clone())), Some(music.clone()));
        // Pausa breve: il preset resta.
        assert_eq!(tr.update(at(5), None), Some(music.clone()));
        assert_eq!(tr.update(at(11), None), Some(music.clone()));
        assert_eq!(tr.update(at(12), Some(music.clone())), Some(music.clone()));
        // Passaggio a un film: cambia dopo START_DELAY, senza buchi nel frattempo.
        assert_eq!(tr.update(at(13), Some(film.clone())), Some(music.clone()));
        assert_eq!(tr.update(at(15), Some(film.clone())), Some(film.clone()));
        // Pausa lunga: dopo STOP_GRACE si torna al preset di prima.
        assert_eq!(tr.update(at(20), None), Some(film.clone()));
        assert_eq!(tr.update(at(25), None), None);
        // Un'anteprima di un secondo non cambia nulla.
        assert_eq!(tr.update(at(30), Some(music.clone())), None);
        assert_eq!(tr.update(at(31), None), None);
        assert_eq!(tr.update(at(33), Some(music)), None);
    }

    #[test]
    fn sanitized_config() {
        let c = MediaConfig {
            music_preset: " ".into(),
            film_preset: BASE_PRESET.into(),
            rules: vec![
                MediaRule { pattern: " Twitch ".into(), preset: FILM.into() },
                MediaRule { pattern: "twitch".into(), preset: MUSIC.into() },
                MediaRule { pattern: "".into(), preset: MUSIC.into() },
                MediaRule { pattern: "x".into(), preset: " ".into() },
            ],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(c.rules, vec![MediaRule { pattern: "twitch".into(), preset: FILM.into() }]);
        assert_eq!((c.music_preset.as_str(), c.film_preset.as_str()), (BASE_PRESET, "b:film"));
        assert_eq!(c.music_key(true), "b:base");
        assert_eq!(c.music_key(false), "b:flat");
        // Impostazioni salvate prima di questa funzione: attiva con i valori predefiniti.
        let old: crate::autoswitch::AutoConfig = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert_eq!(old.media, MediaConfig::default());
    }
}
