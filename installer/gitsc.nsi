; gitsc Windows 安装包（NSIS 3）。
; 构建：makensis -DAPP_VERSION=0.1.0 installer/gitsc.nsi
; 产物：dist/gitsc-<version>-windows-x64-setup.exe
;
; 只装 exe + 开始菜单/桌面快捷方式 + 卸载项；不写系统服务、不改 PATH。
; 运行期仍然依赖系统自带的 d3dcompiler_47.dll 与 PATH 里的 git.exe（Git for Windows）。

Unicode true

!include "MUI2.nsh"
!include "FileFunc.nsh"

!define APP_NAME "gitsc"
!define APP_PUBLISHER "tuean"
!define APP_EXE "gitsc.exe"
!define APP_ID "gitsc"

!ifndef APP_VERSION
  !define APP_VERSION "0.1.0"
!endif

!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP_ID}"

Name "${APP_NAME} ${APP_VERSION}"
BrandingText "${APP_NAME} ${APP_VERSION}"
OutFile "../dist/gitsc-${APP_VERSION}-windows-x64-setup.exe"
InstallDir "$PROGRAMFILES64\${APP_NAME}"
InstallDirRegKey HKLM "Software\${APP_ID}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

!define MUI_ICON "../design/gitsc.ico"
!define MUI_UNICON "../design/gitsc.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "运行 gitsc"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Section "gitsc" SecMain
  SectionIn RO

  SetShellVarContext all
  SetOutPath "$INSTDIR"

  File "/oname=${APP_EXE}" "../dist/gitsc-windows-x86_64.exe"
  File "/oname=gitsc.ico" "../design/gitsc.ico"

  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${APP_NAME}"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\gitsc.ico"
  CreateShortcut "$SMPROGRAMS\${APP_NAME}\卸载 gitsc.lnk" "$INSTDIR\uninstall.exe"
  CreateShortcut "$DESKTOP\${APP_NAME}.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\gitsc.ico"

  WriteRegStr HKLM "Software\${APP_ID}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "Software\${APP_ID}" "Version" "${APP_VERSION}"

  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "${APP_NAME}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "${APP_PUBLISHER}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\gitsc.ico"
  WriteRegStr HKLM "${UNINST_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKLM "${UNINST_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNINST_KEY}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  SetShellVarContext all

  Delete "$INSTDIR\${APP_EXE}"
  Delete "$INSTDIR\gitsc.ico"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${APP_NAME}\${APP_NAME}.lnk"
  Delete "$SMPROGRAMS\${APP_NAME}\卸载 gitsc.lnk"
  RMDir "$SMPROGRAMS\${APP_NAME}"
  Delete "$DESKTOP\${APP_NAME}.lnk"

  DeleteRegKey HKLM "${UNINST_KEY}"
  DeleteRegKey HKLM "Software\${APP_ID}"

  ; 用户配置（最近仓库、日志）保留，卸载不删数据。
SectionEnd
