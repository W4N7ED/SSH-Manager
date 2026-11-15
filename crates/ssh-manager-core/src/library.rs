//! La bibliothèque de connexions : la liste, les groupes, leurs couleurs, et les
//! opérations qui les maintiennent cohérents entre eux.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::connection::{Connection, ConnectionKind};

/// Tranche de la bibliothèque affichée par le rail des groupes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GroupFilter {
    #[default]
    All,
    Favorites,
    Ungrouped,
    Group(String),
}

/// Filtre par protocole de la barre de recherche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KindFilter {
    #[default]
    All,
    Only(ConnectionKind),
}

/// Tout ce que le coffre persiste au sujet des connexions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionLibrary {
    pub connections: Vec<Connection>,
    /// Groupes dans l'ordre d'affichage, y compris les groupes vides.
    pub groups: Vec<String>,
    /// Couleur de chaque groupe (`#rrggbb`).
    #[serde(default)]
    pub group_colors: BTreeMap<String, String>,
}

impl ConnectionLibrary {
    pub fn find(&self, id: &str) -> Option<&Connection> {
        self.connections
            .iter()
            .find(|connection| connection.id == id)
    }

    /// Ajoute ou remplace une connexion, et déclare son groupe s'il est nouveau.
    /// Mémorise (ou oublie) la phrase de passe de la clé privée d'une connexion.
    pub fn set_key_passphrase(&mut self, id: &str, passphrase: Option<String>) {
        if let Some(connection) = self
            .connections
            .iter_mut()
            .find(|connection| connection.id == id)
        {
            connection.key_passphrase = passphrase;
        }
    }

    pub fn upsert(&mut self, connection: Connection) {
        if let Some(group) = &connection.group {
            self.create_group(group);
        }
        match self
            .connections
            .iter_mut()
            .find(|existing| existing.id == connection.id)
        {
            Some(existing) => *existing = connection,
            None => self.connections.push(connection),
        }
    }

    /// Supprime une connexion. Renvoie la connexion retirée, s'il y en avait une.
    /// Retire une connexion ; celles qui passaient par elle (rebond) se
    /// connectent désormais directement.
    pub fn remove(&mut self, id: &str) -> Option<Connection> {
        let index = self
            .connections
            .iter()
            .position(|connection| connection.id == id)?;
        for other in &mut self.connections {
            if other.jump_connection_id.as_deref() == Some(id) {
                other.jump_connection_id = None;
            }
        }
        Some(self.connections.remove(index))
    }

    /// Ajoute des connexions importées (`~/.ssh/config`…). Une connexion déjà
    /// présente vers la même cible (`user@hôte:port`) n'est pas dupliquée ;
    /// les rebonds qui la désignaient pointent vers l'existante.
    /// Renvoie le nombre de connexions ajoutées.
    pub fn import(&mut self, imported: Vec<Connection>) -> usize {
        let mut replaced: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut added = Vec::new();
        for connection in imported {
            match self
                .connections
                .iter()
                .find(|existing| existing.target() == connection.target())
            {
                Some(existing) => {
                    replaced.insert(connection.id, existing.id.clone());
                }
                None => added.push(connection),
            }
        }
        let count = added.len();
        for mut connection in added {
            if let Some(jump) = &connection.jump_connection_id
                && let Some(existing) = replaced.get(jump)
            {
                connection.jump_connection_id = Some(existing.clone());
            }
            self.upsert(connection);
        }
        count
    }

    pub fn toggle_favorite(&mut self, id: &str) {
        if let Some(connection) = self.connections.iter_mut().find(|c| c.id == id) {
            connection.favorite = !connection.favorite;
        }
    }

    pub fn set_connection_color(&mut self, id: &str, color: Option<String>) {
        if let Some(connection) = self.connections.iter_mut().find(|c| c.id == id) {
            connection.color = color;
        }
    }

    /// Mémorise (ou oublie) le mot de passe d'une connexion, sans toucher au reste.
    pub fn set_password(&mut self, id: &str, password: Option<String>) {
        if let Some(connection) = self.connections.iter_mut().find(|c| c.id == id) {
            connection.password = password;
        }
    }

    /// Note l'ouverture d'une session sur la connexion.
    pub fn mark_used(&mut self, id: &str, timestamp: i64) {
        if let Some(connection) = self.connections.iter_mut().find(|c| c.id == id) {
            connection.last_used_at = Some(timestamp);
        }
    }

    /// Déclare un groupe. Sans effet si le nom est vide ou déjà pris.
    pub fn create_group(&mut self, name: &str) {
        let name = name.trim();
        if !name.is_empty() && !self.groups.iter().any(|group| group == name) {
            self.groups.push(name.to_owned());
        }
    }

    /// Renomme un groupe et y fait suivre connexions et couleur.
    /// Refusé (renvoie faux) si le nouveau nom est vide ou déjà pris.
    pub fn rename_group(&mut self, old_name: &str, new_name: &str) -> bool {
        let new_name = new_name.trim();
        if new_name.is_empty() || self.groups.iter().any(|group| group == new_name) {
            return false;
        }
        let Some(slot) = self.groups.iter_mut().find(|group| *group == old_name) else {
            return false;
        };
        *slot = new_name.to_owned();
        if let Some(color) = self.group_colors.remove(old_name) {
            self.group_colors.insert(new_name.to_owned(), color);
        }
        for connection in &mut self.connections {
            if connection.group.as_deref() == Some(old_name) {
                connection.group = Some(new_name.to_owned());
            }
        }
        true
    }

    /// Supprime un groupe ; ses connexions deviennent « sans groupe ».
    pub fn delete_group(&mut self, name: &str) {
        self.groups.retain(|group| group != name);
        self.group_colors.remove(name);
        for connection in &mut self.connections {
            if connection.group.as_deref() == Some(name) {
                connection.group = None;
            }
        }
    }

    pub fn set_group_color(&mut self, name: &str, color: Option<String>) {
        match color {
            Some(color) => self.group_colors.insert(name.to_owned(), color),
            None => self.group_colors.remove(name),
        };
    }

    /// Couleur effective d'une connexion : la sienne, sinon celle de son groupe.
    pub fn effective_color<'a>(&'a self, connection: &'a Connection) -> Option<&'a str> {
        connection.color.as_deref().or_else(|| {
            let group = connection.group.as_deref()?;
            self.group_colors.get(group).map(String::as_str)
        })
    }

    fn is_ungrouped(&self, connection: &Connection) -> bool {
        match &connection.group {
            Some(group) => !self.groups.contains(group),
            None => true,
        }
    }

    /// Nombre de connexions dans chaque tranche du rail.
    pub fn count(&self, filter: &GroupFilter) -> usize {
        self.connections
            .iter()
            .filter(|c| self.matches_group(c, filter))
            .count()
    }

    fn matches_group(&self, connection: &Connection, filter: &GroupFilter) -> bool {
        match filter {
            GroupFilter::All => true,
            GroupFilter::Favorites => connection.favorite,
            GroupFilter::Ungrouped => self.is_ungrouped(connection),
            GroupFilter::Group(name) => connection.group.as_deref() == Some(name.as_str()),
        }
    }

    /// Connexions visibles pour une tranche, un protocole et une recherche libre.
    /// La recherche porte sur le nom, l'hôte et l'utilisateur, sans casse.
    pub fn filter<'a>(
        &'a self,
        group: &GroupFilter,
        kind: KindFilter,
        search: &str,
    ) -> Vec<&'a Connection> {
        let query = search.trim().to_lowercase();
        self.connections
            .iter()
            .filter(|connection| self.matches_group(connection, group))
            .filter(|connection| match kind {
                KindFilter::All => true,
                KindFilter::Only(only) => connection.kind == only,
            })
            .filter(|connection| {
                query.is_empty()
                    || format!(
                        "{} {} {}",
                        connection.name, connection.host, connection.username
                    )
                    .to_lowercase()
                    .contains(&query)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection(id: &str, group: Option<&str>) -> Connection {
        Connection {
            id: id.into(),
            name: id.into(),
            host: format!("{id}.example"),
            username: "root".into(),
            group: group.map(Into::into),
            ..Connection::new(0)
        }
    }

    fn library() -> ConnectionLibrary {
        let mut library = ConnectionLibrary::default();
        library.upsert(connection("web", Some("Production")));
        library.upsert(connection("db", Some("Production")));
        library.upsert(connection("lab", None));
        library
    }

    #[test]
    fn should_connect_directly_once_the_jump_connection_is_removed() {
        let mut library = library();
        let mut db = library.find("db").unwrap().clone();
        db.jump_connection_id = Some("web".into());
        library.upsert(db);
        library.remove("web");
        assert_eq!(library.find("db").unwrap().jump_connection_id, None);
    }

    #[test]
    fn should_import_without_duplicating_and_relink_jumps_to_existing_connections() {
        let mut library = library();
        let duplicate_of_web = Connection {
            id: "imported-web".into(),
            ..connection("web", None)
        };
        let new_host = Connection {
            id: "imported-api".into(),
            jump_connection_id: Some("imported-web".into()),
            ..connection("api", None)
        };
        let added = library.import(vec![duplicate_of_web, new_host]);
        assert_eq!(added, 1);
        assert!(library.find("imported-web").is_none());
        assert_eq!(
            library
                .find("imported-api")
                .unwrap()
                .jump_connection_id
                .as_deref(),
            Some("web")
        );
    }

    #[test]
    fn should_declare_the_group_of_an_inserted_connection() {
        assert_eq!(library().groups, vec!["Production".to_string()]);
    }

    #[test]
    fn should_replace_a_connection_with_the_same_id() {
        let mut library = library();
        library.upsert(Connection {
            name: "renamed".into(),
            ..connection("web", Some("Production"))
        });
        assert_eq!(library.connections.len(), 3);
        assert_eq!(library.find("web").unwrap().name, "renamed");
    }

    #[test]
    fn should_carry_connections_and_color_through_a_group_rename() {
        let mut library = library();
        library.set_group_color("Production", Some("#ef4444".into()));
        assert!(library.rename_group("Production", "Prod"));
        assert_eq!(library.find("web").unwrap().group.as_deref(), Some("Prod"));
        assert_eq!(
            library.group_colors.get("Prod").map(String::as_str),
            Some("#ef4444")
        );
    }

    #[test]
    fn should_refuse_renaming_onto_an_existing_group() {
        let mut library = library();
        library.create_group("Staging");
        assert!(!library.rename_group("Production", "Staging"));
    }

    #[test]
    fn should_ungroup_connections_when_their_group_is_deleted() {
        let mut library = library();
        library.delete_group("Production");
        assert_eq!(library.count(&GroupFilter::Ungrouped), 3);
    }

    #[test]
    fn should_prefer_the_connection_color_over_the_group_color() {
        let mut library = library();
        library.set_group_color("Production", Some("#ef4444".into()));
        library.set_connection_color("web", Some("#22c55e".into()));
        let web = library.find("web").unwrap().clone();
        let db = library.find("db").unwrap().clone();
        assert_eq!(library.effective_color(&web), Some("#22c55e"));
        assert_eq!(library.effective_color(&db), Some("#ef4444"));
    }

    #[test]
    fn should_filter_by_group_kind_and_search_together() {
        let library = library();
        let visible = library.filter(
            &GroupFilter::Group("Production".into()),
            KindFilter::All,
            "DB",
        );
        assert_eq!(
            visible.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["db"]
        );
    }

    #[test]
    fn should_store_a_password_without_touching_other_fields() {
        let mut library = library();
        library.set_password("web", Some("s3cret".into()));
        let web = library.find("web").unwrap();
        assert_eq!(web.password.as_deref(), Some("s3cret"));
        assert_eq!(
            (web.kind, web.group.as_deref()),
            (ConnectionKind::Ssh, Some("Production"))
        );
    }

    #[test]
    fn should_store_a_key_passphrase_apart_from_the_password() {
        let mut library = library();
        library.set_key_passphrase("web", Some("phrase".into()));
        let web = library.find("web").unwrap();
        assert_eq!(web.key_passphrase.as_deref(), Some("phrase"));
        assert_eq!(web.password, None);
    }

    #[test]
    fn should_count_favorites() {
        let mut library = library();
        library.toggle_favorite("lab");
        assert_eq!(library.count(&GroupFilter::Favorites), 1);
    }
}
