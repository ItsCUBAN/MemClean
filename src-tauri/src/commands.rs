//! Funciones que la ventana (TypeScript) puede llamar con `invoke(...)`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::auto::Trigger;
use crate::cleaner::{self, Area};
use crate::history::{History, HistoryEntry};
use crate::i18n::{self, Lang};
use crate::settings::{self, Settings, SettingsState};
use crate::tray::TrayMenu;
use crate::{autostart, hotkey, memory, StartupIssues};

/// La interfaz lo llama al arrancar para pintar datos sin esperar 1 segundo.
#[tauri::command]
pub fn get_memory() -> Result<memory::MemoryStats, String> {
    memory::MemoryReader::new().read()
}

/// Para avisar en la ventana si falta ejecutar como administrador.
#[tauri::command]
pub fn is_admin() -> bool {
    cleaner::privileges::is_elevated()
}

/// Idioma en uso ("es" o "en"), ya resuelto si los ajustes dicen "automático".
#[tauri::command]
pub fn get_language() -> Lang {
    i18n::current()
}

#[tauri::command]
pub fn get_history(history: State<History>) -> Vec<HistoryEntry> {
    history.entries()
}

/// Problemas encontrados al arrancar (ajustes dañados, atajo ocupado…).
#[tauri::command]
pub fn get_startup_issues(issues: State<StartupIssues>) -> Vec<String> {
    issues.all()
}

/// Botón "Limpiar memoria". El resultado llega después con el evento "clean-done".
#[tauri::command]
pub fn clean_now(app: AppHandle) {
    crate::start_clean(&app, Trigger::Button);
}

#[tauri::command]
pub fn get_settings(state: State<SettingsState>) -> Settings {
    state.get()
}

/// Lo que la pantalla de Ajustes necesita para dibujarse: las áreas (la ventana
/// pone sus nombres en el idioma elegido), rangos permitidos y dónde se guarda el archivo.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInfo {
    areas: Vec<Area>,
    config_path: String,
    threshold_range: (u8, u8),
    cooldown_range: (u32, u32),
    interval_range: (u32, u32),
}

#[tauri::command]
pub fn get_settings_info() -> SettingsInfo {
    SettingsInfo {
        areas: Area::ALL.to_vec(),
        config_path: settings::config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|e| e),
        threshold_range: settings::THRESHOLD_RANGE,
        cooldown_range: settings::COOLDOWN_RANGE,
        interval_range: settings::INTERVAL_RANGE,
    }
}

#[derive(Serialize)]
pub struct SaveResult {
    /// Los ajustes tal y como quedaron (corregidos si algo estaba fuera de rango).
    settings: Settings,
    /// Cosas que no se pudieron aplicar, aunque el resto sí se guardó.
    warnings: Vec<String>,
    /// Idioma en uso tras guardar (por si se eligió "automático").
    language: Lang,
}

/// Cambia el idioma de toda la app: textos de Rust, menú de la bandeja y ventana.
pub fn apply_language(app: &AppHandle, settings: &Settings) -> Lang {
    let lang = i18n::resolve(settings.language);
    if lang != i18n::current() {
        i18n::set(lang);
        if let Some(menu) = app.try_state::<TrayMenu>() {
            menu.retranslate();
        }
        let _ = app.emit("language-changed", lang);
    }
    lang
}

/// Guarda los ajustes de la ventana y los aplica (idioma, atajo, inicio con Windows, menú).
/// Es `async` para que la ventana no se congele mientras se crea la tarea programada.
#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<SaveResult, String> {
    settings.validate()?;
    let mut settings = settings.sanitized();
    let mut warnings = Vec::new();

    // Primero el idioma, para que los avisos de abajo ya salgan en el idioma nuevo.
    let language = apply_language(&app, &settings);

    if let Err(e) = hotkey::apply(&app, &settings) {
        warnings.push(e);
    }

    let autostart = if settings.start_with_windows {
        autostart::enable()
    } else {
        autostart::disable()
    };
    if let Err(e) = autostart {
        warnings.push(e);
        // Guardamos la realidad, no lo que se pidió.
        settings.start_with_windows = autostart::is_enabled();
    }

    settings::save(&settings)?;
    app.state::<SettingsState>().set(settings.clone());
    if let Some(menu) = app.try_state::<TrayMenu>() {
        menu.set_auto_checked(settings.auto_clean);
    }
    let _ = app.emit("settings-changed", &settings);

    Ok(SaveResult { settings, warnings, language })
}
