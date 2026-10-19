//! One-time, same-account draft continuation after native session rotation.
//! Handles are not credentials: consuming one requires a fresh authorized cookie.
use crate::error::LificError;
use lru::LruCache;
use rand::{RngCore, rngs::OsRng};
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use super::management_model::Continuation;
struct Stored {
    account: i64,
    expires: Option<Instant>,
    draft: Continuation,
}
// A short-lived handoff across cookie/socket rotation, bounded independently
// of the number of live pages. Never evict an unconsumed draft to admit another.
const CAPACITY: usize = 256;
const HANDOFF_LIFETIME: Duration = Duration::from_secs(600);
#[derive(Clone)]
pub(crate) struct ManagementStore(Arc<Mutex<LruCache<String, Stored>>>);
impl Default for ManagementStore {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(LruCache::new(
            NonZeroUsize::new(CAPACITY).unwrap(),
        ))))
    }
}

impl ManagementStore {
    #[cfg(test)]
    pub(super) fn poison_for_test(&self) {
        let _held = self.0.lock().unwrap();
        panic!("deliberate store poison for post-commit recovery contract");
    }

    #[cfg(test)]
    pub(super) fn preserve(&self, account: i64, draft: Continuation) -> Result<String, LificError> {
        self.insert(account, draft, Some(Instant::now() + HANDOFF_LIFETIME))
    }

    pub(super) fn reserve(
        &self,
        account: i64,
        draft: Continuation,
    ) -> Result<Reservation, LificError> {
        self.reserve_at(account, draft, Instant::now())
    }

    pub(super) fn reserve_at(
        &self,
        account: i64,
        draft: Continuation,
        _now: Instant,
    ) -> Result<Reservation, LificError> {
        let key = self.insert(account, draft, None)?;
        Ok(Reservation {
            store: self.clone(),
            account,
            key: Some(key),
        })
    }

    fn insert(
        &self,
        account: i64,
        draft: Continuation,
        expires: Option<Instant>,
    ) -> Result<String, LificError> {
        let mut entries = self.0.lock().map_err(|_| {
            LificError::Internal("project management continuation store poisoned".into())
        })?;
        let now = Instant::now();
        let expired = entries
            .iter()
            .filter(|(_, entry)| entry.expires.is_some_and(|expires| expires <= now))
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in expired {
            entries.pop(&key);
        }
        if entries.len() >= CAPACITY {
            return Err(LificError::Unavailable(
                "too many pending project management continuations".into(),
            ));
        }
        let mut bytes = [0_u8; 24];
        OsRng.fill_bytes(&mut bytes);
        let mut key = String::with_capacity(48);
        for byte in bytes {
            use std::fmt::Write;
            write!(&mut key, "{byte:02x}").expect("writing a draft key to a string cannot fail");
        }
        entries.put(
            key.clone(),
            Stored {
                account,
                expires,
                draft,
            },
        );
        Ok(key)
    }
    pub(super) fn consume(
        &self,
        key: &str,
        account: i64,
        project: i64,
    ) -> Result<Continuation, LificError> {
        let mut entries = self.0.lock().unwrap_or_else(|error| {
            tracing::error!("recovering published project management continuation store");
            error.into_inner()
        });
        let valid = entries.peek(key).is_some_and(|entry| {
            entry.account == account
                && entry.draft.project == project
                && entry
                    .expires
                    .is_some_and(|expires| expires > Instant::now())
        });
        if !valid {
            return Err(LificError::NotFound(
                "project management continuation unavailable".into(),
            ));
        }
        entries.pop(key).map(|entry| entry.draft).ok_or_else(|| {
            LificError::NotFound("project management continuation unavailable".into())
        })
    }
}

/// Owns one bounded slot until the operation publishes its handoff or exits.
/// Dropping a cancelled or refused operation releases the slot.
pub(super) struct Reservation {
    store: ManagementStore,
    account: i64,
    key: Option<String>,
}

impl Reservation {
    pub(super) fn ensure_owner(&self, account: i64) -> Result<(), LificError> {
        let entries = self.store.0.lock().map_err(|_| {
            LificError::Internal("project management continuation store poisoned".into())
        })?;
        let valid = account == self.account
            && self
                .key
                .as_deref()
                .and_then(|key| entries.peek(key))
                .is_some_and(|entry| entry.account == account);
        if valid {
            Ok(())
        } else {
            Err(LificError::NotFound(
                "project management continuation unavailable".into(),
            ))
        }
    }

    pub(super) fn publish(mut self, error: String) -> Result<String, LificError> {
        let key = self.key.as_ref().expect("live reservation owns its key");
        {
            let mut entries = self.store.0.lock().unwrap_or_else(|error| {
                tracing::error!("recovering admitted project management continuation store");
                error.into_inner()
            });
            let entry = entries
                .peek_mut(key)
                .filter(|entry| entry.account == self.account && entry.expires.is_none())
                .ok_or_else(|| {
                    LificError::NotFound("project management continuation unavailable".into())
                })?;
            entry.draft.error = error;
            entry.expires = Some(Instant::now() + HANDOFF_LIFETIME);
        }
        Ok(self.key.take().expect("published reservation owns its key"))
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            // Cancellation still releases capacity if an unrelated holder panicked.
            let mut entries = self
                .store
                .0
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if entries
                .peek(&key)
                .is_some_and(|entry| entry.account == self.account)
            {
                entries.pop(&key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::management_model::Command;
    use super::*;
    fn continuation() -> Continuation {
        Continuation {
            project: 10,
            command: Command::Add {
                user: 22,
                role: "viewer".into(),
            },
            error: String::new(),
            automatic_note: String::new(),
        }
    }
    #[test]
    fn grant_continuation_is_account_project_bound_one_shot_and_reservations_release_capacity() {
        let store = ManagementStore::default();
        let key = store.preserve(1, continuation()).unwrap();
        assert_eq!(key.len(), 48);
        assert!(
            key.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert!(store.consume(&key, 2, 10).is_err());
        assert!(store.consume(&key, 1, 11).is_err());
        assert_eq!(
            store.consume(&key, 1, 10).unwrap().command,
            continuation().command
        );
        assert!(store.consume(&key, 1, 10).is_err());
        let reservations = (0..CAPACITY)
            .map(|_| store.reserve(1, continuation()).unwrap())
            .collect::<Vec<_>>();
        assert!(store.reserve(1, continuation()).is_err());
        drop(reservations);
        assert!(store.reserve(1, continuation()).is_ok());
    }
    #[test]
    fn admitted_grant_survives_store_poison_after_rotation_without_losing_original_arguments() {
        let store = ManagementStore::default();
        let reservation = store.reserve(1, continuation()).unwrap();
        reservation.ensure_owner(1).unwrap();
        let poisoned = store.clone();
        assert!(
            std::thread::spawn(move || poisoned.poison_for_test())
                .join()
                .is_err()
        );
        let key = reservation
            .publish("Grant refused after confirmation".into())
            .unwrap();
        let snapshot = store.consume(&key, 1, 10).unwrap();
        assert_eq!(snapshot.command, continuation().command);
        assert_eq!(snapshot.error, "Grant refused after confirmation");
        assert!(store.consume(&key, 1, 10).is_err());
        // Poison before admission refuses refresh rather than losing a draft.
        assert!(store.reserve(1, continuation()).is_err());
    }
}
