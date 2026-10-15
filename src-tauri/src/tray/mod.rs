//! Icono de la bandeja del sistema: dibujo dinámico, menú y doble clic.

pub mod icon;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::auto::Trigger;
use crate::i18n::tr;
use crate::memory::MemoryStats;
use crate::settings::SettingsState;
use crate::window;

/// Textos del menú en el idioma activo.
fn labels() -> [&'static str; 5] {
    [
        tr("Limpiar ahora", "Clean now"),
        tr("Mostrar ventana", "Show window"),
        tr("Limpieza automática", "Automatic cleaning"),
        tr("Ajustes", "Settings"),
        tr("Salir", "Exit"),
    ]
}

/// Los elementos del menú, para cambiarles el texto si se cambia de idioma
/// y marcar o desmarcar "Limpieza automática" desde la ventana de Ajustes.
pub struct TrayMenu {
    clean: MenuItem<Wry>,
    show: MenuItem<Wry>,
    auto: CheckMenuItem<Wry>,
    settings: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

impl TrayMenu {
    pub fn set_auto_checked(&self, checked: bool) {
        let _ = self.auto.set_checked(checked);
    }

    /// Vuelve a poner los textos en el idioma activo.
    pub fn retranslate(&self) {
        let [clean, show, auto, settings, quit] = labels();
        let _ = self.clean.set_text(clean);
        let _ = self.show.set_text(show);
        let _ = self.auto.set_text(auto);
        let _ = self.settings.set_text(settings);
        let _ = self.quit.set_text(quit);
    }
}

/// Crea el icono de la bandeja con su menú.
pub fn create(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let [clean_text, show_text, auto_text, settings_text, quit_text] = labels();
    let clean = MenuItem::with_id(app, "clean", clean_text, true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", show_text, true, None::<&str>)?;
    let auto_on = app.state::<SettingsState>().get().auto_clean;
    let auto = CheckMenuItem::with_id(app, "auto", auto_text, true, auto_on, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", settings_text, true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", quit_text, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&clean, &show, &auto, &settings, &separator, &quit])?;

    let auto_item = auto.clone();
    app.manage(TrayMenu { clean, show, auto, settings, quit });
    let size = icon::tray_size();
    TrayIconBuilder::with_id("main")
        .icon(Image::new_owned(icon::render(0, size), size, size))
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
    /// Lado del icono en píxeles (el de la bandeja de Windows).
    size: u32,
    last_percent: Option<u8>,
    last_tooltip: String,
}

impl TrayUpdater {
    pub fn new(tray: TrayIcon) -> Self {
        Self { tray, size: icon::tray_size(), last_percent: None, last_tooltip: String::new() }
    }

    pub fn update(&mut self, stats: &MemoryStats) {
        let percent = stats.phys_percent.round().clamp(0.0, 100.0) as u8;
        if self.last_percent != Some(percent) {
            let image = Image::new_owned(icon::render(percent, self.size), self.size, self.size);
            if self.tray.set_icon(Some(image)).is_ok() {
                self.last_percent = Some(percent);
            }
        }

        const GB: f64 = 1024.0 * 1024.0 * 1024.0;
        let used = stats.phys_used as f64 / GB;
        let total = stats.phys_total as f64 / GB;
        // Al cambiar de idioma cambia el texto, así que se actualiza solo al segundo siguiente.
        let tooltip = tr(
            format!("MemClean — RAM {percent} %\n{used:.1} GB de {total:.1} GB en uso"),
            format!("MemClean — RAM {percent}%\n{used:.1} GB of {total:.1} GB in use"),
        );
        if tooltip != self.last_tooltip && self.tray.set_tooltip(Some(&tooltip)).is_ok() {
            self.last_tooltip = tooltip;
        }
    }
}
