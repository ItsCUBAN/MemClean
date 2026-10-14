//! Privilegios de Windows.
//!
//! Un administrador tiene ciertos "superpoderes" (privilegios), pero vienen
//! apagados. Antes de limpiar hay que encender los que hacen falta:
//! - SeProfileSingleProcessPrivilege: para vaciar/purgar las listas de memoria.
//! - SeIncreaseQuotaPrivilege: para cambiar el tamaño de la caché de archivos.

use std::ffi::c_void;
use std::mem::size_of;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_NOT_ALL_ASSIGNED, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, GetTokenInformation, LookupPrivilegeValueW, TokenElevation,
    LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES, TOKEN_ACCESS_MASK,
    TOKEN_ELEVATION, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::i18n::tr;

pub const PROFILE_SINGLE_PROCESS: PCWSTR = w!("SeProfileSingleProcessPrivilege");
pub const INCREASE_QUOTA: PCWSTR = w!("SeIncreaseQuotaPrivilege");

/// "Llave" de seguridad del proceso actual; se cierra sola al salir de uso.
struct Token(HANDLE);

impl Token {
    fn open(access: TOKEN_ACCESS_MASK) -> Result<Self, String> {
        let mut handle = HANDLE::default();
        // SAFETY: pasamos un puntero válido donde Windows deja el identificador.
        unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut handle) }
            .map_err(|e| tr(format!("no se pudo abrir el token del proceso: {e}"), format!("could not open the process token: {e}")))?;
        Ok(Self(handle))
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: el identificador lo abrimos nosotros y solo se cierra aquí.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Enciende un privilegio. Falla si el usuario no lo tiene (no es administrador).
pub fn enable(name: PCWSTR) -> Result<(), String> {
    let token = Token::open(TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY)?;

    let mut luid = LUID::default();
    // SAFETY: `name` es una cadena constante terminada en cero (macro w!).
    unsafe { LookupPrivilegeValueW(PCWSTR::null(), name, &mut luid) }
        .map_err(|e| tr(format!("privilegio desconocido: {e}"), format!("unknown privilege: {e}")))?;

    let privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
    };
    // SAFETY: `privileges` vive durante la llamada; no pedimos el estado anterior.
    unsafe { AdjustTokenPrivileges(token.0, false, Some(&privileges), 0, None, None) }
        .map_err(|e| tr(format!("no se pudo activar el privilegio: {e}"), format!("could not enable the privilege: {e}")))?;

    // AdjustTokenPrivileges dice "éxito" aunque no haya podido activarlo;
    // el aviso real viene en GetLastError.
    // SAFETY: solo lee el último error del hilo actual.
    if unsafe { GetLastError() } == ERROR_NOT_ALL_ASSIGNED {
        return Err(tr(
            "el usuario no tiene este privilegio (hace falta ejecutar como administrador)",
            "the user does not have this privilege (run as administrator)",
        )
        .into());
    }
    Ok(())
}

/// ¿Se está ejecutando MemClean como administrador?
pub fn is_elevated() -> bool {
    let Ok(token) = Token::open(TOKEN_QUERY) else {
        return false;
    };
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    // SAFETY: el búfer es `elevation` y le pasamos su tamaño exacto.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast::<c_void>()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    }
    .is_ok();
    ok && elevation.TokenIsElevated != 0
}
