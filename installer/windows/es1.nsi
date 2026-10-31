; SPDX-License-Identifier: GPL-3.0-or-later
; Copyright (C) 2026 Claude Henchoz

; ES 1 installer: copies "ES 1.vst3" into the common VST3 folder
; (C:\Program Files\Common Files\VST3) and registers an uninstaller.
;
;   makensis /DVERSION=0.4.2 /DBUNDLE=<path to ES 1.vst3> /DLICENSE=<path to LICENSE>
;            /DOUTFILE=<setup.exe> installer\windows\es1.nsi

Unicode true
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "x64.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!ifndef VERSION
  !error "pass /DVERSION=x.y.z"
!endif
!ifndef BUNDLE
  !error "pass /DBUNDLE=<path to ES 1.vst3>"
!endif
!ifndef LICENSE
  !error "pass /DLICENSE=<path to LICENSE>"
!endif
!ifndef OUTFILE
  !define OUTFILE "ES1-${VERSION}-windows-x64-setup.exe"
!endif

!define APP "ES 1"
!define PUBLISHER "Claude Henchoz"
!define REGKEY "Software\${APP}"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"
!define UNINSTDIR "$PROGRAMFILES64\${APP}"

Name "${APP} ${VERSION}"
OutFile "${OUTFILE}"
InstallDir "$COMMONFILES64\VST3"
InstallDirRegKey HKLM "${REGKEY}" "InstallDir"
RequestExecutionLevel admin
ShowInstDetails show
ShowUninstDetails show

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${APP} VST3 synthesizer installer"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026 ${PUBLISHER}, GPL-3.0-or-later"

!define MUI_ABORTWARNING

; The GPL needs no acceptance to install or run the program: show it with a plain Next button.
!define MUI_LICENSEPAGE_TEXT_TOP "ES 1 is free software, licensed under the GNU General Public License."
!define MUI_LICENSEPAGE_TEXT_BOTTOM "You do not need to accept this license to install or use ES 1. It grants you the rights to share and change the program; read it before doing so."
!define MUI_LICENSEPAGE_BUTTON "$(^NextBtn)"
!insertmacro MUI_PAGE_LICENSE "${LICENSE}"

!define MUI_DIRECTORYPAGE_TEXT_TOP "ES 1 is installed as $\"ES 1.vst3$\" inside your VST3 plugin folder. The standard folder, which every VST3 host scans, is selected; change it only if your host uses another one."
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP "ES 1 requires 64-bit Windows."
    Abort
  ${EndIf}
  SetRegView 64
  ; InstallDirRegKey runs before SetRegView, so read the previous folder again from the 64-bit view.
  ReadRegStr $0 HKLM "${REGKEY}" "InstallDir"
  ${If} $0 != ""
    StrCpy $INSTDIR $0
  ${EndIf}
FunctionEnd

Section "ES 1"
  SetRegView 64
  ; Replace an earlier version completely.
  RMDir /r "$INSTDIR\${APP}.vst3"
  SetOutPath "$INSTDIR\${APP}.vst3"
  File /r "${BUNDLE}\*.*"

  CreateDirectory "${UNINSTDIR}"
  WriteUninstaller "${UNINSTDIR}\uninstall.exe"
  WriteRegStr HKLM "${REGKEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINSTKEY}" "DisplayName" "${APP}"
  WriteRegStr HKLM "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "${UNINSTKEY}" "Publisher" "${PUBLISHER}"
  WriteRegStr HKLM "${UNINSTKEY}" "InstallLocation" "$INSTDIR\${APP}.vst3"
  WriteRegStr HKLM "${UNINSTKEY}" "UninstallString" '"${UNINSTDIR}\uninstall.exe"'
  WriteRegStr HKLM "${UNINSTKEY}" "QuietUninstallString" '"${UNINSTDIR}\uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINSTKEY}" "NoRepair" 1
  ; Size in KiB, for Apps & features.
  ${GetSize} "$INSTDIR\${APP}.vst3" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNINSTKEY}" "EstimatedSize" $0
SectionEnd

Section "Uninstall"
  SetRegView 64
  ReadRegStr $0 HKLM "${REGKEY}" "InstallDir"
  ${If} $0 != ""
    RMDir /r "$0\${APP}.vst3"
  ${EndIf}
  Delete "${UNINSTDIR}\uninstall.exe"
  RMDir "${UNINSTDIR}"
  DeleteRegKey HKLM "${UNINSTKEY}"
  DeleteRegKey HKLM "${REGKEY}"
SectionEnd
