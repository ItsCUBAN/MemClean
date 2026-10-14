//! Funciones que la ventana (TypeScript) puede llamar con `invoke(...)`.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::auto::Trigger;
use crate::cleaner::{self, Area};
use crate::history::{History, HistoryEntry};
use crate::settings::{self, Settings, SettingsState};
use crate::tray::AutoMenuItem;
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaInfo {
    id: Area,
    label: &'static str,
}

/// Lo que la pantalla de Ajustes necesita para dibujarse: nombres de las áreas,
/// rangos permitidos y dónde se guarda el archivo.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInfo {
    areas: Vec<AreaInfo>,
    config_path: String,
    threshold_range: (u8, u8),
    cooldown_range: (u32, u32),
    interval_range: (u32, u32),
}

#[tauri::command]
pub fn get_settings_info() -> SettingsInfo {
    SettingsInfo {
        areas: Area::ALL.into_iter().map(|id| AreaInfo { id, label: id.label() }).collect(),
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
}

/// Guarda los ajustes de la ventana y los aplica (atajo, inicio con Windows, menú).
/// Es `async` para que la ventana no se congele mientras se crea la tarea programada.
#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<SaveResult, String> {
    settings.validate()?;
    let mut settings = settings.sanitized();
    let mut warnings = Vec::new();

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
    if let Some(item) = app.try_state::<AutoMenuItem>() {
        let _ = item.0.set_checked(settings.auto_clean);
    }
    let _ = app.emit("settings-changed", &settings);

    Ok(SaveResult { settings, warnings })
}
