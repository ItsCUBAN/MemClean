//! Atajo de teclado global (por defecto Ctrl+F1): limpia aunque MemClean
//! no tenga la ventana abierta ni el foco.

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::i18n::tr;
use crate::settings::Settings;

/// Convierte un texto como "Ctrl+F1" en un atajo que Windows entiende.
pub fn parse(text: &str) -> Result<Shortcut, String> {
    text.parse::<Shortcut>().map_err(|e| {
        tr(
            format!("El atajo «{text}» no es válido: {e}"),
            format!("The hotkey \"{text}\" is not valid: {e}"),
        )
    })
}

/// Quita el atajo anterior y registra el de los ajustes (si está activado).
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
    let shortcut = parse(&settings.hotkey)?;
    shortcuts.register(shortcut).map_err(|e| {
        let hotkey = &settings.hotkey;
        tr(
            format!("No se pudo activar el atajo {hotkey}: puede que otro programa ya lo use. ({e})"),
            format!("Could not enable the hotkey {hotkey}: another program may be using it. ({e})"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    #[test]
    fn default_hotkey_is_ctrl_f1() {
        let shortcut = parse(&Settings::default().hotkey).expect("Ctrl+F1 válido");
        assert_eq!(shortcut, Shortcut::new(Some(Modifiers::CONTROL), Code::F1));
    }

    #[test]
    fn invalid_hotkey_gives_readable_error() {
        let err = parse("Ctrl+Patata").expect_err("debe fallar");
        assert!(err.contains("Ctrl+Patata"));
    }
}
