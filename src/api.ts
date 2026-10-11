// Tipi condivisi con il backend Rust e wrapper dei comandi Tauri.
import { invoke } from "@tauri-apps/api/core";

export type FilterKind = "PK" | "LSC" | "HSC";

export interface Filter {
  kind: FilterKind;
  freq: number;
  gain: number;
  q: number;
  enabled: boolean;
}

export interface EqProfile {
  name: string;
  filters: Filter[];
  graphic: number[];
  preamp_auto: boolean;
  preamp_manual: number;
  source?: string | null;
}

export interface BuiltinPreset {
  id: string;
  group: string;
  measured: boolean;
  description: string;
  profile: EqProfile;
}

export interface Headset {
  id: string;
  name: string;
  address?: string | null;
  connected: boolean;
  battery?: number | null;
}

export interface ApoStatus {
  installed: boolean;
  install_path?: string | null;
  config_path?: string | null;
  included: boolean;
  download_url: string;
}

export interface OutputDevice {
  id: string;
  name: string;
}

export interface Variant {
  id: string;
  label: string;
  filters: Filter[];
  source?: string | null;
}

export interface HeadphoneProfile {
  name: string;
  variants: Variant[];
}

export interface OutputInfo {
  id: string;
  name: string;
  /** Nome del dispositivo Bluetooth accoppiato, se l'uscita è Bluetooth. */
  bluetooth?: string | null;
}

export interface CandidateVariant {
  label: string;
  path: string;
}

/** Risultato di una ricerca nell'indice AutoEq. */
export interface Candidate {
  name: string;
  source: string;
  form: string;
  variants: CandidateVariant[];
}

export interface Settings {
  setup_done: boolean;
  output?: OutputDevice | null;
  headphone?: HeadphoneProfile | null;
  variant: string;
  name_patterns: string[];
  apo_device: string;
  start_minimized: boolean;
  default_on_connect: boolean;
  bypass: boolean;
  profile: EqProfile;
  active_preset?: string | null;
  user_presets: EqProfile[];
  /** Stesso preamp per tutti i preset con preamp automatico. */
  uniform_volume: boolean;
  /** "auto" (lingua di Windows) oppure il codice di una lingua disponibile. */
  language: string;
  auto: AutoConfig;
}

/** Valori speciali di `Rule.preset` oltre a "b:<id>" e "u:<nome>". */
export const RULE_GAME = "game";
export const RULE_IGNORE = "none";

export interface Rule {
  exe: string;
  preset: string;
}

/** Valori speciali di `MediaRule.preset` e `MediaConfig.music_preset`. */
export const MEDIA_MUSIC = "music";
export const MEDIA_FILM = "film";
/** Preset misurato del modello scelto (chiave "b:base"). */
export const BASE_PRESET = "base";

export interface MediaRule {
  pattern: string;
  preset: string;
}

export interface MediaConfig {
  enabled: boolean;
  music_preset: string;
  film_preset: string;
  rules: MediaRule[];
}

export interface AutoConfig {
  enabled: boolean;
  notify: boolean;
  game_preset: string;
  detect_fullscreen: boolean;
  rules: Rule[];
  media: MediaConfig;
}

export interface AutoStatus {
  exe: string;
  preset: string;
}

export interface Autostart {
  enabled: boolean;
  dev_build: boolean;
  others: string[];
}

export interface InitState {
  version: string;
  /** Percorso dell'eseguibile se è una build di sviluppo. */
  dev_build?: string | null;
  settings: Settings;
  apo: ApoStatus;
  presets: BuiltinPreset[];
  headsets: Headset[];
  auto_status?: AutoStatus | null;
  product: string;
  /** Lingua in uso (codice già risolto, mai "auto"). */
  lang: string;
}

export interface Curve {
  freqs: number[];
  db: number[];
  preamp: number;
  /** Preamp comune del volume uniforme, null se l'opzione è spenta. */
  common?: number | null;
}

export interface ApplyResult {
  preamp: number;
  path?: string | null;
}

export const GRAPHIC_FREQS = [31, 62, 125, 250, 500, 1000, 2000, 4000, 8000, 16000];

export const api = {
  getState: () => invoke<InitState>("get_state"),
  setLanguage: (language: string) => invoke<InitState>("set_language", { language }),
  apoStatus: () => invoke<ApoStatus>("apo_status"),
  autostart: () => invoke<Autostart>("autostart_status"),
  setAutostart: (enabled: boolean) => invoke<Autostart>("set_autostart", { enabled }),
  refreshHeadsets: () => invoke<Headset[]>("refresh_headsets"),
  connect: (connect: boolean) => invoke<string>("headset_connect", { connect }),
  setDefaultOutput: () => invoke<string>("set_default_output"),
  openLink: (target: string) => invoke<void>("open_link", { target }),
  preview: (profile: EqProfile) => invoke<Curve>("eq_preview", { profile }),
  apply: (profile: EqProfile, activePreset: string | null, bypass: boolean) =>
    invoke<ApplyResult>("eq_apply", { profile, activePreset, bypass }),
  updateSettings: (
    patch: Partial<Pick<Settings, "name_patterns" | "apo_device" | "start_minimized" | "default_on_connect" | "uniform_volume">>,
  ) => invoke<Settings>("update_settings", { patch }),
  getPresets: () => invoke<BuiltinPreset[]>("get_presets"),
  setAuto: (config: AutoConfig) => invoke<AutoConfig>("set_auto", { config }),
  runningApps: () => invoke<string[]>("running_apps"),
  saveUserPreset: (profile: EqProfile) => invoke<EqProfile[]>("save_user_preset", { profile }),
  deleteUserPreset: (name: string) => invoke<EqProfile[]>("delete_user_preset", { name }),
  importPreset: (path: string) => invoke<[EqProfile, EqProfile[]]>("import_preset", { path }),
  listOutputs: () => invoke<OutputInfo[]>("list_outputs"),
  chooseOutput: (id: string) => invoke<Settings>("choose_output", { id }),
  searchModels: (query: string, refresh: boolean) => invoke<Candidate[]>("search_models", { query, refresh }),
  chooseModel: (candidate: Candidate) => invoke<Settings>("choose_model", { candidate }),
  chooseModelFile: (path: string) => invoke<Settings>("choose_model_file", { path }),
  chooseVariant: (variant: string) => invoke<Settings>("choose_variant", { variant }),
  clearModel: () => invoke<Settings>("clear_model"),
  finishSetup: () => invoke<Settings>("finish_setup"),
  exportPreset: (path: string, profile: EqProfile) => invoke<void>("export_preset", { path, profile }),
};
