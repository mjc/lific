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

use super::model::Draft;
struct Stored {
    account: i64,
    expires: Option<Instant>,
    draft: Draft,
}
// A short-lived handoff across cookie/socket rotation, bounded independently
// of the number of live pages. Never evict an unconsumed draft to admit another.
const CAPACITY: usize = 256;
const HANDOFF_LIFETIME: Duration = Duration::from_secs(600);
#[derive(Clone)]
pub(crate) struct DraftStore(Arc<Mutex<LruCache<String, Stored>>>);
impl Default for DraftStore {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(LruCache::new(
            NonZeroUsize::new(CAPACITY).unwrap(),
        ))))
    }
}

impl DraftStore {
    #[cfg(test)]
    pub(super) fn poison_for_test(&self) {
        let _held = self.0.lock().unwrap();
        panic!("deliberate store poison for post-commit recovery contract");
    }

    #[cfg(test)]
    pub(crate) fn preserve(&self, account: i64, draft: Draft) -> Result<String, LificError> {
        self.insert(account, draft, Some(Instant::now() + HANDOFF_LIFETIME))
    }

    pub(super) fn reserve(&self, account: i64, draft: Draft) -> Result<Reservation, LificError> {
        self.reserve_at(account, draft, Instant::now())
    }

    pub(super) fn reserve_at(
        &self,
        account: i64,
        draft: Draft,
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
        draft: Draft,
        expires: Option<Instant>,
    ) -> Result<String, LificError> {
        let mut entries = self
            .0
            .lock()
            .map_err(|_| LificError::Internal("project draft store poisoned".into()))?;
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
                "too many pending project drafts".into(),
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
    #[cfg(test)]
    pub(crate) fn update_error(
        &self,
        key: &str,
        account: i64,
        error: String,
    ) -> Result<(), LificError> {
        let mut entries = self
            .0
            .lock()
            .map_err(|_| LificError::Internal("project draft store poisoned".into()))?;
        let Some(entry) = entries.peek_mut(key).filter(|entry| {
            entry.account == account
                && entry
                    .expires
                    .is_some_and(|expires| expires > Instant::now())
        }) else {
            return Err(LificError::NotFound("project draft unavailable".into()));
        };
        entry.draft.error = error;
        Ok(())
    }
    pub(crate) fn consume(&self, key: &str, account: i64) -> Result<Draft, LificError> {
        let mut entries = self
            .0
            .lock()
            .map_err(|_| LificError::Internal("project draft store poisoned".into()))?;
        let valid = entries.peek(key).is_some_and(|entry| {
            entry.account == account
                && entry
                    .expires
                    .is_some_and(|expires| expires > Instant::now())
        });
        if !valid {
            return Err(LificError::NotFound("project draft unavailable".into()));
        }
        entries
            .pop(key)
            .map(|entry| entry.draft)
            .ok_or_else(|| LificError::NotFound("project draft unavailable".into()))
    }
}

/// Owns one bounded slot until the operation publishes its handoff or exits.
/// Dropping a cancelled or refused operation releases the slot.
pub(super) struct Reservation {
    store: DraftStore,
    account: i64,
    key: Option<String>,
}

impl Reservation {
    pub(super) fn ensure_owner(&self, account: i64) -> Result<(), LificError> {
        let entries = self
            .store
            .0
            .lock()
            .map_err(|_| LificError::Internal("project draft store poisoned".into()))?;
        let valid = account == self.account
            && self
                .key
                .as_deref()
                .and_then(|key| entries.peek(key))
                .is_some_and(|entry| entry.account == account);
        if valid {
            Ok(())
        } else {
            Err(LificError::NotFound("project draft unavailable".into()))
        }
    }

    pub(super) fn publish(mut self, error: String) -> Result<String, LificError> {
        let key = self.key.as_ref().expect("live reservation owns its key");
        {
            let mut entries = self
                .store
                .0
                .lock()
                .map_err(|_| LificError::Internal("project draft store poisoned".into()))?;
            let entry = entries
                .peek_mut(key)
                .filter(|entry| entry.account == self.account && entry.expires.is_none())
                .ok_or_else(|| LificError::NotFound("project draft unavailable".into()))?;
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
    use super::*;
    #[test]
    fn capacity_rejects_before_rotation_without_evicting_existing_draft() {
        let store = DraftStore::default();
        let draft = Draft {
            name: "Draft".into(),
            identifier: "D".into(),
            identifier_touched: false,
            description: String::new(),
            emoji: String::new(),
            lead: None,
            group: None,
            error: String::new(),
        };
        let first = store.preserve(1, draft.clone()).unwrap();
        for _ in 1..CAPACITY {
            store.preserve(1, draft.clone()).unwrap();
        }
        assert!(matches!(
            store.preserve(1, draft),
            Err(LificError::Unavailable(_))
        ));
        assert_eq!(store.0.lock().unwrap().len(), CAPACITY);
        assert_eq!(store.consume(&first, 1).unwrap().name, "Draft");
    }
    #[test]
    fn expired_handoff_cannot_be_consumed_or_updated() {
        let store = DraftStore::default();
        let draft = Draft {
            name: "Draft".into(),
            identifier: "D".into(),
            identifier_touched: false,
            description: String::new(),
            emoji: String::new(),
            lead: None,
            group: None,
            error: String::new(),
        };
        let key = store.preserve(1, draft).unwrap();
        store.0.lock().unwrap().peek_mut(&key).unwrap().expires = Some(Instant::now());
        assert!(store.consume(&key, 1).is_err());
        assert!(store.update_error(&key, 1, "Failed".into()).is_err());
    }
    #[test]
    fn draft_is_same_account_one_time_and_retains_all_fields_after_rotation_failure() {
        let store = DraftStore::default();
        let draft = Draft {
            name: "Keep me".into(),
            identifier: "MAN".into(),
            identifier_touched: true,
            description: "Unsaved notes".into(),
            emoji: "lucide:Folder".into(),
            lead: Some(2),
            group: Some(3),
            error: String::new(),
        };
        let key = store.preserve(1, draft.clone()).unwrap();
        assert!(store.consume(&key, 2).is_err());
        store
            .update_error(&key, 1, "Identifier already exists".into())
            .unwrap();
        let resumed = store.consume(&key, 1).unwrap();
        assert_eq!(resumed.name, draft.name);
        assert_eq!(resumed.description, draft.description);
        assert_eq!(resumed.identifier, draft.identifier);
        assert_eq!(resumed.identifier_touched, draft.identifier_touched);
        assert_eq!(resumed.emoji, draft.emoji);
        assert_eq!(resumed.lead, draft.lead);
        assert_eq!(resumed.group, draft.group);
        assert_eq!(resumed.error, "Identifier already exists");
        assert!(store.consume(&key, 1).is_err());
    }

    #[test]
    fn active_reservation_survives_ten_minutes_and_handoff_pruning() {
        let store = DraftStore::default();
        let previous = Instant::now()
            .checked_sub(HANDOFF_LIFETIME + Duration::from_secs(1))
            .expect("clock supports the prior reservation instant");
        let reservation = store
            .reserve_at(
                1,
                Draft {
                    name: "Keep me".into(),
                    ..Default::default()
                },
                previous,
            )
            .unwrap();
        let unrelated = store.preserve(2, Draft::default()).unwrap();
        let handle = reservation
            .publish("Project created, but it wasn't added to the group.".into())
            .unwrap();
        assert!(store.consume(&handle, 2).is_err());
        let warning = store.consume(&handle, 1).unwrap();
        assert_eq!(warning.name, "Keep me");
        assert!(warning.error.contains("Project created"));
        assert!(store.consume(&handle, 1).is_err());
        assert!(store.consume(&unrelated, 2).is_ok());
    }

    #[tokio::test]
    async fn cancelled_reservation_releases_its_slot_without_evicting_another_handoff() {
        let store = DraftStore::default();
        let handoff = store.preserve(2, Draft::default()).unwrap();
        let reservation = store.reserve(1, Draft::default()).unwrap();
        reservation.ensure_owner(1).unwrap();
        assert!(reservation.ensure_owner(2).is_err());
        let (ready, started) = tokio::sync::oneshot::channel();
        let operation = tokio::spawn(async move {
            let _reservation = reservation;
            ready.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        started.await.unwrap();
        assert_eq!(store.0.lock().unwrap().len(), 2);
        operation.abort();
        assert!(operation.await.unwrap_err().is_cancelled());
        assert_eq!(store.0.lock().unwrap().len(), 1);
        assert!(store.consume(&handoff, 2).is_ok());
    }

    #[test]
    fn active_reservation_is_not_consumable_and_publication_starts_handoff_expiry() {
        let store = DraftStore::default();
        let reservation = store.reserve(1, Draft::default()).unwrap();
        let handle = reservation.key.as_ref().unwrap().clone();
        assert!(store.consume(&handle, 1).is_err());
        reservation.publish("Warning".into()).unwrap();
        store.0.lock().unwrap().peek_mut(&handle).unwrap().expires = Some(Instant::now());
        assert!(store.consume(&handle, 1).is_err());
    }
}
