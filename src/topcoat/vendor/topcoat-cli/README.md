# Topcoat CLI browser test support

Unmodified published topcoat-cli 0.9.0 browser modules, extracted from cached
crates.io package SHA256 `2b9795689489f967cab9068b5526342c8dc993a1f4efbea434c184b711fd7db0`, checked against the
local crates.io registry index 0.9.0 checksum. CLI is not a Cargo.lock dependency;
the repository devenv installs the exact version with cargo install --locked.
Upstream https://github.com/tokio-rs/topcoat commit
`96e8f9e0932ea883ced2859d462e9d6d3f52ea59`, path `crates/topcoat-cli`,
verified from the archive .cargo_vcs_info.json.

Runtime dev tests use PageRefresh through the original sibling import. refresh.ts
imports document.ts and restored core/dev; document.ts imports restored core/morph.
No additional transitive modules, Rust dependencies or runtime bundles are added.

Exact source SHA256:

- `browser/src/refresh.ts`: `9abfc1ed71bc3e9cb8881f7b4a4ed7f3e9ba5a5bc34968a839b3e8e26015a4a2`
- `browser/src/document.ts`: `265a5f29a478d295f2fed5b7371126b3836b46a34295ca076e6175bc86c6da8b`

MIT license preserved from existing runtime at the same commit; LICENSE SHA256
`129b3095153109990b6fd776773ca80d13ad20b715530cb73531becd3ecd2cb6`.
