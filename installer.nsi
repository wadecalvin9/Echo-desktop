!include "MUI2.nsh"
!include "FileFunc.nsh"

; General
Name "Echo"
OutFile "target\releases\Echo-Setup-v1.0.4.exe"
InstallDir "$LOCALAPPDATA\Programs\Echo"
InstallDirRegKey HKCU "Software\Echo" "Install_Dir"
RequestExecutionLevel user
SetCompressor /SOLID lzma

; Icons
!define MUI_ICON "assets\installer_icon.ico"
!define MUI_UNICON "assets\installer_icon.ico"

; Modern UI Settings
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TITLE "Welcome to Echo Setup"
!define MUI_WELCOMEPAGE_TEXT "Echo is a pure-Rust, audiophile-grade music player for streaming YouTube Music and playing your local audio library.$\r$\n$\r$\nClick Next to proceed with the installation."

!define MUI_FINISHPAGE_RUN "$INSTDIR\echo-app.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch Echo"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

; Version Information
VIProductVersion "1.0.4.0"
VIAddVersionKey "ProductName" "Echo"
VIAddVersionKey "Comments" "Pure-Rust YouTube Music & Local Audio Player"
VIAddVersionKey "CompanyName" "wadecalvin9"
VIAddVersionKey "LegalCopyright" "Copyright (C) 2026 wadecalvin9"
VIAddVersionKey "FileDescription" "Echo Installer"
VIAddVersionKey "FileVersion" "1.0.4.0"
VIAddVersionKey "ProductVersion" "1.0.4.0"

Section "Echo (required)" SecCore
    SectionIn RO

    SetOutPath "$INSTDIR"

    ; Close running echo-app if running
    nsExec::Exec 'taskkill /F /IM echo-app.exe'
    nsExec::Exec 'taskkill /F /IM echo-auth.exe'
    Sleep 500

    ; Main binaries & dependencies
    File "target\x86_64-pc-windows-gnullvm\release\echo-app.exe"
    File "target\x86_64-pc-windows-gnullvm\release\echo-auth.exe"
    File "target\x86_64-pc-windows-gnullvm\release\libunwind.dll"
    File "target\x86_64-pc-windows-gnullvm\release\WebView2Loader.dll"
    File /oname=icon.ico "assets\icon.ico"

    ; Create Shortcuts
    CreateShortCut "$SMPROGRAMS\Echo.lnk" "$INSTDIR\echo-app.exe" "" "$INSTDIR\icon.ico" 0
    CreateShortCut "$DESKTOP\Echo.lnk" "$INSTDIR\echo-app.exe" "" "$INSTDIR\icon.ico" 0

    ; Write installation path & uninstaller
    WriteRegStr HKCU "Software\Echo" "Install_Dir" "$INSTDIR"
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    ; Windows Add/Remove Programs registration
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo" "DisplayName" "Echo"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo" "DisplayIcon" "$INSTDIR\icon.ico"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo" "DisplayVersion" "1.0.4"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo" "Publisher" "wadecalvin9"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo" "URLInfoAbout" "https://github.com/wadecalvin9/Echo-desktop"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo" "UninstallString" '"$INSTDIR\Uninstall.exe"'
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo" "NoRepair" 1

    ; Estimate installed size
    ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
    IntFmt $0 "0x%08X" $0
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo" "EstimatedSize" "$0"
SectionEnd

Section "Uninstall"
    ; Terminate if running
    nsExec::Exec 'taskkill /F /IM echo-app.exe'
    nsExec::Exec 'taskkill /F /IM echo-auth.exe'
    Sleep 500

    ; Delete Files
    Delete "$INSTDIR\echo-app.exe"
    Delete "$INSTDIR\echo-auth.exe"
    Delete "$INSTDIR\libunwind.dll"
    Delete "$INSTDIR\WebView2Loader.dll"
    Delete "$INSTDIR\icon.ico"
    Delete "$INSTDIR\Uninstall.exe"

    ; Delete Shortcuts
    Delete "$SMPROGRAMS\Echo.lnk"
    Delete "$DESKTOP\Echo.lnk"

    ; Remove Directory
    RMDir "$INSTDIR"

    ; Remove Registry keys
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Echo"
    DeleteRegKey HKCU "Software\Echo"
SectionEnd
