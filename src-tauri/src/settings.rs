//! Ajustes de MemClean, guardados en %APPDATA%\MemClean\config.toml.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};

use crate::auto::{Policy, Trigger};
use crate::cleaner::{Area, TrimScope};
use crate::i18n::{tr, LanguagePref};

pub const THRESHOLD_RANGE: (u8, u8) = (50, 95);
pub const COOLDOWN_RANGE: (u32, u32) = (1, 120);

/// Todo lo que el usuario puede cambiar. `#[serde(default)]` hace que, si al
/// archivo le falta algún ajuste (por ejemplo, de una versión anterior),
/// se use su valor por defecto en vez de fallar. Los ajustes que ya no existen
/// (por ejemplo, la limpieza por intervalo de la 0.1.x) simplemente se ignoran.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub auto_clean: bool,
    pub threshold_percent: u8,
    pub cooldown_minutes: u32,
    /// Áreas de memoria y extras marcados.
    #[serde(deserialize_with = "known_areas")]
    pub areas: Vec<Area>,
    /// Qué procesos recorta "Working sets": todos o solo las apps.
    pub trim_scope: TrimScope,
    /// Atajo global Ctrl+F1 activado o no.
    pub hotkey_enabled: bool,
    pub start_with_windows: bool,
    pub start_minimized: bool,
    pub notifications: bool,
    /// Bloque "Archivo de paginación" en la ventana.
    pub show_page_file: bool,
    /// "auto" (idioma de Windows), "es" o "en".
    pub language: LanguagePref,
}

/// Lee la lista de áreas saltándose las que ya no existen, en vez de
/// rechazar todo el archivo de ajustes por una sola.
fn known_areas<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Area>, D::Error> {
    let ids = Vec::<String>::deserialize(deserializer)?;
    Ok(ids.iter().filter_map(|id| Area::from_id(id)).collect())
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_clean: true,
            threshold_percent: 70,
            cooldown_minutes: 5,
            areas: Area::defaults(),
            trim_scope: TrimScope::All,
            hotkey_enabled: true,
            start_with_windows: false,
            start_minimized: false,
            notifications: true,
            show_page_file: false,
            language: LanguagePref::Auto,
        }
    }
}

impl Settings {
    /// Corrige valores fuera de rango, quita áreas repetidas y las ordena.
    /// Si no queda ningún área de memoria, vuelven las de por defecto (los extras se mantienen).
    pub fn sanitized(mut self) -> Self {
        self.threshold_percent = self.threshold_percent.clamp(THRESHOLD_RANGE.0, THRESHOLD_RANGE.1);
        self.cooldown_minutes = self.cooldown_minutes.clamp(COOLDOWN_RANGE.0, COOLDOWN_RANGE.1);
        let has_memory_area = self.areas.iter().any(|a| !a.is_extra());
        self.areas = Area::ALL
            .into_iter()
            .filter(|a| self.areas.contains(a) || (!has_memory_area && !a.is_extra()))
            .collect();
        self
    }

    /// Comprueba lo que llega desde la ventana antes de aceptarlo.
    pub fn validate(&self) -> Result<(), String> {
        if !self.areas.iter().any(|a| !a.is_extra()) {
            return Err(tr(
                "Marca al menos un área de memoria para limpiar.",
                "Select at least one memory area to clean.",
            )
            .into());
        }
        Ok(())
    }

    /// Lo que se limpia según quién lanzó la limpieza: los extras, solo a mano.
    pub fn areas_for(&self, trigger: Trigger) -> Vec<Area> {
        let manual = trigger.is_manual();
        self.areas.iter().copied().filter(|a| manual || !a.is_extra()).collect()
    }

    pub fn policy(&self) -> Policy {
        Policy {
            threshold_percent: self.threshold_percent,
            cooldown: Duration::from_secs(u64::from(self.cooldown_minutes) * 60),
        }
    }
}

/// Ajustes en uso, compartidos entre la ventana, la bandeja y el bucle de fondo.
pub struct SettingsState(pub RwLock<Settings>);

impl SettingsState {
    pub fn get(&self) -> Settings {
        self.0.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn set(&self, settings: Settings) {
        if let Ok(mut current) = self.0.write() {
            *current = settings;
        }
    }
}

/// %APPDATA%\MemClean\config.toml
pub fn config_path() -> Result<PathBuf, String> {
    let appdata = std::env::var_os("APPDATA").ok_or(tr(
        "No se encontró la carpeta %APPDATA% de Windows.",
        "The Windows %APPDATA% folder was not found.",
    ))?;
    Ok(PathBuf::from(appdata).join("MemClean").join("config.toml"))
}

/// Lee los ajustes. Nunca falla: si hay un problema, usa los de por defecto
/// y devuelve también un aviso para enseñarlo en la ventana.
pub fn load() -> (Settings, Option<String>) {
    match config_path() {
        Ok(path) => load_from(&path),
        Err(e) => (Settings::default(), Some(e)),
    }
}

pub fn load_from(path: &Path) -> (Settings, Option<String>) {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        // Primera vez: todavía no hay archivo; no es un error.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Settings::default(), None),
        Err(e) => {
            let warning = tr(
                format!("No se pudieron leer los ajustes ({}): {e}", path.display()),
                format!("Could not read the settings ({}): {e}", path.display()),
            );
            return (Settings::default(), Some(warning));
        }
    };
    match toml::from_str::<Settings>(&text) {
        Ok(settings) => (settings.sanitized(), None),
        Err(e) => {
            let warning = tr(
                format!(
                    "El archivo de ajustes tiene un error y se usan los valores por defecto. \
                     No se ha borrado; corrígelo o guarda los ajustes de nuevo. ({}): {e}",
                    path.display()
                ),
                format!(
                    "The settings file has an error, so the defaults are used. \
                     It was not deleted; fix it or save the settings again. ({}): {e}",
                    path.display()
                ),
            );
            (Settings::default(), Some(warning))
        }
    }
}

pub fn save(settings: &Settings) -> Result<(), String> {
    save_to(&config_path()?, settings)
}

/// Guarda primero en un archivo temporal y luego lo renombra: así, si el PC
/// se apaga a mitad, el config.toml anterior queda intacto.
pub fn save_to(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| {
            tr(
                format!("No se pudo crear la carpeta {}: {e}", dir.display()),
                format!("Could not create the folder {}: {e}", dir.display()),
            )
        })?;
    }
    let text = toml::to_string_pretty(settings).map_err(|e| {
        tr(
            format!("No se pudieron convertir los ajustes a texto: {e}"),
            format!("Could not convert the settings to text: {e}"),
        )
    })?;
    let header = tr(
        "# Ajustes de MemClean. Puedes editarlos con la app cerrada.",
        "# MemClean settings. You can edit them while the app is closed.",
    );
    let content = format!("{header}\n\n{text}");
    let save_error = |e: std::io::Error| {
        tr(
            format!("No se pudieron guardar los ajustes: {e}"),
            format!("Could not save the settings: {e}"),
        )
    };
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, content).map_err(save_error)?;
    fs::rename(&tmp, path).map_err(save_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("memclean-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("config.toml")
    }

    #[test]
    fn defaults_match_spec() {
        let s = Settings::default();
        assert!(s.auto_clean);
        assert_eq!(s.threshold_percent, 70);
        assert_eq!(s.cooldown_minutes, 5);
        assert!(s.hotkey_enabled);
        assert_eq!(s.areas, Area::defaults());
        assert!(s.notifications);
        assert!(!s.show_page_file);
        assert_eq!(s.trim_scope, TrimScope::All);
        assert_eq!(s.language, LanguagePref::Auto);
    }

    #[test]
    fn extras_only_run_when_cleaning_by_hand() {
        let s = Settings {
            areas: vec![Area::TempFiles, Area::Clipboard, Area::StandbyList],
            ..Settings::default()
        };
        for trigger in [Trigger::Button, Trigger::Tray, Trigger::Hotkey] {
            assert_eq!(s.areas_for(trigger), s.areas);
        }
        assert_eq!(s.areas_for(Trigger::Threshold), vec![Area::StandbyList]);
    }

    #[test]
    fn at_least_one_memory_area_is_needed() {
        let only_extras = Settings { areas: vec![Area::DnsCache], ..Settings::default() };
        assert!(only_extras.validate().is_err());
        // Al cargar un archivo así, vuelven las de memoria y se conserva el extra.
        let mut expected = vec![Area::DnsCache];
        expected.extend(Area::defaults());
        assert_eq!(only_extras.sanitized().areas, expected);
    }

    #[test]
    fn trim_scope_is_saved_as_text() {
        let s: Settings = toml::from_str(r#"trim_scope = "apps""#).expect("toml");
        assert_eq!(s.trim_scope, TrimScope::Apps);
        let text = toml::to_string(&Settings::default()).expect("toml");
        assert!(text.contains(r#"trim_scope = "all""#));
    }

    #[test]
    fn config_from_0_1_x_loads_without_warning() {
        // Ajustes que ya no existen (intervalo, atajo personalizado, dos áreas):
        // se ignoran y el resto se conserva.
        let path = temp_file("old-version");
        fs::create_dir_all(path.parent().expect("carpeta")).expect("crear carpeta");
        let old = r#"
            auto_clean = false
            threshold_percent = 80
            interval_enabled = true
            interval_minutes = 30
            areas = ["workingSets", "lowPriorityStandby", "standbyList", "combineMemoryLists"]
            hotkey_enabled = true
            hotkey = "Ctrl+F2"
            start_with_windows = true
        "#;
        fs::write(&path, old).expect("escribir");
        let (s, warning) = load_from(&path);
        assert_eq!(warning, None);
        assert!(!s.auto_clean);
        assert_eq!(s.threshold_percent, 80);
        assert_eq!(s.areas, vec![Area::WorkingSets, Area::StandbyList]);
        assert!(s.start_with_windows);
        assert!(!s.show_page_file);
        let _ = fs::remove_dir_all(path.parent().expect("carpeta"));
    }

    #[test]
    fn only_removed_areas_fall_back_to_defaults() {
        let s: Settings = toml::from_str(r#"areas = ["combineMemoryLists"]"#).expect("toml");
        assert_eq!(s.sanitized().areas, Area::defaults());
    }

    #[test]
    fn config_from_older_version_gets_automatic_language() {
        // Un config.toml de la versión 0.1.0 no tiene "language".
        let s: Settings = toml::from_str("auto_clean = true\nthreshold_percent = 70").expect("toml");
        assert_eq!(s.language, LanguagePref::Auto);
        let english: Settings = toml::from_str(r#"language = "en""#).expect("toml");
        assert_eq!(english.language, LanguagePref::En);
    }

    #[test]
    fn save_and_load_round_trip() {
        let path = temp_file("roundtrip");
        let mut s = Settings::default();
        s.threshold_percent = 82;
        s.areas = vec![Area::TempFiles, Area::ModifiedList, Area::StandbyList];
        s.trim_scope = TrimScope::Apps;
        s.start_minimized = true;
        s.show_page_file = true;
        save_to(&path, &s).expect("guardar");
        let (loaded, warning) = load_from(&path);
        assert_eq!(warning, None);
        assert_eq!(loaded, s);
        let _ = fs::remove_dir_all(path.parent().expect("carpeta"));
    }

    #[test]
    fn missing_file_gives_defaults_without_warning() {
        let (s, warning) = load_from(&temp_file("missing"));
        assert_eq!(s, Settings::default());
        assert_eq!(warning, None);
    }

    #[test]
    fn broken_file_gives_defaults_with_warning_and_is_kept() {
        let path = temp_file("broken");
        fs::create_dir_all(path.parent().expect("carpeta")).expect("crear carpeta");
        fs::write(&path, "threshold_percent = \"mucho\"").expect("escribir");
        let (s, warning) = load_from(&path);
        assert_eq!(s, Settings::default());
        assert!(warning.is_some());
        assert!(path.exists(), "no debe borrar el archivo del usuario");
        let _ = fs::remove_dir_all(path.parent().expect("carpeta"));
    }

    #[test]
    fn missing_keys_use_defaults() {
        let s: Settings = toml::from_str("threshold_percent = 60").expect("toml");
        assert_eq!(s.threshold_percent, 60);
        assert_eq!(s.cooldown_minutes, 5);
        assert!(s.auto_clean);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let s = Settings {
            threshold_percent: 99,
            cooldown_minutes: 0,
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.threshold_percent, 95);
        assert_eq!(s.cooldown_minutes, 1);

        let low = Settings { threshold_percent: 10, ..Settings::default() }.sanitized();
        assert_eq!(low.threshold_percent, 50);
    }

    #[test]
    fn areas_are_deduplicated_and_ordered() {
        let s = Settings {
            areas: vec![Area::StandbyList, Area::WorkingSets, Area::StandbyList],
            ..Settings::default()
        }
        .sanitized();
        assert_eq!(s.areas, vec![Area::WorkingSets, Area::StandbyList]);
    }

    #[test]
    fn empty_areas_are_rejected_when_saving() {
        let s = Settings { areas: vec![], ..Settings::default() };
        assert!(s.validate().is_err());
    }

    #[test]
    fn policy_follows_settings() {
        let s = Settings {
            threshold_percent: 80,
            cooldown_minutes: 10,
            ..Settings::default()
        };
        let p = s.policy();
        assert_eq!(p.threshold_percent, 80);
        assert_eq!(p.cooldown, Duration::from_secs(600));
    }
}
