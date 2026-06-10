; ─────────────────────────────────────────────────────────────────────────────
; SSH Manager — Script NSIS personnalisé
; Déclenché automatiquement par electron-builder pendant install/uninstall
; ─────────────────────────────────────────────────────────────────────────────

; ── Pré-désinstallation : proposer une sauvegarde ────────────────────────────
!macro customUnInstall
  ; Demander à l'utilisateur s'il veut exporter ses connexions avant suppression
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Voulez-vous exporter vos connexions SSH/FTP avant la désinstallation ?$\r$\n$\r$\nCela vous permettra de les restaurer sur une autre machine.$\r$\n$\r$\nCliquez OUI pour lancer l'export, NON pour désinstaller directement." \
    IDNO skip_export

    ; Lancer l'appli avec le flag --pre-uninstall-export
    ; ExecWait attend que l'appli se ferme avant de continuer la désinstallation
    ExecWait '"$INSTDIR\SSH Manager.exe" --pre-uninstall-export'

  skip_export:
!macroend

; ── Post-désinstallation : nettoyage données utilisateur (optionnel) ──────────
!macro customUnInstallAfter
  ; Proposer de supprimer les données utilisateur (config chiffrée)
  MessageBox MB_YESNO|MB_ICONQUESTION \
    "Voulez-vous supprimer toutes les données enregistrées (connexions, clés chiffrées) ?$\r$\n$\r$\nCette action est irréversible.$\r$\n$\r$\nCliquez NON pour les conserver (utile si vous réinstallez plus tard)." \
    IDNO keep_data

    ; Supprimer les fichiers de config electron-store dans AppData\Roaming
    RMDir /r "$APPDATA\ssh-manager"

  keep_data:
!macroend
