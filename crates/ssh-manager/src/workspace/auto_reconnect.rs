//! Reconnexion automatique d'un terminal après une coupure (réseau, délai
//! d'inactivité, mise en veille), avec des délais croissants. Une session
//! fermée volontairement (« exit ») n'est jamais relancée.

use std::time::{Duration, Instant};

use crate::session::transport::{SessionError, SessionState};

/// Délais avant chaque tentative ; au-delà, on abandonne.
const DELAYS_SECONDS: [u64; 5] = [3, 5, 10, 30, 60];
pub const MAX_ATTEMPTS: u32 = DELAYS_SECONDS.len() as u32;

/// Délai avant la tentative `attempt` (0 pour la première), ou `None` si
/// toutes les tentatives ont été faites.
pub fn delay_before(attempt: u32) -> Option<Duration> {
    DELAYS_SECONDS
        .get(attempt as usize)
        .map(|seconds| Duration::from_secs(*seconds))
}

/// Faut-il retenter ? Après une coupure, oui ; après l'échec d'une tentative
/// automatique, seulement si le serveur était injoignable (une erreur
/// d'authentification ou de clé d'hôte demande une décision humaine).
pub fn should_retry(state: &SessionState, attempts: u32) -> bool {
    match state {
        SessionState::Closed {
            connection_lost, ..
        } => *connection_lost,
        SessionState::Failed(SessionError::Connect(_)) => attempts > 0,
        _ => false,
    }
}

/// Où en est la reconnexion automatique d'une session.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AutoReconnect {
    pub attempts: u32,
    pub due: Option<Instant>,
}

/// Ce que la reconnexion automatique affiche dans le bandeau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoReconnectStatus {
    /// Prochaine tentative dans tant de secondes.
    Scheduled {
        seconds_left: u64,
        attempt: u32,
    },
    GaveUp,
}

impl AutoReconnect {
    /// Fait avancer le compte à rebours ; renvoie vrai quand il faut relancer
    /// la session maintenant.
    pub fn tick(&mut self, state: &SessionState, now: Instant) -> bool {
        if matches!(state, SessionState::Connected) {
            *self = Self::default();
            return false;
        }
        if !should_retry(state, self.attempts) {
            self.due = None;
            return false;
        }
        match self.due {
            None => {
                self.due = delay_before(self.attempts).map(|delay| now + delay);
                false
            }
            Some(due) if now >= due => {
                self.attempts += 1;
                self.due = None;
                true
            }
            Some(_) => false,
        }
    }

    /// L'utilisateur arrête les tentatives pour cette coupure.
    pub fn give_up(&mut self) {
        self.attempts = MAX_ATTEMPTS;
        self.due = None;
    }

    pub fn status(&self, state: &SessionState, now: Instant) -> Option<AutoReconnectStatus> {
        if !should_retry(state, self.attempts) {
            return None;
        }
        Some(match self.due {
            Some(due) => AutoReconnectStatus::Scheduled {
                seconds_left: due.saturating_duration_since(now).as_secs() + 1,
                attempt: self.attempts + 1,
            },
            None if self.attempts >= MAX_ATTEMPTS => AutoReconnectStatus::GaveUp,
            None => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lost() -> SessionState {
        SessionState::Closed {
            reason: Some("connexion perdue".into()),
            connection_lost: true,
        }
    }

    #[test]
    fn should_never_retry_after_a_deliberate_exit() {
        let exited = SessionState::Closed {
            reason: Some("le shell s'est terminé (code 0)".into()),
            connection_lost: false,
        };
        assert!(!should_retry(&exited, 0));
    }

    #[test]
    fn should_retry_an_unreachable_server_only_during_automatic_attempts() {
        let unreachable = SessionState::Failed(SessionError::Connect("refusée".into()));
        assert!(
            !should_retry(&unreachable, 0),
            "échec d'une ouverture manuelle"
        );
        assert!(should_retry(&unreachable, 1));
        assert!(!should_retry(
            &SessionState::Failed(SessionError::Authentication),
            1
        ));
    }

    #[test]
    fn should_wait_the_first_delay_then_ask_for_a_retry() {
        let start = Instant::now();
        let mut auto = AutoReconnect::default();
        assert!(!auto.tick(&lost(), start), "le compte à rebours démarre");
        assert!(!auto.tick(&lost(), start + Duration::from_secs(2)));
        assert!(auto.tick(&lost(), start + Duration::from_secs(3)));
        assert_eq!(auto.attempts, 1);
    }

    #[test]
    fn should_reset_once_connected_again() {
        let start = Instant::now();
        let mut auto = AutoReconnect {
            attempts: 3,
            due: None,
        };
        auto.tick(&SessionState::Connected, start);
        assert_eq!(auto, AutoReconnect::default());
    }

    #[test]
    fn should_give_up_after_the_last_delay() {
        let start = Instant::now();
        let mut auto = AutoReconnect {
            attempts: MAX_ATTEMPTS,
            due: None,
        };
        assert!(!auto.tick(&lost(), start));
        assert_eq!(
            auto.status(&lost(), start),
            Some(AutoReconnectStatus::GaveUp)
        );
    }

    #[test]
    fn should_grow_the_delays() {
        let delays: Vec<u64> = (0..MAX_ATTEMPTS)
            .filter_map(delay_before)
            .map(|delay| delay.as_secs())
            .collect();
        assert!(delays.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(delay_before(MAX_ATTEMPTS), None);
    }
}
