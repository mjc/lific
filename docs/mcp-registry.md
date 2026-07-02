# Publishing Lific to the official MCP Registry

Lific is listed on the official [MCP Registry](https://registry.modelcontextprotocol.io)
(`registry.modelcontextprotocol.io`, API v0.1, currently in **preview**). The
registry hosts **metadata only** — the actual artifact lives on crates.io. This
doc is the exact runbook for (re)publishing.

Tracked by LIF-253.

## What's already in the repo

- **`server.json`** (repo root) — the server manifest the registry ingests.
  Schema version `2025-12-11`.
- **README ownership marker** — a visible line in `README.md`:
  `mcp-name: io.github.VoidNullable/lific`. This is how the registry proves we
  own the `lific` crate (see "Ownership verification" below). **Do not delete
  it and do not move it into an HTML comment** — see the crates.io gotcha below.

## Key facts (verified against the registry docs, July 2026)

- **`registryType: "cargo"` is officially supported.** Rust crates on crates.io
  are a first-class package type. crates.io is the only allowed
  `registryBaseUrl` for cargo.
  Source: <https://modelcontextprotocol.io/registry/package-types> (Cargo section)
  and the base-URL allowlist in
  <https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/official-registry-requirements.md>.
- **No `runtimeHint` for cargo.** `cargo install lific` puts the binary on PATH
  at `~/.cargo/bin`; clients invoke it by name. There is no `npx`/`uvx`/`dnx`
  equivalent for cargo, so the package entry omits `runtimeHint` by design.
- **Run command is `lific mcp`.** Expressed in `server.json` as a single
  positional `packageArguments` entry (`{"type":"positional","value":"mcp"}`),
  so a client resolves the launch as `lific mcp` over stdio.

## Namespace and casing — READ THIS

The server name is **`io.github.VoidNullable/lific`** — matching the GitHub login
`VoidNullable` **with its exact capitalization**.

This is deliberate and it contradicts the naive "namespaces are lowercase"
assumption. As of July 2026 the registry's GitHub-namespace permission checks
are **case-sensitive** and the normalization fix is still open
([registry#689](https://github.com/modelcontextprotocol/registry/issues/689)).
The GitHub OIDC/device token authorizes `io.github.<login>/*` with the login's
literal case, so a lowercase `io.github.voidnullable/lific` would fail namespace
authorization until #689 lands. A real-world instance of this bug and its fix:
<https://github.com/GavinLucas/docker-mcp/pull/74>.

Three strings MUST stay identical, byte-for-byte, including case:

1. `name` in `server.json`
2. the `mcp-name:` marker in `README.md`
3. whatever namespace your `mcp-publisher login github` token grants

If #689 is resolved to force-lowercase before we publish, switch all three to
`io.github.voidnullable/lific` together. Until then, keep the capital `V` and `N`.

## Ownership verification (cargo-specific gotcha)

The registry verifies we own the `lific` crate by looking for an
`mcp-name: <server-name>` string in the crate README **as rendered on
crates.io**.

**crates.io strips HTML comments during markdown→HTML rendering.** The
`<!-- mcp-name: ... -->` hidden-comment trick that works for PyPI and NuGet
**does NOT work for cargo** — the token never appears in the HTML the validator
inspects. The marker therefore lives in `README.md` as **visible markdown text**
(a bullet under the "MCP Registry" heading). Keep it visible.
Source: <https://modelcontextprotocol.io/registry/package-types> (Cargo →
Ownership Verification).

Because crates.io serves the README that ships inside the published crate, the
marker must be present in the crate version you publish. `Cargo.toml` already
`include`s `README.md`, so `cargo publish` carries it. Publish the crate version
BEFORE (or together with) publishing to the MCP Registry, so the marker is live
when the registry fetches it.

## Runbook

### One-time: install the publisher CLI

```bash
# macOS/Linux prebuilt binary
curl -L "https://github.com/modelcontextprotocol/registry/releases/latest/download/mcp-publisher_$(uname -s | tr '[:upper:]' '[:lower:]')_$(uname -m | sed 's/x86_64/amd64/;s/aarch64/arm64/').tar.gz" \
  | tar xz mcp-publisher && sudo mv mcp-publisher /usr/local/bin/

# or, if you have Homebrew
brew install mcp-publisher

mcp-publisher --help   # sanity check
```

### Every publish

Preconditions: the crate version you're announcing is already live on crates.io
(the release pipeline handles this — see below), and its README contains the
`mcp-name` marker.

```bash
# from the repo root, where server.json lives
mcp-publisher login github      # device-code OAuth; grants io.github.VoidNullable/*
mcp-publisher publish           # validates server.json + verifies crate ownership
```

`mcp-publisher publish` will:

1. Validate `server.json` against the schema.
2. Publish the metadata to `registry.modelcontextprotocol.io`.
3. Server-side: verify we own the `lific` crate (the README marker) and that the
   token's namespace matches `name`.

Verify it landed:

```bash
curl "https://registry.modelcontextprotocol.io/v0.1/servers?search=io.github.VoidNullable/lific"
```

### Bumping the version on each release

The registry is versioned. On every Lific release you must re-publish so the
registry's `version` tracks the shipped crate.

1. Bump `version` in **`Cargo.toml`** (the release source of truth).
2. Update **both** `version` fields in **`server.json`** to the same value:
   the top-level `version` and the `packages[0].version`. They should always
   equal `Cargo.toml`'s version.
3. Cut the release as normal (see `AGENTS.md` → "Releasing a new version":
   commit, `git push` to magi, then `git tag vX.Y.Z && git push origin vX.Y.Z`).
   That fires `release.yml`, which builds binaries and runs
   `cargo publish` to crates.io.
4. After crates.io shows the new version, run `mcp-publisher publish` from the
   repo root (per "Every publish" above).

> Tip: keep `Cargo.toml`, `server.json` (×2), and the release tag in lockstep.
> A mismatch between the registry `version` and the crates.io version is the
> most common drift.

## Wiring into CI later (not done here)

The manual `mcp-publisher publish` step could be automated as a final job in
`.github/workflows/release.yml`, running after the existing `publish`
(crates.io) job so the crate — and its ownership marker — is live before the
registry fetches it. The registry supports GitHub Actions OIDC auth
(`mcp-publisher login github-oidc`), which avoids storing a long-lived token.

**This has intentionally NOT been added** — CI changes need human review, and
the release workflow's tag/mirror choreography (see the big comment atop
`release.yml` and `AGENTS.md`) is load-bearing. Sketch of the job to add later,
for reference only:

```yaml
  publish-registry:
    needs: [verify, publish]   # after crates.io publish
    runs-on: ubuntu-latest
    permissions:
      id-token: write          # for github-oidc
      contents: read
    steps:
      - uses: actions/checkout@v4
      - name: Install mcp-publisher
        run: |
          curl -L "https://github.com/modelcontextprotocol/registry/releases/latest/download/mcp-publisher_linux_amd64.tar.gz" \
            | tar xz mcp-publisher && sudo mv mcp-publisher /usr/local/bin/
      - name: Publish to MCP Registry
        run: |
          mcp-publisher login github-oidc
          mcp-publisher publish
```

Before adding that, confirm the OIDC token's namespace casing matches
`io.github.VoidNullable` (see the casing section) and re-verify the schema
version in `server.json` is still current.

## References

- Package types (incl. Cargo): <https://modelcontextprotocol.io/registry/package-types>
- Quickstart / publishing flow: <https://modelcontextprotocol.io/registry/quickstart>
- Authentication & namespaces: <https://modelcontextprotocol.io/registry/authentication>
- Official registry requirements: <https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/official-registry-requirements.md>
- server.json schema (2025-12-11): <https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json>
- Namespace case-sensitivity bug: <https://github.com/modelcontextprotocol/registry/issues/689>
