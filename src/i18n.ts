// Textos de la ventana en español e inglés.
//
// En el HTML, un elemento con data-i18n="clave" recibe el texto de esa clave;
// con data-i18n-title, su "title" y "aria-label". En el código se usa t("clave").
// Los {nombres} entre llaves se sustituyen por valores: t("lastClean", { time: "10:00" }).

export type Lang = "es" | "en";

const TEXTS = {
  // Pantalla principal
  physical: { es: "Memoria física", en: "Physical memory" },
  pageFile: { es: "Archivo de paginación", en: "Page file" },
  systemCache: { es: "Caché del sistema", en: "System cache" },
  loading: { es: "Cargando…", en: "Loading…" },
  physValues: {
    es: "Usada {used} · Libre {free} · Total {total}",
    en: "Used {used} · Free {free} · Total {total}",
  },
  pageValues: {
    es: "Usado {used} · Libre {free} · Total {total}",
    en: "Used {used} · Free {free} · Total {total}",
  },
  noPageFile: { es: "Windows no tiene archivo de paginación.", en: "Windows has no page file." },
  cacheValues: { es: "Standby {standby} · Archivos {files}", en: "Standby {standby} · Files {files}" },
  notAvailable: { es: "no disponible", en: "not available" },
  clean: { es: "Limpiar memoria", en: "Clean memory" },
  cleaning: { es: "Limpiando…", en: "Cleaning…" },
  settings: { es: "Ajustes", en: "Settings" },
  allAreasOk: { es: "✓ Todas las áreas ({n})", en: "✓ All areas ({n})" },
  oneAreaOk: { es: "✓ 1 área bien", en: "✓ 1 area OK" },
  someAreasOk: { es: "✓ {n} áreas bien", en: "✓ {n} areas OK" },
  tempDeleted: { es: "✓ {size} de archivos temporales borrados", en: "✓ {size} of temporary files deleted" },
  diskShort: { es: "{size} disco", en: "{size} disk" },
  history: { es: "Historial", en: "History" },
  last10: { es: "últimas 10", en: "last 10" },

  // Origen de cada limpieza (src-tauri/src/auto.rs → Trigger)
  "trigger.button": { es: "Limpieza manual", en: "Manual cleaning" },
  "trigger.tray": { es: "Limpieza manual", en: "Manual cleaning" },
  "trigger.hotkey": { es: "Limpieza con atajo", en: "Hotkey cleaning" },
  "trigger.threshold": { es: "Limpieza automática", en: "Automatic cleaning" },

  // Áreas (src-tauri/src/cleaner/mod.rs → Area)
  "area.workingSets": { es: "Working sets de procesos", en: "Process working sets" },
  "area.systemFileCache": { es: "Caché de archivos del sistema", en: "System file cache" },
  "area.modifiedList": { es: "Lista Modified", en: "Modified list" },
  "area.standbyList": { es: "Lista Standby", en: "Standby list" },
  "area.clipboard": { es: "Vaciar el portapapeles", en: "Empty the clipboard" },
  "area.dnsCache": { es: "Vaciar la caché DNS", en: "Flush the DNS cache" },
  "area.tempFiles": {
    es: "Borrar archivos temporales (de más de 24 h)",
    en: "Delete temporary files (older than 24 h)",
  },

  // Pantalla de Ajustes
  back: { es: "← Volver", en: "← Back" },
  autoSection: { es: "Limpieza automática", en: "Automatic cleaning" },
  enabled: { es: "Activada", en: "Enabled" },
  thresholdLabel: { es: "Limpiar al llegar al", en: "Clean when RAM reaches" },
  cooldownLabel: { es: "Espera entre limpiezas", en: "Wait between cleanings" },
  minutes: { es: "min", en: "min" },
  areasSection: { es: "Áreas a limpiar", en: "Areas to clean" },
  trimLabel: { es: "Recortar procesos", en: "Trim processes" },
  trimAll: { es: "Todos", en: "All" },
  trimApps: { es: "Solo apps (sin Windows)", en: "Apps only (not Windows)" },
  extrasSection: { es: "Extras", en: "Extras" },
  extrasNote: {
    es: "Solo al limpiar a mano: botón, menú de la bandeja o Ctrl+F1.",
    en: "Only when cleaning by hand: button, tray menu or Ctrl+F1.",
  },
  generalSection: { es: "General", en: "General" },
  hotkey: { es: "Atajo global", en: "Global hotkey" },
  startWithWindows: { es: "Iniciar con Windows", en: "Start with Windows" },
  startMinimized: { es: "Iniciar minimizado (solo en la bandeja)", en: "Start minimized (tray only)" },
  notifications: { es: "Notificaciones", en: "Notifications" },
  showPageFile: { es: "Mostrar archivo de paginación", en: "Show page file" },
  language: { es: "Idioma", en: "Language" },
  languageAuto: { es: "Automático (Windows)", en: "Automatic (Windows)" },
  save: { es: "Guardar", en: "Save" },
  saved: { es: "Ajustes guardados.", en: "Settings saved." },
  savedWithWarnings: { es: "Guardado, pero con avisos:", en: "Saved, with warnings:" },
  pickOneArea: {
    es: "Marca al menos un área de memoria para limpiar.",
    en: "Select at least one memory area to clean.",
  },
} satisfies Record<string, Record<Lang, string>>;

export type Key = keyof typeof TEXTS;

let current: Lang = "es";
const listeners: Array<() => void> = [];

/// Formato de hora y números según el idioma (por ejemplo 10:05:07 p. m. o 10:05:07 PM).
export function locale(): string {
  return current === "en" ? "en-US" : "es-ES";
}

export function t(key: Key, vars: Record<string, string | number> = {}): string {
  return TEXTS[key][current].replace(/\{(\w+)\}/g, (match, name: string) =>
    name in vars ? String(vars[name]) : match,
  );
}

/// Para claves que se construyen con un identificador que llega de Rust
/// ("area." + "standbyList"). Si no existe, se muestra el identificador tal cual.
export function tKey(key: string): string {
  return key in TEXTS ? t(key as Key) : key;
}

/// Lo que hay que volver a dibujar cuando cambia el idioma.
export function onLangChange(listener: () => void) {
  listeners.push(listener);
}

export function setLang(next: Lang) {
  current = next;
  document.documentElement.lang = next;
  document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((element) => {
    element.textContent = tKey(`${element.dataset.i18n}`);
  });
  document.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((element) => {
    const text = tKey(`${element.dataset.i18nTitle}`);
    element.title = text;
    element.setAttribute("aria-label", text);
  });
  for (const listener of listeners) listener();
}
