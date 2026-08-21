; ─────────────────────────────────────────────────────────────────────────────
; SSH Manager — Script NSIS personnalisé
; Déclenché automatiquement par electron-builder pendant install/uninstall
;
; IMPORTANT — mise à jour vs désinstallation :
; electron-builder lance l'ancien désinstalleur en mode silencieux avant
; d'installer une nouvelle version, et `customUnInstall` s'exécute alors aussi.
; Tout ce qui bloque (boîte de dialogue, ExecWait sur l'application) fige cette
; étape ; pire, `ExecWait` rouvrirait l'app dont les fichiers doivent justement
; être supprimés, ce qui fait échouer la suppression et laisse l'ancienne
; version installée à côté de la nouvelle.
; D'où le garde `${ifNot} ${isUpdated}` : ces questions n'ont de sens que lors
; d'une désinstallation demandée par l'utilisateur.
; Le `/SD IDNO` est une sécurité supplémentaire : en mode silencieux, NSIS
; retiendra « Non » au lieu d'attendre une réponse.
; ─────────────────────────────────────────────────────────────────────────────

!macro customUnInstall
  ${ifNot} ${isUpdated}

    ; ── Proposer une sauvegarde des connexions ───────────────────────────────
    MessageBox MB_YESNO|MB_ICONQUESTION /SD IDNO \
      "Voulez-vous exporter vos connexions SSH/FTP avant la désinstallation ?$\r$\n$\r$\nCela vous permettra de les restaurer sur une autre machine.$\r$\n$\r$\nCliquez OUI pour lancer l'export, NON pour désinstaller directement." \
      IDNO skip_export

      ; ExecWait attend que l'appli se ferme avant de poursuivre
      ExecWait '"$INSTDIR\SSH Manager.exe" --pre-uninstall-export'

    skip_export:

    ; ── Proposer la suppression des données utilisateur ──────────────────────
    MessageBox MB_YESNO|MB_ICONQUESTION /SD IDNO \
      "Voulez-vous supprimer toutes les données enregistrées (connexions, clés chiffrées) ?$\r$\n$\r$\nCette action est irréversible.$\r$\n$\r$\nCliquez NON pour les conserver (utile si vous réinstallez plus tard)." \
      IDNO keep_data

      ; Fermer l'application si elle tourne encore : des fichiers verrouillés
      ; (Local Storage, caches) empêcheraient la suppression du dossier.
      nsExec::Exec 'taskkill /F /IM "SSH Manager.exe"'
      Sleep 1000

      ; IMPORTANT : forcer le contexte utilisateur courant.
      ; En installation "tous les utilisateurs" (élévation), $APPDATA pointe vers
      ; C:\ProgramData au lieu du profil de l'utilisateur — la suppression
      ; échouait silencieusement. SetShellVarContext current corrige la cible.
      SetShellVarContext current
      RMDir /r "$APPDATA\ssh-manager"

      ; Couvrir aussi le contexte machine au cas où des données y existeraient
      SetShellVarContext all
      RMDir /r "$APPDATA\ssh-manager"

    keep_data:

  ${endIf}
!macroend
