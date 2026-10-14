//! Historial de las últimas limpiezas. Se guarda en la parte de Rust porque
//! la ventana se destruye al cerrarla.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;

use crate::auto::Trigger;
use crate::cleaner::CleanReport;

pub const MAX_ENTRIES: usize = 10;

/// Una limpieza del historial. El texto "Limpieza manual", "Automatic cleaning"…
/// lo pone la ventana a partir de `trigger`, así cambia si se cambia de idioma.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub trigger: Trigger,
    pub report: CleanReport,
}

#[derive(Default)]
pub struct History(Mutex<VecDeque<HistoryEntry>>);

impl History {
    /// Añade una limpieza; si ya hay 10, se olvida la más antigua.
    pub fn push(&self, trigger: Trigger, report: CleanReport) -> HistoryEntry {
        let entry = HistoryEntry { trigger, report };
        if let Ok(mut list) = self.0.lock() {
            list.push_front(entry.clone());
            list.truncate(MAX_ENTRIES);
        }
        entry
    }

    /// De la más reciente a la más antigua.
    pub fn entries(&self) -> Vec<HistoryEntry> {
        self.0.lock().map(|list| list.iter().cloned().collect()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(freed: i64) -> CleanReport {
        CleanReport { timestamp_ms: 0, freed_bytes: freed, results: Vec::new() }
    }

    #[test]
    fn keeps_only_the_last_ten_newest_first() {
        let history = History::default();
        for i in 0..15 {
            history.push(Trigger::Threshold, report(i));
        }
        let entries = history.entries();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].report.freed_bytes, 14);
        assert_eq!(entries[9].report.freed_bytes, 5);
    }

    #[test]
    fn stores_what_triggered_it() {
        let history = History::default();
        history.push(Trigger::Tray, report(1));
        assert_eq!(history.entries()[0].trigger, Trigger::Tray);
    }
}
