# Topcoat core browser source support

Unmodified browser modules extracted from topcoat-core 0.9.0, crates.io package
SHA256 `7c5ec122218c570f00b4ad6d3cb102e3f8d8054877919f595ce233bc10a1dce8`, verified against Cargo.lock.
Upstream https://github.com/tokio-rs/topcoat commit
`96e8f9e0932ea883ced2859d462e9d6d3f52ea59`, path `crates/topcoat-core`,
as recorded by package .cargo_vcs_info.json. This matches the vendored runtime.

These self-contained modules restore original sibling imports for runtime source
tests. No Rust crate, Cargo patch, bundle rebuild or application model is added.
The packaged runtime already contains the upstream implementations.

Exact source SHA256:

- `browser/morph.ts`: `d85d3d0290fe4b450484599ee29c6cbf7c00ac0f26963393d749cf5b83b4978b`
- `browser/dev.ts`: `ab2a0215f500edf254dacabd7359ef6ca12da08a89e56c33c51f37bdd34e1e39`

MIT license preserved from the existing runtime at the same upstream commit;
LICENSE SHA256 `129b3095153109990b6fd776773ca80d13ad20b715530cb73531becd3ecd2cb6`.
