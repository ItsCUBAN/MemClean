import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { el, li } from "./dom";
import { initSettings } from "./settings";

// Debe coincidir con `MemoryStats` en src-tauri/src/memory.rs (todo en bytes).
interface MemoryStats {
  physTotal: number;
  physAvail: number;
  physUsed: number;
  physPercent: number;
  pageTotal: number;
  pageUsed: number;
  pageFree: number;
  standby: number | null;
  fileCache: number | null;
}

// Deben coincidir con `CleanReport` y `AreaResult` en src-tauri/src/cleaner/mod.rs.
interface AreaResult {
  area: string;
  label: string;
  error: string | null;
}

interface CleanReport {
  timestampMs: number;
  freedBytes: number;
  results: AreaResult[];
}

// Debe coincidir con `HistoryEntry` en src-tauri/src/history.rs.
interface HistoryEntry {
  trigger: "button" | "tray" | "hotkey" | "threshold" | "interval";
  triggerLabel: string;
  report: CleanReport;
}

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const gb = (bytes: number) => `${(bytes / GB).toFixed(2)} GB`;
const time = (ms: number) => new Date(ms).toLocaleTimeString("es-ES");

// Si es negativo, otro programa ocupó memoria mientras limpiábamos: se muestra 0.
function freed(bytes: number): string {
  const mb = Math.max(0, bytes) / MB;
  return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${Math.round(mb)} MB`;
}
const pct = (part: number, total: number) => (total > 0 ? (part / total) * 100 : 0);

// Mismos umbrales que tendrá el icono de la bandeja: <60 verde, 60-69 amarillo, >=70 rojo.
function levelClass(percent: number): string {
  if (percent >= 70) return "high";
  if (percent >= 60) return "warn";
  return "";
}

function setBar(id: string, percent: number, colorByLevel = false) {
  const bar = el(id);
  bar.style.width = `${Math.min(100, Math.max(0, percent))}%`;
  if (colorByLevel) bar.className = `fill ${levelClass(percent)}`;
}

function render(s: MemoryStats) {
  // Solo se borra el error de lectura de memoria; los demás avisos se quedan.
  el("memory-error").hidden = true;

  el("phys-pct").textContent = `${Math.round(s.physPercent)} %`;
  setBar("phys-bar", s.physPercent, true);
  el("phys-values").textContent =
    `Usada ${gb(s.physUsed)} · Libre ${gb(s.physAvail)} · Total ${gb(s.physTotal)}`;

  const pagePct = pct(s.pageUsed, s.pageTotal);
  el("page-pct").textContent = `${Math.round(pagePct)} %`;
  setBar("page-bar", pagePct);
  el("page-values").textContent =
    `Usado ${gb(s.pageUsed)} · Libre ${gb(s.pageFree)} · Total ${gb(s.pageTotal)}`;

  const cacheTotal = (s.standby ?? 0) + (s.fileCache ?? 0);
  const cachePct = pct(cacheTotal, s.physTotal);
  el("cache-pct").textContent = `${Math.round(cachePct)} %`;
  setBar("cache-bar", cachePct);
  const standby = s.standby === null ? "requiere admin" : gb(s.standby);
  const fileCache = s.fileCache === null ? "no disponible" : gb(s.fileCache);
  el("cache-values").textContent = `Standby ${standby} · Archivos ${fileCache}`;
}

function showError(message: string, id = "error") {
  const box = el(id);
  box.textContent = message;
  box.hidden = false;
}

// Avisos que siguen siendo ciertos mientras MemClean esté abierto (atajo ocupado, etc.).
function addIssue(message: string) {
  const list = el("issues");
  const already = Array.from(list.children).some((item) => item.textContent === message);
  if (!already) list.append(li("fail", message));
}

function setCleaning(cleaning: boolean) {
  const button = el("clean-btn") as HTMLButtonElement;
  button.disabled = cleaning;
  button.textContent = cleaning ? "Limpiando…" : "Limpiar memoria";
}

// Si todo fue bien basta una línea; si algo falló, se detalla cada área que falló.
function renderAreas(report: CleanReport) {
  const failed = report.results.filter((r) => r.error);
  const okCount = report.results.length - failed.length;
  const items = failed.map((r) => li("fail", `✗ ${r.label}: ${r.error}`));
  if (okCount > 0) {
    const summary =
      failed.length === 0
        ? `✓ Todas las áreas (${okCount})`
        : `✓ ${okCount} ${okCount === 1 ? "área bien" : "áreas bien"}`;
    items.unshift(li("ok", summary));
  }
  el("area-results").replaceChildren(...items);
}

function renderHistory(entries: HistoryEntry[]) {
  setCleaning(false);
  el("history-block").hidden = entries.length === 0;
  if (entries.length === 0) return;

  const last = entries[0];
  el("last-clean").textContent =
    `Última limpieza: ${time(last.report.timestampMs)} · ${freed(last.report.freedBytes)} liberados`;
  renderAreas(last.report);

  el("history").replaceChildren(
    ...entries.map((entry) => {
      const item = document.createElement("li");
      const when = document.createElement("span");
      when.textContent = `${time(entry.report.timestampMs)} · ${entry.triggerLabel}`;
      const amount = document.createElement("span");
      amount.className = "freed";
      amount.textContent = freed(entry.report.freedBytes);
      item.append(when, amount);
      return item;
    }),
  );
}

async function refreshHistory() {
  renderHistory(await invoke<HistoryEntry[]>("get_history"));
}

async function start() {
  // Primera lectura inmediata; después llegan actualizaciones cada segundo desde Rust.
  try {
    render(await invoke<MemoryStats>("get_memory"));
    el("admin-warning").hidden = await invoke<boolean>("is_admin");
    for (const issue of await invoke<string[]>("get_startup_issues")) addIssue(issue);
    await refreshHistory();
  } catch (e) {
    showError(String(e));
  }
  await listen<MemoryStats>("memory-stats", (event) => render(event.payload));
  await listen<string>("memory-error", (event) => showError(event.payload, "memory-error"));
  await listen<string>("app-error", (event) => addIssue(event.payload));

  // La limpieza puede empezar desde el botón, la bandeja, el atajo o sola (automática).
  await listen("clean-started", () => {
    el("error").hidden = true;
    setCleaning(true);
  });
  await listen("clean-done", () => refreshHistory().catch((e) => showError(String(e))));
  await listen<string>("clean-error", (event) => {
    setCleaning(false);
    showError(event.payload);
  });

  el("clean-btn").addEventListener("click", () => {
    setCleaning(true);
    invoke("clean_now").catch((e) => {
      setCleaning(false);
      showError(String(e));
    });
  });

  await initSettings(showError);
}

start().catch((e) => showError(String(e)));
