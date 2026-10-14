; Tauri NSIS hooks: set up and tear down the elevated temperature helper.
; The installer is per-machine, so these run with administrator rights (one UAC prompt).

; The installer is a 32-bit process; "sysnative" reaches the 64-bit PowerShell so the
; scripts see the real Program Files and the 64-bit task scheduler / registry views.
!macro SYSREWIND_PS SCRIPT ARGS
  ; Preserve registers the surrounding Tauri installer code may rely on.
  Push $0
  Push $1
  StrCpy $1 "$WINDIR\sysnative\WindowsPowerShell\v1.0\powershell.exe"
  IfFileExists "$1" +2 0
    StrCpy $1 "powershell.exe"
  nsExec::ExecToLog '"$1" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\${SCRIPT}" ${ARGS}'
  Pop $0 ; exit code, unused
  Pop $1
  Pop $0
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; Upgrading: stop the running helper so sysrewind.exe can be replaced.
  IfFileExists "$INSTDIR\uninstall-sensors.ps1" 0 +2
    !insertmacro SYSREWIND_PS "uninstall-sensors.ps1" ""
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Setting up temperature sensors..."
  !insertmacro SYSREWIND_PS "install-sensors.ps1" '-InstallDir "$INSTDIR"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro SYSREWIND_PS "uninstall-sensors.ps1" "-Remove"
!macroend
