# Topcoat frontend architecture

Lific v3 uses pinned Topcoat 0.9.0 for its browser interface. The application
pages and embedded assets live under `src/topcoat`; Axum owns the HTTP listener
and sends frontend requests to Topcoat's Tower adapter. Cargo and devenv pin
Rust 1.99.0.

Production Home renders authorized initial HTML through shared Rust services.
Its Rust components handle local clock and recents, active work, project
activity, project disclosure and navigation, sidebar collapse, theme selection,
and phone focus and history ownership. Native Home reads and actions use
Topcoat transport without browser REST requests or loopback HTTP.

Home captures its initial account as the session baseline. Focus checks use
the current HTTP cookie; connected shard credentials and stored token text
cannot replace that baseline. Session and theme listeners belong to their
rendered scopes and are removed when those scopes are disposed.

Each private native socket owns its authority lifetime from admission, before
its first render. Matching revocations and the existing 60-second database
revalidation retire the connection; unrelated revocations leave it open.
Rerenders do not add authority receivers or quota slots. Home's content-event
subscription follows its render scope. Sibling redirects cause one fresh Home
request using the current browser cookie.

Private native sockets share the existing REST per-user and global quotas.
Published admission checks supported routes and current publication, then claims
only a shared global slot without resolving cookies or operator identity.
Full native published application rendering remains separate work.

Home's native palette resolves qualified and bare issue references through
shared Rust services, with per-result authorization and personal project order.
Before publishing a query, a current-cookie check compares the rendered account
and admin flag. An owner change reloads Home; a missing session reaches login.
Rust owns selection, query reset, and pending Enter state. Project search remains
available. Reference navigation, pending Enter ownership and query revision
have real browser coverage. Other search domains remain unported.

Reusable native issue controls cover title, exact description text, status,
priority, sequence conflicts, draft retention, and retries. Their production
component tests do not establish complete issue-detail parity. Pure Rust
attachment-snippet helpers are tested separately and are not yet connected to
a native Markdown composer.

Other route families still use browser JavaScript controllers and the existing
JSON API. The full native port remains unfinished. Home still needs project
management and grouping, sidebar resizing, the complete command palette and
appearance preferences. Native Home live refresh, reconnect, membership loss
and idle session retirement have real browser coverage at root and both mounted
paths. Remaining route families need their own recovery and authority tests.
Paired screenshots cover selected Home geometry and theme details; they do not
establish complete visual parity.

## Target data flow

Async Topcoat components should read authorized application data through the
shared Rust services. Initial HTML should contain that data. Browser actions
should call native Rust procedures; shards should render changed regions on
the server. Neither components nor procedures should make loopback HTTP calls.

Topcoat 0.9.0 procedures and shards use framework-managed HTTP endpoints. This
removes the frontend's dependency on the existing JSON API while retaining
browser-server communication. File uploads/downloads still transfer bytes.
Home live regions already read committed shared events over native sockets.
Other route families must preserve reconnect, account isolation and public-scope
behavior through their native transport.

Reuse the same domain services and authorization as the external API. Keep
REST available for its other clients. Extract shared behavior from handlers
where needed, and preserve transactions, conflict handling, recent
authentication, and side effects. Do not duplicate those rules in the frontend.

See the pinned [procedure documentation](https://docs.rs/topcoat/0.9.0/topcoat/runtime/attr.procedure.html)
and [shard documentation](https://docs.rs/topcoat/0.9.0/topcoat/runtime/attr.shard.html).

## Build and development

```sh
devenv tasks run lific:debug-build
devenv up
devenv tasks run lific:topcoat:test
devenv --profile topcoat-e2e tasks run lific:topcoat:e2e
```

The normal application build includes Topcoat. Runtime scripts, feature
scripts, stylesheets, vendored Markdown/diagram libraries, and the mascot are
embedded with `include_str!` or `include_bytes!`. Checkout builds compile the
interface and these assets into one binary.

The native socket driver and connection helpers live in `src/topcoat/runtime`.
They compile into checkout and Cargo package builds using the same pinned
registry framework types. No application Cargo patch is required. The standalone
runtime tests use these same source files. Their MIT license and provenance are
included in the package. CI verifies the packaged application build.

Format Rust with `cargo fmt`. The macro formatter uses the pinned CLI:

```sh
devenv tasks run lific:topcoat:install-cli
devenv tasks run lific:topcoat:fmt
```

The install task uses `cargo install --locked --version 0.9.0 topcoat-cli`
and stores its executable in devenv's Cargo install root. The formatter task
uses that executable directly.

## Current API adapter

`src/topcoat/api.rs` provides a typed HTTP client for the existing `/api`
routes. Its DTOs mirror the JSON contract and remain separate from database
models. Required nullable fields stay required on the wire, and unknown fields
fail decoding. Issue DTOs retain the server's `seq`, import `source`, and
optional `waits` fields.

The client builds authenticated requests, encodes query parameters, serializes
JSON bodies, and decodes responses. Multipart requests use reqwest's form
builder to create the content-type boundary. Downloads retain response bytes
and content headers. `send_json_with_headers` retains HTTP status and headers,
including comment pagination metadata; `send_json` is the data-only convenience
method. Errors keep the status and server message, including when response-body
reading fails. Stale-write conflicts retain `current` and `update_conflict`
so callers can reconcile and retry with `expected_seq`.

Browser feature scripts use the shared session and request boundary in
`src/topcoat/session.rs`. It reads current credentials for each private request
and routes supported anonymous reads through the public API. Feature modules
own their draft, upload, paging, and cancellation state. Backend authorization
remains authoritative.

## Existing coverage

Adapter tests cover project and issue JSON compatibility, required fields,
conflict recovery data, body-read failures, pagination headers, bearer headers,
query encoding, mutation bodies with `expected_seq`, multipart boundaries,
and authenticated downloads. Route tests cover discovered pages and embedded
assets. Adjacent unit and browser suites cover the application features and
the separation between private and public requests.

See [the v3 migration notes](topcoat-migration.md) for the hosting,
session, and packaging contracts.
