//! Iniciar con Windows mediante una tarea programada.
//!
//! MemClean exige administrador. Windows no arranca programas así desde la
//! clave "Run" del registro, pero una tarea programada con "ejecutar con los
//! privilegios más altos" sí, y sin mostrar el aviso de UAC en cada arranque.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use crate::i18n::tr;

const TASK_NAME: &str = "MemClean";
/// Evita que se vea una consola negra al lanzar schtasks.exe.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Ruta completa a schtasks.exe. No la buscamos en el PATH: un programa con
/// permisos de administrador nunca debe ejecutar algo encontrado "por ahí".
fn schtasks_path() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join("schtasks.exe")
}

fn schtasks(args: &[&str]) -> Result<(), String> {
    let output = Command::new(schtasks_path())
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| {
            tr(
                format!("No se pudo ejecutar schtasks.exe: {e}"),
                format!("Could not run schtasks.exe: {e}"),
            )
        })?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(detail)
    }
}

/// ¿Existe ya la tarea programada?
pub fn is_enabled() -> bool {
    schtasks(&["/Query", "/TN", TASK_NAME]).is_ok()
}

/// Crea (o actualiza) la tarea para que apunte a este mismo .exe.
pub fn enable() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| {
        tr(
            format!("No se pudo saber dónde está MemClean: {e}"),
            format!("Could not find where MemClean is installed: {e}"),
        )
    })?;
    let user = current_user()?;
    let xml = task_xml(&exe.to_string_lossy(), &user);

    // schtasks lee la definición desde un archivo XML en UTF-16.
    let file = std::env::temp_dir().join(format!("memclean-task-{}.xml", std::process::id()));
    let mut bytes = vec![0xFF, 0xFE]; // marca de UTF-16 LE
    bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    fs::write(&file, bytes).map_err(|e| {
        tr(
            format!("No se pudo preparar la tarea programada: {e}"),
            format!("Could not prepare the scheduled task: {e}"),
        )
    })?;

    let file_arg = file.to_string_lossy().to_string();
    let result = schtasks(&["/Create", "/TN", TASK_NAME, "/XML", &file_arg, "/F"]);
    let _ = fs::remove_file(&file);
    result.map_err(|detail| {
        tr(
            format!(
                "Windows no dejó crear la tarea para iniciar con Windows. \
                 MemClean debe ejecutarse como administrador. ({detail})"
            ),
            format!(
                "Windows did not allow creating the task to start with Windows. \
                 MemClean must run as administrator. ({detail})"
            ),
        )
    })
}

/// Borra la tarea (si no existe, no hace nada).
pub fn disable() -> Result<(), String> {
    if !is_enabled() {
        return Ok(());
    }
    schtasks(&["/Delete", "/TN", TASK_NAME, "/F"]).map_err(|detail| {
        tr(
            format!("No se pudo quitar el inicio con Windows. ({detail})"),
            format!("Could not remove start with Windows. ({detail})"),
        )
    })
}

fn current_user() -> Result<String, String> {
    let name = std::env::var("USERNAME").map_err(|_| {
        tr("No se pudo saber el usuario actual.", "Could not find the current user.")
    })?;
    Ok(match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.is_empty() => format!("{domain}\\{name}"),
        _ => name,
    })
}

/// Escapa los caracteres especiales de XML (por si la ruta tiene un "&", por ejemplo).
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Definición de la tarea:
/// - se lanza al iniciar sesión este usuario (con 10 s de margen),
/// - con los privilegios más altos (sin aviso de UAC),
/// - sin límite de tiempo (por defecto Windows la cerraría a los 3 días),
/// - también con el portátil a batería.
fn task_xml(exe: &str, user: &str) -> String {
    let exe = xml_escape(exe);
    let user = xml_escape(user);
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Inicia MemClean al entrar en Windows.</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{user}</UserId>
      <Delay>PT10S</Delay>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{user}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>HighestAvailable</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
    </Exec>
  </Actions>
</Task>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_runs_elevated_at_logon_without_time_limit() {
        let xml = task_xml(r"C:\Program Files\MemClean\memclean.exe", r"PC\Luis");
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.contains("<LogonTrigger>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
        assert!(xml.contains(r"<Command>C:\Program Files\MemClean\memclean.exe</Command>"));
        assert!(xml.contains(r"<UserId>PC\Luis</UserId>"));
    }

    #[test]
    fn special_characters_are_escaped() {
        let xml = task_xml(r"C:\Tom & Jerry\<memclean>.exe", "u");
        assert!(xml.contains(r"C:\Tom &amp; Jerry\&lt;memclean&gt;.exe"));
        assert!(!xml.contains("Tom & Jerry"));
    }

    #[test]
    fn schtasks_is_called_by_full_path() {
        let path = schtasks_path();
        assert!(path.is_absolute());
        assert!(path.ends_with(r"System32\schtasks.exe"));
    }
}
