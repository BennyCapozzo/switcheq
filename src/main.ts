import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api, BASE_PRESET, GRAPHIC_FREQS, MEDIA_FILM, MEDIA_MUSIC, RULE_GAME, RULE_IGNORE } from "./api";
import type { AutoConfig, Autostart, AutoStatus, BuiltinPreset, Candidate, Curve, EqProfile, Filter, FilterKind, Headset, InitState, OutputInfo, Settings } from "./api";
import { drawCurve } from "./curve";
import { guessLanguage, LANGUAGE_CODES, languageName, setLanguage, t, translateDocument } from "./i18n";

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

// ------------------------------------------------------------------ stato

let init: InitState;
let presets: BuiltinPreset[] = [];
let userPresets: EqProfile[] = [];
let profile: EqProfile;
let activePreset: string | null = null; // "b:<id>" o "u:<nome>"
let bypass = false;
let headsets: Headset[] = [];
let outputs: OutputInfo[] = [];
let curve: Curve | null = null;
let dirty = false;
let auto: AutoConfig;
let autoStatus: AutoStatus | null = null;
let lastAutostart: Autostart | null = null;

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));
const fmtDb = (v: number) => `${v > 0 ? "+" : ""}${v.toFixed(1)} dB`;
const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));

function flash(el: HTMLElement, text: string, kind: "ok" | "err" | "" = "") {
  el.textContent = text;
  el.className = `msg ${kind}`;
}

// ------------------------------------------------------------------ cuffie

function renderHeadsets() {
  const list = $("headset-list");
  list.replaceChildren();
  if (headsets.length === 0) {
    const p = document.createElement("p");
    p.className = "empty";
    p.textContent = t("hs.none");
    list.append(p);
  }
  for (const h of headsets) {
    const row = document.createElement("div");
    row.className = "headset";
    const dot = document.createElement("span");
    dot.className = `dot ${h.connected ? "on" : ""}`;
    const name = document.createElement("div");
    name.className = "hs-name";
    name.textContent = h.name;
    const state = document.createElement("div");
    state.className = "hs-state";
    state.textContent = h.connected ? t("hs.connected") : t("hs.disconnected");
    const info = document.createElement("div");
    info.append(name, state);
    const bat = document.createElement("div");
    bat.className = "battery";
    if (h.connected && h.battery != null) {
      bat.innerHTML = `<span class="bar"><span style="width:${h.battery}%"></span></span>`;
      bat.append(` ${h.battery}%`);
      bat.title = t("hs.battery.title");
    } else if (h.connected) {
      bat.textContent = t("hs.battery.na");
      bat.title = t("hs.battery.na.title");
    }
    row.append(dot, info, bat);
    list.append(row);
  }
  const connected = headsets.find((h) => h.connected);
  const pill = $("pill-headset");
  const bt = isBluetooth();
  const out = init.settings.output;
  pill.textContent = !bt
    ? (out ? out.name : t("pill.no_output"))
    : connected
      ? `${connected.name}${connected.battery != null ? ` · ${connected.battery}%` : ""}`
      : headsets.length
        ? t("pill.not_connected")
        : t("pill.not_found");
  pill.classList.toggle("on", bt ? !!connected : !!out);
  $("headset-bt").hidden = !bt;
  ($("btn-connect") as HTMLButtonElement).disabled = headsets.length === 0 || !!connected;
  ($("btn-disconnect") as HTMLButtonElement).disabled = !connected;
  ($("btn-default") as HTMLButtonElement).disabled = bt ? !connected : !out;
}

/** L'uscita scelta è un dispositivo Bluetooth (stato e batteria disponibili). */
function isBluetooth(): boolean {
  return !!init.settings.output && init.settings.name_patterns.length > 0;
}

function outputLabel(o: OutputInfo): string {
  return o.bluetooth ? t("out.bluetooth", { name: o.name }) : o.name;
}

async function loadOutputs() {
  try {
    outputs = await api.listOutputs();
  } catch (e) {
    flash($("headset-msg"), errText(e), "err");
  }
  renderDevice();
}

/** Uscita audio, modello e variante di misura nella scheda Cuffie. */
function renderDevice() {
  const s = init.settings;
  fillOutputSelect($<HTMLSelectElement>("output-select"), s);
  const model = $("model-name");
  model.textContent = s.headphone ? s.headphone.name : t("model.none");
  model.classList.toggle("none", !s.headphone);
  const variants = s.headphone?.variants ?? [];
  $("variant-wrap").hidden = variants.length < 2;
  const vsel = $<HTMLSelectElement>("variant-select");
  vsel.replaceChildren(...variants.map((v) => new Option(v.label, v.id)));
  vsel.value = variants.find((v) => v.id === s.variant)?.id ?? variants[0]?.id ?? "";
  renderHeadsets();
}

function fillOutputSelect(sel: HTMLSelectElement, s: Settings) {
  sel.replaceChildren();
  if (!s.output) sel.append(new Option(t("out.choose"), ""));
  for (const o of outputs) sel.append(new Option(outputLabel(o), o.id));
  if (s.output && !outputs.some((o) => o.id === s.output!.id)) sel.append(new Option(t("out.unavailable", { name: s.output.name }), s.output.id));
  sel.value = s.output?.id ?? "";
}

async function chooseOutput(id: string, msg: HTMLElement) {
  if (!id) return;
  flash(msg, t("out.applying"));
  try {
    await adopt(await api.chooseOutput(id));
    flash(msg, t("out.chosen", { name: init.settings.output?.name ?? "" }), "ok");
    refreshHeadsets();
  } catch (e) {
    flash(msg, errText(e), "err");
    renderDevice();
  }
}

// ------------------------------------------------------------------ modello

function pickRow(title: string, sub: string, label: string, onClick: () => void): HTMLElement {
  const row = document.createElement("div");
  row.className = "pick";
  const text = document.createElement("div");
  const head = document.createElement("div");
  head.className = "pick-title";
  head.textContent = title;
  const u = document.createElement("div");
  u.className = "pick-sub";
  u.textContent = sub;
  text.append(head, u);
  const b = document.createElement("button");
  b.className = "small";
  b.textContent = label;
  b.addEventListener("click", onClick);
  row.append(text, b);
  return row;
}

/** Ricerca nell'indice AutoEq, elenco "in evidenza" e importazione da file. */
function buildModelPicker(msg: HTMLElement, onChosen: (s: Settings) => void): HTMLElement {
  const root = document.createElement("div");
  root.className = "picker";
  const row = document.createElement("div");
  row.className = "row";
  const input = document.createElement("input");
  input.type = "text";
  input.maxLength = 80;
  input.placeholder = t("pick.search_placeholder");
  input.className = "grow";
  const go = document.createElement("button");
  go.textContent = t("pick.search");
  row.append(input, go);
  const results = document.createElement("div");
  results.className = "picker-list";

  const pick = async (job: () => Promise<Settings>, what: string) => {
    flash(msg, t("pick.downloading", { what }));
    try {
      onChosen(await job());
    } catch (e) {
      flash(msg, errText(e), "err");
    }
  };

  let searching = false;
  const search = async (refresh: boolean) => {
    const q = input.value.trim();
    if (q.length < 2 || searching) return flash(msg, t("pick.min_chars"), "err");
    searching = true;
    go.disabled = true;
    flash(msg, t("pick.searching"));
    try {
      const found: Candidate[] = await api.searchModels(q, refresh);
      results.replaceChildren();
      if (!found.length) {
        const p = document.createElement("p");
        p.className = "empty";
        p.textContent = t("pick.no_results");
        results.append(p);
      }
      for (const c of found) {
        const extra = c.variants.length > 1 ? ` · ${c.variants.map((v) => v.label).join(", ")}` : "";
        results.append(pickRow(c.name, `${c.source} · ${c.form}${extra}`, t("pick.choose"), () => pick(() => api.chooseModel(c), c.name)));
      }
      const count = found.length === 1 ? t("pick.result_one", { n: 1 }) : t("pick.result_other", { n: found.length });
      flash(msg, found.length ? count : "", "ok");
    } catch (e) {
      flash(msg, errText(e), "err");
    } finally {
      searching = false;
      go.disabled = false;
    }
  };
  go.addEventListener("click", () => search(false));
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") search(false);
  });

  root.append(row, results);

  const tools = document.createElement("div");
  tools.className = "row wrap";
  const file = document.createElement("button");
  file.className = "ghost small";
  file.textContent = t("pick.import_file");
  file.addEventListener("click", async () => {
    const path = await open({ multiple: false, filters: [{ name: "Equalizer APO / AutoEq (TXT)", extensions: ["txt"] }] });
    if (typeof path === "string") pick(() => api.chooseModelFile(path), t("pick.the_file"));
  });
  const fresh = document.createElement("button");
  fresh.className = "ghost small";
  fresh.textContent = t("pick.refresh_index");
  fresh.title = t("pick.refresh_index.title");
  fresh.addEventListener("click", () => search(true));
  const repo = document.createElement("button");
  repo.className = "ghost small";
  repo.textContent = t("pick.about_autoeq");
  repo.addEventListener("click", () => api.openLink("https://github.com/jaakkopasanen/AutoEq"));
  tools.append(file, fresh, repo);
  const hint = document.createElement("p");
  hint.className = "hint";
  hint.textContent = t("pick.hint_download");
  const licence = document.createElement("p");
  licence.className = "hint";
  licence.textContent = t("pick.hint_licence");
  root.append(tools, hint, licence);
  return root;
}

function renderModelPicker() {
  $("model-picker-host").replaceChildren(
    buildModelPicker($("headset-msg"), async (ns) => {
      await adopt(ns);
      $<HTMLDetailsElement>("model-details").open = false;
      flash($("headset-msg"), t("model.chosen", { name: ns.headphone?.name ?? "" }), "ok");
    }),
  );
}

// ------------------------------------------------------------------ procedura guidata

let wizardStep = 0;

function showWizard() {
  wizardStep = 0;
  $("wizard").hidden = false;
  renderWizard();
}

async function closeWizard() {
  try {
    await adopt(await api.finishSetup());
  } catch {
    init.settings.setup_done = true;
  }
  $("wizard").hidden = true;
}

function renderWizard() {
  const s = init.settings;
  const body = $("wizard-body");
  const msg = $("wizard-msg");
  body.replaceChildren();
  flash(msg, "");
  $("wizard-step").textContent = t("wiz.step", { n: wizardStep + 1, total: 3 });
  const next = $<HTMLButtonElement>("wizard-next");
  $<HTMLButtonElement>("wizard-back").hidden = wizardStep === 0;
  const para = (text: string) => {
    const p = document.createElement("p");
    p.className = "desc";
    p.textContent = text;
    return p;
  };

  if (wizardStep === 0) {
    body.append(para(t("wiz.s0.text")));
    const label = document.createElement("label");
    label.className = "field";
    const sel = document.createElement("select");
    fillOutputSelect(sel, s);
    sel.addEventListener("change", async () => {
      await chooseOutput(sel.value, msg);
      renderWizard();
    });
    const lbl = document.createElement("span");
    lbl.className = "lbl";
    lbl.textContent = t("hp.output");
    label.append(lbl, sel);
    const again = document.createElement("button");
    again.className = "ghost small";
    again.textContent = t("common.refresh_list");
    again.addEventListener("click", async () => {
      await loadOutputs();
      renderWizard();
    });
    body.append(label, again);
    next.textContent = t("wiz.next");
    next.disabled = !s.output;
  } else if (wizardStep === 1) {
    body.append(para(t("wiz.s1.text")));
    if (s.headphone) body.append(para(t("wiz.s1.chosen", { name: s.headphone.name })));
    body.append(
      buildModelPicker(msg, async (ns) => {
        await adopt(ns);
        wizardStep = 2;
        renderWizard();
      }),
    );
    next.textContent = s.headphone ? t("wiz.next") : t("wiz.s1.skip_model");
    next.disabled = false;
  } else {
    if (s.headphone) {
      body.append(para(t("wiz.s2.with_model", { name: s.headphone.name })));
      if (s.headphone.variants.length > 1) {
        const label = document.createElement("label");
        label.className = "field";
        const lbl = document.createElement("span");
        lbl.className = "lbl";
        lbl.textContent = t("wiz.s2.start_variant");
        const sel = document.createElement("select");
        sel.append(...s.headphone.variants.map((v) => new Option(v.label, v.id)));
        sel.value = s.headphone.variants.find((v) => v.id === s.variant)?.id ?? s.headphone.variants[0].id;
        sel.addEventListener("change", async () => {
          try {
            await adopt(await api.chooseVariant(sel.value));
          } catch (e) {
            flash(msg, errText(e), "err");
          }
        });
        label.append(lbl, sel);
        body.append(label);
      }
    } else {
      body.append(para(t("wiz.s2.no_model")));
    }
    if (!init.apo.installed) body.append(para(t("wiz.s2.apo_missing")));
    next.textContent = t("wiz.finish");
    next.disabled = false;
  }
}

function initWizard() {
  $("wizard-skip").addEventListener("click", closeWizard);
  $("wizard-back").addEventListener("click", () => {
    wizardStep = Math.max(0, wizardStep - 1);
    renderWizard();
  });
  $("wizard-next").addEventListener("click", () => {
    if (wizardStep >= 2) return void closeWizard();
    wizardStep += 1;
    renderWizard();
  });
}

/** Applica nuove impostazioni arrivate dal backend, aggiornando anche i preset integrati. */
async function adopt(s: Settings) {
  init.settings = s;
  presets = await api.getPresets();
  applyBackendSettings(s);
}

async function refreshHeadsets() {
  try {
    headsets = await api.refreshHeadsets();
    renderHeadsets();
  } catch (e) {
    flash($("headset-msg"), errText(e), "err");
  }
}

async function connect(on: boolean) {
  const msg = $("headset-msg");
  flash(msg, on ? t("conn.reconnecting") : t("conn.releasing"));
  try {
    flash(msg, await api.connect(on), "ok");
    setTimeout(refreshHeadsets, 2500);
  } catch (e) {
    // Ripiego documentato: il driver non accetta il comando → impostazioni di Windows.
    flash(msg, t("conn.fallback", { error: errText(e) }), "err");
    api.openLink("ms-settings:bluetooth").catch(() => {});
  }
}

// ------------------------------------------------------------------ preset

/** Aggiunge a `sel` i preset integrati (per gruppo) e quelli utente. */
function appendPresetGroups(sel: HTMLSelectElement) {
  const group = (label: string, items: [string, string][]) => {
    if (!items.length) return;
    const g = document.createElement("optgroup");
    g.label = label;
    for (const [value, text] of items) g.append(new Option(text, value));
    sel.append(g);
  };
  const groups = new Map<string, [string, string][]>();
  for (const p of presets) {
    if (!groups.has(p.group)) groups.set(p.group, []);
    groups.get(p.group)!.push([`b:${p.id}`, p.profile.name]);
  }
  for (const [label, items] of groups) group(label, items);
  group(t("preset.group.user"), userPresets.map((p) => [`u:${p.name}`, p.name]));
}

/** Riempie una select di preset mantenendo, se possibile, il valore `value`. */
function fillPresetOptions(sel: HTMLSelectElement, value: string, special: [string, string][] = []) {
  sel.replaceChildren();
  for (const [v, text] of special) sel.append(new Option(text, v));
  appendPresetGroups(sel);
  if (value && ![...sel.options].some((o) => o.value === value)) sel.append(new Option(t("preset.not_found", { name: value }), value));
  sel.value = value;
}

function fillPresetSelect() {
  const sel = $<HTMLSelectElement>("preset-select");
  sel.replaceChildren();
  appendPresetGroups(sel);
  if (dirty || !activePreset) sel.append(new Option(t("preset.modified", { name: profile.name }), "custom"));
  sel.value = !dirty && activePreset && [...sel.options].some((o) => o.value === activePreset) ? activePreset : "custom";
}

function renderPresetInfo() {
  const builtin = activePreset?.startsWith("b:") ? presets.find((p) => `b:${p.id}` === activePreset) : undefined;
  $("preset-desc").textContent = builtin
    ? builtin.description + (dirty ? t("preset.desc.modified_suffix") : "")
    : dirty
      ? t("preset.desc.modified")
      : t("preset.desc.user");
  const src = $("preset-source");
  src.textContent = profile.source ? t("preset.source", { source: profile.source }) : "";
}

function selectPreset(value: string) {
  if (value.startsWith("b:")) {
    const p = presets.find((x) => `b:${x.id}` === value);
    if (!p) return;
    profile = clone(p.profile);
  } else if (value.startsWith("u:")) {
    const p = userPresets.find((x) => `u:${x.name}` === value);
    if (!p) return;
    profile = clone(p);
    $<HTMLInputElement>("user-name").value = p.name;
  } else {
    return;
  }
  activePreset = value;
  dirty = false;
  renderAll();
  scheduleApply();
}

// ------------------------------------------------------------------ editor

function buildGraphic() {
  const root = $("graphic");
  root.replaceChildren();
  GRAPHIC_FREQS.forEach((f, i) => {
    const col = document.createElement("div");
    col.className = "band";
    const val = document.createElement("output");
    val.className = "mono";
    const input = document.createElement("input");
    input.type = "range";
    input.min = "-12";
    input.max = "12";
    input.step = "0.5";
    input.setAttribute("orient", "vertical");
    input.setAttribute("aria-label", t("eq.band.aria", { freq: f }));
    input.dataset.band = String(i);
    input.addEventListener("input", () => {
      profile.graphic[i] = Number(input.value);
      val.textContent = Number(input.value).toFixed(1);
      changed();
    });
    input.addEventListener("dblclick", () => {
      input.value = "0";
      input.dispatchEvent(new Event("input"));
    });
    const label = document.createElement("span");
    label.className = "band-label";
    label.textContent = f >= 1000 ? `${f / 1000}k` : String(f);
    col.append(val, input, label);
    root.append(col);
  });
}

function renderGraphic() {
  $("graphic").querySelectorAll<HTMLInputElement>("input[type=range]").forEach((input) => {
    const g = profile.graphic[Number(input.dataset.band)] ?? 0;
    input.value = String(g);
    (input.previousElementSibling as HTMLElement).textContent = g.toFixed(1);
  });
}

function numberInput(value: number, step: string, min: string, max: string, onChange: (v: number) => void) {
  const el = document.createElement("input");
  el.type = "number";
  el.step = step;
  el.min = min;
  el.max = max;
  el.value = String(value);
  el.addEventListener("change", () => {
    const v = Number(el.value);
    if (Number.isFinite(v)) {
      const clamped = Math.min(Number(max), Math.max(Number(min), v));
      el.value = String(clamped);
      onChange(clamped);
    }
  });
  return el;
}

function renderParametric() {
  const body = $("peq-body");
  body.replaceChildren();
  profile.filters.forEach((f: Filter, i) => {
    const tr = document.createElement("tr");
    const td = (child: HTMLElement) => {
      const c = document.createElement("td");
      c.append(child);
      tr.append(c);
    };
    const on = document.createElement("input");
    on.type = "checkbox";
    on.checked = f.enabled;
    on.addEventListener("change", () => {
      f.enabled = on.checked;
      changed();
    });
    td(on);
    const kind = document.createElement("select");
    for (const [v, text] of [["PK", "Peaking"], ["LSC", "Low shelf"], ["HSC", "High shelf"]]) kind.append(new Option(text, v));
    kind.value = f.kind;
    kind.addEventListener("change", () => {
      f.kind = kind.value as FilterKind;
      changed();
    });
    td(kind);
    td(numberInput(f.freq, "1", "10", "22000", (v) => ((f.freq = v), changed())));
    td(numberInput(f.gain, "0.1", "-24", "24", (v) => ((f.gain = v), changed())));
    td(numberInput(f.q, "0.01", "0.1", "20", (v) => ((f.q = v), changed())));
    const del = document.createElement("button");
    del.className = "ghost small";
    del.textContent = "✕";
    del.title = t("eq.filter.remove");
    del.addEventListener("click", () => {
      profile.filters.splice(i, 1);
      renderParametric();
      changed();
    });
    td(del);
    body.append(tr);
  });
}

function renderPreamp() {
  const auto = $<HTMLInputElement>("preamp-auto");
  const manual = $<HTMLInputElement>("preamp-manual");
  auto.checked = profile.preamp_auto;
  manual.disabled = profile.preamp_auto;
  if (!profile.preamp_auto) manual.value = String(profile.preamp_manual);
  const pre = curve?.preamp ?? (profile.preamp_auto ? 0 : profile.preamp_manual);
  const common = curve?.common ?? null;
  if (profile.preamp_auto) manual.value = String(pre);
  // Preamp effettivamente scritto in Equalizer APO (in bypass solo il comune).
  $("preamp-value").textContent = fmtDb(bypass ? (common ?? 0) : pre);
  $("preamp-note").textContent = preampNote(pre, common);
}

function preampNote(pre: number, common: number | null): string {
  if (bypass) {
    return common === null ? t("preamp.bypass_off") : t("preamp.bypass_common", { common: fmtDb(common) });
  }
  if (!profile.preamp_auto) return t("preamp.manual");
  if (common === null) return t("preamp.auto_variable");
  if (pre < common - 0.05) return t("preamp.own", { common: fmtDb(common) });
  return t("preamp.common");
}

function renderBypass() {
  $<HTMLInputElement>("eq-enabled").checked = !bypass;
  $("eq-enabled-label").textContent = bypass ? t("eq.bypass") : t("eq.on");
  const pill = $("pill-eq");
  pill.textContent = bypass ? t("pill.eq_bypass") : t("pill.eq", { name: profile.name });
  pill.classList.toggle("on", !bypass);
  document.body.classList.toggle("bypass", bypass);
}

function renderAll() {
  fillPresetSelect();
  renderPresetInfo();
  renderGraphic();
  renderParametric();
  renderPreamp();
  renderBypass();
  renderAuto();
  updateCurve();
}

/** Chiamata dopo ogni modifica manuale del profilo. */
function changed() {
  if (!dirty) {
    dirty = true;
    fillPresetSelect();
    renderPresetInfo();
  }
  updateCurve();
  scheduleApply();
}

let previewPending = false;
function updateCurve() {
  if (previewPending) return;
  previewPending = true;
  requestAnimationFrame(async () => {
    try {
      curve = await api.preview(profile);
    } catch {
      curve = null;
    } finally {
      previewPending = false;
    }
    drawCurve($<HTMLCanvasElement>("curve"), curve, bypass);
    renderPreamp();
  });
}

let applyTimer: number | undefined;
function scheduleApply() {
  clearTimeout(applyTimer);
  applyTimer = window.setTimeout(async () => {
    const msg = $("eq-msg");
    try {
      const r = await api.apply(profile, dirty ? null : activePreset, bypass);
      const bypassText = r.preamp ? t("apply.bypass_common", { preamp: fmtDb(r.preamp) }) : t("apply.bypass_none");
      if (r.path) flash(msg, bypass ? bypassText : t("apply.ok", { preamp: fmtDb(r.preamp) }), "ok");
      else flash(msg, t("apply.saved_no_apo"), "err");
      if (!init.apo.included && r.path) {
        init.apo = await api.apoStatus();
        renderApo();
      }
    } catch (e) {
      flash(msg, errText(e), "err");
    }
    renderBypass();
  }, 300);
}

// ------------------------------------------------------------------ Equalizer APO

function renderApo() {
  const b = $("apo-banner");
  const apo = init.apo;
  if (!apo.installed) {
    b.hidden = false;
    b.className = "banner warn";
    const title = document.createElement("strong");
    title.textContent = t("apo.missing.title");
    const steps = document.createElement("ol");
    for (const key of ["apo.missing.step1", "apo.missing.step2", "apo.missing.step3"]) {
      const li = document.createElement("li");
      li.textContent = t(key);
      steps.append(li);
    }
    const download = document.createElement("button");
    download.id = "apo-download";
    download.textContent = t("apo.download");
    download.addEventListener("click", () => api.openLink(apo.download_url));
    b.replaceChildren(title, ` ${t("apo.missing.intro")}`, steps, download);
  } else {
    b.hidden = true;
  }
  $("apo-path").textContent = apo.installed
    ? t(apo.included ? "apo.path.included" : "apo.path.first_save", { path: apo.config_path ?? "" })
    : "";
}

// ------------------------------------------------------------------ preset utente

async function saveUser() {
  const msg = $("eq-msg");
  const name = $<HTMLInputElement>("user-name").value.trim();
  if (!name) return flash(msg, t("user.name_required"), "err");
  const p = { ...clone(profile), name };
  try {
    userPresets = await api.saveUserPreset(p);
    profile = p;
    activePreset = `u:${name}`;
    dirty = false;
    renderAll();
    scheduleApply();
    flash(msg, t("user.saved", { name }), "ok");
  } catch (e) {
    flash(msg, errText(e), "err");
  }
}

async function deleteUser() {
  const msg = $("eq-msg");
  if (!activePreset?.startsWith("u:")) return flash(msg, t("user.select_to_delete"), "err");
  const name = activePreset.slice(2);
  try {
    userPresets = await api.deleteUserPreset(name);
    activePreset = null;
    dirty = true;
    renderAll();
    scheduleApply();
    flash(msg, t("user.deleted", { name }), "ok");
  } catch (e) {
    flash(msg, errText(e), "err");
  }
}

async function importUser() {
  const msg = $("eq-msg");
  const path = await open({
    multiple: false,
    filters: [{ name: t("user.import_filter"), extensions: ["json", "txt"] }],
  });
  if (typeof path !== "string") return;
  try {
    const [p, list] = await api.importPreset(path);
    userPresets = list;
    profile = clone(p);
    profile.graphic ??= Array(10).fill(0);
    activePreset = `u:${p.name}`;
    dirty = false;
    $<HTMLInputElement>("user-name").value = p.name;
    renderAll();
    scheduleApply();
    flash(msg, t("user.imported", { name: p.name, n: p.filters.length }), "ok");
  } catch (e) {
    flash(msg, errText(e), "err");
  }
}

async function exportUser() {
  const msg = $("eq-msg");
  const name = $<HTMLInputElement>("user-name").value.trim() || profile.name;
  const path = await save({ defaultPath: `${name.replace(/[\\/:*?"<>|]/g, "_")}.json`, filters: [{ name: "Preset JSON", extensions: ["json"] }] });
  if (!path) return;
  try {
    await api.exportPreset(path, { ...clone(profile), name });
    flash(msg, t("user.exported", { path }), "ok");
  } catch (e) {
    flash(msg, errText(e), "err");
  }
}

// ------------------------------------------------------------------ preset automatici

const ruleSpecial = (): [string, string][] => [
  [RULE_GAME, t("auto.special.game")],
  [RULE_IGNORE, t("auto.special.not_game")],
];

const mediaSpecial = (): [string, string][] => [
  [MEDIA_MUSIC, t("auto.special.music")],
  [MEDIA_FILM, t("auto.special.film")],
  [RULE_IGNORE, t("auto.special.ignore")],
];

function renderAutoStatus() {
  $("auto-status").textContent = !auto.enabled
    ? ""
    : autoStatus
      ? t("auto.status.active", { preset: autoStatus.preset, exe: autoStatus.exe })
      : t("auto.status.waiting");
}

function emptyRow(text: string): HTMLElement {
  const tr = document.createElement("tr");
  const td = document.createElement("td");
  td.colSpan = 3;
  td.className = "empty";
  td.textContent = text;
  tr.append(td);
  return tr;
}

function ruleTools(index: number, onUp: () => void, onDelete: () => void): HTMLElement {
  const tools = document.createElement("td");
  tools.className = "row";
  const up = document.createElement("button");
  up.className = "ghost small";
  up.textContent = "↑";
  up.title = t("auto.rule.priority");
  up.disabled = index === 0;
  up.addEventListener("click", onUp);
  const del = document.createElement("button");
  del.className = "ghost small";
  del.textContent = "✕";
  del.title = t("auto.rule.delete");
  del.addEventListener("click", onDelete);
  tools.append(up, del);
  return tools;
}

function renderAuto() {
  if (!auto) return;
  $<HTMLInputElement>("auto-enabled").checked = auto.enabled;
  $("auto-enabled-label").textContent = auto.enabled ? t("auto.on") : t("auto.off");
  $<HTMLInputElement>("auto-fullscreen").checked = auto.detect_fullscreen;
  $<HTMLInputElement>("auto-notify").checked = auto.notify;
  fillPresetOptions($<HTMLSelectElement>("auto-game-preset"), auto.game_preset);
  const rulePreset = $<HTMLSelectElement>("rule-preset");
  fillPresetOptions(rulePreset, rulePreset.value || RULE_GAME, ruleSpecial());

  const body = $("rules-body");
  body.replaceChildren();
  if (!auto.rules.length) body.append(emptyRow(t("auto.rules.empty")));
  auto.rules.forEach((rule, i) => {
    const tr = document.createElement("tr");
    const exe = document.createElement("td");
    exe.textContent = rule.exe;
    const action = document.createElement("td");
    const sel = document.createElement("select");
    fillPresetOptions(sel, rule.preset, ruleSpecial());
    sel.addEventListener("change", () => {
      rule.preset = sel.value;
      saveAuto(t("auto.rule.updated", { name: rule.exe }));
    });
    action.append(sel);
    const tools = ruleTools(
      i,
      () => {
        auto.rules.splice(i - 1, 0, ...auto.rules.splice(i, 1));
        saveAuto();
      },
      () => {
        auto.rules.splice(i, 1);
        saveAuto(t("auto.rule.deleted", { name: rule.exe }));
      },
    );
    tr.append(exe, action, tools);
    body.append(tr);
  });
  renderMedia();
  renderAutoStatus();
}

function renderMedia() {
  const m = auto.media;
  $<HTMLInputElement>("media-enabled").checked = m.enabled;
  fillPresetOptions($<HTMLSelectElement>("media-music"), m.music_preset, [[BASE_PRESET, t("auto.special.base")]]);
  fillPresetOptions($<HTMLSelectElement>("media-film"), m.film_preset);
  const rulePreset = $<HTMLSelectElement>("media-rule-preset");
  fillPresetOptions(rulePreset, rulePreset.value || MEDIA_MUSIC, mediaSpecial());

  const body = $("media-rules-body");
  body.replaceChildren();
  if (!m.rules.length) body.append(emptyRow(t("auto.mrules.empty")));
  m.rules.forEach((rule, i) => {
    const tr = document.createElement("tr");
    const pattern = document.createElement("td");
    pattern.textContent = rule.pattern;
    const action = document.createElement("td");
    const sel = document.createElement("select");
    fillPresetOptions(sel, rule.preset, mediaSpecial());
    sel.addEventListener("change", () => {
      rule.preset = sel.value;
      saveAuto(t("auto.mrule.updated", { name: rule.pattern }));
    });
    action.append(sel);
    const tools = ruleTools(
      i,
      () => {
        m.rules.splice(i - 1, 0, ...m.rules.splice(i, 1));
        saveAuto();
      },
      () => {
        m.rules.splice(i, 1);
        saveAuto(t("auto.mrule.deleted", { name: rule.pattern }));
      },
    );
    tr.append(pattern, action, tools);
    body.append(tr);
  });
}

function addMediaRule() {
  const input = $<HTMLInputElement>("media-rule-pattern");
  const pattern = input.value.trim().toLowerCase();
  if (!pattern) return flash($("auto-msg"), t("auto.mrule.required"), "err");
  auto.media.rules = auto.media.rules.filter((r) => r.pattern !== pattern);
  auto.media.rules.push({ pattern, preset: $<HTMLSelectElement>("media-rule-preset").value });
  input.value = "";
  saveAuto(t("auto.mrule.added", { name: pattern }));
}

async function saveAuto(okText = "") {
  const msg = $("auto-msg");
  try {
    auto = await api.setAuto(auto);
    if (okText) flash(msg, okText, "ok");
  } catch (e) {
    flash(msg, errText(e), "err");
  }
  renderAuto();
}

async function loadRunningApps() {
  try {
    const list = $("running-apps");
    list.replaceChildren(...(await api.runningApps()).map((n) => new Option(n, n)));
  } catch {
    /* solo suggerimenti: nessun errore da mostrare */
  }
}

function addRule() {
  const input = $<HTMLInputElement>("rule-exe");
  let exe = input.value.trim().split(/[\\/]/).pop()!.toLowerCase();
  if (!exe) return flash($("auto-msg"), t("auto.rule.required"), "err");
  if (!exe.includes(".")) exe += ".exe";
  auto.rules = auto.rules.filter((r) => r.exe !== exe);
  auto.rules.push({ exe, preset: $<HTMLSelectElement>("rule-preset").value });
  input.value = "";
  saveAuto(t("auto.rule.added", { name: exe }));
}

function initAuto() {
  $("auto-enabled").addEventListener("change", (e) => {
    auto.enabled = (e.target as HTMLInputElement).checked;
    saveAuto(auto.enabled ? t("auto.msg.on") : t("auto.msg.off"));
  });
  $("auto-fullscreen").addEventListener("change", (e) => {
    auto.detect_fullscreen = (e.target as HTMLInputElement).checked;
    saveAuto(t("common.saved"));
  });
  $("auto-notify").addEventListener("change", (e) => {
    auto.notify = (e.target as HTMLInputElement).checked;
    saveAuto(t("common.saved"));
  });
  $("auto-game-preset").addEventListener("change", (e) => {
    auto.game_preset = (e.target as HTMLSelectElement).value;
    saveAuto(t("auto.msg.game_saved"));
  });
  $("media-enabled").addEventListener("change", (e) => {
    auto.media.enabled = (e.target as HTMLInputElement).checked;
    saveAuto(auto.media.enabled ? t("auto.msg.media_on") : t("auto.msg.media_off"));
  });
  $("media-music").addEventListener("change", (e) => {
    auto.media.music_preset = (e.target as HTMLSelectElement).value;
    saveAuto(t("auto.msg.music_saved"));
  });
  $("media-film").addEventListener("change", (e) => {
    auto.media.film_preset = (e.target as HTMLSelectElement).value;
    saveAuto(t("auto.msg.film_saved"));
  });
  $("media-rule-add").addEventListener("click", addMediaRule);
  $("media-rule-pattern").addEventListener("keydown", (e) => {
    if (e.key === "Enter") addMediaRule();
  });
  $("rule-add").addEventListener("click", addRule);
  $("rule-exe").addEventListener("keydown", (e) => {
    if (e.key === "Enter") addRule();
  });
  $("rule-exe").addEventListener("focus", loadRunningApps, { once: true });
  $("rule-refresh").addEventListener("click", async () => {
    await loadRunningApps();
    flash($("auto-msg"), t("auto.msg.list_refreshed"), "ok");
  });
}

/** Il backend ha cambiato le impostazioni (preset automatico o menu del tray). */
function applyBackendSettings(s: Settings) {
  clearTimeout(applyTimer);
  init.settings = s;
  profile = s.profile;
  profile.graphic ??= Array(10).fill(0);
  activePreset = s.active_preset ?? null;
  dirty = !activePreset;
  bypass = s.bypass;
  userPresets = s.user_presets;
  auto = s.auto;
  renderAll();
  renderDevice();
  syncSettingsFields(s);
}

function syncSettingsFields(s: Settings) {
  $<HTMLInputElement>("set-patterns").value = s.name_patterns.join(", ");
  $<HTMLInputElement>("set-device").value = s.apo_device;
  $<HTMLInputElement>("set-uniform").checked = s.uniform_volume;
  fillLanguageSelect();
}

// ------------------------------------------------------------------ lingua

function fillLanguageSelect() {
  const sel = $<HTMLSelectElement>("set-language");
  sel.replaceChildren(new Option(t("lang.auto"), "auto"), ...LANGUAGE_CODES.map((c) => new Option(languageName(c), c)));
  sel.value = LANGUAGE_CODES.includes(init.settings.language) ? init.settings.language : "auto";
}

/** Stato completo dal backend (all'avvio e dopo un cambio di lingua): ricostruisce tutta l'interfaccia. */
function applyInitState(s: InitState) {
  init = s;
  setLanguage(s.lang);
  translateDocument();
  presets = s.presets;
  headsets = s.headsets.length ? s.headsets : headsets;
  autoStatus = s.auto_status ?? null;
  document.title = s.product;
  $("app-name").textContent = s.product;
  $("version").textContent = `${s.product} ${s.version}`;
  const banner = $("dev-banner");
  banner.hidden = !s.dev_build;
  if (s.dev_build) banner.textContent = t("dev.banner", { path: s.dev_build });
  buildGraphic();
  renderApo();
  applyBackendSettings(s.settings);
  renderModelPicker();
  if (lastAutostart) renderAutostart(lastAutostart);
  if (!$("wizard").hidden) renderWizard();
}

// ------------------------------------------------------------------ impostazioni

function renderAutostart(a: Autostart) {
  lastAutostart = a;
  const autostart = $<HTMLInputElement>("set-autostart");
  const hint = $("autostart-hint");
  autostart.checked = a.enabled;
  autostart.disabled = a.dev_build;
  const notes: string[] = [];
  if (a.dev_build) notes.push(t("set.autostart.dev"));
  if (a.others.length) notes.push(t("set.autostart.others", { list: a.others.join("; ") }));
  hint.textContent = notes.join(" ");
  hint.hidden = !notes.length;
}

async function initSettings() {
  const s = init.settings;
  const autostart = $<HTMLInputElement>("set-autostart");
  const msg = $("settings-msg");
  const reloadAutostart = () => api.autostart().then(renderAutostart, () => (autostart.disabled = true));
  await reloadAutostart();
  autostart.addEventListener("change", async () => {
    try {
      renderAutostart(await api.setAutostart(autostart.checked));
      flash(msg, autostart.checked ? t("set.autostart.enabled") : t("set.autostart.disabled"), "ok");
    } catch (e) {
      flash(msg, errText(e), "err");
      await reloadAutostart();
    }
  });

  $("set-language").addEventListener("change", async (e) => {
    try {
      applyInitState(await api.setLanguage((e.target as HTMLSelectElement).value));
      refreshHeadsets();
      flash(msg, t("common.saved"), "ok");
    } catch (err) {
      flash(msg, errText(err), "err");
      fillLanguageSelect();
    }
  });

  const bindCheck = (id: string, value: boolean, key: "start_minimized" | "default_on_connect") => {
    const el = $<HTMLInputElement>(id);
    el.checked = value;
    el.addEventListener("change", () =>
      api.updateSettings({ [key]: el.checked }).then(() => flash(msg, t("common.saved"), "ok"), (e) => flash(msg, errText(e), "err")),
    );
  };
  bindCheck("set-minimized", s.start_minimized, "start_minimized");
  bindCheck("set-default", s.default_on_connect, "default_on_connect");

  const uniform = $<HTMLInputElement>("set-uniform");
  uniform.checked = s.uniform_volume;
  uniform.addEventListener("change", async () => {
    try {
      // Il backend riscrive subito la configurazione con il nuovo preamp.
      await api.updateSettings({ uniform_volume: uniform.checked });
      updateCurve();
      flash(msg, uniform.checked ? t("set.uniform.on") : t("set.uniform.off"), "ok");
    } catch (e) {
      uniform.checked = !uniform.checked;
      flash(msg, errText(e), "err");
    }
  });

  const patterns = $<HTMLInputElement>("set-patterns");
  patterns.value = s.name_patterns.join(", ");
  patterns.addEventListener("change", async () => {
    const list = patterns.value.split(",").map((x) => x.trim()).filter(Boolean);
    try {
      const ns = await api.updateSettings({ name_patterns: list });
      patterns.value = ns.name_patterns.join(", ");
      flash(msg, t("set.patterns.updated"), "ok");
      refreshHeadsets();
    } catch (e) {
      flash(msg, errText(e), "err");
    }
  });

  const device = $<HTMLInputElement>("set-device");
  device.value = s.apo_device;
  device.addEventListener("change", async () => {
    try {
      const ns = await api.updateSettings({ apo_device: device.value });
      device.value = ns.apo_device;
      flash(msg, t("set.device.applied", { device: ns.apo_device }), "ok");
    } catch (e) {
      flash(msg, errText(e), "err");
    }
  });
}

// ------------------------------------------------------------------ avvio

async function main() {
  setLanguage(guessLanguage());
  translateDocument();
  applyInitState(await api.getState());
  await initSettings();
  initAuto();
  initWizard();
  $("output-select").addEventListener("change", (e) => chooseOutput((e.target as HTMLSelectElement).value, $("headset-msg")));
  $("variant-select").addEventListener("change", async (e) => {
    try {
      await adopt(await api.chooseVariant((e.target as HTMLSelectElement).value));
      flash($("eq-msg"), t("variant.applied"), "ok");
    } catch (err) {
      flash($("eq-msg"), errText(err), "err");
    }
  });
  loadOutputs();

  $("preset-select").addEventListener("change", (e) => selectPreset((e.target as HTMLSelectElement).value));
  $("eq-enabled").addEventListener("change", (e) => {
    bypass = !(e.target as HTMLInputElement).checked;
    renderBypass();
    updateCurve();
    scheduleApply();
  });
  $("preamp-auto").addEventListener("change", (e) => {
    profile.preamp_auto = (e.target as HTMLInputElement).checked;
    if (!profile.preamp_auto) profile.preamp_manual = curve?.preamp ?? 0;
    changed();
  });
  $("preamp-manual").addEventListener("input", (e) => {
    profile.preamp_manual = Number((e.target as HTMLInputElement).value);
    changed();
  });
  $("graphic-reset").addEventListener("click", () => {
    profile.graphic = Array(10).fill(0);
    renderGraphic();
    changed();
  });
  $("peq-add").addEventListener("click", () => {
    profile.filters.push({ kind: "PK", freq: 1000, gain: 0, q: 1, enabled: true });
    renderParametric();
    changed();
  });
  document.querySelectorAll<HTMLButtonElement>(".tab").forEach((tab) =>
    tab.addEventListener("click", () => {
      document.querySelectorAll(".tab").forEach((other) => other.classList.toggle("active", other === tab));
      $("tab-graphic").hidden = tab.dataset.tab !== "graphic";
      $("tab-parametric").hidden = tab.dataset.tab !== "parametric";
    }),
  );
  $("user-save").addEventListener("click", saveUser);
  $("user-delete").addEventListener("click", deleteUser);
  $("user-import").addEventListener("click", importUser);
  $("user-export").addEventListener("click", exportUser);
  $("btn-refresh").addEventListener("click", refreshHeadsets);
  $("btn-connect").addEventListener("click", () => connect(true));
  $("btn-disconnect").addEventListener("click", () => connect(false));
  $("btn-btsettings").addEventListener("click", () => api.openLink("ms-settings:bluetooth"));
  $("btn-default").addEventListener("click", async () => {
    try {
      flash($("headset-msg"), t("out.default_set", { name: await api.setDefaultOutput() }), "ok");
    } catch (e) {
      flash($("headset-msg"), errText(e), "err");
    }
  });
  window.addEventListener("resize", () => drawCurve($<HTMLCanvasElement>("curve"), curve, bypass));

  await listen<Headset[]>("headsets", (e) => {
    headsets = e.payload;
    renderHeadsets();
  });
  await listen<boolean>("bypass", (e) => {
    bypass = e.payload;
    renderBypass();
    updateCurve();
  });
  await listen<Settings>("settings", (e) => adopt(e.payload).catch(() => applyBackendSettings(e.payload)));
  await listen<AutoStatus | null>("auto-status", (e) => {
    autoStatus = e.payload;
    renderAutoStatus();
  });
  await listen<string>("default-output", (e) => flash($("headset-msg"), t("out.default_set", { name: e.payload }), "ok"));

  refreshHeadsets();
  if (!init.settings.setup_done) showWizard();
}

main().catch((e) => {
  document.body.insertAdjacentHTML("afterbegin", `<p class="msg err"></p>`);
  document.body.firstElementChild!.textContent = t("start.error", { error: errText(e) });
});
