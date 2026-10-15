//! Extras que no son RAM: caché DNS, archivos temporales y portapapeles.
//! Solo se hacen al limpiar a mano (ver `Area::is_extra`).

use std::fs::{self, Metadata, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use windows::core::PWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Security::TOKEN_QUERY;
use windows::Win32::Storage::FileSystem::{
    GetFinalPathNameByHandleW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_NAME_NORMALIZED, FILE_SHARE_READ, FILE_SHARE_WRITE,
};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard};
use windows::Win32::UI::Shell::GetUserProfileDirectoryW;

use super::areas::windows_dir;
use super::privileges::Token;
use crate::i18n::tr;

#[link(name = "dnsapi", kind = "raw-dylib")]
extern "system" {
    /// Lo mismo que hace `ipconfig /flushdns` (Windows no la documenta, pero existe desde 2000).
    fn DnsFlushResolverCache() -> i32;
}

/// Vacía la caché DNS de Windows.
pub fn dns_cache() -> Result<(), String> {
    // SAFETY: no recibe nada ni devuelve punteros.
    if unsafe { DnsFlushResolverCache() } != 0 {
        Ok(())
    } else {
        Err(tr("Windows no pudo vaciar la caché DNS", "Windows could not flush the DNS cache").into())
    }
}

/// Vacía el portapapeles. El historial de Win+V se conserva.
pub fn clipboard() -> Result<(), String> {
    // Si otro programa lo tiene abierto justo ahora, se reintenta un momento.
    let mut attempt = 1;
    loop {
        // SAFETY: sin ventana propietaria; se cierra más abajo.
        match unsafe { OpenClipboard(None) } {
            Ok(()) => break,
            Err(e) if attempt == 5 => {
                return Err(tr(
                    format!("otro programa está usando el portapapeles: {e}"),
                    format!("another program is using the clipboard: {e}"),
                ))
            }
            Err(_) => {
                attempt += 1;
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    // SAFETY: el portapapeles está abierto por este hilo.
    let emptied = unsafe { EmptyClipboard() };
    // SAFETY: lo abrimos arriba.
    let _ = unsafe { CloseClipboard() };
    emptied.map_err(|e| {
        tr(
            format!("no se pudo vaciar el portapapeles: {e}"),
            format!("could not empty the clipboard: {e}"),
        )
    })
}

/// Solo se borra lo que lleva más de un día sin crearse ni modificarse
/// (así no se rompe una instalación que esté en marcha).
const MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Borra los archivos temporales viejos de tu usuario y de Windows.
/// Lo que está en uso, Windows no deja borrarlo y se salta.
/// Devuelve los bytes de disco liberados.
pub fn temp_files() -> Result<u64, String> {
    let now = SystemTime::now();
    let mut freed = 0;
    let mut errors = Vec::new();
    for folder in temp_folders() {
        match folder.and_then(|folder| clean_temp_folder(&folder, now)) {
            Ok(bytes) => freed += bytes,
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        Ok(freed)
    } else {
        Err(errors.join("; "))
    }
}

/// Temp de tu usuario y de Windows. Como MemClean es administrador, no se usa
/// la variable TEMP (cualquier programa podría apuntarla a una carpeta de Windows):
/// las rutas salen de datos que solo puede cambiar un administrador.
fn temp_folders() -> [Result<PathBuf, String>; 2] {
    [
        user_profile_dir().map(|profile| profile.join(r"AppData\Local\Temp")),
        windows_dir()
            .map(|windows| PathBuf::from(windows).join("Temp"))
            .ok_or_else(|| tr("no se encontró la carpeta de Windows", "the Windows folder was not found").into()),
    ]
}

/// Carpeta de tu usuario (C:\Users\nombre), según el registro de Windows.
fn user_profile_dir() -> Result<PathBuf, String> {
    let token = Token::open(TOKEN_QUERY)?;
    let mut buffer = [0u16; 512];
    let mut len = buffer.len() as u32;
    // SAFETY: `len` le dice a Windows cuánto cabe en `buffer`.
    unsafe { GetUserProfileDirectoryW(token.0, Some(PWSTR(buffer.as_mut_ptr())), &mut len) }.map_err(|e| {
        tr(
            format!("no se encontró tu carpeta de usuario: {e}"),
            format!("your user folder was not found: {e}"),
        )
    })?;
    // `len` incluye el cero final.
    Ok(PathBuf::from(String::from_utf16_lossy(&buffer[..len.saturating_sub(1) as usize])))
}

/// Borra lo viejo de una carpeta Temp sin seguir nunca enlaces a otras carpetas.
fn clean_temp_folder(folder: &Path, now: SystemTime) -> Result<u64, String> {
    let fail = |why: &str| format!("{}: {why}", folder.display());

    // Mientras `guard` siga abierto, nadie puede mover la carpeta ni cambiarla
    // por un enlace a otro sitio: no se comparte el permiso de borrar.
    let guard = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(folder)
        .map_err(|e| fail(&e.to_string()))?;
    let meta = guard.metadata().map_err(|e| fail(&e.to_string()))?;
    let real = final_path(HANDLE(guard.as_raw_handle()));
    if !meta.is_dir() || is_link(&meta) || !real.is_some_and(|real| same_path(&real, folder)) {
        return Err(fail(tr(
            "no es una carpeta normal (apunta a otro sitio); no se toca",
            "it is not a regular folder (it points elsewhere); left untouched",
        )));
    }

    let mut freed = 0;
    let entries = fs::read_dir(folder).map_err(|e| fail(&e.to_string()))?;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        // Los enlaces a otras carpetas ni se siguen ni se borran.
        if is_link(&meta) {
            continue;
        }
        let path = entry.path();
        let Some(size) = old_size(&path, &meta, now) else { continue };
        // remove_dir_all no sigue enlaces: si encuentra uno, borra el enlace, no su destino.
        let removed = if meta.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) };
        if removed.is_ok() {
            freed += size;
        }
    }
    drop(guard);
    Ok(freed)
}

/// Tamaño de `path` si todo lo que contiene es viejo. `None` si algo es reciente
/// o no se puede leer: entonces no se toca nada de ahí.
fn old_size(path: &Path, meta: &Metadata, now: SystemTime) -> Option<u64> {
    if !is_old(meta, now) {
        return None;
    }
    if !meta.is_dir() {
        return Some(meta.len());
    }
    let mut total = 0;
    for entry in fs::read_dir(path).ok()? {
        let entry = entry.ok()?;
        let meta = entry.metadata().ok()?;
        // Un enlace de dentro se borra como enlace, sin entrar: no ocupa espacio.
        if !is_link(&meta) {
            total += old_size(&entry.path(), &meta, now)?;
        }
    }
    Some(total)
}

/// Creado y modificado hace más de `MIN_AGE`. Si no se sabe, se considera nuevo.
fn is_old(meta: &Metadata, now: SystemTime) -> bool {
    [meta.created(), meta.modified()].into_iter().all(|time| {
        time.ok()
            .and_then(|time| now.duration_since(time).ok())
            .is_some_and(|age| age >= MIN_AGE)
    })
}

/// Enlaces simbólicos, uniones de carpetas y similares.
fn is_link(meta: &Metadata) -> bool {
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
}

/// Ruta real de una carpeta abierta, con todos los enlaces resueltos.
fn final_path(handle: HANDLE) -> Option<String> {
    let mut buffer = [0u16; 1024];
    // SAFETY: `handle` es válido mientras dura la llamada y Windows no escribe fuera de `buffer`.
    let len = unsafe { GetFinalPathNameByHandleW(handle, &mut buffer, FILE_NAME_NORMALIZED) } as usize;
    (len > 0 && len < buffer.len()).then(|| String::from_utf16_lossy(&buffer[..len]))
}

/// Compara rutas sin distinguir mayúsculas, ni el prefijo \\?\ ni la barra final.
fn same_path(real: &str, expected: &Path) -> bool {
    let clean = |path: &str| path.trim_start_matches(r"\\?\").trim_end_matches('\\').to_lowercase();
    clean(real) == clean(&expected.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{File, FileTimes};
    use std::os::windows::fs::FileTimesExt;
    use std::process::Command;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("memclean-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("crear carpeta de prueba");
        dir
    }

    /// Pone la fecha de creación y de modificación hace dos días.
    fn make_old(path: &Path) {
        let two_days_ago = SystemTime::now() - 2 * DAY;
        let times = FileTimes::new().set_modified(two_days_ago).set_created(two_days_ago);
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
            .expect("abrir para cambiar la fecha");
        file.set_times(times).expect("cambiar la fecha");
    }

    fn write(path: &Path, text: &str) {
        fs::write(path, text).expect("escribir");
    }

    fn junction(link: &Path, target: &Path) {
        let status = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .expect("mklink");
        assert!(status.status.success(), "no se pudo crear la unión de carpetas");
    }

    #[test]
    fn only_old_things_are_deleted_and_links_are_never_followed() {
        let root = test_dir("temp");
        let temp = root.join("Temp");
        let outside = root.join("Importante");
        fs::create_dir_all(&temp).expect("Temp");
        fs::create_dir_all(&outside).expect("fuera");

        // Fuera de Temp: no se puede tocar nunca.
        write(&outside.join("viejo.txt"), "no me borres");
        make_old(&outside.join("viejo.txt"));

        write(&temp.join("viejo.txt"), "12345");
        make_old(&temp.join("viejo.txt"));
        write(&temp.join("nuevo.txt"), "recién creado");

        let old_dir = temp.join("carpeta-vieja");
        fs::create_dir(&old_dir).expect("carpeta vieja");
        write(&old_dir.join("a.txt"), "1234567");
        make_old(&old_dir.join("a.txt"));
        make_old(&old_dir);

        let mixed_dir = temp.join("carpeta-con-algo-nuevo");
        fs::create_dir(&mixed_dir).expect("carpeta mixta");
        write(&mixed_dir.join("viejo.txt"), "x");
        make_old(&mixed_dir.join("viejo.txt"));
        write(&mixed_dir.join("nuevo.txt"), "y");
        make_old(&mixed_dir);

        // Una unión vieja que apunta fuera de Temp.
        junction(&temp.join("enlace"), &outside);
        make_old(&temp.join("enlace"));

        let freed = clean_temp_folder(&temp, SystemTime::now()).expect("limpiar");

        assert_eq!(freed, 5 + 7);
        assert!(!temp.join("viejo.txt").exists());
        assert!(!old_dir.exists());
        assert!(temp.join("nuevo.txt").exists());
        assert!(mixed_dir.join("viejo.txt").exists(), "su carpeta tiene algo nuevo");
        assert!(temp.join("enlace").exists(), "los enlaces no se tocan");
        assert!(outside.join("viejo.txt").exists(), "nunca se borra fuera de Temp");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_temp_folder_that_is_a_link_is_refused() {
        let root = test_dir("link");
        let outside = root.join("Importante");
        fs::create_dir_all(&outside).expect("fuera");
        write(&outside.join("viejo.txt"), "no me borres");
        make_old(&outside.join("viejo.txt"));
        junction(&root.join("Temp"), &outside);

        assert!(clean_temp_folder(&root.join("Temp"), SystemTime::now()).is_err());
        assert!(outside.join("viejo.txt").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn both_temp_folders_are_found() {
        for folder in temp_folders() {
            let folder = folder.expect("carpeta Temp");
            assert!(folder.ends_with("Temp"), "{}", folder.display());
        }
    }

    #[test]
    fn paths_are_compared_like_windows_does() {
        assert!(same_path(r"\\?\C:\Users\Ana\AppData\Local\Temp", Path::new(r"C:\Users\ana\AppData\Local\Temp\")));
        assert!(!same_path(r"\\?\C:\Windows\System32", Path::new(r"C:\Users\Ana\AppData\Local\Temp")));
    }

    #[test]
    fn age_needs_both_dates_old() {
        let root = test_dir("age");
        let file = root.join("a.txt");
        write(&file, "a");
        let now = SystemTime::now();
        assert!(!is_old(&fs::metadata(&file).expect("meta"), now));
        make_old(&file);
        assert!(is_old(&fs::metadata(&file).expect("meta"), now));
        // Modificado hoy aunque se creara hace dos días: es nuevo.
        File::options()
            .write(true)
            .open(&file)
            .expect("abrir")
            .set_modified(now)
            .expect("fecha");
        assert!(!is_old(&fs::metadata(&file).expect("meta"), now));
        let _ = fs::remove_dir_all(&root);
    }
}
