//! Limpieza de memoria: decide qué áreas limpiar, en qué orden, y mide lo liberado.

mod areas;
pub mod privileges;

use std::sync::{Mutex, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::memory;

/// Las zonas de memoria que MemClean sabe limpiar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Area {
    WorkingSets,
    SystemFileCache,
    ModifiedList,
    StandbyList,
    LowPriorityStandby,
    CombineMemoryLists,
}

impl Area {
    /// Orden en que se ejecutan. Primero se vacían los working sets y se vuelcan
    /// las páginas modificadas, que acaban en Standby; después se purga Standby.
    pub const ALL: [Area; 6] = [
        Area::WorkingSets,
        Area::SystemFileCache,
        Area::ModifiedList,
        Area::StandbyList,
        Area::LowPriorityStandby,
        Area::CombineMemoryLists,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Area::WorkingSets => "Working sets de procesos",
            Area::SystemFileCache => "Caché de archivos del sistema",
            Area::ModifiedList => "Lista Modified",
            Area::StandbyList => "Lista Standby",
            Area::LowPriorityStandby => "Standby de baja prioridad",
            Area::CombineMemoryLists => "Combinar listas de memoria",
        }
    }

    pub fn enabled_by_default(self) -> bool {
        !matches!(self, Area::LowPriorityStandby | Area::CombineMemoryLists)
    }

    pub fn defaults() -> Vec<Area> {
        Area::ALL.into_iter().filter(|a| a.enabled_by_default()).collect()
    }

    fn run(self) -> Result<(), String> {
        match self {
            Area::WorkingSets => areas::working_sets(),
            Area::SystemFileCache => areas::system_file_cache(),
            Area::ModifiedList => areas::modified_list(),
            Area::StandbyList => areas::standby_list(),
            Area::LowPriorityStandby => areas::low_priority_standby(),
            Area::CombineMemoryLists => areas::combine_memory_lists(),
        }
    }
}

/// Resultado de un área: `error` vacío significa que fue bien.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaResult {
    pub area: Area,
    pub label: &'static str,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    /// Momento de la limpieza (milisegundos desde 1970, lo que entiende JavaScript).
    pub timestamp_ms: u64,
    /// Disponible después menos disponible antes. Puede ser negativo si otro
    /// programa reservó memoria justo mientras limpiábamos.
    pub freed_bytes: i64,
    pub results: Vec<AreaResult>,
}

/// Evita que se lancen dos limpiezas a la vez (por ejemplo, botón + menú).
static CLEANING: Mutex<()> = Mutex::new(());

/// Limpia las áreas pedidas, siempre en el orden de `Area::ALL`.
pub fn clean(selected: &[Area]) -> Result<CleanReport, String> {
    let _guard = match CLEANING.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => return Err("Ya hay una limpieza en curso.".into()),
    };
    if selected.is_empty() {
        return Err("No hay ninguna área seleccionada para limpiar.".into());
    }

    let (_, before) = memory::physical_memory()?;

    // Si un privilegio no se puede activar, lo apuntamos y lo mostramos
    // en las áreas que lo necesitan, pero lo intentamos igualmente.
    let profile = privileges::enable(privileges::PROFILE_SINGLE_PROCESS).err();
    let quota = privileges::enable(privileges::INCREASE_QUOTA).err();

    let results = Area::ALL
        .into_iter()
        .filter(|area| selected.contains(area))
        .map(|area| {
            let error = area.run().err().map(|err| {
                let privilege_error = match area {
                    Area::SystemFileCache => &quota,
                    _ => &profile,
                };
                privilege_error.clone().unwrap_or(err)
            });
            AreaResult { area, label: area.label(), error }
        })
        .collect();

    let (_, after) = memory::physical_memory()?;

    Ok(CleanReport {
        timestamp_ms: now_ms(),
        freed_bytes: freed_bytes(before, after),
        results,
    })
}

/// Memoria liberada = disponible después − disponible antes.
pub fn freed_bytes(available_before: u64, available_after: u64) -> i64 {
    available_after as i64 - available_before as i64
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ntapi::status_message;

    #[test]
    fn freed_is_after_minus_before() {
        assert_eq!(freed_bytes(1_000, 1_500), 500);
        assert_eq!(freed_bytes(1_500, 1_000), -500);
        assert_eq!(freed_bytes(0, 0), 0);
    }

    #[test]
    fn default_areas_match_spec() {
        assert_eq!(
            Area::defaults(),
            vec![
                Area::WorkingSets,
                Area::SystemFileCache,
                Area::ModifiedList,
                Area::StandbyList
            ]
        );
    }

    #[test]
    fn modified_is_flushed_before_standby_is_purged() {
        let pos = |a| Area::ALL.iter().position(|&x| x == a);
        assert!(pos(Area::ModifiedList) < pos(Area::StandbyList));
    }

    #[test]
    fn nothing_selected_is_an_error_not_a_panic() {
        assert!(clean(&[]).is_err());
    }

    #[test]
    fn known_status_codes_are_translated() {
        assert!(status_message(0xC000_0061_u32 as i32).contains("privilegio"));
        assert!(status_message(0x1234_5678).contains("0x12345678"));
    }
}
