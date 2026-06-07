!macro NSIS_HOOK_PREINSTALL
  ; Tauri 2.11 can miss its silent downgrade check because the reinstall page is skipped.
  ; Re-check the installed version before any application files are replaced.
  ReadRegStr $R8 SHCTX "${UNINSTKEY}" "DisplayVersion"
  ${If} $R8 != ""
    nsis_tauri_utils::SemverCompare "${VERSION}" $R8
    Pop $R9
    ${If} $R9 = -1
      MessageBox MB_ICONSTOP|MB_OK "$(silentDowngrades)" /SD IDOK
      SetErrorLevel 2
      Quit
    ${EndIf}
  ${EndIf}
!macroend
