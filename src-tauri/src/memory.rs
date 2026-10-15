//! Lectura del estado de la memoria del sistema.

use std::ffi::c_void;

use serde::Serialize;
use windows::core::{BOOL, PCWSTR};
use windows::Win32::System::ProcessStatus::{EnumPageFilesW, ENUM_PAGE_FILE_INFORMATION};
use windows::Win32::System::SystemInformation::{
    GetSystemInfo, GlobalMemoryStatusEx, MEMORYSTATUSEX, SYSTEM_INFO,
};

use crate::i18n::tr;
use crate::ntapi;

/// Foto de la memoria en un instante. Todos los tamaños van en bytes;
/// la interfaz se encarga de convertirlos a GB.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryStats {
    pub phys_total: u64,
    pub phys_avail: u64,
    pub phys_used: u64,
    pub phys_percent: f64,
    /// `None` si el bloque está oculto en Ajustes (entonces ni se lee).
    pub page_file: Option<PageFileStats>,
    /// Motivo si se quiso leer el archivo de paginación y Windows no dejó.
    pub page_file_error: Option<String>,
    /// `None` si Windows no nos deja leerlo (suele pedir administrador).
    pub standby: Option<u64>,
    pub file_cache: Option<u64>,
}

/// Archivo de paginación (si hay varios, la suma de todos).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct PageFileStats {
    pub total: u64,
    pub used: u64,
}

/// % de RAM usada = (Total - Disponible) / Total.
/// Si "disponible" fuese mayor que el total (no debería), se trata como 0 % usado.
pub fn used_percent(total: u64, available: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let used = total.saturating_sub(available);
    used as f64 / total as f64 * 100.0
}

/// Lee la memoria una y otra vez sin tener que volver a preguntar
/// a Windows el tamaño de página cada segundo.
pub struct MemoryReader {
    page_size: u64,
}

impl MemoryReader {
    pub fn new() -> Self {
        let mut info = SYSTEM_INFO::default();
        // SAFETY: GetSystemInfo solo escribe en la estructura que le pasamos.
        unsafe { GetSystemInfo(&mut info) };
        let page_size = match info.dwPageSize {
            0 => 4096,
            size => u64::from(size),
        };
        Self { page_size }
    }

    /// `with_page_file`: si el bloque del archivo de paginación se ve en la ventana.
    pub fn read(&mut self, with_page_file: bool) -> Result<MemoryStats, String> {
        let (phys_total, phys_avail) = physical_memory()?;

        let (page_file, page_file_error) = if with_page_file {
            match page_file(self.page_size) {
                Ok(stats) => (Some(stats), None),
                Err(e) => (None, Some(e)),
            }
        } else {
            (None, None)
        };

        Ok(MemoryStats {
            phys_total,
            phys_avail,
            phys_used: phys_total.saturating_sub(phys_avail),
            phys_percent: used_percent(phys_total, phys_avail),
            page_file,
            page_file_error,
            standby: ntapi::standby_bytes(self.page_size),
            file_cache: ntapi::file_cache_bytes(),
        })
    }
}

/// Devuelve (total, disponible) de la RAM física usando GlobalMemoryStatusEx.
pub fn physical_memory() -> Result<(u64, u64), String> {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `status` tiene `dwLength` correcto, como exige la API.
    unsafe { GlobalMemoryStatusEx(&mut status) }
        .map_err(|e| {
            tr(
                format!("No se pudo leer la memoria (GlobalMemoryStatusEx): {e}"),
                format!("Could not read memory (GlobalMemoryStatusEx): {e}"),
            )
        })?;
    Ok((status.ullTotalPhys, status.ullAvailPhys))
}

/// Tamaño y uso reales del archivo de paginación (EnumPageFilesW).
/// Windows llama a `add_page_file` una vez por cada archivo; sin ninguno, da 0 y 0.
fn page_file(page_size: u64) -> Result<PageFileStats, String> {
    let mut pages = PageFileStats::default();
    // SAFETY: `pages` vive durante toda la llamada y `add_page_file` solo lo usa
    // mientras tanto, como el mismo tipo que le pasamos aquí.
    unsafe { EnumPageFilesW(Some(add_page_file), (&mut pages as *mut PageFileStats).cast()) }
        .map_err(|e| {
            tr(
                format!("No se pudo leer el archivo de paginación: {e}"),
                format!("Could not read the page file: {e}"),
            )
        })?;
    Ok(PageFileStats {
        total: pages.total * page_size,
        used: pages.used.min(pages.total) * page_size,
    })
}

/// Suma un archivo de paginación (Windows da los tamaños en páginas).
unsafe extern "system" fn add_page_file(
    context: *mut c_void,
    info: *mut ENUM_PAGE_FILE_INFORMATION,
    _file_name: PCWSTR,
) -> BOOL {
    // SAFETY: `context` es el `PageFileStats` de `page_file`, y `info` lo rellena Windows.
    if let (Some(pages), Some(info)) =
        unsafe { (context.cast::<PageFileStats>().as_mut(), info.as_ref()) }
    {
        pages.total += info.TotalSize as u64;
        pages.used += info.TotalInUse as u64;
    }
    true.into() // seguir con el siguiente archivo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_basic() {
        assert_eq!(used_percent(100, 30), 70.0);
        assert_eq!(used_percent(16, 4), 75.0);
    }

    #[test]
    fn percent_all_free_and_all_used() {
        assert_eq!(used_percent(8_000, 8_000), 0.0);
        assert_eq!(used_percent(8_000, 0), 100.0);
    }

    #[test]
    fn percent_zero_total_does_not_divide_by_zero() {
        assert_eq!(used_percent(0, 0), 0.0);
    }

    #[test]
    fn percent_available_greater_than_total_is_zero() {
        assert_eq!(used_percent(100, 150), 0.0);
    }

    #[test]
    fn percent_real_sizes() {
        let gb = 1024_u64.pow(3);
        let p = used_percent(32 * gb, 12 * gb);
        assert!((p - 62.5).abs() < 1e-9);
    }

    #[test]
    fn reader_returns_sane_values() {
        let stats = MemoryReader::new().read(false).expect("lectura de memoria");
        assert!(stats.phys_total > 0);
        assert!(stats.phys_avail <= stats.phys_total);
        assert!((0.0..=100.0).contains(&stats.phys_percent));
        assert_eq!(stats.page_file, None, "oculto: no se lee");
    }

    #[test]
    fn page_file_is_read_when_shown() {
        let stats = MemoryReader::new().read(true).expect("lectura de memoria");
        assert_eq!(stats.page_file_error, None);
        let page = stats.page_file.expect("archivo de paginación");
        assert!(page.used <= page.total);
    }
}
