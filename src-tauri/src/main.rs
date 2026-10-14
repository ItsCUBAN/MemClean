// En la versión final no se abre una consola negra junto a la ventana.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod auto;
mod autostart;
mod cleaner;
mod commands;
mod history;
mod hotkey;
mod memory;
mod ntapi;
mod settings;
mod tray;
mod window;

use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, RunEvent};
use tauri_plugin_global_shortcut::ShortcutState;
use tauri_plugin_notification::NotificationExt;

use auto::{AutoState, Trigger};
use history::{History, HistoryEntry};
use settings::{Settings, SettingsState};

/// Problemas al arrancar, para enseñarlos cuando se abra la ventana.
pub struct StartupIssues(Mutex<Vec<String>>);

impl StartupIssues {
    pub fn all(&self) -> Vec<String> {
        self.0.lock().map(|list| list.clone()).unwrap_or_default()
    }
}

/// Apunta un problema y, si la ventana está abierta, lo enseña ya.
fn report_issue(app: &AppHandle, message: String) {
    if let Ok(mut list) = app.state::<StartupIssues>().0.lock() {
        list.push(message.clone());
    }
    let _ = app.emit("app-error", message);
}

/// Cambia un ajuste desde fuera de la ventana (por ejemplo, el menú de la bandeja)
/// y lo guarda en config.toml.
pub fn update_settings(app: &AppHandle, change: impl FnOnce(&mut Settings)) {
    let state = app.state::<SettingsState>();
    let mut current = state.get();
    change(&mut current);
    state.set(current.clone());
    if let Err(e) = settings::save(&current) {
        let _ = app.emit("app-error", e);
    }
    let _ = app.emit("settings-changed", current);
}

/// Lanza una limpieza en segundo plano (la ventana no se congela mientras tanto),
/// la apunta en el historial y avisa a la interfaz al empezar y al terminar.
pub fn start_clean(app: &AppHandle, trigger: Trigger) {
    // Se apunta ya, para que la limpieza automática no se dispare mientras tanto.
    app.state::<AutoState>().mark_cleaned();
    let settings = app.state::<SettingsState>().get();

    let app = app.clone();
    thread::spawn(move || {
        let _ = app.emit("clean-started", ());
        match cleaner::clean(&settings.areas) {
            Ok(report) => {
                let entry = app.state::<History>().push(trigger, report);
                // Con el botón ya se ve el resultado en la ventana; en el resto, aviso de Windows.
                if trigger != Trigger::Button && settings.notifications {
                    notify(&app, &entry);
                }
                let _ = app.emit("clean-done", entry);
            }
            Err(message) => {
                let _ = app.emit("clean-error", message);
            }
        }
    });
}

fn notify(app: &AppHandle, entry: &HistoryEntry) {
    let failed = entry.report.results.iter().filter(|r| r.error.is_some()).count();
    let mut body = format!(
        "{}: {} liberados.",
        entry.trigger_label,
        format_freed(entry.report.freed_bytes)
    );
    if failed > 0 {
        body.push_str(&format!(" {failed} área(s) fallaron; ábrela para ver el motivo."));
    }
    let _ = app.notification().builder().title("MemClean").body(body).show();
}

/// 512 MB, 1.25 GB… (si es negativo, otro programa ocupó memoria a la vez: 0 MB).
fn format_freed(bytes: i64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mb = bytes.max(0) as f64 / MB;
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

/// Bucle de fondo: cada segundo lee la memoria, actualiza el icono, avisa a la
/// ventana (si está abierta) y decide si toca una limpieza automática.
fn monitor(app: AppHandle, tray_icon: tauri::tray::TrayIcon) {
    let mut reader = memory::MemoryReader::new();
    let mut updater = tray::TrayUpdater::new(tray_icon);
    loop {
        match reader.read() {
            Ok(stats) => {
                updater.update(&stats);
                // Se leen cada vez, así un cambio en Ajustes se nota al segundo siguiente.
                let settings = app.state::<SettingsState>().get();
                let auto_state = app.state::<AutoState>();
                let decision = auto::decide(
                    &settings.policy(),
                    settings.auto_clean,
                    stats.phys_percent,
                    auto_state.since_last_clean(),
                    auto_state.since_start(),
                );
                if let Some(trigger) = decision {
                    start_clean(&app, trigger);
                }
                let _ = app.emit("memory-stats", stats);
            }
            Err(message) => {
                let _ = app.emit("memory-error", message);
            }
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn main() {
    let (settings, settings_warning) = settings::load();

    let shortcut_plugin = tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                start_clean(app, Trigger::Hotkey);
            }
        })
        .build();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(shortcut_plugin)
        .manage(AutoState::default())
        .manage(History::default())
        .manage(SettingsState(RwLock::new(settings)))
        .manage(StartupIssues(Mutex::new(settings_warning.into_iter().collect())))
        .invoke_handler(tauri::generate_handler![
            commands::get_memory,
            commands::is_admin,
            commands::get_history,
            commands::get_startup_issues,
            commands::clean_now,
            commands::get_settings,
            commands::get_settings_info,
            commands::save_settings,
        ])
        .setup(|app| {
            let handle = app.handle();
            let settings = handle.state::<SettingsState>().get();

            let tray_icon = tray::create(handle)?;

            if let Err(e) = hotkey::apply(handle, &settings) {
                report_issue(handle, e);
            }

            // Se vuelve a crear la tarea en cada arranque por si MemClean cambió de carpeta.
            if settings.start_with_windows {
                let handle = handle.clone();
                thread::spawn(move || {
                    if let Err(e) = autostart::enable() {
                        report_issue(&handle, e);
                    }
                });
            }

            if !settings.start_minimized {
                window::show(handle);
            }

            let handle = handle.clone();
            thread::spawn(move || monitor(handle, tray_icon));
            Ok(())
        })
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(e) => {
            eprintln!("MemClean no pudo iniciarse: {e}");
            return;
        }
    };

    app.run(|_app, event| {
        // Cerrar la última ventana no cierra MemClean: sigue en la bandeja.
        // Solo "Salir" (que pasa un código de salida) termina la app.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::format_freed;

    #[test]
    fn freed_is_formatted_for_humans() {
        assert_eq!(format_freed(0), "0 MB");
        assert_eq!(format_freed(-5_000_000), "0 MB");
        assert_eq!(format_freed(300 * 1024 * 1024), "300 MB");
        assert_eq!(format_freed(1536 * 1024 * 1024), "1.50 GB");
    }
}
