//! Acceso a la API nativa de Windows (ntdll.dll).
//!
//! Estas funciones no vienen en la API "normal" de Win32, así que declaramos
//! nosotros mismos la firma de la función y las estructuras que usa.

use std::ffi::c_void;
use std::mem::size_of;

use crate::i18n::tr;

const SYSTEM_FILE_CACHE_INFORMATION: u32 = 21;
const SYSTEM_MEMORY_LIST_INFORMATION: u32 = 80;

/// Órdenes que acepta `SystemMemoryListInformation` (enum SYSTEM_MEMORY_LIST_COMMAND).
#[derive(Debug, Clone, Copy)]
#[repr(i32)]
pub enum MemoryListCommand {
    EmptyWorkingSets = 2,
    FlushModifiedList = 3,
    PurgeStandbyList = 4,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQuerySystemInformation(
        class: u32,
        info: *mut c_void,
        info_len: u32,
        return_len: *mut u32,
    ) -> i32;

    fn NtSetSystemInformation(class: u32, info: *const c_void, info_len: u32) -> i32;
}

/// Envía una estructura a Windows. Devuelve el código NTSTATUS si falla.
fn set<T>(class: u32, value: &T) -> Result<(), i32> {
    // SAFETY: el puntero apunta a `value`, que vive durante toda la llamada,
    // y le pasamos a Windows su tamaño exacto.
    let status = unsafe {
        NtSetSystemInformation(
            class,
            (value as *const T).cast::<c_void>(),
            size_of::<T>() as u32,
        )
    };
    if status >= 0 {
        Ok(())
    } else {
        Err(status)
    }
}

/// Ejecuta una orden sobre las listas de memoria (vaciar working sets, purgar Standby…).
pub fn memory_list_command(command: MemoryListCommand) -> Result<(), i32> {
    set(SYSTEM_MEMORY_LIST_INFORMATION, &(command as i32))
}

/// Traduce los códigos de error de Windows más habituales a algo comprensible.
pub fn status_message(status: i32) -> String {
    match status as u32 {
        0xC000_0061 => tr(
            "falta el privilegio necesario (¿se está ejecutando como administrador?)",
            "missing the required privilege (is it running as administrator?)",
        )
        .into(),
        0xC000_0022 => tr(
            "acceso denegado (se necesita administrador)",
            "access denied (administrator required)",
        )
        .into(),
        0xC000_0003 | 0xC000_0002 => tr(
            "esta versión de Windows no lo admite",
            "this version of Windows does not support it",
        )
        .into(),
        code => tr(
            format!("Windows devolvió el código 0x{code:08X}"),
            format!("Windows returned code 0x{code:08X}"),
        ),
    }
}

/// Contadores (en páginas) de las listas de memoria del sistema.
#[repr(C)]
#[derive(Default)]
#[allow(dead_code)]
struct SystemMemoryListInformation {
    zero_page_count: usize,
    free_page_count: usize,
    modified_page_count: usize,
    modified_no_write_page_count: usize,
    bad_page_count: usize,
    page_count_by_priority: [usize; 8],
    repurposed_pages_by_priority: [usize; 8],
    modified_page_count_page_file: usize,
}

/// Información de la caché de archivos del sistema (tamaños en bytes).
#[repr(C)]
#[derive(Default)]
#[allow(dead_code)]
struct SystemFileCacheInformation {
    current_size: usize,
    peak_size: usize,
    page_fault_count: u32,
    minimum_working_set: usize,
    maximum_working_set: usize,
    current_size_including_transition_in_pages: usize,
    peak_size_including_transition_in_pages: usize,
    transition_repurpose_count: u32,
    flags: u32,
}

/// Rellena una de las estructuras de arriba. Devuelve el código NTSTATUS si falla.
///
/// Solo se llama con estructuras `#[repr(C)]` de números, por eso es seguro
/// dejar que Windows escriba en ellas.
fn query<T: Default>(class: u32) -> Result<T, i32> {
    let mut value = T::default();
    // SAFETY: el puntero apunta a `value`, que vive durante toda la llamada,
    // y le pasamos a Windows su tamaño exacto.
    let status = unsafe {
        NtQuerySystemInformation(
            class,
            (&mut value as *mut T).cast::<c_void>(),
            size_of::<T>() as u32,
            std::ptr::null_mut(),
        )
    };
    if status >= 0 {
        Ok(value)
    } else {
        Err(status)
    }
}

/// Bytes en la lista Standby (suma de sus 8 prioridades).
/// Windows puede negarlo sin permisos de administrador; entonces devuelve `None`.
pub fn standby_bytes(page_size: u64) -> Option<u64> {
    let info = query::<SystemMemoryListInformation>(SYSTEM_MEMORY_LIST_INFORMATION).ok()?;
    let pages: u64 = info.page_count_by_priority.iter().map(|&p| p as u64).sum();
    Some(pages * page_size)
}

/// Bytes que ocupa ahora mismo la caché de archivos del sistema.
pub fn file_cache_bytes() -> Option<u64> {
    let info = query::<SystemFileCacheInformation>(SYSTEM_FILE_CACHE_INFORMATION).ok()?;
    Some(info.current_size as u64)
}
