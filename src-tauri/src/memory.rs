//! Lectura del estado de la memoria del sistema.

use serde::Serialize;
use sysinfo::System;
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
    pub page_total: u64,
    pub page_used: u64,
    pub page_free: u64,
    /// `None` si Windows no nos deja leerlo (suele pedir administrador).
    pub standby: Option<u64>,
    pub file_cache: Option<u64>,
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

/// Lee la memoria una y otra vez reutilizando los mismos recursos,
/// para no gastar RAM creando objetos nuevos cada segundo.
pub struct MemoryReader {
    sys: System,
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
        Self {
            sys: System::new(),
            page_size,
        }
    }

    pub fn read(&mut self) -> Result<MemoryStats, String> {
        let (phys_total, phys_avail) = physical_memory()?;

        self.sys.refresh_memory();
        let page_total = self.sys.total_swap();
        let page_used = self.sys.used_swap().min(page_total);

        Ok(MemoryStats {
            phys_total,
            phys_avail,
            phys_used: phys_total.saturating_sub(phys_avail),
            phys_percent: used_percent(phys_total, phys_avail),
            page_total,
            page_used,
            page_free: page_total - page_used,
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
        let stats = MemoryReader::new().read().expect("lectura de memoria");
        assert!(stats.phys_total > 0);
        assert!(stats.phys_avail <= stats.phys_total);
        assert!((0.0..=100.0).contains(&stats.phys_percent));
    }
}
