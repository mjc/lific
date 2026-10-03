# Contributing to Lific

Lific accepts contributions. The project is primarily a one-person effort built heavily with AI agents, under human direction and review. AI-assisted PRs aren't just welcome, they're the dominant authorship model here. Don't hide it, and don't apologize for it. The same bar applies either way: you should be able to explain your change to a reviewer, and it has to pass the test suite.

If you want to discuss an idea before writing code, open an issue. If you have a small, focused fix ready, just send the PR. Either path is fine.

## Repository

- **Source**: [github.com/VoidNullable/lific](https://github.com/VoidNullable/lific)
- **License**: Apache-2.0
- **Stack**: Rust 2024 edition (MSRV 1.99), Topcoat 0.9 frontend with embedded JavaScript and CSS
- **Docs**: [lific.dev/docs](https://lific.dev/docs)

## Building

The repository provides the development toolchain and project commands through
[devenv](https://devenv.sh/). From a fresh checkout, approve it:

```bash
devenv allow
```

Enter the shell explicitly for interactive work. From a regular shell, use the
same task commands directly through devenv:

```bash
devenv shell
# inside the devenv shell:
devenv test
```

The default environment supplies Rust and Node for the application and its
frontend checks. Bun installs dependencies for the documentation, browser-test,
and promo workspaces through frozen-lockfile tasks. Entering a shell prepares
the selected workspace; project checks run through explicit tasks.

```bash
devenv tasks run lific:debug-build
devenv --profile docs tasks run lific:docs:check
devenv --profile topcoat-e2e tasks run lific:topcoat:e2e
devenv --profile promo tasks run lific:promo:check
```

Lific v3 uses Topcoat for all browser routes. Rust embeds its pages, browser
runtime, JavaScript, CSS, and image assets from `src/topcoat` into each debug
and release executable. The same build works from an isolated package source
copy, and the resulting executable can serve its interface from any directory.

Start the development server through devenv's process manager:

```bash
devenv up
```

The Rust server handles the browser interface, REST, MCP, OAuth, and WebSocket
connections on one port. The managed backend uses an explicit development
configuration and stores its database under `.devenv/state/`. Its initial
administrator name is `Devenv`, with password `devenv-local-password`.
These credentials belong to the development instance; operator configurations
are excluded from that process. The backend restarts when its watched sources
or Cargo configuration change.

Use `cargo fmt` for Rust and the pinned Topcoat CLI for macro bodies:

```bash
devenv tasks run lific:topcoat:fmt
```

On NixOS and nix-darwin, use the repository's devenv workflow. If an existing
shell's `DEVENV_ROOT` belongs to another project, start a fresh command in this
checkout.

## Tests

```bash
devenv test
devenv tasks run lific:topcoat:test
devenv --profile topcoat-e2e tasks run lific:topcoat:e2e
```

The project graph runs Rust tests, frontend unit checks, release smoke
regressions, and environment checks. Treefmt and Clippy use the pinned Rust
toolchain. Browser tests use the E2E workspace's pinned Playwright dependency
and the profile's Chromium executable. Topcoat tests live next to their
features under `src/topcoat`; `e2e` holds their shared browser dependency.

The checks exercise these behaviors:

- Rust tests cover MCP and REST contracts, CLI behavior, first boot, imports,
  exports, authorization, migrations, issue references, retention, and server
  routing. Topcoat route tests check page composition, asset content types,
  private/public/auth chrome, and preservation of the Axum API routes.
- Frontend unit tests cover session and role changes, request scope, shared
  shell navigation, preferences, keyboard controls, live synchronization,
  issue filters and edits, attachments, pages, plans, modules, and public reads.
- Browser tests exercise focus and keyboard behavior, responsive navigation,
  session transitions, draft recovery, serialized saves, project administration,
  public Markdown and Mermaid safety, attachment capture and upload, previews,
  and anonymous media playback and seeking.
- Release smoke checks run each native artifact's version and help commands,
  start it with a temporary configuration and database, and fetch its API,
  rendered HTML, JavaScript, and CSS from a directory outside the checkout.
  Empty assets or HTML returned in place of JavaScript/CSS fail the check.
- Environment checks cover shell/task boundaries, formatter ordering,
  compiler consistency, cross-linker settings, and package source allowlists.
  Package sources exclude local databases, dependency installs, and build caches.
- `devenv test` starts the actual backend through the process manager, waits for
  readiness, and stops it afterward. Its temporary database lives under
  `DEVENV_RUNTIME`, separate from the persistent development database.

Documentation and promo checks have their own profiles:

```bash
devenv --profile docs tasks run lific:docs:check
devenv --profile promo tasks run lific:promo:check
```

Every new MCP tool and REST endpoint should ship with tests. Conventions:

- All tests use in-memory SQLite via `crate::db::open_memory()`.
- **Exception:** full-command tests of `lific init` (_`cmd_init`_) exercise the
  real filesystem and a genuine on-disk DB, because init writes a config file
  and opens the DB from a path — it cannot run against an in-memory database.
  These use a self-cleaning temp dir (`TempDir` in `init_target_tests`) and
  stay out of the repo tree.
- MCP tool tests call methods directly via `Parameters(...)` on a `LificMcp` instance.
- REST API tests use `tower::ServiceExt::oneshot` against the axum router.
- Test names describe behavior, not implementation.

## What CI runs (run it before pushing)

CI checks formatting and lints with warnings-as-errors. Reproduce the complete
project check locally with:

```bash
devenv test
```

If clippy complains, fix it. Don't `#[allow]` a lint without a comment explaining why.

The devenv shell installs the repository's generated pre-commit hooks. Treefmt
and the native Clippy hook use the pinned toolchain; no separate installation
is needed. They run automatically as part of `devenv test`:

```bash
devenv test
```

Use the native treefmt integration to apply formatting fixes across the
configured languages. Run `devenv tasks run lific:check` when you need the
project check task without the test lifecycle.

## Release builds

Pushing a version tag runs the release workflow. It compiles the Topcoat pages
and browser assets into locked `dist` artifacts for Linux x86_64 and aarch64,
macOS x86_64 and aarch64, and Windows x86_64 (MSVC). Linux targets use the
devenv-provided Zig linker. macOS targets build on macOS runners; the Windows
MSVC artifact is cross-built on Linux using the `release-windows-msvc` profile.
A Windows runner verifies its checksum and executes those exact bytes before
publication. CI also retains native Windows Clippy and all-target Rust tests.
The workflow publishes SHA-256 checksums and attaches all five binaries to the
GitHub release.

Follow the [deployment and rollback instructions](docs/topcoat-migration.md#deployment-and-rollback)
when upgrading an instance. Dependency changes must follow the
[pinned Topcoat upgrade policy](docs/topcoat-migration.md#pinned-topcoat-upgrade-policy),
including route parity and release artifact checks.

For a local Linux cross-build:

```bash
devenv --profile release-linux tasks run lific:release:aarch64-unknown-linux-gnu
```

For the Windows release cross-build on Linux:

```bash
devenv --profile release-windows-msvc tasks run lific:release:x86_64-pc-windows-msvc
```

This profile accepts the Microsoft Visual Studio SDK license for its build
inputs. The default development shell does not enable those unfree packages.
The executable is written to `target/x86_64-pc-windows-msvc/dist/lific.exe`.

On macOS, `devenv` also provides both Apple Rust targets and the Apple SDK
used by the credential and SQLite stacks. Build the current Mac
architecture normally;
on Apple Silicon, build the Intel artifact with:

```bash
devenv --profile release-darwin tasks run lific:release:aarch64-apple-darwin
devenv --profile release-darwin tasks run lific:release:x86_64-apple-darwin
```

Platform signing, notarization, installers, and update channels are later
phases of the release plan, not part of this MVP.

## Commit message style

Conventional-commits style, with a Lific issue identifier in parens where applicable:

```
type(scope): short description (LIF-NNN)

Body explains the WHY and any non-obvious WHAT. Bullet lists are fine.
```

- **Types**: `feat`, `fix`, `perf`, `test`, `refactor`, `chore`, `docs`. Pick the closest one.
- **Scope** is freeform. Common scopes: `auth`, `mcp`, `db`, `plans`, `theme`, `issues`, `oauth`, `api`.
- The issue reference (e.g. `(LIF-215)`) goes at the end of the subject line. External contributors usually won't have one; that's fine, leave it off.

Examples from the log:

```
feat(auth): single-user web auto-login (LIF-215)
perf(plans): 2x faster list_plans via page-then-aggregate
fix(theme): clear WCAG AA for --text-faint, add --warn-text token
```

A short subject with no body is fine for trivial changes. A non-trivial change should explain the why, not just describe the diff.

## Pull requests

- Open against `master`.
- Title and description should make the user-facing change obvious. Link related issues.
- Make sure CI is green before requesting review.
- AI-generated PR descriptions are fine. So are AI-generated commit messages, code, and tests.
- **Don't update `CHANGELOG.md`**; it's generated from commit history.
- No DCO, no CLA, no signoff requirement.

## Before adding something big

Lific has a deliberately fixed scope: single binary, SQLite, MCP-native, agent-first defaults. Things that are out of scope on purpose include Docker-required deployment, sprints/estimates-style project management, and multi-writer database backends. If your idea pushes against any of that, open an issue first so we can talk it through before you put in the work.

## Reporting bugs and requesting features

File an issue on GitHub. For bugs, include reproduction steps and the version (`lific --version`). For features, describe the use case (agent workflow, human workflow, or both) and any constraints you have in mind.
