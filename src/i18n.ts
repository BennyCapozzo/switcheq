// Traduzioni dell'interfaccia: file piatti `chiave → testo` condivisi con il backend Rust.
// Per aggiungere una lingua: un file in src/i18n/, un import e una riga in TABLES (vedi README).
import en from "./i18n/en.json";
import it from "./i18n/it.json";

type Table = Record<string, string>;

const TABLES: Record<string, Table> = { it, en };
const FALLBACK = "en";

export const LANGUAGE_CODES = Object.keys(TABLES);

let current = FALLBACK;

/** Lingua del sistema secondo il WebView, usata solo prima che il backend comunichi la sua. */
export function guessLanguage(): string {
  const code = navigator.language.slice(0, 2).toLowerCase();
  return code in TABLES ? code : FALLBACK;
}

export function setLanguage(code: string) {
  current = code in TABLES ? code : FALLBACK;
  document.documentElement.lang = current;
}

export function languageName(code: string): string {
  return TABLES[code]?.["lang.name"] ?? code;
}

/** Testo di `key` con i segnaposto `{nome}` sostituiti; chiave mancante: inglese, poi la chiave stessa. */
export function t(key: string, params: Record<string, string | number> = {}): string {
  let text = TABLES[current][key] ?? TABLES[FALLBACK][key] ?? key;
  for (const [name, value] of Object.entries(params)) text = text.split(`{${name}}`).join(String(value));
  return text;
}

/** Traduce gli elementi marcati con data-i18n, data-i18n-title, data-i18n-placeholder e data-i18n-aria. */
export function translateDocument(root: ParentNode = document) {
  const apply = (attr: string, set: (el: HTMLElement, text: string) => void) =>
    root.querySelectorAll<HTMLElement>(`[${attr}]`).forEach((el) => set(el, t(el.getAttribute(attr)!)));
  apply("data-i18n", (el, text) => (el.textContent = text));
  apply("data-i18n-title", (el, text) => (el.title = text));
  apply("data-i18n-placeholder", (el, text) => ((el as HTMLInputElement).placeholder = text));
  apply("data-i18n-aria", (el, text) => el.setAttribute("aria-label", text));
}
