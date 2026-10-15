//! Decide cuándo limpiar solo. Aquí no se toca Windows: es lógica pura,
//! para poder probarla con tests sin esperar minutos de verdad.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::i18n::tr;

/// Qué provocó una limpieza.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    /// Botón "Limpiar memoria" de la ventana.
    Button,
    /// "Limpiar ahora" en el menú de la bandeja.
    Tray,
    /// Atajo de teclado global (Ctrl+F1).
    Hotkey,
    /// La RAM llegó al umbral.
    Threshold,
}

impl Trigger {
    /// Limpieza pedida por la persona (no la automática): solo entonces se hacen los extras.
    pub fn is_manual(self) -> bool {
        !matches!(self, Trigger::Threshold)
    }

    /// Nombre para las notificaciones (la ventana usa su propio diccionario).
    pub fn label(self) -> &'static str {
        match self {
            Trigger::Button | Trigger::Tray => tr("Limpieza manual", "Manual cleaning"),
            Trigger::Hotkey => tr("Limpieza con atajo", "Hotkey cleaning"),
            Trigger::Threshold => tr("Limpieza automática", "Automatic cleaning"),
        }
    }
}

/// Reglas de la limpieza automática (salen de los ajustes, ver `Settings::policy`).
#[derive(Debug, Clone, Copy)]
pub struct Policy {
    /// Limpiar cuando el % de RAM usada sea mayor o igual que esto.
    pub threshold_percent: u8,
    /// Tiempo mínimo entre una limpieza y la siguiente limpieza automática.
    pub cooldown: Duration,
}

/// Decide si toca limpiar ahora.
///
/// - `percent`: % de RAM usada en este momento.
/// - `since_last_clean`: tiempo desde la última limpieza (de cualquier tipo),
///   o `None` si todavía no se ha limpiado nunca.
pub fn decide(
    policy: &Policy,
    enabled: bool,
    percent: f64,
    since_last_clean: Option<Duration>,
) -> Option<Trigger> {
    if !enabled {
        return None;
    }

    // Si la RAM ya está alta al arrancar, se limpia enseguida.
    let cooldown_over = since_last_clean.map_or(true, |elapsed| elapsed >= policy.cooldown);
    (percent >= f64::from(policy.threshold_percent) && cooldown_over).then_some(Trigger::Threshold)
}

/// Recuerda cuándo fue la última limpieza, para respetar el tiempo de espera.
/// (Si la limpieza automática está activada o no se guarda en los ajustes.)
#[derive(Default)]
pub struct AutoState {
    last_clean: Mutex<Option<Instant>>,
}

impl AutoState {
    /// Apunta que acaba de empezar una limpieza (manual o automática).
    /// Se apunta aunque luego falle, para no reintentar cada segundo.
    pub fn mark_cleaned(&self) {
        if let Ok(mut last) = self.last_clean.lock() {
            *last = Some(Instant::now());
        }
    }

    pub fn since_last_clean(&self) -> Option<Duration> {
        self.last_clean.lock().ok().and_then(|last| last.map(|t| t.elapsed()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: Duration = Duration::from_secs(60);

    /// Los valores por defecto: 70 % y 5 minutos de espera.
    fn policy() -> Policy {
        Policy { threshold_percent: 70, cooldown: 5 * MIN }
    }

    #[test]
    fn below_threshold_does_nothing() {
        assert_eq!(decide(&policy(), true, 69.9, None), None);
        assert_eq!(decide(&policy(), true, 10.0, Some(60 * MIN)), None);
    }

    #[test]
    fn at_or_above_threshold_cleans_when_never_cleaned() {
        assert_eq!(decide(&policy(), true, 70.0, None), Some(Trigger::Threshold));
        assert_eq!(decide(&policy(), true, 95.0, None), Some(Trigger::Threshold));
    }

    #[test]
    fn cooldown_blocks_until_it_has_passed() {
        let p = policy();
        assert_eq!(decide(&p, true, 85.0, Some(Duration::ZERO)), None);
        assert_eq!(decide(&p, true, 85.0, Some(4 * MIN + Duration::from_secs(59))), None);
        assert_eq!(decide(&p, true, 85.0, Some(5 * MIN)), Some(Trigger::Threshold));
        assert_eq!(decide(&p, true, 85.0, Some(30 * MIN)), Some(Trigger::Threshold));
    }

    #[test]
    fn disabled_never_cleans() {
        assert_eq!(decide(&policy(), false, 99.0, None), None);
        assert_eq!(decide(&policy(), false, 99.0, Some(60 * MIN)), None);
    }

    #[test]
    fn custom_threshold_is_respected() {
        let p = Policy { threshold_percent: 50, ..policy() };
        assert_eq!(decide(&p, true, 49.0, None), None);
        assert_eq!(decide(&p, true, 50.0, None), Some(Trigger::Threshold));
    }

    #[test]
    fn only_the_threshold_is_automatic() {
        assert!(Trigger::Button.is_manual());
        assert!(Trigger::Tray.is_manual());
        assert!(Trigger::Hotkey.is_manual());
        assert!(!Trigger::Threshold.is_manual());
    }

    #[test]
    fn state_remembers_last_clean() {
        let state = AutoState::default();
        assert!(state.since_last_clean().is_none());
        state.mark_cleaned();
        assert!(state.since_last_clean().is_some_and(|d| d < MIN));
    }
}
