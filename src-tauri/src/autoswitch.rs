//! Attivazione automatica dei preset per app e giochi.
//!
//! Questo modulo contiene solo logica pura (testabile): la lettura dei
//! processi e della finestra in primo piano è in `procs.rs`.
//!
//! - [`Detector`] decide, da un'istantanea dei processi, quale preset
//!   vorrebbe attivo (regola utente o gioco rilevato).
//! - [`Engine`] applica quel preset alle impostazioni salvando lo stato
//!   precedente, e lo ripristina quando l'app o il gioco si chiude.

use crate::eq::EqProfile;
use crate::media::MediaConfig;
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Valore di `Rule::preset`: tratta l'eseguibile come un gioco.
pub const GAME: &str = "game";
/// Valore di `Rule::preset`: ignora l'eseguibile (non è mai un gioco).
pub const IGNORE: &str = "none";
const MAX_RULES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// Nome dell'eseguibile in minuscolo, es. "vlc.exe".
    pub exe: String,
    /// "b:<id>", "u:<nome>", [`GAME`] o [`IGNORE`].
    pub preset: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoConfig {
    pub enabled: bool,
    /// Notifica di Windows quando il preset cambia da solo.
    pub notify: bool,
    /// Preset da attivare quando parte un gioco.
    pub game_preset: String,
    /// Considera gioco anche un'app a schermo intero in primo piano.
    pub detect_fullscreen: bool,
    pub rules: Vec<Rule>,
    /// Film e musica riconosciuti dalla sessione multimediale di Windows.
    pub media: MediaConfig,
}

impl Default for AutoConfig {
    fn default() -> Self {
        AutoConfig {
            enabled: false,
            notify: true,
            game_preset: "b:gaming_fps".into(),
            detect_fullscreen: true,
            rules: Vec::new(),
            media: MediaConfig::default(),
        }
    }
}

/// Stato da ripristinare quando l'app o il gioco che ha attivato il preset si
/// chiude. Salvato nelle impostazioni: sopravvive a un riavvio dell'app.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoRestore {
    pub profile: EqProfile,
    pub active_preset: Option<String>,
    /// Preset applicato automaticamente.
    pub applied: String,
}

/// Normalizza un nome di eseguibile: solo il nome file, minuscolo, ".exe"
/// aggiunto se manca un'estensione. `None` se non è un nome valido.
pub fn normalize_exe(input: &str) -> Option<String> {
    let name = input.trim().rsplit(['\\', '/']).next()?.trim().to_lowercase();
    if name.is_empty() || name.len() > 120 || name.chars().any(|c| c.is_control() || "<>:\"|?*".contains(c)) {
        return None;
    }
    Some(if name.contains('.') { name } else { format!("{name}.exe") })
}

impl AutoConfig {
    /// Ripulisce la configurazione arrivata dal frontend: nomi normalizzati,
    /// niente duplicati (vince la prima regola), preset non vuoti.
    pub fn sanitized(mut self) -> Self {
        let mut seen = HashSet::new();
        self.rules = self
            .rules
            .into_iter()
            .filter_map(|r| {
                let exe = normalize_exe(&r.exe)?;
                let preset = r.preset.trim().to_string();
                (!preset.is_empty() && seen.insert(exe.clone())).then_some(Rule { exe, preset })
            })
            .take(MAX_RULES)
            .collect();
        self.game_preset = self.game_preset.trim().to_string();
        if self.game_preset.is_empty() || self.game_preset == GAME || self.game_preset == IGNORE {
            self.game_preset = AutoConfig::default().game_preset;
        }
        self.media = self.media.sanitized();
        self
    }

    fn rule_for(&self, exe: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.exe == exe)
    }
}

// ---------------------------------------------------------------- rilevamento

/// Processo in esecuzione. `exe` e `path` sono in minuscolo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    pub exe: String,
    /// Percorso completo; `None` se Windows non lo concede (processi protetti).
    pub path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Foreground {
    pub pid: u32,
    /// Finestra senza barra del titolo che copre tutto il monitor
    /// (schermo intero esclusivo o borderless).
    pub fullscreen: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub procs: Vec<Proc>,
    pub foreground: Option<Foreground>,
}

/// Cartelle delle librerie dei launcher: un eseguibile qui dentro è un gioco.
const LIBRARY_DIRS: &[&str] = &[
    r"\steamapps\common\",
    r"\epic games\",
    r"\gog galaxy\games\",
    r"\gog games\",
    r"\ea games\",
    r"\origin games\",
    r"\ubisoft game launcher\games\",
    r"\xboxgames\",
    r"\riot games\",
];

/// Sottocartelle delle librerie che contengono strumenti, non giochi.
const LIBRARY_EXCLUDED: &[&str] = &[
    r"\steamapps\common\steamworks shared\",
    r"\steamapps\common\steam controller configs\",
    r"\steamapps\common\wallpaper_engine\",
    r"\epic games\launcher\",
    r"\epic games\epic online services\",
    r"\riot games\riot client\",
];

/// Launcher: i processi figli che stanno fuori dalle cartelle dei launcher
/// sono giochi (copre Battle.net ed EA, che installano i giochi dove capita).
const LAUNCHERS: &[&str] = &[
    "steam.exe",
    "epicgameslauncher.exe",
    "battle.net.exe",
    "eadesktop.exe",
    "ealauncher.exe",
    "origin.exe",
    "upc.exe",
    "ubisoftconnect.exe",
    "galaxyclient.exe",
    "riotclientservices.exe",
];

/// Avvia i giochi Xbox / Microsoft Store (anche in WindowsApps) dalla loro
/// stessa cartella: i suoi figli sono sempre giochi.
const XBOX_LAUNCH_HELPER: &str = "gamelaunchhelper.exe";

/// Cartelle dei launcher e dei loro servizi: i figli che stanno qui sono
/// helper (browser integrati, aggiornamenti, Battle.net Agent…), non giochi.
const LAUNCHER_DIRS: &[&str] = &[
    r"\steam\",
    r"\battle.net\",
    r"\epic games\launcher\",
    r"\epic online services\",
    r"\gog galaxy\",
    r"\electronic arts\",
    r"\origin\",
    r"\ubisoft game launcher\",
    r"\riot client\",
    r"\riot vanguard\",
];

/// Programmi che non vanno mai considerati giochi, nemmeno a schermo intero
/// o se aperti da un launcher (browser, lettori video, shell di Windows…).
const NOT_GAMES: &[&str] = &[
    "chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe", "iexplore.exe",
    "arc.exe", "vlc.exe", "mpc-hc.exe", "mpc-hc64.exe", "mpc-be.exe", "mpc-be64.exe", "potplayer.exe",
    "potplayermini.exe", "potplayermini64.exe", "mpv.exe", "wmplayer.exe", "kodi.exe", "plex.exe",
    "plex htpc.exe", "spotify.exe", "explorer.exe", "applicationframehost.exe", "wwahost.exe",
    "lockapp.exe", "searchhost.exe", "shellexperiencehost.exe", "startmenuexperiencehost.exe",
    "textinputhost.exe", "dwm.exe", "logonui.exe", "mstsc.exe", "msrdc.exe", "powerpnt.exe",
    "obs64.exe", "discord.exe", "steamwebhelper.exe", "epicwebhelper.exe",
    // Lobby di League of Legends: spesso resta aperta; la partita è "league of legends.exe".
    "leagueclient.exe", "leagueclientux.exe", "leagueclientuxrender.exe",
];

/// Parti di nome tipiche di strumenti di supporto che vivono nelle cartelle
/// dei giochi (crash reporter, installer, anti-cheat…).
const HELPER_HINTS: &[&str] =
    &["crash", "report", "unins", "setup", "redist", "installer", "updater", "anticheat", "beservice"];

fn is_excluded(p: &Proc) -> bool {
    NOT_GAMES.contains(&p.exe.as_str())
        || crate::brand::EXE_NAMES.contains(&p.exe.as_str())
        || HELPER_HINTS.iter().any(|h| p.exe.contains(h))
        || p.path.as_deref().is_some_and(|path| path.starts_with(r"c:\windows\"))
}

/// Eseguibile dentro la libreria di un launcher (Steam, Epic, GOG, EA, Ubisoft, Xbox).
pub fn in_game_library(path: &str) -> bool {
    LIBRARY_DIRS.iter().any(|d| path.contains(d)) && !LIBRARY_EXCLUDED.iter().any(|d| path.contains(d))
}

fn parent_dir(path: &str) -> &str {
    path.rfind('\\').map_or(path, |i| &path[..=i])
}

/// `path` è stato avviato da `parent`, che è un launcher, e non è un suo helper.
fn launched_by_launcher(path: &str, parent: &Proc) -> bool {
    if parent.exe == XBOX_LAUNCH_HELPER {
        return true;
    }
    LAUNCHERS.contains(&parent.exe.as_str())
        && !LAUNCHER_DIRS.iter().any(|d| path.contains(d))
        && parent.path.as_deref().map_or(true, |lp| !path.starts_with(parent_dir(lp)))
}

/// Preset desiderato e chi lo ha richiesto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub preset: String,
    /// Eseguibile, oppure app o sito riconosciuto dalla sessione multimediale.
    pub exe: String,
    /// Richiesto da un gioco (rilevato o con regola "Trattalo come gioco").
    pub game: bool,
}

/// Priorità tra le fonti: un gioco vince su film e musica, che vincono sulle
/// altre regole per eseguibile. Senza film o musica resta l'ordine del
/// [`Detector`] (app in primo piano, poi regole, poi giochi).
pub fn pick(apps: Vec<Target>, media: Option<Target>) -> Option<Target> {
    match media {
        Some(m) => apps.into_iter().find(|t| t.game).or(Some(m)),
        None => apps.into_iter().next(),
    }
}

/// Rilevatore con memoria: un processo riconosciuto come gioco perché a
/// schermo intero resta un gioco finché non si chiude (anche se l'utente
/// passa ad altre finestre con Alt+Tab).
#[derive(Debug, Default)]
pub struct Detector {
    fullscreen_games: HashSet<u32>,
}

impl Detector {
    /// Il preset più prioritario (usato dai test).
    #[cfg(test)]
    pub fn desired(&mut self, snap: &Snapshot, cfg: &AutoConfig) -> Option<Target> {
        self.candidates(snap, cfg).into_iter().next()
    }

    /// Tutti i preset richiesti, dal più prioritario.
    pub fn candidates(&mut self, snap: &Snapshot, cfg: &AutoConfig) -> Vec<Target> {
        let by_pid: HashMap<u32, &Proc> = snap.procs.iter().map(|p| (p.pid, p)).collect();
        self.fullscreen_games.retain(|pid| by_pid.contains_key(pid));

        if let Some(fg) = snap.foreground.filter(|f| f.fullscreen && cfg.detect_fullscreen) {
            if let Some(p) = by_pid.get(&fg.pid) {
                if cfg.rule_for(&p.exe).is_none() && !is_excluded(p) {
                    self.fullscreen_games.insert(p.pid);
                }
            }
        }

        let base_game = |p: &Proc| -> bool {
            if is_excluded(p) {
                return false;
            }
            if self.fullscreen_games.contains(&p.pid) {
                return true;
            }
            let Some(path) = p.path.as_deref() else { return false };
            in_game_library(path) || by_pid.get(&p.parent).is_some_and(|parent| launched_by_launcher(path, parent))
        };
        let games: HashSet<u32> = snap.procs.iter().filter(|p| base_game(p)).map(|p| p.pid).collect();
        // Processi avviati da un gioco (es. il bootstrapper che lancia l'eseguibile vero).
        let is_game = |p: &Proc| games.contains(&p.pid) || (games.contains(&p.parent) && !is_excluded(p));

        // (posizione della regola o usize::MAX per i giochi, pid, target)
        let mut candidates: Vec<(usize, u32, Target)> = Vec::new();
        for p in &snap.procs {
            let rule = cfg.rules.iter().position(|r| r.exe == p.exe);
            let (rank, preset, game) = match rule.map(|i| (i, cfg.rules[i].preset.as_str())) {
                Some((_, IGNORE)) => continue,
                Some((i, GAME)) => (i, cfg.game_preset.as_str(), true),
                Some((i, preset)) => (i, preset, false),
                None if is_game(p) => (usize::MAX, cfg.game_preset.as_str(), true),
                None => continue,
            };
            candidates.push((rank, p.pid, Target { preset: preset.to_string(), exe: p.exe.clone(), game }));
        }
        // Vince l'app in primo piano; altrimenti le regole nell'ordine dato
        // dall'utente, poi i giochi.
        let fg_pid = snap.foreground.map(|f| f.pid);
        candidates.sort_by_key(|(rank, pid, _)| (Some(*pid) != fg_pid, *rank, *pid));
        candidates.into_iter().map(|(_, _, t)| t).collect()
    }
}

// ---------------------------------------------------------------- applicazione

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Attivato `preset` (nome leggibile) perché è in esecuzione `exe`.
    Applied { preset: String, exe: String },
    /// Ripristinato il preset precedente.
    Restored { preset: String },
}

/// Macchina a stati attivazione/ripristino. Lo stato da ripristinare vive in
/// `Settings::auto_restore`; qui resta solo il preset che l'utente ha
/// cambiato a mano (per non ri-applicarlo finché la situazione non cambia).
#[derive(Debug, Default)]
pub struct Engine {
    overridden: Option<String>,
}

impl Engine {
    /// Fa un passo: `desired` è il risultato di [`Detector::desired`] (o
    /// `None` se la funzione è disattivata), `resolve` trasforma una chiave di
    /// preset in profilo. Modifica `s` e restituisce cosa è cambiato.
    pub fn step(
        &mut self,
        s: &mut Settings,
        desired: Option<&Target>,
        resolve: impl Fn(&str, &Settings) -> Option<EqProfile>,
    ) -> Option<Change> {
        // L'utente ha scelto un altro preset mentre era attivo quello
        // automatico: rispetta la scelta, niente ripristino.
        if let Some(r) = &s.auto_restore {
            if s.active_preset.as_deref() != Some(r.applied.as_str()) {
                self.overridden = Some(r.applied.clone());
                s.auto_restore = None;
                return None;
            }
        }
        if self.overridden.is_some() && desired.map(|t| &t.preset) != self.overridden.as_ref() {
            self.overridden = None;
        }
        let want = desired.filter(|t| self.overridden.as_ref() != Some(&t.preset));

        match (want, s.auto_restore.is_some()) {
            (Some(t), active) => {
                let current = if active { s.auto_restore.as_ref().map(|r| r.applied.as_str()) } else { s.active_preset.as_deref() };
                if current == Some(t.preset.as_str()) {
                    return None;
                }
                let profile = resolve(&t.preset, s)?;
                match s.auto_restore.as_mut() {
                    Some(r) => r.applied = t.preset.clone(),
                    None => {
                        s.auto_restore = Some(AutoRestore {
                            profile: s.profile.clone(),
                            active_preset: s.active_preset.clone(),
                            applied: t.preset.clone(),
                        })
                    }
                }
                let name = profile.name.clone();
                s.profile = profile;
                s.active_preset = Some(t.preset.clone());
                Some(Change::Applied { preset: name, exe: t.exe.clone() })
            }
            (None, true) => {
                let r = s.auto_restore.take()?;
                // I preset integrati si rileggono: la base ANC può essere
                // cambiata mentre era attivo il preset automatico.
                let builtin = r.active_preset.as_deref().filter(|k| k.starts_with("b:")).and_then(|k| resolve(k, s));
                s.profile = builtin.unwrap_or(r.profile);
                s.active_preset = r.active_preset;
                Some(Change::Restored { preset: s.profile.name.clone() })
            }
            (None, false) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: u32, path: &str) -> Proc {
        let path = path.to_lowercase();
        Proc { pid, parent, exe: path.rsplit('\\').next().unwrap().to_string(), path: Some(path) }
    }

    fn cfg(rules: &[(&str, &str)]) -> AutoConfig {
        AutoConfig {
            enabled: true,
            rules: rules.iter().map(|(e, p)| Rule { exe: e.to_string(), preset: p.to_string() }).collect(),
            ..Default::default()
        }
    }

    fn snap(procs: Vec<Proc>, fg: Option<(u32, bool)>) -> Snapshot {
        Snapshot { procs, foreground: fg.map(|(pid, fullscreen)| Foreground { pid, fullscreen }) }
    }

    fn desired(procs: Vec<Proc>, fg: Option<(u32, bool)>, c: &AutoConfig) -> Option<(String, String)> {
        Detector::default().desired(&snap(procs, fg), c).map(|t| (t.preset, t.exe))
    }

    #[test]
    fn normalize_exe_names() {
        assert_eq!(normalize_exe("  VLC.exe "), Some("vlc.exe".into()));
        assert_eq!(normalize_exe(r"C:\Program Files\VideoLAN\VLC\vlc.exe"), Some("vlc.exe".into()));
        assert_eq!(normalize_exe("Cyberpunk2077"), Some("cyberpunk2077.exe".into()));
        assert_eq!(normalize_exe("   "), None);
        assert_eq!(normalize_exe("a|b.exe"), None);
    }

    #[test]
    fn sanitized_dedupes_rules_and_fixes_game_preset() {
        let c = AutoConfig {
            game_preset: " ".into(),
            rules: vec![
                Rule { exe: "VLC".into(), preset: "b:film".into() },
                Rule { exe: "vlc.exe".into(), preset: "b:night".into() },
                Rule { exe: "x.exe".into(), preset: "".into() },
            ],
            ..Default::default()
        }
        .sanitized();
        assert_eq!(c.rules, vec![Rule { exe: "vlc.exe".into(), preset: "b:film".into() }]);
        assert_eq!(c.game_preset, "b:gaming_fps");
    }

    #[test]
    fn library_paths_are_games_but_tools_are_not() {
        assert!(in_game_library(r"d:\steamlibrary\steamapps\common\counter-strike global offensive\game\bin\win64\cs2.exe"));
        assert!(in_game_library(r"c:\program files\epic games\fortnite\fortniteclient-win64-shipping.exe"));
        assert!(in_game_library(r"c:\xboxgames\forza horizon 5\content\forzahorizon5.exe"));
        assert!(in_game_library(r"c:\program files (x86)\ubisoft\ubisoft game launcher\games\far cry 6\bin\farcry6.exe"));
        assert!(!in_game_library(r"c:\program files (x86)\steam\steamapps\common\wallpaper_engine\wallpaper64.exe"));
        assert!(!in_game_library(r"c:\program files (x86)\epic games\launcher\portal\binaries\win64\epicgameslauncher.exe"));
        assert!(!in_game_library(r"c:\program files\videolan\vlc\vlc.exe"));
    }

    #[test]
    fn detects_games_by_library_launcher_parent_and_children() {
        let c = cfg(&[]);
        let steam_game = proc(10, 1, r"D:\SteamLibrary\steamapps\common\Hades\Hades.exe");
        assert_eq!(desired(vec![steam_game], None, &c), Some(("b:gaming_fps".into(), "hades.exe".into())));

        // Battle.net installa dove vuole: conta il processo padre.
        let bnet = proc(5, 1, r"C:\Program Files (x86)\Battle.net\Battle.net.exe");
        let ow = proc(6, 5, r"C:\Program Files (x86)\Overwatch\_retail_\Overwatch.exe");
        let helper = proc(7, 5, r"C:\Program Files (x86)\Battle.net\Battle.net Helper.exe");
        assert_eq!(desired(vec![bnet.clone(), ow.clone()], None, &c).unwrap().1, "overwatch.exe");
        assert_eq!(desired(vec![bnet.clone(), helper], None, &c), None);
        // Helper dei launcher fuori dalla cartella dell'eseguibile: non sono giochi.
        let agent = proc(9, 5, r"C:\ProgramData\Battle.net\Agent\Agent.9321\Agent.exe");
        assert_eq!(desired(vec![bnet.clone(), agent], None, &c), None);
        let epic = proc(11, 1, r"C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe");
        let cef = proc(12, 11, r"C:\Program Files (x86)\Epic Games\Launcher\Engine\Binaries\Win64\UnrealCEFSubProcess.exe");
        assert_eq!(desired(vec![epic, cef], None, &c), None);
        // Riot: libreria propria e client come launcher.
        let riot = proc(13, 1, r"C:\Riot Games\Riot Client\RiotClientServices.exe");
        let valorant = proc(14, 13, r"C:\Riot Games\VALORANT\live\VALORANT.exe");
        assert_eq!(desired(vec![riot.clone()], None, &c), None);
        assert_eq!(desired(vec![riot, valorant], None, &c).unwrap().1, "valorant.exe");
        let lobby = proc(17, 1, r"C:\Riot Games\League of Legends\LeagueClient.exe");
        let lol = proc(18, 17, r"C:\Riot Games\League of Legends\Game\League of Legends.exe");
        assert_eq!(desired(vec![lobby.clone()], None, &c), None);
        assert_eq!(desired(vec![lobby, lol], None, &c).unwrap().1, "league of legends.exe");
        // Giochi Xbox / Store: gamelaunchhelper avvia l'eseguibile dalla stessa cartella.
        let helper_x = proc(15, 1, r"C:\Program Files\WindowsApps\Game_1.0_x64__abc\gamelaunchhelper.exe");
        let game_x = proc(16, 15, r"C:\Program Files\WindowsApps\Game_1.0_x64__abc\Game.exe");
        assert_eq!(desired(vec![helper_x, game_x], None, &c).unwrap().1, "game.exe");
        // Un browser aperto da un link nel launcher non è un gioco.
        let chrome = proc(8, 5, r"C:\Program Files\Google\Chrome\Application\chrome.exe");
        assert_eq!(desired(vec![bnet, chrome], None, &c), None);

        // Figlio di un gioco (bootstrapper → eseguibile vero), crash reporter escluso.
        let boot = proc(20, 1, r"C:\XboxGames\Starfield\Content\Starfield.exe");
        let child = proc(21, 20, r"C:\Users\x\AppData\Local\Temp\real_game.exe");
        let crash = proc(22, 20, r"C:\Users\x\AppData\Local\Temp\CrashReportClient.exe");
        let d = Detector::default().desired(&snap(vec![child.clone(), crash.clone()], None), &c);
        assert_eq!(d, None, "senza il padre non sono giochi");
        let mut det = Detector::default();
        let s = snap(vec![boot, crash, child], Some((21, false)));
        assert_eq!(det.desired(&s, &c).unwrap().exe, "real_game.exe");

        // Processi di sistema e strumenti: niente.
        let sys = vec![proc(1, 0, r"C:\Windows\explorer.exe"), proc(2, 1, r"C:\Program Files\VideoLAN\VLC\vlc.exe")];
        assert_eq!(desired(sys, Some((2, true)), &c), None);
    }

    #[test]
    fn fullscreen_detection_is_sticky_and_optional() {
        let game = proc(30, 1, r"C:\Games\Indie\indie.exe");
        let discord = proc(31, 1, r"C:\Users\x\AppData\Local\Discord\Discord.exe");
        let c = cfg(&[]);
        let mut det = Detector::default();
        // In finestra: non è riconoscibile.
        assert_eq!(det.desired(&snap(vec![game.clone(), discord.clone()], Some((30, false))), &c), None);
        // A schermo intero sì, e resta gioco anche dopo Alt+Tab.
        assert!(det.desired(&snap(vec![game.clone(), discord.clone()], Some((30, true))), &c).is_some());
        assert_eq!(det.desired(&snap(vec![game.clone(), discord.clone()], Some((31, false))), &c).unwrap().exe, "indie.exe");
        // Chiuso il gioco, il pid esce dalla memoria.
        assert_eq!(det.desired(&snap(vec![discord.clone()], Some((31, false))), &c), None);
        assert_eq!(det.desired(&snap(vec![game.clone()], Some((30, false))), &c), None);
        // Con l'euristica disattivata lo schermo intero non conta.
        let off = AutoConfig { detect_fullscreen: false, ..cfg(&[]) };
        assert_eq!(Detector::default().desired(&snap(vec![game], Some((30, true))), &off), None);
        // Browser a schermo intero (video) non è un gioco.
        let edge = proc(40, 1, r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe");
        assert_eq!(Detector::default().desired(&snap(vec![edge], Some((40, true))), &c), None);
    }

    #[test]
    fn rules_override_heuristics_and_foreground_wins() {
        let vlc = proc(50, 1, r"C:\Program Files\VideoLAN\VLC\vlc.exe");
        let game = proc(51, 1, r"D:\SteamLibrary\steamapps\common\Doom\doom.exe");
        let tool = proc(52, 1, r"C:\Tools\mygame.exe");
        let c = cfg(&[("vlc.exe", "b:film"), ("doom.exe", IGNORE), ("mygame.exe", GAME)]);

        // Regola "Ignora" batte la libreria Steam; regola "Gioco" usa il preset gaming.
        assert_eq!(desired(vec![game.clone()], None, &c), None);
        assert_eq!(desired(vec![tool.clone()], None, &c), Some(("b:gaming_fps".into(), "mygame.exe".into())));
        // Senza primo piano rilevante vincono le regole nel loro ordine.
        assert_eq!(desired(vec![tool.clone(), vlc.clone()], None, &c).unwrap().0, "b:film");
        // L'app in primo piano vince sull'ordine.
        assert_eq!(desired(vec![tool.clone(), vlc.clone()], Some((52, false)), &c).unwrap().0, "b:gaming_fps");
        // Regola utente sul nome anche senza percorso (processo protetto).
        let protected = Proc { pid: 60, parent: 1, exe: "vlc.exe".into(), path: None };
        assert_eq!(desired(vec![protected], None, &c).unwrap().0, "b:film");
        // Un gioco della libreria senza regole usa il preset gaming scelto.
        let c2 = AutoConfig { game_preset: "b:gaming".into(), ..cfg(&[("vlc.exe", "b:film")]) };
        let game2 = proc(53, 1, r"D:\SteamLibrary\steamapps\common\Portal 2\portal2.exe");
        assert_eq!(desired(vec![vlc, game2], Some((53, true)), &c2).unwrap().0, "b:gaming");
    }

    // ------------------------------------------------------------ ripristino

    fn resolver(key: &str, _: &Settings) -> Option<EqProfile> {
        (key != "b:missing").then(|| EqProfile { name: format!("P {key}"), ..Default::default() })
    }

    fn user_state(name: &str) -> Settings {
        Settings {
            profile: EqProfile { name: name.into(), preamp_manual: -2.0, ..Default::default() },
            active_preset: Some(format!("u:{name}")),
            ..Default::default()
        }
    }

    fn t(preset: &str, exe: &str) -> Target {
        Target { preset: preset.into(), exe: exe.into(), game: false }
    }

    #[test]
    fn game_beats_media_which_beats_exe_rules() {
        let game = Target { game: true, ..t("b:gaming_fps", "cs2.exe") };
        let rule = t("b:calls", "discord.exe");
        let music = t("b:base", "Spotify");
        // Gioco > film/musica, anche se l'app con regola è in primo piano.
        assert_eq!(pick(vec![rule.clone(), game.clone()], Some(music.clone())), Some(game.clone()));
        // Film/musica > regole per eseguibile.
        assert_eq!(pick(vec![rule.clone()], Some(music.clone())), Some(music.clone()));
        assert_eq!(pick(vec![], Some(music.clone())), Some(music));
        // Senza riproduzione resta l'ordine del Detector; nulla → preset manuale.
        assert_eq!(pick(vec![rule.clone(), game], None), Some(rule));
        assert_eq!(pick(vec![], None), None);
    }

    #[test]
    fn detector_marks_games() {
        let c = cfg(&[("vlc.exe", "b:film"), ("mygame.exe", GAME)]);
        let procs = vec![
            proc(1, 0, r"C:\Program Files\VideoLAN\VLC\vlc.exe"),
            proc(2, 0, r"C:\Tools\mygame.exe"),
            proc(3, 0, r"D:\SteamLibrary\steamapps\common\Hades\Hades.exe"),
        ];
        let all = Detector::default().candidates(&snap(procs, None), &c);
        let flags: Vec<(&str, bool)> = all.iter().map(|t| (t.exe.as_str(), t.game)).collect();
        assert_eq!(flags, vec![("vlc.exe", false), ("mygame.exe", true), ("hades.exe", true)]);
    }

    /// Gioco e musica insieme: alla chiusura del gioco resta la musica, alla
    /// fine della musica torna il preset scelto a mano.
    #[test]
    fn media_and_games_share_the_same_restore() {
        let mut s = user_state("Mio");
        let mut e = Engine::default();
        let game = Target { game: true, ..t("b:gaming_fps", "cs2.exe") };
        let music = t("b:base", "Spotify");
        e.step(&mut s, pick(vec![], Some(music.clone())).as_ref(), resolver);
        assert_eq!(s.active_preset.as_deref(), Some("b:base"));
        e.step(&mut s, pick(vec![game.clone()], Some(music.clone())).as_ref(), resolver);
        assert_eq!(s.active_preset.as_deref(), Some("b:gaming_fps"));
        e.step(&mut s, pick(vec![], Some(music)).as_ref(), resolver);
        assert_eq!(s.active_preset.as_deref(), Some("b:base"));
        let c = e.step(&mut s, pick(vec![], None).as_ref(), resolver);
        assert_eq!(c, Some(Change::Restored { preset: "Mio".into() }));
        assert_eq!(s.active_preset.as_deref(), Some("u:Mio"));
    }

    #[test]
    fn applies_then_restores_previous_preset() {
        let mut s = user_state("Mio");
        let original = s.profile.clone();
        let mut e = Engine::default();
        let game = t("b:gaming_fps", "cs2.exe");

        let c = e.step(&mut s, Some(&game), resolver);
        assert_eq!(c, Some(Change::Applied { preset: "P b:gaming_fps".into(), exe: "cs2.exe".into() }));
        assert_eq!(s.active_preset.as_deref(), Some("b:gaming_fps"));
        assert_eq!(s.auto_restore.as_ref().unwrap().profile, original);
        // Stato stabile: nessuna modifica ai giri successivi.
        assert_eq!(e.step(&mut s, Some(&game), resolver), None);

        let c = e.step(&mut s, None, resolver);
        assert_eq!(c, Some(Change::Restored { preset: "Mio".into() }));
        assert_eq!(s.profile, original);
        assert_eq!(s.active_preset.as_deref(), Some("u:Mio"));
        assert!(s.auto_restore.is_none());
        assert_eq!(e.step(&mut s, None, resolver), None);
    }

    #[test]
    fn switching_between_targets_keeps_the_original_state() {
        let mut s = user_state("Mio");
        let mut e = Engine::default();
        e.step(&mut s, Some(&t("b:film", "vlc.exe")), resolver);
        let c = e.step(&mut s, Some(&t("b:gaming", "game.exe")), resolver);
        assert_eq!(c, Some(Change::Applied { preset: "P b:gaming".into(), exe: "game.exe".into() }));
        assert_eq!(s.auto_restore.as_ref().unwrap().active_preset.as_deref(), Some("u:Mio"));
        e.step(&mut s, None, resolver);
        assert_eq!(s.active_preset.as_deref(), Some("u:Mio"));
    }

    #[test]
    fn manual_change_during_auto_is_respected() {
        let mut s = user_state("Mio");
        let mut e = Engine::default();
        let game = t("b:gaming_fps", "cs2.exe");
        e.step(&mut s, Some(&game), resolver);
        // L'utente sceglie un altro preset mentre gioca.
        s.active_preset = Some("b:flat".into());
        assert_eq!(e.step(&mut s, Some(&game), resolver), None);
        assert!(s.auto_restore.is_none());
        // Non viene ri-applicato finché il gioco resta aperto…
        assert_eq!(e.step(&mut s, Some(&game), resolver), None);
        assert_eq!(s.active_preset.as_deref(), Some("b:flat"));
        // …e alla chiusura non si torna al preset di prima.
        assert_eq!(e.step(&mut s, None, resolver), None);
        assert_eq!(s.active_preset.as_deref(), Some("b:flat"));
        // Al gioco successivo l'attivazione automatica riparte.
        assert!(e.step(&mut s, Some(&game), resolver).is_some());
    }

    #[test]
    fn edge_cases_do_nothing() {
        let mut e = Engine::default();
        // Preset desiderato già attivo a mano: niente da fare né da ripristinare.
        let mut s = user_state("Mio");
        s.active_preset = Some("b:film".into());
        assert_eq!(e.step(&mut s, Some(&t("b:film", "vlc.exe")), resolver), None);
        assert!(s.auto_restore.is_none());
        // Preset non risolvibile (es. preset utente eliminato): ignorato.
        let mut s = user_state("Mio");
        assert_eq!(e.step(&mut s, Some(&t("b:missing", "x.exe")), resolver), None);
        assert_eq!(s.active_preset.as_deref(), Some("u:Mio"));
        assert!(s.auto_restore.is_none());
    }

    #[test]
    fn restore_survives_restart() {
        let mut s = user_state("Mio");
        Engine::default().step(&mut s, Some(&t("b:film", "vlc.exe")), resolver);
        let json = serde_json::to_string(&s).unwrap();
        let mut reloaded: Settings = serde_json::from_str(&json).unwrap();
        // Nuovo processo, nuovo Engine: VLC è stato chiuso nel frattempo.
        let c = Engine::default().step(&mut reloaded, None, resolver);
        assert_eq!(c, Some(Change::Restored { preset: "Mio".into() }));
        assert_eq!(reloaded.active_preset.as_deref(), Some("u:Mio"));
    }
}
