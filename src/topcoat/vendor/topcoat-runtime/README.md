This directory vendors the `topcoat-runtime` crate source from the Lific Topcoat
fork at commit `e2444306b237c9b771ec33f76f9ae891392a0554`, based on official
Topcoat `main` commit `341f3ff2`. The packaged `assets/runtime.js` retains
Lific's existing transport patches on its prior minified baseline while using
the JSON hydration marker parser from this fork revision.

The original fork manifest is preserved in `Cargo.toml.orig`. `Cargo.toml` is a
normalized standalone manifest: it pins the facade and framework dependencies
to the same fork revision and patches that revision's `topcoat-runtime`
dependency to this directory. The standalone `Cargo.lock` is maintained
separately from Lific's application lockfile.

Lific's canonical `connection.rs` and `socket.rs` are compiled here through
relative paths, so application and standalone tests exercise the same bridge.
See `../../runtime/README.md` for the Lific bridge protocol and local behavior.
`UPSTREAM-SHA256SUMS` records the original fork package files before these local
manifest/path changes; it is not a crates.io checksum.

The upstream MIT license and copyright are preserved in `LICENSE`.
