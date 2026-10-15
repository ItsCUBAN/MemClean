import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { el, li, showView } from "./dom";
import { type Lang, setLang, t, tKey } from "./i18n";

// Debe coincidir con `Settings` en src-tauri/src/settings.rs.
interface Settings {
  auto_clean: boolean;
  threshold_percent: number;
  cooldown_minutes: number;
  areas: string[];
  trim_scope: "all" | "apps";
  hotkey_enabled: boolean;
  start_with_windows: boolean;
  start_minimized: boolean;
  notifications: boolean;
  show_page_file: boolean;
  language: "auto" | "es" | "en";
}

interface SettingsInfo {
  memoryAreas: string[];
  extraAreas: string[];
  thresholdRange: [number, number];
  cooldownRange: [number, number];
}

interface SaveResult {
  settings: Settings;
  warnings: string[];
  language: Lang;
}

// Casillas cuyo nombre coincide con un ajuste de sí/no.
const CHECKBOXES = [
  "auto_clean",
  "hotkey_enabled",
  "start_with_windows",
  "start_minimized",
  "notifications",
  "show_page_file",
] as const;

let current: Settings | null = null;

function form(): HTMLFormElement {
  return el("settings-form") as HTMLFormElement;
}

function field(name: string): HTMLInputElement {
  const input = form().elements.namedItem(name);
  if (!(input instanceof HTMLInputElement)) throw new Error(`Falta el campo ${name}`);
  return input;
}

function select(name: string): HTMLSelectElement {
  const found = form().elements.namedItem(name);
  if (!(found instanceof HTMLSelectElement)) throw new Error(`Falta el campo ${name}`);
  return found;
}

// Casillas de áreas: las de memoria (#areas-list) y los extras (#extras-list).
function areaBoxes(list = "#settings-form"): HTMLInputElement[] {
  return Array.from(document.querySelectorAll<HTMLInputElement>(`${list} input[name="area"]`));
}

// Una casilla por área. El nombre lleva data-i18n, así se traduce solo.
function areaCheckbox(area: string): HTMLLabelElement {
  const label = document.createElement("label");
  label.className = "check";
  const box = document.createElement("input");
  box.type = "checkbox";
  box.name = "area";
  box.value = area;
  const text = document.createElement("span");
  text.dataset.i18n = `area.${area}`;
  text.textContent = tKey(`area.${area}`);
  label.append(box, " ", text);
  return label;
}

function fill(s: Settings) {
  for (const name of CHECKBOXES) field(name).checked = s[name];
  field("threshold_percent").value = String(s.threshold_percent);
  field("cooldown_minutes").value = String(s.cooldown_minutes);
  for (const box of areaBoxes()) box.checked = s.areas.includes(box.value);
  select("trim_scope").value = s.trim_scope;
  select("language").value = s.language;
  updateDependentFields();
}

function numberOr(name: string, fallback: number): number {
  const value = Number.parseInt(field(name).value, 10);
  return Number.isFinite(value) ? value : fallback;
}

function read(base: Settings): Settings {
  const s: Settings = { ...base };
  for (const name of CHECKBOXES) s[name] = field(name).checked;
  s.threshold_percent = numberOr("threshold_percent", base.threshold_percent);
  s.cooldown_minutes = numberOr("cooldown_minutes", base.cooldown_minutes);
  s.areas = areaBoxes().filter((b) => b.checked).map((b) => b.value);
  s.trim_scope = select("trim_scope").value === "apps" ? "apps" : "all";
  const language = select("language").value;
  s.language = language === "es" || language === "en" ? language : "auto";
  return s;
}

function updateDependentFields() {
  el("threshold-out").textContent = `${field("threshold_percent").value} %`;
  // "Recortar procesos" solo tiene sentido con "Working sets" marcado.
  const workingSets = areaBoxes().find((b) => b.value === "workingSets");
  select("trim_scope").disabled = !workingSets?.checked;
}

function setMessage(ok: string, problems: string[] = []) {
  el("settings-ok").textContent = ok;
  el("settings-ok").hidden = ok === "";
  el("settings-problems").replaceChildren(...problems.map((p) => li("fail", p)));
}

async function openSettings() {
  setMessage("");
  current = await invoke<Settings>("get_settings");
  fill(current);
  showView("settings");
}

async function save(event: SubmitEvent) {
  event.preventDefault();
  if (!current) return;
  const wanted = read(current);
  if (!areaBoxes("#areas-list").some((b) => b.checked)) {
    setMessage("", [t("pickOneArea")]);
    return;
  }
  const button = el("settings-save") as HTMLButtonElement;
  button.disabled = true;
  try {
    const result = await invoke<SaveResult>("save_settings", { settings: wanted });
    current = result.settings;
    // Primero el idioma, para que el mensaje de abajo ya salga en el idioma nuevo.
    setLang(result.language);
    fill(current); // por si Rust corrigió algún valor fuera de rango
    setMessage(result.warnings.length === 0 ? t("saved") : t("savedWithWarnings"), result.warnings);
  } catch (e) {
    setMessage("", [String(e)]);
  } finally {
    button.disabled = false;
  }
}

export async function initSettings(onError: (message: string) => void) {
  const info = await invoke<SettingsInfo>("get_settings_info");

  el("areas-list").replaceChildren(...info.memoryAreas.map(areaCheckbox));
  el("extras-list").replaceChildren(...info.extraAreas.map(areaCheckbox));

  const setRange = (name: string, [min, max]: [number, number]) => {
    field(name).min = String(min);
    field(name).max = String(max);
  };
  setRange("threshold_percent", info.thresholdRange);
  setRange("cooldown_minutes", info.cooldownRange);

  form().addEventListener("input", updateDependentFields);
  form().addEventListener("submit", (e) => save(e as SubmitEvent));
  el("settings-btn").addEventListener("click", () => openSettings().catch((e) => onError(String(e))));
  el("settings-back").addEventListener("click", () => showView("main"));

  // Desde el menú de la bandeja "Ajustes" (con la ventana ya abierta).
  await listen("open-settings", () => openSettings().catch((e) => onError(String(e))));

  // Si se cambia la limpieza automática desde la bandeja con Ajustes abiertos,
  // solo se actualiza esa casilla para no perder lo que se estaba editando.
  await listen<Settings>("settings-changed", (event) => {
    if (current) current = { ...current, auto_clean: event.payload.auto_clean };
    field("auto_clean").checked = event.payload.auto_clean;
  });

  // Ventana recién creada desde "Ajustes" de la bandeja: index.html#settings
  if (location.hash === "#settings") await openSettings();
}
