//! Packaged native socket driver sharing the registry runtime's wire values.
//!
//! Compile the pinned driver and its connection marker together so native
//! renders observe the same context type. Signals, procedures, shards and
//! macros continue to use the registry runtime.

use topcoat::runtime::{RUNTIME_PROTOCOL, SignalValues};
use topcoat_core::context::Cx;
use topcoat_router::{Body, Layer, LayerFuture, Next, Path};

mod connection;
pub(crate) mod procedure;
pub(crate) mod signal_vec;
mod socket;

pub(crate) use connection::{ConnectedRender, connected, connected_untracked};
#[cfg(test)]
pub(crate) use socket::SocketPolicy;
pub(crate) use socket::{SocketLifetime, SocketRetirement};

/// Handles native runtime sockets before the registry layer's upgrade path.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SocketLayer;

impl Layer for SocketLayer {
    fn path(&self) -> Option<&Path> {
        None
    }

    fn handle<'a>(&'a self, cx: &'a Cx, body: Body, next: Next<'a>) -> LayerFuture<'a> {
        if socket::requested(cx) {
            return Box::pin(socket::accept(cx, body));
        }
        next.run(cx, body)
    }
}
