; Ganchos del instalador .exe (NSIS).
;
; Al desinstalar MemClean se borra también la tarea programada de
; "Iniciar con Windows"; si no, Windows intentaría abrir en cada inicio
; de sesión un programa que ya no existe.
; En una actualización ($UpdateMode = 1) la tarea se deja, porque la
; versión nueva la sigue usando.

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$SYSDIR\schtasks.exe" /Delete /TN "MemClean" /F'
    Pop $0 ; código de salida (se ignora: puede que la tarea no exista)
  ${EndIf}
!macroend
