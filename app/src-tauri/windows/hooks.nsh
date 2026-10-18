; Metamorphosis NSIS hooks: remove the per-user Explorer menu entries on uninstall.
!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\metamorphosis.exe" --uninstall-menus'
!macroend
