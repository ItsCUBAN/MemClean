//! Icono de la bandeja del sistema: dibujo dinámico, menú y doble clic.

pub mod icon;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::auto::Trigger;
use crate::memory::MemoryStats;
use crate::settings::SettingsState;
use crate::window;

/// La casilla "Limpieza automática" del menú, para marcarla o desmarcarla
/// cuando se cambia desde la ventana de Ajustes.
pub struct AutoMenuItem(pub CheckMenuItem<Wry>);

/// Crea el icono de la bandeja con su menú.
pub fn create(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let clean = MenuItem::with_id(app, "clean", "Limpiar ahora", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Mostrar ventana", true, None::<&str>)?;
    let auto_on = app.state::<SettingsState>().get().auto_clean;
    let auto = CheckMenuItem::with_id(app, "auto", "Limpieza automática", true, auto_on, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Ajustes", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&clean, &show, &auto, &settings, &separator, &quit])?;
    app.manage(AutoMenuItem(auto.clone()));

    let auto_item = auto.clone();
    TrayIconBuilder::with_id("main")
        .icon(Image::new_owned(icon::render(0), icon::SIZE, icon::SIZE))
        .tooltip("MemClean")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "clean" => crate::start_clean(app, Trigger::Tray),
            "show" => window::show(app),
            "auto" => {
                // Windows ya cambió la marca del menú; copiamos su estado.
                let enabled = auto_item.is_checked().unwrap_or(true);
                crate::update_settings(app, |s| s.auto_clean = enabled);
            }
            "settings" => window::show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = event {
                window::show(tray.app_handle());
            }
        })
        .build(app)
}

/// Mantiene el icono al día sin redibujarlo si el número no ha cambiado.
pub struct TrayUpdater {
    tray: TrayIcon,
    last_percent: Option<u8>,
    last_tooltip: String,
}

impl TrayUpdater {
    pub fn new(tray: TrayIcon) -> Self {
        Self { tray, last_percent: None, last_tooltip: String::new() }
    }

    pub fn update(&mut self, stats: &MemoryStats) {
        let percent = stats.phys_percent.round().clamp(0.0, 100.0) as u8;
        if self.last_percent != Some(percent) {
            let image = Image::new_owned(icon::render(percent), icon::SIZE, icon::SIZE);
            if self.tray.set_icon(Some(image)).is_ok() {
                self.last_percent = Some(percent);
            }
        }

        const GB: f64 = 1024.0 * 1024.0 * 1024.0;
        let tooltip = format!(
            "MemClean — RAM {percent} %\n{:.1} GB de {:.1} GB en uso",
            stats.phys_used as f64 / GB,
            stats.phys_total as f64 / GB,
        );
        if tooltip != self.last_tooltip && self.tray.set_tooltip(Some(&tooltip)).is_ok() {
            self.last_tooltip = tooltip;
        }
    }
}
