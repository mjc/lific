# Topcoat v3 migration

Lific v3 makes Topcoat the production browser interface. Application routes,
shared shell controls, private project screens, account and instance settings,
and anonymous published projects live under `src/topcoat`. The normal Cargo,
Devenv, Docker, and release builds compile that interface into the executable.

See [the frontend architecture](topcoat-frontend.md) for the API adapter and
embedded runtime.

## Local commands

Use the repository's pinned environment:

```sh
devenv tasks run lific:debug-build
devenv up
devenv tasks run lific:topcoat:test
devenv --profile topcoat-e2e tasks run lific:topcoat:e2e
```

Rust 1.99.0 and Topcoat 0.9.0 are pinned. The macro formatter is available as
`devenv tasks run lific:topcoat:fmt`.

## Hosting and packaging

Axum owns REST, MCP, OAuth, WebSocket, and public API routes, along with
middleware, CORS, and compression. Requests outside those routes reach
Topcoat's `TowerService`, which discovers the frontend pages and asset routes
and enables its browser runtime. `/api/health` stays an Axum route.

When hosting below a URL prefix, configure the reverse proxy to strip that
prefix before forwarding requests, set `X-Forwarded-Prefix` to the public
prefix, and add the proxy's IP or CIDR to `server.trusted_proxies`. The server
uses that trusted prefix for rendered document URLs and same-origin redirects;
untrusted forwarded headers are ignored. The REST and WebSocket routes remain
under the same public prefix.

The runtime, JavaScript, stylesheets, vendored libraries, and mascot are
embedded from `src/topcoat` using `include_str!` and `include_bytes!`.
The crate's `src/**/*` source allowlist includes them for Cargo installs and
isolated Nix packages. Production executables serve their interface from any
working directory. Release smoke checks run artifacts outside the checkout
and fetch their rendered page, referenced JavaScript/CSS, install manifest,
and app icons.

## Deployment and rollback

Before deploying v3, keep the currently installed executable at
`lific.previous` in your deployment directory. For containers, retain the
previous image by digest or tag it `lific:previous`; keep that image available
until the upgrade has been accepted. Record the executable path or image
reference used by your service so you can restore that exact deployment.

1. Stop the previous service or container and take a `lific dump` archive
   with the previous executable. Store it as `lific-before-v3.tar.gz` outside
   the active data directory. Preserve the instance configuration too.
2. Install the v3 executable or start the v3 image using the same configuration,
   database location, and attachment volume. Check `/api/health`, sign in, and
   open representative private and public project screens before accepting
   the deployment.
3. If rollback is needed, stop v3 first. Preserve a separate dump of the v3
   state if you need to recover work performed after the upgrade.
4. Restore `lific.previous` to the executable path used by the service, or
   switch the container back to `lific:previous` or the recorded image digest.
5. If the previous executable supports the current database schema and its
   recorded migration checksums, restart it against that data. Otherwise use
   the previous executable's `restore lific-before-v3.tar.gz --force` command,
   with the original instance's `--config` and `--db` options, before restarting.
   A restore replaces the database and attachment snapshot with the pre-upgrade
   state, so work performed after that snapshot must be recovered separately.
6. Check startup, `/api/health`, sign-in, and representative project screens
   using the restored executable or image. Retain both snapshots until recovery
   is complete.

Database compatibility is determined by the migration history, rather than the
frontend implementation. V3 supports schema version 58, including the migration
that adds `audio/mp4` attachments while preserving attachment rows and IDs.
An older executable that supports only version 57 or earlier refuses that
upgraded database. Startup also verifies checksums of already applied migrations.
The frontend cutover preserves the REST and MCP contracts, but does not bypass
those schema checks or reverse migrations. Restore the pre-upgrade archive when
returning to a release that cannot use the upgraded schema; the old release
cannot restore a v3 archive with a newer schema either.

## Staging acceptance

Record the target URL and mount prefix, candidate commit, deployed executable
checksum or image digest, previous deployment, and backup location. Use a
designated test account and project whose password, publication, and content
can be changed during the pass. Deploy the candidate using the rollback
procedure above before running these checks.
Use the route inventory in `src/topcoat/acceptance/routes.browser.test.js`,
substituting the deployed test project's identifiers.

| Flow | Action | Expected result |
| --- | --- | --- |
| Routes | Open each private route family and each published issue/page route directly, including after reload. | The intended screen loads its data; assets and navigation stay under the configured mount prefix. |
| Authentication | Open a private detail anonymously, try a wrong password, then sign in. Rotate the test account's password, reload, and sign out. | Anonymous navigation reaches login; rejected credentials create no session; replacement survives reload; the replaced and signed-out tokens are rejected. |
| Realtime | Open the same issue in two authenticated browser contexts. Record its sync cursor, disconnect one context, edit from the other, then reconnect it. Inspect the mounted WebSocket connection and frames. | A new `/api/events/ws` connection upgrades successfully and sends `resume` with the project ID and cursor; the committed change renders. REST catch-up may finish before WebSocket reconnection. |
| Public sharing | Publish the test project and open its issue and page in an anonymous context. Repeat with private credentials already stored in that context. | Published content is read-only; public resource requests carry neither bearer credentials nor session cookies. |
| Exports | Use issue-list **Export selected**, page **Export Markdown**, and project settings **Export project data**. | Markdown contains persisted content; **Export project data** downloads a ZIP with a matching filename and expected project data. The separate project archive export uses `.tar.gz`. |
| Attachments | Upload a known text file, reload its issue, and download it. Upload playable audio and seek in its public preview. | Metadata and bytes persist; downloaded bytes match; public playback and byte-range seeking work. |
| Proxy and services | Use the public mounted URL for documents, REST reads, MCP initialization, WebSockets, downloads, and media. | Requests stay under the mount; service endpoints remain reachable and enforce their existing authentication rules. |
| Packaging | Fetch the rendered document's scripts/styles, install manifest, and icons from the deployed target; request a retired bundle path. | Embedded assets have their expected content types, manifest URLs preserve the mount, icons are PNGs, and retired bundles return 404. |

To prove cursor replay after reconnecting, open a fresh native WebSocket to
the same mounted endpoint using the authenticated browser's session cookie.
Send `{"type":"resume","project_id":<id>,"cursor":<saved pre-disconnect cursor>}`
and verify an `issue.updated` event for the offline edit's issue ID and
committed sequence before closing the connection. The request pattern is in
`src/topcoat/acceptance/session.browser.test.js`.

Record the UTC time, route or operation, expected and observed result, and
sanitized evidence for each row. Link the deployment's artifact identity and
these results in the parity matrix. Keep bearer tokens, cookies, passwords,
and private content out of captured evidence. A failed row leaves staging
acceptance open until its fix passes on the deployed candidate.

After staging passes, repeat the route, service, and asset checks on the
deployed production artifact. Exercise writes only in its designated test
project. Record production results separately; isolated executable tests and
staging results establish their respective coverage.

## Pinned Topcoat upgrade policy

`Cargo.toml` pins Topcoat with `version = "=0.9.0"`; the macro formatter CLI
is pinned to `--version 0.9.0` as well. Keep both exact pins and the committed
Cargo lockfile. A Topcoat upgrade is a deliberate dependency change with code
review: review upstream API/runtime changes, update the library and CLI pins
together, and regenerate the lockfile through the repository environment.

Before accepting an upgrade, run the complete Topcoat unit and browser suites
and the server route tests. Check parity across private, public, and auth
screens, legacy navigation, session isolation, writes, uploads, and live updates.
Keep Axum's REST, MCP, OAuth, WebSocket, and health routes reachable through the
assembled router.

Build the release artifacts and run their existing smoke verifiers on supported
native hosts. Check packaged-source installs and the container path as well:
rendered pages, embedded JavaScript/CSS content types, runtime events, and asset
requests must work from outside the checkout. Review the shipped runtime and
vendored asset versions and their license notices alongside the dependency
change. Keep the previous executable/image and pre-upgrade data snapshot until
these route and artifact checks pass.

## Authentication boundary

The session bridge reads `localStorage['lific_token']` at request time.
It injects bearer credentials for private API calls and rewrites supported
public reads without credentials. Role data controls presentation; the backend
remains the authorization boundary. The Rust API adapter owns origin and wire
DTOs without sharing database models with frontend code.

The shared request boundary preserves these contracts:

- Private REST calls read the current token and send it as
  `Authorization: Bearer …`. Missing tokens send no bearer header.
- Public REST calls use `credentials: 'omit'` and send no bearer header.
- WebSockets use the Axum endpoint with its same-origin session-cookie and
  origin validation.
- Account changes clear identity-specific caches, roles, and live state.
  Browser tests exercise separate users and verify that public requests carry
  neither private bearer credentials nor cookies.

## Feature and integration coverage

- The executable acceptance suite starts an isolated instance with a real
  database behind a stripping `/app` proxy. It seeds resources through REST,
  opens private and public route families in headless Chromium, and checks
  login, session replacement, logout, WebSocket reconnect and replay, MCP
  initialization, uploads, downloads, exports, and public audio seeking.
  Requests reach the assembled server without API mocks. The `e2e` profile's
  `lific:e2e` task runs this suite after building the executable.
- These isolated checks establish local integration coverage. Deployment
  acceptance still requires the same flows on the staging target and a
  smoke check of the deployed production artifact.
- Route tests exercise shared document composition, private/public/auth chrome,
  project setup and settings, issues, pages, files, plans, modules, activity,
  insights, dependency graphs, and embedded assets. They also check that the
  Topcoat fallback preserves the Axum health route.
- Shared-shell suites cover project navigation, recents, preferences, mobile
  focus behavior, command palette actions, and live synchronization.
- Feature suites cover issue lists and boards, scalar and Markdown edits,
  serialized writes, drafts and conflicts, comments, relations, waits,
  page editing, plans, modules, project administration, and account changes.
- Attachment suites cover upload progress, retry and cancellation, paste,
  image annotation and resizing, mobile capture, voice recording, and file
  previews. Public suites cover Markdown and Mermaid safety, scoped resources,
  anonymous downloads, and audio/video playback and seeking.
- Release and environment regression checks protect embedded asset delivery,
  startup in an isolated directory, source allowlists, task boundaries, and
  platform build configuration.

The browser workspace under `e2e` retains the pinned Playwright dependency.
Feature tests live beside the Topcoat modules that they exercise.

## References

- [Topcoat 0.9.0 release](https://github.com/tokio-rs/topcoat/releases/tag/v0.9.0)
- [Topcoat Tower adapter](https://docs.rs/topcoat/0.9.0/topcoat/router/tower/index.html)
- [Topcoat asset guide](https://github.com/tokio-rs/topcoat/blob/v0.9.0/crates/topcoat/docs/asset.md)
