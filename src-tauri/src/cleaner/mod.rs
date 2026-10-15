//! Limpieza de memoria: decide qué áreas limpiar, en qué orden, y mide lo liberado.

mod areas;
mod extras;
pub mod privileges;

use std::sync::{Mutex, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::value::StrDeserializer;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use crate::i18n::tr;
use crate::memory;

/// Lo que MemClean sabe limpiar: zonas de memoria y "extras" que no son RAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Area {
    // Extras (solo al limpiar a mano).
    Clipboard,
    DnsCache,
    TempFiles,
    // Memoria.
    WorkingSets,
    SystemFileCache,
    ModifiedList,
    StandbyList,
}

/// Qué procesos recorta el área "Working sets".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrimScope {
    /// Todos a la vez, con una sola orden a Windows.
    #[default]
    All,
    /// Solo las apps: se salta los procesos de Windows.
    Apps,
}

impl Area {
    /// Orden en que se ejecutan. Primero los extras; después se vacían los working sets
    /// y se vuelcan las páginas modificadas, que acaban en Standby; al final se purga Standby.
    pub const ALL: [Area; 7] = [
        Area::Clipboard,
        Area::DnsCache,
        Area::TempFiles,
        Area::WorkingSets,
        Area::SystemFileCache,
        Area::ModifiedList,
        Area::StandbyList,
    ];

    /// Los extras no liberan RAM: solo se hacen al limpiar a mano (botón, menú o
    /// atajo), nunca en la limpieza automática.
    pub fn is_extra(self) -> bool {
        matches!(self, Area::Clipboard | Area::DnsCache | Area::TempFiles)
    }

    /// Por defecto, todas las de memoria y ningún extra.
    pub fn defaults() -> Vec<Area> {
        Area::ALL.into_iter().filter(|a| !a.is_extra()).collect()
    }

    /// Convierte el nombre guardado en config.toml ("standbyList"…) en un área.
    /// `None` si ese área ya no existe (por ejemplo, "combineMemoryLists" de la 0.1.x).
    pub fn from_id(id: &str) -> Option<Area> {
        let id: StrDeserializer<serde::de::value::Error> = id.into_deserializer();
        Area::deserialize(id).ok()
    }

    /// Devuelve los bytes de disco liberados (solo cuentan en los archivos temporales).
    fn run(self, trim: TrimScope) -> Result<u64, String> {
        let result = match self {
            Area::TempFiles => return extras::temp_files(),
            Area::Clipboard => extras::clipboard(),
            Area::DnsCache => extras::dns_cache(),
            Area::WorkingSets => match trim {
                TrimScope::All => areas::working_sets(),
                TrimScope::Apps => areas::app_working_sets(),
            },
            Area::SystemFileCache => areas::system_file_cache(),
            Area::ModifiedList => areas::modified_list(),
            Area::StandbyList => areas::standby_list(),
        };
        result.map(|()| 0)
    }
}

/// Resultado de un área: `error` vacío significa que fue bien.
/// (El nombre del área lo pone la ventana en el idioma elegido.)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaResult {
    pub area: Area,
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
    /// Disco liberado al borrar archivos temporales (0 si no se borraron).
    pub disk_freed_bytes: u64,
    pub results: Vec<AreaResult>,
}

/// Evita que se lancen dos limpiezas a la vez (por ejemplo, botón + menú).
static CLEANING: Mutex<()> = Mutex::new(());

/// Limpia las áreas pedidas, siempre en el orden de `Area::ALL`.
pub fn clean(selected: &[Area], trim: TrimScope) -> Result<CleanReport, String> {
    let _guard = match CLEANING.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => {
            return Err(tr("Ya hay una limpieza en curso.", "A cleaning is already running.").into())
        }
    };
    if selected.is_empty() {
        return Err(tr(
            "No hay ninguna área seleccionada para limpiar.",
            "No area is selected for cleaning.",
        )
        .into());
    }

    let (_, before) = memory::physical_memory()?;

    // Si un privilegio no se puede activar, lo apuntamos y lo mostramos
    // en las áreas que lo necesitan, pero lo intentamos igualmente.
    let profile = privileges::enable(privileges::PROFILE_SINGLE_PROCESS).err();
    let quota = privileges::enable(privileges::INCREASE_QUOTA).err();

    let mut disk_freed_bytes = 0;
    let results = Area::ALL
        .into_iter()
        .filter(|area| selected.contains(area))
        .map(|area| {
            let error = match area.run(trim) {
                Ok(disk) => {
                    disk_freed_bytes += disk;
                    None
                }
                Err(err) => {
                    // Si faltaba el privilegio que usa el área, ese es el motivo real.
                    let privilege_error = match area {
                        Area::SystemFileCache => quota.as_ref(),
                        Area::WorkingSets if trim == TrimScope::Apps => None,
                        Area::WorkingSets | Area::ModifiedList | Area::StandbyList => profile.as_ref(),
                        Area::Clipboard | Area::DnsCache | Area::TempFiles => None,
                    };
                    Some(privilege_error.cloned().unwrap_or(err))
                }
            };
            AreaResult { area, error }
        })
        .collect();

    let (_, after) = memory::physical_memory()?;

    Ok(CleanReport {
        timestamp_ms: now_ms(),
        freed_bytes: freed_bytes(before, after),
        disk_freed_bytes,
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
    fn extras_are_off_by_default_and_run_first() {
        assert!(Area::defaults().iter().all(|a| !a.is_extra()));
        let first_memory = Area::ALL.iter().position(|a| !a.is_extra());
        let last_extra = Area::ALL.iter().rposition(|a| a.is_extra());
        assert!(last_extra < first_memory);
        assert_eq!(TrimScope::default(), TrimScope::All);
    }

    #[test]
    fn area_ids_from_config() {
        assert_eq!(Area::from_id("standbyList"), Some(Area::StandbyList));
        assert_eq!(Area::from_id("workingSets"), Some(Area::WorkingSets));
        assert_eq!(Area::from_id("tempFiles"), Some(Area::TempFiles));
        // Áreas que existían en la 0.1.x y ya no.
        assert_eq!(Area::from_id("lowPriorityStandby"), None);
        assert_eq!(Area::from_id("combineMemoryLists"), None);
    }

    #[test]
    fn modified_is_flushed_before_standby_is_purged() {
        let pos = |a| Area::ALL.iter().position(|&x| x == a);
        assert!(pos(Area::ModifiedList) < pos(Area::StandbyList));
    }

    #[test]
    fn nothing_selected_is_an_error_not_a_panic() {
        assert!(clean(&[], TrimScope::All).is_err());
    }

    #[test]
    fn known_status_codes_are_translated() {
        assert!(status_message(0xC000_0061_u32 as i32).contains("privilegio"));
        assert!(status_message(0x1234_5678).contains("0x12345678"));
    }
}
