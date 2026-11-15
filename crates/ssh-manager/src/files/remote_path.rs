//! Chemins distants, toujours au format POSIX (`/` comme séparateur), quel que
//! soit le système local.

/// `dir` + `name`, sans double séparateur.
pub fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Dossier parent ; `None` à la racine.
pub fn parent(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_owned()),
        Some(index) => Some(trimmed[..index].to_owned()),
        None => None,
    }
}

/// Segments du fil d'Ariane : `(nom affiché, chemin complet)`, racine comprise.
pub fn breadcrumb(path: &str) -> Vec<(String, String)> {
    let mut crumbs = vec![("/".to_owned(), "/".to_owned())];
    let mut current = String::new();
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        current.push('/');
        current.push_str(segment);
        crumbs.push((segment.to_owned(), current.clone()));
    }
    crumbs
}

/// « rwxr-x--- » à partir des 9 bits de permission POSIX.
pub fn format_permissions(mode: u32) -> String {
    const FLAGS: [(u32, char); 9] = [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ];
    FLAGS
        .iter()
        .map(|(bit, flag)| if mode & bit != 0 { *flag } else { '-' })
        .collect()
}

/// Un nom de fichier fourni par l'utilisateur ne doit désigner qu'un élément
/// du dossier courant : pas de séparateur, pas de remontée.
pub fn is_valid_name(name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\0')
}

/// Droits saisis en octal (« 755 », « 0644 ») ; `None` si la saisie n'en est pas.
pub fn parse_mode(text: &str) -> Option<u32> {
    let digits = text.trim();
    if digits.is_empty() || digits.len() > 4 {
        return None;
    }
    u32::from_str_radix(digits, 8).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_octal_permissions() {
        assert_eq!(parse_mode("755"), Some(0o755));
        assert_eq!(parse_mode(" 0644 "), Some(0o644));
        assert_eq!(parse_mode("4755"), Some(0o4755));
        assert_eq!(parse_mode("789"), None, "8 et 9 ne sont pas octaux");
        assert_eq!(parse_mode("77777"), None);
        assert_eq!(parse_mode(""), None);
    }

    #[test]
    fn should_join_without_doubling_separators() {
        assert_eq!(join("/", "etc"), "/etc");
        assert_eq!(join("/var/log", "syslog"), "/var/log/syslog");
        assert_eq!(join("/var/log/", "syslog"), "/var/log/syslog");
    }

    #[test]
    fn should_find_parents_up_to_the_root() {
        assert_eq!(parent("/var/log").as_deref(), Some("/var"));
        assert_eq!(parent("/var").as_deref(), Some("/"));
        assert_eq!(parent("/var/log/").as_deref(), Some("/var"));
        assert_eq!(parent("/"), None);
    }

    #[test]
    fn should_build_a_breadcrumb_from_the_root() {
        let crumbs = breadcrumb("/volume1/backups");
        assert_eq!(
            crumbs,
            vec![
                ("/".into(), "/".into()),
                ("volume1".into(), "/volume1".into()),
                ("backups".into(), "/volume1/backups".into()),
            ]
        );
    }

    #[test]
    fn should_format_posix_permissions() {
        assert_eq!(format_permissions(0o755), "rwxr-xr-x");
        assert_eq!(format_permissions(0o640), "rw-r-----");
        // Les bits de type de fichier (ici répertoire) sont ignorés.
        assert_eq!(format_permissions(0o040_750), "rwxr-x---");
    }

    #[test]
    fn should_reject_names_that_escape_the_current_folder() {
        assert!(is_valid_name("rapport 2026.txt"));
        assert!(!is_valid_name("../etc"));
        assert!(!is_valid_name(".."));
        assert!(!is_valid_name("a/b"));
        assert!(!is_valid_name("   "));
    }
}
