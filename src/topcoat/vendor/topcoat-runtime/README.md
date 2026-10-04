This crate is part of [`topcoat`](https://github.com/tokio-rs/topcoat).


## Lific vendored source

This directory contains the pinned `topcoat-runtime` 0.9.0 crates.io source.
It is MIT licensed; the upstream copyright and license are preserved in
`LICENSE`. The normalized packaged `Cargo.toml` is used so its dependencies
continue to resolve to their pinned registry packages. Workspace builds select
this runtime through the root Cargo patch. Cargo registry packaging does not
preserve that patch; registry publication remains gated on a coherent upstream
release or separately reviewed dependency solution.

- Upstream repository: https://github.com/tokio-rs/topcoat/tree/v0.9.0/crates/topcoat-runtime
- Package SHA-256: `c4c4dc39a1c6ef6f0f604ecba3949baf401a30b5b44e7b3f830c3509e6b560dd`
- Unchanged upstream `src/layer/socket.rs` SHA-256: `960d8e516a4178d5f47a30648abb44b21aba7a1f06b0fc5dde779e617ccdc026`
- Unchanged `browser/dist/index.js` SHA-256: `980dd1be1962006b98b8c1646b0e6a4f86a78ec721e2739f59ddf4c4c1c5c8b4`

`src/layer/socket.rs::accept` retains the incoming `Cx`: clone it,
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
The additional Rust patch introduces `SocketPolicy` in `src/layer/socket.rs`
and reexports it from `src/layer.rs`. Its checked constructor accepts nonzero
ping, progress and send intervals, with pings preceding progress expiry. The
defaults are 30 seconds, 120 seconds and 5 seconds respectively, matching
Lific's existing REST transport. `accept` reads the optional application policy
before upgrading; connections without one use these defaults.

The remaining changes to `src/layer/socket.rs` replace the upstream unbounded
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
the tests and example on the application's patched implementation. The application
root owns that same patch for application builds. Its standalone `Cargo.lock`
adds the test dependency's closure while
retaining existing package versions. This preserves the original runtime tests.
`Cargo.toml.orig` remains the unmodified upstream workspace manifest for
provenance; it is not the manifest used to build this copy.

Only this README, those two Rust files, the normalized Cargo manifest and lock
differ; LICENSE and this checksum list are additions. Reversing the declared
policy, lifecycle, context retention and manifest changes reproduces the
upstream source hashes.
Unit tests cover one-shot consumption, exact Close outcomes and retaining a
Send-only future in the Send + Sync request context. Actual application TCP
tests cover request-scoped Redirect lifetimes, idle session retirement,
periodic revalidation, stable ownership across rerenders and peer cleanup.
The browser source and distribution are unchanged. Lific's existing
`src/topcoat/assets/runtime.js` transport patches remain separate. Its document
runtime claims navigation once across sibling framework redirects.
