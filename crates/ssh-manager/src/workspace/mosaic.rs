//! Mosaïque : jusqu'à quatre sessions affichées côte à côte, et saisie
//! synchronisée entre sessions.

use std::collections::BTreeSet;

pub const SLOT_COUNT: usize = 4;
pub const SLOT_LABELS: [&str; SLOT_COUNT] =
    ["Haut gauche", "Haut droite", "Bas gauche", "Bas droite"];

/// Quelle session occupe chaque quart de la mosaïque.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Mosaic {
    slots: [Option<u64>; SLOT_COUNT],
}

impl Mosaic {
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Option::is_none)
    }

    pub fn count(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub fn slot(&self, index: usize) -> Option<u64> {
        self.slots.get(index).copied().flatten()
    }

    pub fn contains(&self, id: u64) -> bool {
        self.slots.contains(&Some(id))
    }

    /// Place une session dans un quart ; elle quitte celui qu'elle occupait.
    pub fn place(&mut self, id: u64, slot: usize) {
        if slot >= SLOT_COUNT {
            return;
        }
        self.remove(id);
        self.slots[slot] = Some(id);
    }

    pub fn remove(&mut self, id: u64) {
        for slot in &mut self.slots {
            if *slot == Some(id) {
                *slot = None;
            }
        }
    }

    pub fn clear(&mut self) {
        self.slots = [None; SLOT_COUNT];
    }

    /// Répartit les premières sessions ouvertes sur les quarts (Ctrl+M).
    pub fn fill(&mut self, ids: impl IntoIterator<Item = u64>) {
        self.clear();
        for (slot, id) in ids.into_iter().take(SLOT_COUNT).enumerate() {
            self.slots[slot] = Some(id);
        }
    }
}

/// Sessions où rejouer une saisie faite dans `source` : les autres sessions
/// synchronisées, si `source` l'est elle-même.
pub fn replicas(synced: &BTreeSet<u64>, source: u64) -> Vec<u64> {
    if !synced.contains(&source) {
        return Vec::new();
    }
    synced.iter().copied().filter(|id| *id != source).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_fill_at_most_four_slots_in_order() {
        let mut mosaic = Mosaic::default();
        mosaic.fill([7, 8, 9, 10, 11]);
        assert_eq!(
            (0..SLOT_COUNT)
                .map(|slot| mosaic.slot(slot))
                .collect::<Vec<_>>(),
            [Some(7), Some(8), Some(9), Some(10)]
        );
        assert!(!mosaic.contains(11));
    }

    #[test]
    fn should_move_a_session_instead_of_showing_it_twice() {
        let mut mosaic = Mosaic::default();
        mosaic.place(5, 0);
        mosaic.place(5, 3);
        assert_eq!(mosaic.slot(0), None);
        assert_eq!(mosaic.slot(3), Some(5));
        assert_eq!(mosaic.count(), 1);
    }

    #[test]
    fn should_become_empty_when_its_last_session_leaves() {
        let mut mosaic = Mosaic::default();
        mosaic.place(5, 1);
        mosaic.remove(5);
        assert!(mosaic.is_empty());
        assert_eq!(mosaic.count(), 0);
    }

    #[test]
    fn should_replay_input_only_from_a_synced_session_to_the_others() {
        let synced = BTreeSet::from([1, 2, 3]);
        assert_eq!(replicas(&synced, 2), [1, 3]);
        assert!(
            replicas(&synced, 4).is_empty(),
            "une session non synchronisée ne diffuse rien"
        );
    }
}
