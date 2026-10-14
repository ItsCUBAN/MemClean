import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { el, li, showView } from "./dom";

// Debe coincidir con `Settings` en src-tauri/src/settings.rs.
interface Settings {
  auto_clean: boolean;
  threshold_percent: number;
  cooldown_minutes: number;
  interval_enabled: boolean;
  interval_minutes: number;
  areas: string[];
  hotkey_enabled: boolean;
  hotkey: string;
  start_with_windows: boolean;
  start_minimized: boolean;
  notifications: boolean;
}

interface SettingsInfo {
  areas: { id: string; label: string }[];
  configPath: string;
  thresholdRange: [number, number];
  cooldownRange: [number, number];
  intervalRange: [number, number];
}

interface SaveResult {
  settings: Settings;
  warnings: string[];
}

// Casillas cuyo nombre coincide con un ajuste de sí/no.
const CHECKBOXES = [
  "auto_clean",
  "interval_enabled",
  "hotkey_enabled",
  "start_with_windows",
  "start_minimized",
  "notifications",
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

function areaBoxes(): HTMLInputElement[] {
  return Array.from(form().querySelectorAll<HTMLInputElement>('input[name="area"]'));
}

function fill(s: Settings) {
  for (const name of CHECKBOXES) field(name).checked = s[name];
  field("threshold_percent").value = String(s.threshold_percent);
  field("cooldown_minutes").value = String(s.cooldown_minutes);
  field("interval_minutes").value = String(s.interval_minutes);
  for (const box of areaBoxes()) box.checked = s.areas.includes(box.value);
  el("hotkey-label").textContent = s.hotkey;
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
  s.interval_minutes = numberOr("interval_minutes", base.interval_minutes);
  s.areas = areaBoxes().filter((b) => b.checked).map((b) => b.value);
  return s;
}

function updateDependentFields() {
  el("threshold-out").textContent = `${field("threshold_percent").value} %`;
  field("interval_minutes").disabled = !field("interval_enabled").checked;
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
  if (wanted.areas.length === 0) {
    setMessage("", ["Marca al menos un área para limpiar."]);
    return;
  }
  const button = el("settings-save") as HTMLButtonElement;
  button.disabled = true;
  try {
    const result = await invoke<SaveResult>("save_settings", { settings: wanted });
    current = result.settings;
    fill(current); // por si Rust corrigió algún valor fuera de rango
    setMessage(
      result.warnings.length === 0 ? "Ajustes guardados." : "Guardado, pero con avisos:",
      result.warnings,
    );
  } catch (e) {
    setMessage("", [String(e)]);
  } finally {
    button.disabled = false;
  }
}

export async function initSettings(onError: (message: string) => void) {
  const info = await invoke<SettingsInfo>("get_settings_info");

  // Una casilla por área, con los nombres que da Rust.
  el("areas-list").replaceChildren(
    ...info.areas.map((area) => {
      const label = document.createElement("label");
      label.className = "check";
      const box = document.createElement("input");
      box.type = "checkbox";
      box.name = "area";
      box.value = area.id;
      label.append(box, ` ${area.label}`);
      return label;
    }),
  );

  const setRange = (name: string, [min, max]: [number, number]) => {
    field(name).min = String(min);
    field(name).max = String(max);
  };
  setRange("threshold_percent", info.thresholdRange);
  setRange("cooldown_minutes", info.cooldownRange);
  setRange("interval_minutes", info.intervalRange);
  el("config-path").textContent = `Se guardan en ${info.configPath}`;

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
