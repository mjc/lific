# Native runtime sources

`connection.rs` and `socket.rs` originate in
[`topcoat-runtime` 0.9.0](https://github.com/tokio-rs/topcoat/tree/v0.9.0/crates/topcoat-runtime).
The upstream MIT license and copyright are preserved in `LICENSE`.

Original package SHA-256:
`c4c4dc39a1c6ef6f0f604ecba3949baf401a30b5b44e7b3f830c3509e6b560dd`.
The unchanged `connection.rs` SHA-256 is
`67aaeaf4f39ba790d4dad72234e998467635ef41ab92f124d31ee4b9920d59ca`.
The original upstream `src/layer/socket.rs` SHA-256 was
`960d8e516a4178d5f47a30648abb44b21aba7a1f06b0fc5dde779e617ccdc026`.

Lific's socket changes retain the upgrade request context, accept an application
retirement future, bound outbound sends, send protocol pings, enforce inbound
progress deadlines, and cancel active renders when the socket retires. The
driver imports its connection marker and wire values from its parent module.
Application authentication and quota decisions run in the admission layer.

`string.rs` adds local Unicode uppercase and scalar-vector operations to the
registry string values. The browser uses the same owned String and Vec wire
types; a typed usize supplies the vector's target width. Source and packaged
browser tests compare actual Rust expressions and serialized values, including
case expansion, astral scalars, hydration and index bounds.

`mod.rs` uses the registry runtime's signal values and protocol constant with
the registry core, router and view crates, all pinned to 0.9.0. Native connection
checks and the driver use the same local marker. HTTP reruns, signals, procedures,
shards and macros continue to use the registry runtime.

These canonical sources live here so Cargo includes them in application packages.
The [standalone source test crate](../vendor/topcoat-runtime/README.md) compiles
these same files through relative module paths in a repository checkout. Its
`UPSTREAM-SHA256SUMS` preserves checksums at the original upstream paths.
