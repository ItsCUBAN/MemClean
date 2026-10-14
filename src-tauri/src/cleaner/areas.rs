//! Una función por cada área de memoria que se puede limpiar.
//! Ninguna cierra procesos: solo le piden a Windows que suelte memoria.

use windows::Win32::System::Memory::SetSystemFileCacheSize;

use crate::i18n::tr;
use crate::ntapi::{self, MemoryListCommand};

fn list_command(command: MemoryListCommand) -> Result<(), String> {
    ntapi::memory_list_command(command).map_err(ntapi::status_message)
}

/// Saca de la RAM las páginas que los programas no están usando ahora mismo.
/// Siguen disponibles: si un programa las necesita, Windows las recupera.
pub fn working_sets() -> Result<(), String> {
    list_command(MemoryListCommand::EmptyWorkingSets)
}

/// Vacía la caché de archivos del sistema (pasar -1, -1 significa "vaciar").
pub fn system_file_cache() -> Result<(), String> {
    // SAFETY: llamada sin punteros; solo pasa números.
    unsafe { SetSystemFileCacheSize(usize::MAX, usize::MAX, 0) }
        .map_err(|e| {
            tr(
                format!("no se pudo vaciar la caché de archivos: {}", e.message()),
                format!("could not empty the file cache: {}", e.message()),
            )
        })
}

/// Escribe en disco las páginas modificadas y las pasa a la lista Standby.
pub fn modified_list() -> Result<(), String> {
    list_command(MemoryListCommand::FlushModifiedList)
}

/// Libera la lista Standby: datos en caché de programas que ya se cerraron.
pub fn standby_list() -> Result<(), String> {
    list_command(MemoryListCommand::PurgeStandbyList)
}

/// Libera solo la parte de prioridad baja de la lista Standby.
pub fn low_priority_standby() -> Result<(), String> {
    list_command(MemoryListCommand::PurgeLowPriorityStandbyList)
}

/// Junta las páginas de memoria idénticas en una sola.
pub fn combine_memory_lists() -> Result<(), String> {
    ntapi::combine_physical_memory().map_err(ntapi::status_message)
}
