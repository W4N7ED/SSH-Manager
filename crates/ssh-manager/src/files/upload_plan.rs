//! Ce qu'un envoi implique : pour un dossier local, les dossiers distants à
//! créer (parents d'abord) et chaque fichier à envoyer.

use std::path::{Path, PathBuf};

use super::remote_path;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct UploadPlan {
    /// Dossiers distants à créer, chaque parent avant ses enfants.
    pub directories: Vec<String>,
    /// Fichier local, chemin distant, taille.
    pub files: Vec<(PathBuf, String, u64)>,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Prévoit l'envoi de `source` (fichier ou dossier) dans le dossier distant
/// `remote_parent`. Les liens symboliques locaux sont ignorés : les suivre
/// pourrait envoyer bien plus que prévu, voire boucler.
pub fn plan(source: &Path, remote_parent: &str) -> std::io::Result<UploadPlan> {
    let mut plan = UploadPlan::default();
    let metadata = std::fs::symlink_metadata(source)?;
    let root_remote = remote_path::join(remote_parent, &file_name(source));
    if metadata.is_file() {
        plan.files
            .push((source.to_owned(), root_remote, metadata.len()));
        return Ok(plan);
    }
    if !metadata.is_dir() {
        return Ok(plan);
    }
    let mut pending = vec![(source.to_owned(), root_remote)];
    while let Some((local_dir, remote_dir)) = pending.pop() {
        plan.directories.push(remote_dir.clone());
        let mut children: Vec<_> = std::fs::read_dir(&local_dir)?.collect::<Result<_, _>>()?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            let kind = child.file_type()?;
            let remote = remote_path::join(&remote_dir, &child.file_name().to_string_lossy());
            if kind.is_dir() {
                pending.push((child.path(), remote));
            } else if kind.is_file() {
                plan.files
                    .push((child.path(), remote, child.metadata()?.len()));
            }
        }
    }
    // L'exploration en profondeur peut visiter un enfant avant un cousin :
    // le tri par longueur garantit seulement que chaque parent précède ses enfants.
    plan.directories
        .sort_by_key(|path| path.matches('/').count());
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree() -> PathBuf {
        let root = std::env::temp_dir()
            .join(format!("ssh-manager-plan-{:x}", rand::random::<u64>()))
            .join("site");
        std::fs::create_dir_all(root.join("assets/img")).unwrap();
        std::fs::write(root.join("index.html"), b"<html>").unwrap();
        std::fs::write(root.join("assets/app.js"), b"run()").unwrap();
        std::fs::write(root.join("assets/img/logo.png"), b"png!").unwrap();
        root
    }

    #[test]
    fn should_plan_a_single_file() {
        let root = temp_tree();
        let plan = plan(&root.join("index.html"), "/var/www").unwrap();
        assert!(plan.directories.is_empty());
        assert_eq!(
            plan.files,
            [(root.join("index.html"), "/var/www/index.html".to_owned(), 6)]
        );
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn should_create_parents_before_children_and_send_every_file() {
        let root = temp_tree();
        let plan = plan(&root, "/var/www").unwrap();
        assert_eq!(
            plan.directories,
            [
                "/var/www/site",
                "/var/www/site/assets",
                "/var/www/site/assets/img"
            ]
        );
        let mut remotes: Vec<&str> = plan
            .files
            .iter()
            .map(|(_, remote, _)| remote.as_str())
            .collect();
        remotes.sort_unstable();
        assert_eq!(
            remotes,
            [
                "/var/www/site/assets/app.js",
                "/var/www/site/assets/img/logo.png",
                "/var/www/site/index.html"
            ]
        );
        let sizes: u64 = plan.files.iter().map(|(_, _, size)| size).sum();
        assert_eq!(sizes, 6 + 5 + 4);
        std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }
}
