Unicode True
RequestExecutionLevel admin

!include "LogicLib.nsh"

!define PRODUCT_NAME "VanySound"
!define PRODUCT_VERSION "1.0.0"
!define PRODUCT_PUBLISHER "VanySound"
!define APP_ROOT "..\.."
!define PACKAGE_ROOT "..\..\setup-prueba-otra-pc"
!define OUTPUT_DIR "..\..\dist"
!define OUTPUT_EXE "VanySound-Setup-NSIS.exe"
!define INSTALLER_ICON "..\..\src-tauri\icons\icon.ico"

Name "${PRODUCT_NAME}"
Caption "${PRODUCT_NAME} Setup"
OutFile "${OUTPUT_DIR}\${OUTPUT_EXE}"
InstallDir "$TEMP\VanySound-SetupRuntime"
Icon "${INSTALLER_ICON}"
WindowIcon On
ShowInstDetails show
BrandingText "${PRODUCT_NAME} production setup"

Page instfiles

Function .onInit
  SetShellVarContext current
FunctionEnd

Section "Install"
  DetailPrint "Preparing bootstrap package..."
  RMDir /r "$INSTDIR"
  SetOutPath "$INSTDIR"
  File /r "${PACKAGE_ROOT}\*"

  DetailPrint "Running VanySound setup..."
  ExecWait '"$SYSDIR\cmd.exe" /c ""$INSTDIR\Instalar-VanySound-Prueba.cmd""' $0
  DetailPrint "Installer exit code: $0"

  ${If} $0 != 0
    MessageBox MB_ICONSTOP|MB_OK "VanySound setup failed with exit code $0.$\r$\nCheck the logs in %TEMP% and %ProgramData%\\VanySound\\logs."
    Abort
  ${EndIf}

  DetailPrint "VanySound setup completed successfully."
SectionEnd
