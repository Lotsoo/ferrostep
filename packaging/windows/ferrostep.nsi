; Ferrostep Windows installer (NSIS 3, Modern UI 2).
;
; Per-user install: no administrator prompt, installs to
; %LOCALAPPDATA%\Programs\Ferrostep, adds Desktop + Start menu shortcuts and
; an entry in Settings > Apps so it can be uninstalled like any other app.
; The uninstaller never deletes the player's Songs folder.
;
; Build (from the repo root):
;   makensis -DVERSION=0.1.0 packaging/windows/ferrostep.nsi
; Compile-time paths use "/" and are relative to this file, so the same
; script works with makensis on Linux and on Windows.

Unicode true
SetCompressor /SOLID lzma

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef EXE
  !define EXE "../../target/x86_64-pc-windows-gnu/release/ferrostep.exe"
!endif
!ifndef OUTDIR
  !define OUTDIR "../../dist"
!endif

!define APP "Ferrostep"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"
!define ROOT "../.."

Name "${APP}"
OutFile "${OUTDIR}/Ferrostep-Setup-${VERSION}.exe"
InstallDir "$LOCALAPPDATA\Programs\${APP}"
InstallDirRegKey HKCU "${UNINST_KEY}" "InstallLocation"
RequestExecutionLevel user
BrandingText "${APP} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=0 "ProductName" "${APP}"
VIAddVersionKey /LANG=0 "FileDescription" "${APP} Setup"
VIAddVersionKey /LANG=0 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=0 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=0 "LegalCopyright" "MIT License"

!include "MUI2.nsh"

!define MUI_ICON "${ROOT}/assets/icons/ferrostep.ico"
!define MUI_UNICON "${ROOT}/assets/icons/ferrostep.ico"
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Selamat datang di ${APP}"
!define MUI_WELCOMEPAGE_TEXT "Ferrostep adalah game ritme 4 panah untuk keyboard dan dance mat.$\r$\n$\r$\nInstaller akan memasang Ferrostep untuk akun Windows-mu saja (tanpa hak administrator) dan membuat pintasan di Desktop serta menu Start. Hapus kapan saja lewat Settings > Apps."
!define MUI_FINISHPAGE_RUN "$INSTDIR\ferrostep.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Mainkan ${APP} sekarang"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Indonesian"

Section "Ferrostep" SecMain
  SetOutPath "$INSTDIR"
  File "/oname=ferrostep.exe" "${EXE}"
  File "/oname=LICENSE.txt" "${ROOT}/LICENSE"
  File "/oname=LICENSE-Inter.txt" "${ROOT}/assets/fonts/LICENSE-Inter.txt"
  File "/oname=README.md" "${ROOT}/README.md"

  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; Shortcuts: Desktop and Start menu, both with the app icon.
  CreateShortcut "$DESKTOP\${APP}.lnk" "$INSTDIR\ferrostep.exe" "" "$INSTDIR\ferrostep.exe" 0
  CreateShortcut "$SMPROGRAMS\${APP}.lnk" "$INSTDIR\ferrostep.exe" "" "$INSTDIR\ferrostep.exe" 0

  ; Entry in Settings > Apps > Installed apps.
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "${APP}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\ferrostep.exe"
  WriteRegStr HKCU "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
  SectionGetSize ${SecMain} $0
  WriteRegDWORD HKCU "${UNINST_KEY}" "EstimatedSize" $0
SectionEnd

Section "Uninstall"
  Delete "$DESKTOP\${APP}.lnk"
  Delete "$SMPROGRAMS\${APP}.lnk"
  Delete "$INSTDIR\ferrostep.exe"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\LICENSE-Inter.txt"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\Uninstall.exe"
  ; Not recursive on purpose: the Songs folder (the player's songs) stays.
  RMDir "$INSTDIR"
  DeleteRegKey HKCU "${UNINST_KEY}"
SectionEnd
