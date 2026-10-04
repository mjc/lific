# Topcoat frontend architecture

Lific v3 uses pinned Topcoat 0.9.0 for its browser interface. The application
pages and embedded assets live under `src/topcoat`; Axum owns the HTTP listener
and sends frontend requests to Topcoat's Tower adapter. Cargo and devenv pin
Rust 1.99.0.

The current implementation still uses browser JavaScript controllers and the
existing JSON API. This is an intermediate port: native server data access,
complete behavior parity, and visual parity remain unfinished.

The native Home checkpoint now has a shared catalog of visible projects in
the user's order, used by REST and native readers, cookie-based identity
snapshots, and bounded reads for active work and project activity. Its prepared
model and renderer cover grouped active work, a short project digest, mounted
links, safe text, project icons, and the empty state. This is not yet wired as
the production Home: local clock and recents, the complete shell, live refresh,
locale-aware ordering, and full issue editing remain open. The native
components and readers are preparation for that integration, not complete
production route families.

## Target data flow

Async Topcoat components should read authorized application data through the
shared Rust services. Initial HTML should contain that data. Browser actions
should call native Rust procedures; shards should render changed regions on
the server. Neither components nor procedures should make loopback HTTP calls.

Topcoat 0.9.0 procedures and shards use framework-managed HTTP endpoints. This
removes the frontend's dependency on the existing JSON API while retaining
browser-server communication. File uploads/downloads still transfer bytes.
Realtime updates need a prototype before choosing between the existing
WebSocket and native streaming. Preserve reconnect, account isolation, and
public-scope behavior whichever transport is used.

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
embedded with `include_str!` or `include_bytes!`. Release packages and Cargo
installs carry those sources and compile the same interface into one binary.

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
