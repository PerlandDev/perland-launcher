; Tauri NSIS hooks, added only by the release workflow (`tauri build --config`, see
; .github/workflows/release.yml and docs/superpowers/specs/2026-11-04-branded-setup-design.md §6.7).
; After every install and update, point Windows' Uninstall at the branded uninstaller when the
; build carried it; its quiet form stays NSIS's own uninstaller.
!macro NSIS_HOOK_POSTINSTALL
  ${If} ${FileExists} "$INSTDIR\perland-uninstall.exe"
    WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\perland-uninstall.exe$\""
    WriteRegStr SHCTX "${UNINSTKEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  ${EndIf}
!macroend
