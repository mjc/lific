This crate is part of [`topcoat`](https://github.com/tokio-rs/topcoat).


## Lific vendored source

This directory preserves the pinned `topcoat-runtime` 0.9.0 crates.io source.
It is MIT licensed; the upstream copyright and license are preserved in
`LICENSE`. Two sources have moved to their canonical application locations:
`src/topcoat/runtime/connection.rs` and `src/topcoat/runtime/socket.rs`, relative
to the repository root. Cargo excludes nested crates from application packages,
so these files live outside this nested crate. The application compiles them as
ordinary modules in both checkout and packaged builds, without a Cargo patch.
This copy's `src/lib.rs` and `src/layer.rs` use relative `#[path]` attributes to
compile those same files for standalone upstream tests. The normalized manifest
is retained for that standalone workflow in a repository checkout.

The bridge uses the registry runtime's `RUNTIME_PROTOCOL` and `SignalValues`,
and the registry core, router and view crates, each pinned to `=0.9.0` in the
application manifest. Signals, procedures, shards, rendering scopes and macros
share that dependency graph. The canonical connection source defines one local
`ConnectedRender` marker, used by both the driver and every native
connection check. No private registry marker or duplicate signal type is used.
The application registers its socket admission layer outside the bridge, and
the bridge outside the registry runtime layer. HTTP reruns continue through
the registry layer; admitted native socket upgrades use the canonical driver.

The socket source imports its marker and wire values from `super`, so it can
compile under both the application bridge and the standalone vendor layer.
The standalone `src/layer.rs` imports the original crate's marker and signal
values for that purpose. The moved `connection.rs` is unchanged upstream source.
Its original path in `UPSTREAM-SHA256SUMS` remains `src/connection.rs`; the moved
socket source's original path remains `src/layer/socket.rs`.

- Upstream repository: https://github.com/tokio-rs/topcoat/tree/v0.9.0/crates/topcoat-runtime
- Package SHA-256: `c4c4dc39a1c6ef6f0f604ecba3949baf401a30b5b44e7b3f830c3509e6b560dd`
- Unchanged moved `connection.rs` SHA-256: `67aaeaf4f39ba790d4dad72234e998467635ef41ab92f124d31ee4b9920d59ca`
- Unchanged upstream `src/layer/socket.rs` SHA-256: `960d8e516a4178d5f47a30648abb44b21aba7a1f06b0fc5dde779e617ccdc026`
- Unchanged `browser/dist/index.js` SHA-256: `980dd1be1962006b98b8c1646b0e6a4f86a78ec721e2739f59ddf4c4c1c5c8b4`

The canonical `socket.rs::accept` retains the incoming `Cx`: clone it,
move it into the upgrade callback, and explicitly drop it after the raw socket
`run` future completes. This retains request-scoped resources through idle
connections before their first render. A failed upgrade drops the callback
and its captured context. Application authentication and quota decisions stay
in Lific's Rust admission layer. Synthetic render requests retain the upstream
header/remote-address behavior; their extensions are not the lifetime owner.

Context retention and policy selection in the upgrade callback:

```diff
     let target = Arc::new(ConnectionTarget::from_handshake(cx));
+    let policy = try_app_context::<SocketPolicy>(cx).copied().unwrap_or_default();
+    let retirement = try_request_context::<SocketLifetime>(cx)
+        .and_then(SocketLifetime::take)
+        .unwrap_or_else(|| Box::pin(std::future::pending()));
+    let connection_context = cx.clone();
     upgrade
         .protocols([RUNTIME_PROTOCOL])
-        .on_upgrade(move |socket| run(target, socket))
+        .on_upgrade(move |socket| async move {
+            run(target, socket, policy, retirement).await;
+            drop(connection_context);
+        })
```

`UPSTREAM-SHA256SUMS` records every copied package file before modification.
The additional Rust patch introduces `SocketPolicy` in the canonical `socket.rs`
and reexports it from `src/layer.rs`. Its checked constructor accepts nonzero
ping, progress and send intervals, with pings preceding progress expiry. The
defaults are 30 seconds, 120 seconds and 5 seconds respectively, matching
Lific's existing REST transport. `accept` reads the optional application policy
before upgrading; connections without one use these defaults.

The remaining changes to `socket.rs` replace the upstream unbounded
forwarding/join with bounded sends, scheduled protocol Ping frames and an
independent absolute progress deadline. Actual inbound frames extend that
deadline. A watch channel carries deadlines so a receive pump waiting for a
full output queue cannot suspend expiry. Completion of either pump or expiry
retires the connection and aborts/awaits its active render. `RenderTask` also
aborts on drop if the socket owner is cancelled. Render replacement retains
the upstream abort-and-await ordering before the next run announcement, and
the existing output channel capacity of 16 is preserved.

The socket layer also exports `SocketLifetime` and `SocketRetirement`. The
application installs one `Send` retirement future in the upgrade request context.
`accept` takes it once, before the upgrade callback captures that context. A
missing hook remains pending. Its future is polled independently of the input,
output and progress pumps; completing it cancels those pumps and aborts/awaits
the active render before retirement output. A `Redirect` uses the existing
protocol frame, followed by a close; each operation has the configured send
bound, so retirement output can take up to two send intervals. `Close` sends
only the close. The driver contains no application authority decisions.
The application must capture its parent context before installing the lifetime
on a child, so its future cannot hold a reference cycle to itself. The original
render request construction and browser distribution remain unchanged.

The normalized `Cargo.toml` adds the native Tokio `time` and `macros` features
needed by this coordinator. It also restores the upstream test-only Topcoat
dependency omitted by the packaged manifest, using registry version `=0.9.0`,
default features disabled and `asset`/`view`/`runtime` enabled instead of the upstream
workspace path. The runtime feature supports the original documentation example; the asset
feature enables the facade script component's asset attribute rendering.
A standalone Cargo patch directs the test facade back to this runtime, keeping
the standalone tests and example on this copy's implementation. The application
does not use that patch or this manifest. The standalone `Cargo.lock`
adds the test dependency's closure while
retaining existing package versions. This preserves the original runtime tests.
`Cargo.toml.orig` remains the unmodified upstream workspace manifest for
provenance; it is not the manifest used to build this copy.

This README, `src/lib.rs`, `src/layer.rs`, the moved socket source, and the
normalized Cargo manifest and lock differ from the original package. The
connection source is moved without changes; LICENSE and this checksum list are
additions. Restoring the two original paths and reversing the declared module,
policy, lifecycle, context retention and manifest changes reproduces the
upstream source hashes.
The original connection and socket unit tests also compile in the
application's test module tree. Unit tests cover one-shot consumption, exact
Close outcomes and retaining a
Send-only future in the Send + Sync request context. Actual application TCP
tests cover request-scoped Redirect lifetimes, idle session retirement,
periodic revalidation, stable ownership across rerenders and peer cleanup.
The browser source includes the generic procedure, vector signal and native
event adapters described below. The vendor distribution is unchanged. Lific's existing
`src/topcoat/assets/runtime.js` transport patches remain separate. Its document
runtime claims navigation once across sibling framework redirects.


## Generic procedure keepalive browser adapter

`browser/src/surrogate/procedure.ts` adds `call_keepalive` and the callable
`with_keepalive` adapter, sharing the original lazy Future transport. Ordinary
`call` retains its request options; keepalive calls add only Fetch's keepalive
flag. The application uses the registry Rust type graph and a typed extension
in `src/topcoat/runtime/procedure.rs`, rather than this copy's Rust procedure
module. Its expression adapter preserves the registry Args/Output contract.
The packaged application browser asset carries a matching reversible patch;
its license provenance and reconstruction oracle describe that addition.
The browser source therefore differs from the original package in this file;
the checked-in vendor distribution has not been rebuilt by this change.

## Generic vector signal writes

The browser signal and sequence sources add immutable `push` and `remove`
writes, preserving earlier snapshots and validating index width and bounds.
The application uses the registry signal types through the typed extension in
`src/topcoat/runtime/signal_vec.rs`. These primitives contain no application
decisions. The packaged runtime carries the same two reversible substitutions;
its reconstruction test still proves the exact pinned upstream SHA-256. The
vendor distribution remains unchanged.


## Native event adapter

`browser/src/expression/context.ts` adds `Context.event(nativeEvent)` using
the existing browser Event surrogate, matching framework DOM event listeners.
Wire hydration remains restricted to serialized runtime values. The packaged
application asset carries the matching reversible Context method addition;
the vendor distribution remains unchanged.


## Scoped render failure notification

RenderUnit retains ordinary error logging and notifies its live DOM owner through
`topcoat:render-error`, with the render URL in `detail.path`. Shards notify their
marker parent; pages notify document. Disposed units do not dispatch. HTTP
scheduled failures delegate to the owning unit; existing stale-run and canceled
request guards remain. The packaged asset carries three reversible substitutions
for this generic seam; upstream distribution remains unchanged. No application
refresh policy belongs to this notification.
