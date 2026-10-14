//! Idioma de MemClean (español o inglés).
//!
//! Cada texto se escribe en los dos idiomas en el mismo sitio donde se usa:
//! `tr("Limpiar ahora", "Clean now")`. Así la traducción queda al lado del
//! original y es fácil de revisar.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};
use windows::Win32::Globalization::GetUserDefaultUILanguage;

/// Lo que el usuario elige en Ajustes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguagePref {
    /// Sigue el idioma de Windows.
    #[default]
    Auto,
    Es,
    En,
}

/// El idioma que se está usando de verdad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    Es,
    En,
}

/// Idioma activo: 0 = español, 1 = inglés.
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(if lang == Lang::En { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 1 {
        Lang::En
    } else {
        Lang::Es
    }
}

/// Devuelve el texto en el idioma activo.
pub fn tr<T>(es: T, en: T) -> T {
    pick(current(), es, en)
}

fn pick<T>(lang: Lang, es: T, en: T) -> T {
    match lang {
        Lang::Es => es,
        Lang::En => en,
    }
}

/// Traduce la preferencia a un idioma concreto. "Automático" usa español si
/// Windows está en español (cualquier país) e inglés en cualquier otro caso.
pub fn resolve(pref: LanguagePref) -> Lang {
    match pref {
        LanguagePref::Es => Lang::Es,
        LanguagePref::En => Lang::En,
        // SAFETY: función de Windows sin parámetros que solo devuelve un número.
        LanguagePref::Auto => from_windows_langid(unsafe { GetUserDefaultUILanguage() }),
    }
}

/// Los 10 bits bajos del identificador de idioma de Windows son el idioma
/// principal; 0x0A es el español (España, México, Argentina…).
fn from_windows_langid(langid: u16) -> Lang {
    const LANG_SPANISH: u16 = 0x0A;
    if langid & 0x3FF == LANG_SPANISH {
        Lang::Es
    } else {
        Lang::En
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_spanish_variant_is_spanish() {
        assert_eq!(from_windows_langid(0x0C0A), Lang::Es); // España
        assert_eq!(from_windows_langid(0x080A), Lang::Es); // México
        assert_eq!(from_windows_langid(0x2C0A), Lang::Es); // Argentina
        assert_eq!(from_windows_langid(0x540A), Lang::Es); // EE. UU.
    }

    #[test]
    fn other_languages_fall_back_to_english() {
        assert_eq!(from_windows_langid(0x0409), Lang::En); // inglés EE. UU.
        assert_eq!(from_windows_langid(0x0407), Lang::En); // alemán
        assert_eq!(from_windows_langid(0x0416), Lang::En); // portugués (Brasil)
    }

    #[test]
    fn explicit_choice_ignores_windows() {
        assert_eq!(resolve(LanguagePref::Es), Lang::Es);
        assert_eq!(resolve(LanguagePref::En), Lang::En);
    }

    #[test]
    fn pick_returns_the_right_text() {
        assert_eq!(pick(Lang::Es, "Salir", "Exit"), "Salir");
        assert_eq!(pick(Lang::En, "Salir", "Exit"), "Exit");
    }

    #[test]
    fn preference_is_stored_in_lowercase() {
        #[derive(Serialize, Deserialize)]
        struct S {
            language: LanguagePref,
        }
        let text = toml::to_string(&S { language: LanguagePref::En }).expect("toml");
        assert_eq!(text.trim(), r#"language = "en""#);
        let back: S = toml::from_str(r#"language = "auto""#).expect("toml");
        assert_eq!(back.language, LanguagePref::Auto);
    }
}
