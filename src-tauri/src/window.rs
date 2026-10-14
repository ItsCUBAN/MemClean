//! La ventana principal se crea al abrirla y se destruye al cerrarla.
//! Así, mientras MemClean vive solo en la bandeja, WebView2 no ocupa RAM.

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const MAIN: &str = "main";

/// Muestra la ventana principal; si no existe, la crea.
pub fn show(app: &AppHandle) {
    open(app, false);
}

/// Muestra la ventana directamente en la pantalla de Ajustes.
pub fn show_settings(app: &AppHandle) {
    open(app, true);
}

fn open(app: &AppHandle, settings: bool) {
    if let Some(window) = app.get_webview_window(MAIN) {
        if settings {
            let _ = window.emit("open-settings", ());
        }
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    // Si la ventana es nueva, la interfaz mira el "#settings" de la dirección al cargar.
    let page = if settings { "index.html#settings" } else { "index.html" };

    // En Windows, crear una ventana desde el hilo del menú puede bloquear la app,
    // así que la creamos desde un hilo aparte.
    let app = app.clone();
    std::thread::spawn(move || {
        let built = WebviewWindowBuilder::new(&app, MAIN, WebviewUrl::App(page.into()))
            .title("MemClean")
            .inner_size(380.0, 420.0)
            .resizable(false)
            .maximizable(false)
            .center()
            .build();
        match built {
            Ok(window) => {
                let _ = window.set_focus();
            }
            Err(e) => eprintln!("No se pudo abrir la ventana: {e}"),
        }
    });
}
