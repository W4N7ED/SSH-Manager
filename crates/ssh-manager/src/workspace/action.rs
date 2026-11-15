//! Les demandes que les panneaux adressent à l'espace de travail.

use ssh_manager_core::Connection;

/// Demande émise par un panneau, traitée après le rendu de l'image : on évite
/// ainsi de modifier les données pendant qu'on les parcourt pour l'affichage.
pub enum Action {
    Select(String),
    Open(String),
    /// Ouvre une connexion SSH en navigateur de fichiers (SFTP).
    OpenFiles(String),
    /// Ouvre une connexion SSH dans PowerShell ou l'invite de commandes.
    OpenExternal {
        id: String,
        shell: crate::external_terminal::ExternalShell,
    },
    /// Retire l'empreinte épinglée d'un serveur (réinstallation légitime).
    ForgetHostKey(String),
    /// Connexion ponctuelle saisie dans la recherche, non enregistrée.
    QuickConnect(Box<Connection>),
    /// Ouvre le formulaire prérempli avec une saisie `user@hôte:port`.
    SaveQuickTarget(Box<Connection>),
    OpenSettings,
    OpenRecordings,
    OpenToolbox,
    /// Exporte toute la bibliothèque : `.enc` chiffré, ou `.json` en clair.
    ExportBackup {
        plain: bool,
    },
    ImportBackup,
    /// Crée des connexions à partir de `~/.ssh/config`.
    ImportSshConfig,
    /// Ctrl+M : répartit les sessions en mosaïque, ou revient aux onglets.
    ToggleMosaic,
    PlaceInMosaic {
        session: u64,
        slot: usize,
    },
    RemoveFromMosaic(u64),
    /// Inclut ou retire une session de la saisie synchronisée.
    ToggleSync(u64),
    SetTerminalTheme(crate::session::terminal_theme::TerminalTheme),
    /// Taille du texte des terminaux (Ctrl+molette).
    SetTerminalFontSize(f32),
    NewConnection {
        group: Option<String>,
    },
    Edit(String),
    Save(Box<Connection>),
    Delete(String),
    ToggleFavorite(String),
    CreateGroup(String),
    RenameGroup {
        from: String,
        to: String,
    },
    DeleteGroup(String),
    SetGroupColor {
        group: String,
        color: Option<String>,
    },
    CloseSession(u64),
    Reconnect(u64),
    DuplicateSession(u64),
    /// Arrête les tentatives de reconnexion automatique d'une session.
    CancelAutoReconnect(u64),
    ExportSession(u64),
    RetryWithPassword(u64),
    /// La clé privée est chiffrée : redemander sa phrase de passe.
    RetryWithKeyPassphrase {
        session: u64,
        wrong: bool,
    },
    TrustHostKey {
        session: u64,
        /// `hôte:port` du serveur dont la clé a changé (la cible ou le rebond).
        host: String,
        fingerprint: String,
    },
}
