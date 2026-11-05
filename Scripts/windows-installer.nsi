; GlassMC Windows 安装包
; 用 makensis 编译：
;   makensis -DEXE_PATH=glassmc.exe -DICON_PATH=icon.ico -DOUTFILE=GlassMC_1.3.2_x64-setup.exe windows-installer.nsi
; Linux 上可以直接用 apt 的 nsis（原生 makensis），不需要 wine。

Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

; ---- 外部传入，缺省值便于本地手动试
!ifndef EXE_PATH
  !define EXE_PATH "glassmc.exe"
!endif
!ifndef ICON_PATH
  !define ICON_PATH "icon.ico"
!endif
!ifndef OUTFILE
  !define OUTFILE "GlassMC_1.3.2_x64-setup.exe"
!endif
; 需要跟 exe 一起安装的同目录文件（WebView2Loader.dll、mingw 运行时等），
; 由 makensis -DEXTRA_DIR=<目录> 指定；不传则跳过。
!ifndef EXTRA_DIR
  !define EXTRA_DIR ""
!endif

!define PRODUCT_NAME "GlassMC"
!define PRODUCT_VERSION "1.3.2"
!define PRODUCT_VERSION4 "1.3.2.0"
!define PRODUCT_PUBLISHER "GlassMC"
!define PRODUCT_WEB_SITE "https://shaun-chen2.github.io/GlassMC/"
!define PRODUCT_EXE "glassmc.exe"
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"

Name "${PRODUCT_NAME} ${PRODUCT_VERSION}"
OutFile "${OUTFILE}"
InstallDir "$PROGRAMFILES64\${PRODUCT_NAME}"
InstallDirRegKey HKLM "Software\${PRODUCT_NAME}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
XPStyle on
BrandingText "GlassMC · 液态玻璃 Minecraft 启动器"

VIProductVersion "${PRODUCT_VERSION4}"
VIAddVersionKey /LANG=2052 "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=2052 "FileDescription" "液态玻璃 Minecraft 启动器"
VIAddVersionKey /LANG=2052 "FileVersion" "${PRODUCT_VERSION}"
VIAddVersionKey /LANG=2052 "ProductVersion" "${PRODUCT_VERSION}"
VIAddVersionKey /LANG=2052 "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey /LANG=2052 "LegalCopyright" "GPLv3"

!define MUI_ABORTWARNING
!define MUI_ICON "${ICON_PATH}"
!define MUI_UNICON "${ICON_PATH}"
!define MUI_WELCOMEPAGE_TITLE "安装 GlassMC"
!define MUI_WELCOMEPAGE_TEXT "GlassMC 是一个跨平台的 Minecraft 启动器。$\r$\n$\r$\n装好之后可以直接打开，不需要额外安装 Java —— 启动器会自己检测系统里的 JDK，也可以在「安装 → Java 环境」里一键下载。$\r$\n$\r$\n游戏需要 WebView2 运行时，Windows 10 1809 及以上、Windows 11 都已自带。"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\${PRODUCT_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "立即启动 GlassMC"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

Section "GlassMC" SecMain
  SectionIn RO
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File "${EXE_PATH}"

  ; Tauri 在 Windows 上依赖 WebView2Loader.dll（与 exe 同目录）；
  ; windows-gnu 目标还可能带 libgcc_s_seh-1.dll / libwinpthread-1.dll。
  !if "${EXTRA_DIR}" != ""
    File /nonfatal "${EXTRA_DIR}\*.dll"
  !endif

  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; 快捷方式
  CreateShortcut "$SMPROGRAMS\GlassMC.lnk" "$INSTDIR\${PRODUCT_EXE}" "" "" 0
  CreateShortcut "$DESKTOP\GlassMC.lnk" "$INSTDIR\${PRODUCT_EXE}" "" "" 0

  ; 卸载信息
  WriteRegStr HKLM "Software\${PRODUCT_NAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKLM "${UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKLM "${UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr HKLM "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${PRODUCT_EXE}"
  WriteRegStr HKLM "${UNINST_KEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  WriteRegStr HKLM "${UNINST_KEY}" "QuietUninstallString" "$\"$INSTDIR\uninstall.exe$\" /S"
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${UNINST_KEY}" "NoRepair" 1

  ; 安装体积（KB），用于「应用和功能」列表显示
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${UNINST_KEY}" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\${PRODUCT_EXE}"
  !if "${EXTRA_DIR}" != ""
    Delete "$INSTDIR\*.dll"
  !endif
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\GlassMC.lnk"
  Delete "$DESKTOP\GlassMC.lnk"
  DeleteRegKey HKLM "${UNINST_KEY}"
  DeleteRegKey HKLM "Software\${PRODUCT_NAME}"

  ; 用户数据（%APPDATA%\GlassMC）刻意保留：里面有版本、模组和存档
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "要一起删除游戏数据吗？$\r$\n$\r$\n位置：$APPDATA\GlassMC（已安装的版本、模组、存档都在里面）$\r$\n选择「否」会保留，下次装回来还能接着用。" \
    /SD IDNO IDNO done
  RMDir /r "$APPDATA\GlassMC"
  done:
SectionEnd
