; ─────────────────────────────────────────────────────────────────────────────
; SSH Manager — installeur Windows (NSIS 3, interface MUI2)
;
; Compilé par scripts/build-release.ps1 :
;   makensis /INPUTCHARSET UTF8 /DVERSION=x.y.z /DSOURCE_DIR=… /DOUTPUT_FILE=… ssh-manager.nsi
;
; Choix :
; - Installation par utilisateur (%LOCALAPPDATA%\Programs), sans droits
;   administrateur : les données sont de toute façon propres à chaque compte
;   (coffre protégé par DPAPI).
; - Une mise à jour réinstalle par-dessus le même dossier, après avoir fermé
;   l'application : aucune ancienne version ne reste à côté de la nouvelle.
; - Si l'ancienne application Electron (1.x) est présente, l'installeur propose
;   de la désinstaller ; ses connexions restent sur le disque et sont reprises
;   au premier lancement.
; ─────────────────────────────────────────────────────────────────────────────

Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !error "VERSION manquante : passez /DVERSION=x.y.z à makensis"
!endif
!ifndef SOURCE_DIR
  !error "SOURCE_DIR manquant : dossier contenant ssh-manager.exe"
!endif
!ifndef OUTPUT_FILE
  !error "OUTPUT_FILE manquant : chemin de l'installeur à produire"
!endif

!define PRODUCT_NAME "SSH Manager"
!define EXECUTABLE "ssh-manager.exe"
!define UNINSTALLER "Désinstaller SSH Manager.exe"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SSHManager"
; Clé de désinstallation de l'application Electron (dérivée de son appId).
!define LEGACY_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\280147ea-6e46-5b82-af7a-408260e77452"
!define LEGACY_EXECUTABLE "SSH Manager.exe"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\SSH Manager 2"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
BrandingText "${PRODUCT_NAME} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "Installeur de ${PRODUCT_NAME}"
VIAddVersionKey "LegalCopyright" "GPL-3.0"

!define MUI_ICON "${SOURCE_DIR}\icon.ico"
!define MUI_UNICON "${SOURCE_DIR}\icon.ico"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXECUTABLE}"
!define MUI_FINISHPAGE_RUN_TEXT "Lancer ${PRODUCT_NAME}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "${SOURCE_DIR}\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "French"

; ── Ferme l'application si elle tourne : un exécutable ouvert ne peut pas être
; remplacé, et l'ancienne version resterait en place.
!macro CloseRunningApp IMAGE
  nsExec::Exec 'taskkill /F /IM "${IMAGE}"'
  Pop $0
  Sleep 800
!macroend

; ── Ancienne application Electron ───────────────────────────────────────────
Function RemoveLegacyApp
  ReadRegStr $0 HKCU "${LEGACY_UNINSTALL_KEY}" "UninstallString"
  ${If} $0 == ""
    Return
  ${EndIf}
  ReadRegStr $1 HKCU "${LEGACY_UNINSTALL_KEY}" "DisplayVersion"
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "L'ancienne version de SSH Manager ($1) est installée.$\r$\n$\r$\nLa désinstaller maintenant ? Vos connexions sont conservées et seront reprises au premier lancement de la nouvelle version." \
    /SD IDYES IDNO keep_legacy
    !insertmacro CloseRunningApp "${LEGACY_EXECUTABLE}"
    ; Désinstallation silencieuse : l'ancien script garde les données dans ce mode.
    ExecWait '$0 /S' $2
    ; Un désinstalleur NSIS se relance depuis un dossier temporaire : ExecWait
    ; rend la main avant la fin. On attend la disparition de l'ancien
    ; exécutable (30 s au plus), sinon il supprimerait nos raccourcis, qui
    ; portent le même nom que les siens.
    StrCpy $3 0
    wait_legacy:
    ${If} ${FileExists} "$LOCALAPPDATA\Programs\SSH Manager\${LEGACY_EXECUTABLE}"
    ${AndIf} $3 < 60
      Sleep 500
      IntOp $3 $3 + 1
      Goto wait_legacy
    ${EndIf}
    DetailPrint "Ancienne version désinstallée (code $2)."
  keep_legacy:
FunctionEnd

Section "Installer"
  Call RemoveLegacyApp
  !insertmacro CloseRunningApp "${EXECUTABLE}"

  SetOutPath "$INSTDIR"
  File "${SOURCE_DIR}\${EXECUTABLE}"
  File "${SOURCE_DIR}\LICENSE"
  SetOutPath "$INSTDIR\licences"
  File "${SOURCE_DIR}\licences\*.*"
  SetOutPath "$INSTDIR"

  WriteUninstaller "$INSTDIR\${UNINSTALLER}"

  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}.lnk" "$INSTDIR\${EXECUTABLE}"
  CreateShortcut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\${EXECUTABLE}"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${EXECUTABLE}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\${UNINSTALLER}"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\${UNINSTALLER}" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
  ; Taille affichée dans « Applications installées », en Kio.
  SectionGetSize 0 $0
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "EstimatedSize" $0
SectionEnd

; ── Désinstallation ─────────────────────────────────────────────────────────
; En mode silencieux (/S), aucune question n'est posée et les données sont
; gardées : /SD retient « Non ». Les questions n'ont de sens que pour une
; désinstallation demandée par l'utilisateur.
Section "Uninstall"
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Exporter vos connexions avant la désinstallation ?$\r$\n$\r$\nUne sauvegarde chiffrée (.enc) vous permettra de les restaurer, ici ou sur une autre machine, avec votre phrase de 12 mots." \
    /SD IDNO IDNO skip_export
    ; ExecWait attend la fin de l'export avant de supprimer les fichiers.
    ExecWait '"$INSTDIR\${EXECUTABLE}" --pre-uninstall-export'
  skip_export:

  !insertmacro CloseRunningApp "${EXECUTABLE}"

  Delete "$INSTDIR\${EXECUTABLE}"
  Delete "$INSTDIR\LICENSE"
  RMDir /r "$INSTDIR\licences"
  Delete "$INSTDIR\${UNINSTALLER}"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${PRODUCT_NAME}.lnk"
  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"

  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Supprimer aussi toutes vos données (coffre des connexions, réglages, enregistrements de sessions) ?$\r$\n$\r$\nCette action est irréversible. Répondez NON pour les garder, par exemple en vue d'une réinstallation." \
    /SD IDNO IDNO keep_data
    ; Le dossier du compte courant, et non celui de tous les utilisateurs.
    SetShellVarContext current
    RMDir /r "$APPDATA\SSH Manager"
  keep_data:
SectionEnd
