// Disegno della curva di risposta su canvas (asse x logaritmico 20 Hz–20 kHz).
import type { Curve } from "./api";

const F_MIN = 20;
const F_MAX = 20000;
const DB_RANGE = 15;

const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

export function drawCurve(canvas: HTMLCanvasElement, curve: Curve | null, bypass: boolean): void {
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
  }
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);

  const padL = 34;
  const padB = 18;
  const pw = w - padL - 6;
  const ph = h - padB - 6;
  const x = (f: number) => padL + (Math.log(f / F_MIN) / Math.log(F_MAX / F_MIN)) * pw;
  const y = (db: number) => 6 + ((DB_RANGE - Math.max(-DB_RANGE, Math.min(DB_RANGE, db))) / (2 * DB_RANGE)) * ph;

  // Griglia
  ctx.font = "10px Segoe UI, sans-serif";
  ctx.fillStyle = css("--muted");
  ctx.lineWidth = 1;
  for (const f of [50, 100, 200, 500, 1000, 2000, 5000, 10000]) {
    ctx.strokeStyle = css("--grid");
    ctx.beginPath();
    ctx.moveTo(x(f), 6);
    ctx.lineTo(x(f), 6 + ph);
    ctx.stroke();
    ctx.fillText(f >= 1000 ? `${f / 1000}k` : `${f}`, x(f) - 8, h - 4);
  }
  for (let db = -DB_RANGE; db <= DB_RANGE; db += 5) {
    ctx.strokeStyle = db === 0 ? css("--grid-strong") : css("--grid");
    ctx.beginPath();
    ctx.moveTo(padL, y(db));
    ctx.lineTo(padL + pw, y(db));
    ctx.stroke();
    ctx.fillText(`${db > 0 ? "+" : ""}${db}`, 4, y(db) + 3);
  }
  if (!curve) return;

  const line = (offset: number, color: string, width: number, dash: number[] = []) => {
    ctx.strokeStyle = color;
    ctx.lineWidth = width;
    ctx.setLineDash(dash);
    ctx.beginPath();
    curve.freqs.forEach((f, i) => {
      const px = x(f);
      const py = y(curve.db[i] + offset);
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    });
    ctx.stroke();
    ctx.setLineDash([]);
  };

  if (bypass) {
    line(0, css("--muted"), 1.5, [4, 4]);
    return;
  }
  // Curva dei filtri e, tratteggiata, la curva finale con il preamp.
  line(0, css("--accent"), 2);
  if (Math.abs(curve.preamp) >= 0.05) line(curve.preamp, css("--accent-dim"), 1.25, [5, 4]);
}
