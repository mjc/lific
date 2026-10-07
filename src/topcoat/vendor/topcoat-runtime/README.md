This directory vendors the `topcoat-runtime` crate source from the Lific Topcoat
fork at commit `9c909ed4ea16b7058ae23c5e1938c83039f3e985`, rebased on official
Topcoat `main` commit `8cdc2bfd`.

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
