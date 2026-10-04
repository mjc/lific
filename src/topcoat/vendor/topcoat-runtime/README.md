This crate is part of [`topcoat`](https://github.com/tokio-rs/topcoat).


## Lific vendored source

This directory contains the pinned `topcoat-runtime` 0.9.0 crates.io source.
It is MIT licensed; the upstream copyright and license are preserved in
`LICENSE`. The normalized packaged `Cargo.toml` is used so its dependencies
continue to resolve to their pinned registry packages. No external publication
or separate dependency repository is required.

- Upstream repository: https://github.com/tokio-rs/topcoat/tree/v0.9.0/crates/topcoat-runtime
- Package SHA-256: `c4c4dc39a1c6ef6f0f604ecba3949baf401a30b5b44e7b3f830c3509e6b560dd`
- Unchanged upstream `src/layer/socket.rs` SHA-256: `960d8e516a4178d5f47a30648abb44b21aba7a1f06b0fc5dde779e617ccdc026`
- Unchanged `browser/dist/index.js` SHA-256: `980dd1be1962006b98b8c1646b0e6a4f86a78ec721e2739f59ddf4c4c1c5c8b4`

`src/layer/socket.rs::accept` has the sole Rust patch: clone the incoming `Cx`,
move it into the upgrade callback, and explicitly drop it after the raw socket
`run` future completes. This retains request-scoped resources through idle
connections before their first render. A failed upgrade drops the callback
and its captured context. Application authentication and quota decisions stay
in Lific's Rust admission layer. Synthetic render requests retain the upstream
header/remote-address behavior; their extensions are not the lifetime owner.

Exact source change:

```diff
     let target = Arc::new(ConnectionTarget::from_handshake(cx));
+    let connection_context = cx.clone();
     upgrade
         .protocols([RUNTIME_PROTOCOL])
-        .on_upgrade(move |socket| run(target, socket))
+        .on_upgrade(move |socket| async move {
+            run(target, socket).await;
+            drop(connection_context);
+        })
```

`UPSTREAM-SHA256SUMS` records every copied package file before modification.
Only this README and the one Rust file differ; LICENSE and this checksum list
are additions. Reversing the declared socket change reproduces its upstream
hash. The browser source and distribution are unchanged. Lific's existing
`src/topcoat/assets/runtime.js` transport patches remain separate and unchanged.
