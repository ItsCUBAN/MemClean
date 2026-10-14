//! Decide cuándo limpiar solo. Aquí no se toca Windows: es lógica pura,
//! para poder probarla con tests sin esperar minutos de verdad.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

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
    /// Limpieza periódica cada X minutos.
    Interval,
}

impl Trigger {
    pub fn label(self) -> &'static str {
        match self {
            Trigger::Button | Trigger::Tray => "Limpieza manual",
            Trigger::Hotkey => "Limpieza con atajo",
            Trigger::Threshold => "Limpieza automática",
            Trigger::Interval => "Limpieza programada",
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
    /// Si tiene valor, limpiar también cada cierto tiempo aunque no se llegue al umbral.
    pub interval: Option<Duration>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            threshold_percent: 70,
            cooldown: Duration::from_secs(5 * 60),
            interval: None,
        }
    }
}

/// Decide si toca limpiar ahora.
///
/// - `percent`: % de RAM usada en este momento.
/// - `since_last_clean`: tiempo desde la última limpieza (de cualquier tipo),
///   o `None` si todavía no se ha limpiado nunca.
/// - `since_start`: tiempo que lleva abierto MemClean. Si nunca se ha limpiado,
///   el intervalo cuenta desde aquí (no se limpia nada más arrancar Windows).
pub fn decide(
    policy: &Policy,
    enabled: bool,
    percent: f64,
    since_last_clean: Option<Duration>,
    since_start: Duration,
) -> Option<Trigger> {
    if !enabled {
        return None;
    }

    // El umbral sí actúa enseguida: si la RAM ya está alta al arrancar, se limpia.
    let cooldown_over = since_last_clean.map_or(true, |elapsed| elapsed >= policy.cooldown);
    if percent >= f64::from(policy.threshold_percent) && cooldown_over {
        return Some(Trigger::Threshold);
    }

    let since_reference = since_last_clean.unwrap_or(since_start);
    match policy.interval {
        Some(interval) if since_reference >= interval => Some(Trigger::Interval),
        _ => None,
    }
}

/// Recuerda cuándo arrancó MemClean y cuándo fue la última limpieza,
/// para respetar el tiempo de espera y el intervalo.
/// (Si la limpieza automática está activada o no se guarda en los ajustes.)
pub struct AutoState {
    started: Instant,
    last_clean: Mutex<Option<Instant>>,
}

impl Default for AutoState {
    fn default() -> Self {
        Self { started: Instant::now(), last_clean: Mutex::new(None) }
    }
}

impl AutoState {
    pub fn since_start(&self) -> Duration {
        self.started.elapsed()
    }

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

    fn policy() -> Policy {
        Policy::default() // 70 %, 5 minutos, sin intervalo
    }

    /// `decide` con la app recién abierta (0 segundos desde el arranque).
    fn d(p: &Policy, enabled: bool, percent: f64, last: Option<Duration>) -> Option<Trigger> {
        decide(p, enabled, percent, last, Duration::ZERO)
    }

    #[test]
    fn below_threshold_does_nothing() {
        assert_eq!(d(&policy(), true, 69.9, None), None);
        assert_eq!(d(&policy(), true, 10.0, Some(60 * MIN)), None);
    }

    #[test]
    fn at_or_above_threshold_cleans_when_never_cleaned() {
        assert_eq!(d(&policy(), true, 70.0, None), Some(Trigger::Threshold));
        assert_eq!(d(&policy(), true, 95.0, None), Some(Trigger::Threshold));
    }

    #[test]
    fn cooldown_blocks_until_it_has_passed() {
        let p = policy();
        assert_eq!(d(&p, true, 85.0, Some(Duration::ZERO)), None);
        assert_eq!(d(&p, true, 85.0, Some(4 * MIN + Duration::from_secs(59))), None);
        assert_eq!(d(&p, true, 85.0, Some(5 * MIN)), Some(Trigger::Threshold));
        assert_eq!(d(&p, true, 85.0, Some(30 * MIN)), Some(Trigger::Threshold));
    }

    #[test]
    fn disabled_never_cleans() {
        assert_eq!(d(&policy(), false, 99.0, None), None);
        let with_interval = Policy { interval: Some(MIN), ..policy() };
        assert_eq!(d(&with_interval, false, 10.0, Some(60 * MIN)), None);
    }

    #[test]
    fn custom_threshold_is_respected() {
        let p = Policy { threshold_percent: 50, ..policy() };
        assert_eq!(d(&p, true, 49.0, None), None);
        assert_eq!(d(&p, true, 50.0, None), Some(Trigger::Threshold));
    }

    #[test]
    fn interval_cleans_even_below_threshold() {
        let p = Policy { interval: Some(30 * MIN), ..policy() };
        assert_eq!(d(&p, true, 20.0, Some(29 * MIN)), None);
        assert_eq!(d(&p, true, 20.0, Some(30 * MIN)), Some(Trigger::Interval));
    }

    #[test]
    fn interval_counts_from_start_when_never_cleaned() {
        let p = Policy { interval: Some(30 * MIN), ..policy() };
        // Recién abierta: no limpia por intervalo.
        assert_eq!(decide(&p, true, 20.0, None, Duration::ZERO), None);
        assert_eq!(decide(&p, true, 20.0, None, 29 * MIN), None);
        // A los 30 minutos de abrirla, sí.
        assert_eq!(decide(&p, true, 20.0, None, 30 * MIN), Some(Trigger::Interval));
    }

    #[test]
    fn threshold_acts_right_after_start() {
        // Con la RAM alta al arrancar no se espera nada.
        assert_eq!(decide(&policy(), true, 90.0, None, Duration::ZERO), Some(Trigger::Threshold));
    }

    #[test]
    fn threshold_wins_over_interval() {
        let p = Policy { interval: Some(MIN), ..policy() };
        assert_eq!(d(&p, true, 80.0, Some(10 * MIN)), Some(Trigger::Threshold));
    }

    #[test]
    fn state_remembers_last_clean() {
        let state = AutoState::default();
        assert!(state.since_last_clean().is_none());
        state.mark_cleaned();
        assert!(state.since_last_clean().is_some_and(|d| d < MIN));
    }
}
