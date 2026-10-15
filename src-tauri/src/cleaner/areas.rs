//! Una función por cada área de memoria que se puede limpiar.
//! Ninguna cierra procesos: solo le piden a Windows que suelte memoria.

use std::mem::size_of;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Memory::SetSystemFileCacheSize;
use windows::Win32::System::ProcessStatus::{EmptyWorkingSet, EnumProcesses};
use windows::Win32::System::SystemInformation::GetSystemWindowsDirectoryW;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SET_QUOTA,
};

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

/// Lo mismo que `working_sets`, pero proceso por proceso y solo con las apps:
/// se salta los programas de la carpeta de Windows, los que Windows protege
/// (no se dejan abrir) y el propio MemClean.
pub fn app_working_sets() -> Result<(), String> {
    let windows_dir = windows_dir().ok_or_else(|| {
        tr("no se encontró la carpeta de Windows", "the Windows folder was not found").to_string()
    })?;
    let own_pid = std::process::id();
    for pid in process_ids()? {
        if pid != own_pid {
            trim_if_app(pid, &windows_dir);
        }
    }
    Ok(())
}

/// Identificadores de todos los procesos abiertos.
fn process_ids() -> Result<Vec<u32>, String> {
    let mut ids = vec![0u32; 1024];
    loop {
        let capacity = (ids.len() * size_of::<u32>()) as u32;
        let mut used = 0u32;
        // SAFETY: Windows escribe como mucho `capacity` bytes en `ids`.
        unsafe { EnumProcesses(ids.as_mut_ptr(), capacity, &mut used) }.map_err(|e| {
            tr(
                format!("no se pudo obtener la lista de procesos: {e}"),
                format!("could not list the processes: {e}"),
            )
        })?;
        if used < capacity {
            ids.truncate(used as usize / size_of::<u32>());
            return Ok(ids);
        }
        // La lista no cabía entera: se prueba con el doble de sitio.
        ids.resize(ids.len() * 2, 0);
    }
}

fn trim_if_app(pid: u32, windows_dir: &str) {
    let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SET_QUOTA;
    // SAFETY: solo pide un identificador del proceso; se cierra al final.
    let Ok(process) = (unsafe { OpenProcess(access, false, pid) }) else {
        return; // del sistema o protegido: no se toca
    };
    if process_path(process).is_some_and(|path| !is_windows_path(&path, windows_dir)) {
        // SAFETY: `process` es válido y tiene los permisos que pide EmptyWorkingSet.
        let _ = unsafe { EmptyWorkingSet(process) };
    }
    // SAFETY: lo abrimos arriba y no se vuelve a usar.
    let _ = unsafe { CloseHandle(process) };
}

/// Ruta completa del programa (C:\Program Files\...\app.exe).
fn process_path(process: HANDLE) -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut len = buffer.len() as u32;
    // SAFETY: `len` le dice a Windows cuánto cabe en `buffer`.
    unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len) }
        .ok()?;
    Some(String::from_utf16_lossy(&buffer[..len as usize]))
}

/// Carpeta de Windows (normalmente C:\Windows). Se le pregunta a Windows en vez de
/// leer una variable de entorno, que cualquier programa podría cambiar.
pub(super) fn windows_dir() -> Option<String> {
    let mut buffer = [0u16; 260];
    // SAFETY: Windows escribe como mucho `buffer.len()` caracteres.
    let len = unsafe { GetSystemWindowsDirectoryW(Some(&mut buffer)) } as usize;
    (len > 0 && len < buffer.len()).then(|| String::from_utf16_lossy(&buffer[..len]))
}

/// ¿Está el programa dentro de la carpeta de Windows?
fn is_windows_path(path: &str, windows_dir: &str) -> bool {
    let folder = format!("{}\\", windows_dir.trim_end_matches('\\')).to_lowercase();
    path.to_lowercase().starts_with(&folder)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_programs_are_protected() {
        let dir = r"C:\Windows";
        assert!(is_windows_path(r"C:\Windows\explorer.exe", dir));
        assert!(is_windows_path(r"c:\windows\System32\svchost.exe", dir));
        assert!(!is_windows_path(r"C:\Program Files\App\app.exe", dir));
        // Una carpeta que solo empieza igual no es la de Windows.
        assert!(!is_windows_path(r"C:\WindowsApps\app.exe", dir));
        assert!(is_windows_path(r"C:\Windows\explorer.exe", r"C:\Windows\"));
    }

    #[test]
    fn real_windows_folder_and_processes() {
        let dir = windows_dir().expect("carpeta de Windows");
        assert!(std::path::Path::new(&dir).join("explorer.exe").exists());
        assert!(process_ids().expect("procesos").contains(&std::process::id()));
    }
}
