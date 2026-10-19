//! Bounded owner-bound receipts. Cached outcomes contain no catalog metadata.
use super::actions::Write;
use crate::{
    db::DbPool, error::LificError, realtime::RealtimeHub, resolve_caller::ResolvedIdentity,
};
use lru::LruCache;
use rand::{RngCore, rngs::OsRng};
use std::{
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
const CAPACITY: usize = 256;
const LIFETIME: Duration = Duration::from_secs(600);
struct Stored {
    owner: i64,
    expires: Instant,
    write: Write,
    state: Status,
}
enum Status {
    Ready,
    Running,
    Applied(Commit),
}
#[derive(Clone)]
pub(crate) struct SidebarWriteStore(Arc<Mutex<LruCache<String, Stored>>>);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Commit {
    pub(super) error: Option<String>,
    pub(super) warning: String,
}
impl Default for SidebarWriteStore {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(LruCache::new(
            NonZeroUsize::new(CAPACITY).unwrap(),
        ))))
    }
}
fn fresh_owner(db: &DbPool, identity: &Option<ResolvedIdentity>) -> Result<i64, LificError> {
    let user = crate::api::require_user(identity)?;
    let conn = db.read()?;
    crate::auth::fresh_caller(&conn, user.id)?;
    Ok(user.id)
}
impl SidebarWriteStore {
    pub(super) fn reserve(&self, account: i64, write: Write) -> Result<String, LificError> {
        // Same upper boundary as the real router's JSON request body. Slots are
        // independently bounded and live slots are never evicted for admission.
        if super::actions::encode(&write)?.len() > 2 * 1024 * 1024 {
            return Err(LificError::BadRequest(
                "Sidebar change is too large.".into(),
            ));
        }
        let mut entries = self
            .0
            .lock()
            .map_err(|_| LificError::Internal("sidebar write store poisoned".into()))?;
        let now = Instant::now();
        let expired = entries
            .iter()
            .filter(|(_, entry)| entry.expires <= now && !matches!(entry.state, Status::Running))
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in expired {
            entries.pop(&key);
        }
        if entries.len() >= CAPACITY {
            return Err(LificError::Unavailable(
                "Too many pending sidebar changes. Try again.".into(),
            ));
        }
        let key = loop {
            let mut bytes = [0_u8; 24];
            OsRng.fill_bytes(&mut bytes);
            let candidate = crate::auth::hex_encode(&bytes);
            if !entries.contains(&candidate) {
                break candidate;
            }
        };
        entries.put(
            key.clone(),
            Stored {
                owner: account,
                expires: now + LIFETIME,
                write,
                state: Status::Ready,
            },
        );
        Ok(key)
    }
    pub(super) fn execute(
        &self,
        db: &DbPool,
        hub: &RealtimeHub,
        identity: &Option<ResolvedIdentity>,
        key: &str,
    ) -> Result<Commit, LificError> {
        let mut entries = self
            .0
            .lock()
            .map_err(|_| LificError::Internal("sidebar write store poisoned".into()))?;
        let owner = fresh_owner(db, identity)?;
        let entry = entries
            .get_mut(key)
            .filter(|entry| entry.owner == owner && entry.expires > Instant::now())
            .ok_or_else(|| {
                LificError::NotFound(
                    "Sidebar change receipt is unavailable. Refresh before trying again.".into(),
                )
            })?;
        match &entry.state {
            Status::Applied(commit) => return Ok(commit.clone()),
            Status::Running => {
                return Err(LificError::Unavailable(
                    "Sidebar change is still being confirmed.".into(),
                ));
            }
            Status::Ready => {}
        }
        entry.state = Status::Running;
        // Synchronous shared transaction functions do not yield while this
        // receipt is Running. Duplicate apply/recovery waits and replays its
        // terminal outcome; it cannot enter the domain mutation a second time.
        let commit = match super::actions::commit(db, hub, identity, &entry.write) {
            Ok(warning) => Commit {
                error: None,
                warning,
            },
            Err(error) => Commit {
                error: Some(super::actions::error(error)),
                warning: String::new(),
            },
        };
        entry.expires = Instant::now() + LIFETIME;
        entry.state = Status::Applied(commit.clone());
        Ok(commit)
    }
    /// Test-only expiry changes the real host clock boundary, never the outcome.
    #[cfg(test)]
    pub(crate) fn expire_applied_for_test(&self, owner: i64, key: &str) -> bool {
        let mut entries = self.0.lock().unwrap();
        let Some(entry) = entries.get_mut(key).filter(|entry| {
            entry.owner == owner
                && entry.expires > Instant::now()
                && matches!(&entry.state, Status::Applied(_))
        }) else {
            return false;
        };
        entry.expires = Instant::now();
        true
    }
    pub(super) fn confirmed(
        &self,
        db: &DbPool,
        identity: &Option<ResolvedIdentity>,
        key: &str,
    ) -> Result<(Write, Commit), LificError> {
        let mut entries = self
            .0
            .lock()
            .map_err(|_| LificError::Internal("sidebar write store poisoned".into()))?;
        let owner = fresh_owner(db, identity)?;
        let entry = entries
            .get_mut(key)
            .filter(|entry| entry.owner == owner && entry.expires > Instant::now())
            .ok_or_else(|| {
                LificError::NotFound(
                    "Sidebar change receipt is unavailable. Refresh before trying again.".into(),
                )
            })?;
        match &entry.state {
            Status::Applied(commit) => Ok((entry.write.clone(), commit.clone())),
            Status::Ready | Status::Running => Err(LificError::Unavailable(
                "Sidebar change is still being confirmed.".into(),
            )),
        }
    }
    pub(super) fn recover(
        &self,
        db: &DbPool,
        identity: &Option<ResolvedIdentity>,
        key: &str,
    ) -> Result<Commit, LificError> {
        let mut entries = self
            .0
            .lock()
            .map_err(|_| LificError::Internal("sidebar write store poisoned".into()))?;
        let owner = fresh_owner(db, identity)?;
        let entry = entries
            .get_mut(key)
            .filter(|entry| entry.owner == owner && entry.expires > Instant::now())
            .ok_or_else(|| {
                LificError::NotFound(
                    "Sidebar change receipt is unavailable. Refresh before trying again.".into(),
                )
            })?;
        match &entry.state {
            Status::Applied(commit) => Ok(commit.clone()),
            Status::Running => Err(LificError::Unavailable(
                "Sidebar change is still being confirmed.".into(),
            )),
            Status::Ready => {
                let commit = Commit {
                    error: Some("The change wasn't sent. Try again.".into()),
                    warning: String::new(),
                };
                entry.expires = Instant::now() + LIFETIME;
                entry.state = Status::Applied(commit.clone());
                Ok(commit)
            }
        }
    }
}
#[cfg(test)]
#[path = "receipts_tests.rs"]
mod tests;
