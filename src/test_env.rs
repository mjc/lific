//! LIF-401: one process, one environment, one lock.
//!
//! The unit-test binary runs ~1800 tests in a single process, and a handful
//! of them mutate or read the `LIFIC_TOKEN` environment variable. They used
//! to serialize on two *module-local* mutexes (`cli::credentials` and
//! `auth`) that never serialized against each other, plus one reader in the
//! doctor tests that took no lock at all — concrete interleavings existed
//! where a `remove_var` in one module landed between another module's
//! `set_var` and read, and the doctor path could run `getenv` concurrently
//! with an `unsafe setenv` (UB on glibc: environ can be reallocated under
//! the reader). This module is the single lock they all take.
//!
//! tokio's `Mutex` rather than `std`'s, for two reasons. The doctor test
//! holds the lock across an `.await` (a `std` guard across an await point
//! trips `clippy::await_holding_lock`, and rightly so — it would block the
//! runtime thread). And tokio's mutex does not poison: a test that panics
//! while holding the lock must not cascade into every later env test
//! failing on `PoisonError`.

use tokio::sync::{Mutex, MutexGuard};

static LIFIC_TOKEN_ENV: Mutex<()> = Mutex::const_new(());

/// Take the process-wide `LIFIC_TOKEN` lock from a synchronous test. Every
/// test that calls `set_var`/`remove_var` on `LIFIC_TOKEN`, or exercises a
/// production path that reads it, must hold this guard for the whole
/// mutate-read-restore sequence.
pub(crate) fn lock_lific_token_env_blocking() -> MutexGuard<'static, ()> {
    LIFIC_TOKEN_ENV.blocking_lock()
}

/// The same lock from an async test (held across the awaited call that
/// reads the environment).
pub(crate) async fn lock_lific_token_env() -> MutexGuard<'static, ()> {
    LIFIC_TOKEN_ENV.lock().await
}
