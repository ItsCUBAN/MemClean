//! Atajo de teclado global Ctrl+F1: limpia aunque MemClean
//! no tenga la ventana abierta ni el foco.

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use crate::i18n::tr;
use crate::settings::Settings;

/// Quita el atajo y lo vuelve a registrar si está activado en los ajustes.
pub fn apply(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let shortcuts = app.global_shortcut();
    shortcuts.unregister_all().map_err(|e| {
        tr(
            format!("No se pudo quitar el atajo anterior: {e}"),
            format!("Could not remove the previous hotkey: {e}"),
        )
    })?;
    if !settings.hotkey_enabled {
        return Ok(());
    }
    let ctrl_f1 = Shortcut::new(Some(Modifiers::CONTROL), Code::F1);
    shortcuts.register(ctrl_f1).map_err(|e| {
        tr(
            format!("No se pudo activar el atajo Ctrl+F1: puede que otro programa ya lo use. ({e})"),
            format!("Could not enable the Ctrl+F1 hotkey: another program may be using it. ({e})"),
        )
    })
}
