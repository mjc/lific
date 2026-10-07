# Lific Topcoat runtime bridge

The vendored framework source under `src/topcoat/vendor/topcoat-runtime` is from
`mjc/topcoat` commit `9c909ed4ea16b7058ae23c5e1938c83039f3e985`, rebased on
official Topcoat `main` commit `8cdc2bfd`. `UPSTREAM-SHA256SUMS` records the
fork package files before Lific's path and manifest changes. These source hashes
do not represent a crates.io package checksum.

Lific compiles the vendored runtime's canonical bridge files from
`src/topcoat/runtime/connection.rs` and `socket.rs`, both in the application and
in the standalone vendor package. `ConnectionEpoch` gives every render on one
physical socket the same identity. `ConnectionTarget` retains the handshake
context until all aborted render tasks release their references.

The bridge implements Topcoat 0.10's concurrent Run/Stop protocol. Each Run
carries its HTTP method, logical path, allowed headers and body; every output
frame is wrapped with its run ID. Stop aborts and awaits only that run. A socket
can carry up to 64 simultaneous runs by default. The application bridge limit is configured
by `SocketPolicy`. In the standalone vendored framework layer, the public
`RouterBuilderRuntimeExt::max_runs_per_connection` setting is forwarded to the
same policy before the upgrade.

The bridge preserves handshake authority headers and allows per-run overrides
only for `Content-Type`, `X-Topcoat-Runtime` and the shard identity header. An
optional route-agnostic `SocketRunPolicy`, installed by the application's
admission layer, authorizes every requested method and URI before dispatch.
Application authentication, public/private route scope and socket quotas stay
in Lific.

The bridge also keeps the local bounded output queue, send and progress
deadlines, ping handling, one-shot retirement hook, and abort-and-await cleanup.
It wraps raw NDJSON frame bytes directly instead of parsing and serializing the
frame body again. The retained connection context remains alive while any run
is still unwinding.

The standalone manifest points all framework dependencies at the same fork
revision and patches that revision's `topcoat-runtime` dependency to this local
copy. This keeps the framework context, signal and surrogate types unified.
The package's standalone lockfile is updated separately from the application
lockfile.
