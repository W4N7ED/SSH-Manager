# SSH Manager

Gestionnaire de connexions **SSH, SFTP et FTP/FTPS** pour Windows, écrit en Rust
(interface native egui). Successeur de la version Electron 1.x, dont il reprend
automatiquement le coffre et les sauvegardes.

## Fonctionnalités

- **Terminal SSH** intégré (émulation alacritty), thèmes (8 prédéfinis + couleurs
  personnalisées), collage au clic droit ou à Ctrl+V au choix ; recherche dans
  l'historique (Ctrl+Maj+F), taille du texte à Ctrl+molette, liens ouverts par Ctrl+clic.
- **Clés privées** OpenSSH, PEM, PKCS#8 et PuTTY, y compris protégées par une phrase
  de passe (demandée à l'ouverture, mémorisable dans le coffre).
- **Mosaïque** de quatre sessions (Ctrl+M) et **saisie synchronisée** (Ctrl+Maj+S).
- **Navigateur de fichiers** SFTP et FTP/FTPS : envoi (y compris dossiers entiers et
  glisser-déposer depuis l'Explorateur), téléchargement avec progression et annulation,
  renommage (F2), droits, suppression récursive.
- **Reconnexion** dans le même onglet (Entrée, bouton ou clic droit), écran conservé ;
  reconnexion automatique après une coupure (réseau, délai, veille), jamais après « exit ».
- **Rebond** (ProxyJump) par une autre connexion enregistrée, dont les identifiants
  doivent être mémorisés ; **tunnels locaux** (`ssh -L`) sur 127.0.0.1, ouverts avec le terminal.
- **Import de `~/.ssh/config`** : HostName, User, Port, IdentityFile, ProxyJump, LocalForward.
- **Enregistrement des sessions** (chiffré) et export en `.txt`, fenêtre « Historique »,
  durée de conservation réglable.
- **Connexion rapide** `user@hôte:port` dans la recherche, **palette Ctrl+K**.
- **Boîte à outils** : générateur de mots de passe ; copies tenues hors de
  l'historique Windows (Win+V) et effacées après un délai.
- **Clés SSH** : création Ed25519 / RSA 4096 nommée dans `~/.ssh` depuis le formulaire.
- Ouverture d'une connexion dans **PowerShell** ou **CMD**.
- **Sauvegardes** `.enc` (chiffrées) et `.json`, compatibles avec la version Electron.

## Sécurité

- Coffre chiffré en AES-256-GCM ; clé dérivée d'une **phrase de 12 mots** (BIP39,
  PBKDF2-SHA512, 210 000 itérations) et protégée par **DPAPI** pour l'ouverture
  automatique sur le poste.
- Vérification de la **clé d'hôte** (épinglage à la première connexion), avertissement
  explicite en cas de changement.
- Enregistrements de session chiffrés avec une clé dérivée de celle du coffre.

## Installation

Aucun prérequis : un seul exécutable, sans runtime.

| Livrable | Données | Usage |
|---|---|---|
| `SSH-Manager-Setup-x.y.z.exe` | `%APPDATA%\SSH Manager` | Installation par utilisateur, sans droits administrateur. Propose de retirer la version Electron (ses connexions sont reprises). |
| `SSH-Manager-x.y.z-Portable.exe` | `SSH Manager Data\` à côté de l'exécutable | Clé USB. Sur un nouveau PC, la phrase de 12 mots est demandée une fois, puis l'ouverture y devient automatique. |

Un exécutable est portable si son nom contient « portable », ou si un dossier
`SSH Manager Data` existe à côté de lui.

À la désinstallation, l'installeur propose un export chiffré des connexions, puis la
suppression des données.

## Développement

Prérequis : Rust stable (édition 2024, ≥ 1.100) avec la chaîne MSVC et le SDK
Windows (`rc.exe`, pour l'icône de l'exécutable) ; NSIS pour l'installeur.

```powershell
cargo run -p ssh-manager                 # lancer en développement
cargo test --workspace                   # tests (dont SSH/SFTP de bout en bout, serveur local)
cargo clippy --workspace --all-targets   # analyse
powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1   # livrables dans dist\
```

Organisation :

| Crate | Rôle |
|---|---|
| `ssh-manager-core` | Domaine pur : connexions, bibliothèque, mots de passe, formats. |
| `ssh-manager-vault` | Coffre, chiffrement, sauvegardes, journaux scellés, migration Electron. |
| `ssh-manager` | Application egui : terminal, fichiers, interface, installeur (`installer/`). |

## Variables d'environnement

| Variable | Effet |
|---|---|
| `SSH_MANAGER_LOG` | Filtre des journaux (`tracing`), par exemple `debug` ou `ssh_manager=debug`. Par défaut : `info`. |

Les journaux (JSON) sont écrits dans `<dossier de données>\logs\ssh-manager.log`.

## Licence

GPL-3.0 — voir [LICENSE](LICENSE). Polices Archivo et JetBrains Mono sous licence
SIL OFL (`assets/fonts/*-OFL.txt`).
