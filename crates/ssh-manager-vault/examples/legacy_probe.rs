//! Diagnostic : vérifie que le coffre Electron de ce poste est lisible par la
//! récupération automatique, sans rien modifier et sans afficher de secret —
//! uniquement des compteurs.
//!
//! cargo run -p ssh-manager-vault --example legacy_probe

use ssh_manager_vault::legacy::LegacyVault;
use ssh_manager_vault::paths;

fn main() {
    let dir = paths::legacy_dir();
    let Some(legacy) = LegacyVault::locate(&dir) else {
        println!("aucun coffre Electron initialisé dans {}", dir.display());
        return;
    };
    println!(
        "coffre Electron trouvé : {} (chiffré : {})",
        dir.display(),
        legacy.is_encrypted()
    );

    let key = if legacy.is_encrypted() {
        match legacy.recover_key_automatically() {
            Ok(key) => Some(key),
            Err(error) => {
                println!("ÉCHEC de la récupération automatique : {error}");
                return;
            }
        }
    } else {
        None
    };

    match legacy.read(key) {
        Ok(import) => println!(
            "lecture réussie — connexions : {}, groupes : {}, empreintes d'hôtes : {}",
            import.data.library.connections.len(),
            import.data.library.groups.len(),
            import.data.host_keys.len()
        ),
        Err(error) => println!("ÉCHEC de la lecture : {error}"),
    }
}
