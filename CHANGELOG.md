# Changelog

## v1.4.0 (2026-05-28)

The biggest release yet. Pages become first-class documents with comments, labels, lifecycle status, and search. Issues gain fuzzy search and activity-aware sorting. Modules get a real management UI and icons. The markdown renderer learns Mermaid diagrams and code-copy buttons, the commenting experience is rebuilt, and login and OAuth security are meaningfully hardened. (This is the first GitHub release since v1.1.3; the 1.2.x and 1.3.x line shipped on crates.io only.)

### Pages

- Threaded comments, the same as issues.
- Labels, with the same tagging and filtering as issues.
- A lifecycle status (Draft, Active, Complete, or Archived), shown and filterable in the page list and available everywhere pages are: web, API, CLI, and connected tools.
- Full-text search across title and content, plus instant filtering in the page list.

### Issues

- Fuzzy full-text search across title, identifier, and description.
- Sort by most recent activity. Adding a comment or changing labels now counts as activity, not just editing the issue itself.

### Modules

- A dedicated management UI: list, detail, and sidebar navigation.
- Icons, picked the same way as project icons (a built-in glyph or any emoji).

### Markdown and editing

- Mermaid diagrams render from fenced `mermaid` code blocks, anywhere markdown appears.
- A one-click copy button on code blocks.
- An explicit Edit/Preview toggle for page and issue bodies, replacing click-to-edit, so selecting and copying text no longer drops you into the editor. Press `E` to edit.
- Quote-to-comment: highlight text in a page or issue and quote it directly into a comment.

### Comments

- The comment thread was rebuilt from the ground up.

### Integrations

- New tools for agents to make targeted find-and-replace edits to issue descriptions and page bodies, instead of resending the whole field.
- Pages are now fully accessible to connected tools, including their comments, status, and labels, and module icons are exposed too.
- Adding a comment returns a leaner response (an id and metadata) instead of echoing the whole comment back.

### Throughout

- Issue, page, and module detail pages now share one consistent layout.
- A unified top bar across the app, a refreshed New Issue panel, the app version shown in the sidebar and on the sign-in screen, and the logo now links to the project repository.
- Removed a "display options" dropdown that never did anything. Grouping and density controls are still planned.

### Security

- Login rate limiting now applies per source IP in addition to per username, closing a lockout vector where someone could lock you out just by guessing your username. A counting bug that effectively halved the limit was also fixed.
- OAuth access tokens are now tied to the user who approved them, so connected tools act as that user rather than an anonymous identity. Existing tokens keep working.

### Fixes

- Projects with no assigned lead can be edited again, and a project's lead or icon can be cleared.
- Fixed a crash in issue search.
- The page tree now fills the available width.

### Upgrading

- The database upgrades itself automatically on first launch. Upgrading from any 1.x is safe and needs no manual steps.

## v1.3.1 (2026-05-17)

Bug-fix release (crates.io).

- Relations between issues in different projects now show the correct identifier.
- Issue list and board view state is preserved when navigating into an issue and back.
- Page content moved to double-click-to-edit (later replaced by the Edit/Preview toggle in v1.4.0).

## v1.3.0 (2026-05-14)

Major web UI release (crates.io).

- A redesigned interface with a kanban board view and drag-and-drop status changes.
- Browser-based integrations can now connect (CORS).

## v1.2.1 (2026-05-03)

Bug-fix release (crates.io).

- Comments added through local/stdio integrations are attributed to the first admin user.

## v1.2.0 (2026-05-02)

Feature release (crates.io).

### Features

- Full command-line CRUD for issues, projects, pages, and resources.
- Markdown export for issues, pages, and projects.
- Pagination for integration list operations.

### Security Fixes

- Hardened OAuth client registration with rate limiting and redirect-URI validation.

### Bug Fixes

- Compatibility fixes for various integration clients and reverse proxies.

### CI

- Dropped Windows build targets from the release pipeline.

## v1.1.3 (2026-04-06)

Security hardening release closing the remaining vulnerabilities identified in the auth audit.

### Security Fixes

- **CSRF on OAuth authorize form**: The OAuth approval form had no CSRF protection. An attacker could auto-submit the form from a malicious page, tricking a logged-in user into granting a 30-day access token. Added HMAC-SHA256 CSRF tokens with 10-minute expiry.
- **Signup CPU exhaustion**: The signup endpoint had no rate limiting, allowing attackers to burn CPU by spamming Argon2 password hashing. Added rate limiting keyed by email.
- **CORS allows any origin**: CORS was hardcoded to `Any`. Added a `server.cors_origins` config option. Falls back to `Any` for development if unset.
- **Session tokens stored plaintext**: Session tokens were stored as-is in the database. A database leak (backup, disk access) exposed all active sessions. Now stored as SHA-256 hashes.
- **OAuth revocation unauthenticated**: Anyone could revoke any OAuth token without authentication. Now requires a valid Bearer token.
- **Username enumeration via timing**: Login for non-existent users returned faster than wrong-password logins (no Argon2 computation). Added dummy Argon2 verification to normalize timing.

### CI

- Unified auto-tag and release into a single workflow to fix cross-workflow token permission issues.

### Upgrade Notes

- **Existing sessions are invalidated**: Sessions created before this version used plaintext storage and will no longer validate against the new SHA-256 lookup. Users will need to log in again.
- New config option: `server.cors_origins` (array of allowed origins). If unset, CORS allows all origins (previous behavior). Set this in production.

## v1.1.2 (2026-04-06)

Security and correctness fixes for auth endpoints, cookies, and server hardening.

### Security Fixes

- **Comment auth bypass**: `add_comment` silently fell back to the first admin user when no auth context was present. Now requires authentication and returns an error.
- **OAuth client_id not required**: Token exchange accepted requests without `client_id`, violating OAuth 2.1 for public clients. Now required.
- **Argon2 CPU DoS via password length**: No max password length was enforced. A multi-megabyte password would burn CPU in Argon2. Added a 1024-character max on both signup and login.
- **Session cookie missing security flags**: Session cookies lacked HttpOnly, Secure, and SameSite attributes. Added `HttpOnly; Secure; SameSite=Lax` to login, signup, and logout cookie handling.
- **World-readable backups**: Backup files were created with default permissions (0644). Now set to 0600 (owner-only) on Unix.
- **No request body size limit**: No limit on request body size allowed memory exhaustion via large POSTs. Added a 2MB default limit.

## v1.1.1 (2026-04-06)

Stability and data integrity fixes.

### Security Fixes

- **SQL injection via table name**: `get_resource_project_id` interpolated the table name directly into SQL. Added whitelist validation for allowed table names.

### Bug Fixes

- **Mutex poison crash**: The rate limiter panicked on a poisoned Mutex, crashing the process. Now recovers gracefully.
- **OAuth writes silently discarded**: Five database write operations in OAuth discarded their errors. Now propagated with proper error responses.
- **Non-atomic multi-statement writes**: Update operations for issues, projects, modules, labels, and pages ran multiple SQL statements without transactions. A failure mid-way left partial state. Wrapped in SQLite savepoints.
- **Migrations not atomic**: Each migration's SQL and tracking insert ran without a transaction. Wrapped in savepoints so partial failures roll back.
- **Rate limiter memory leak**: The rate limiter's map never evicted expired keys, growing without bound. Added a periodic sweep when the key count exceeds a threshold.

### CI

- Fixed the auto-tag workflow (missing git identity for annotated tags).
- Fixed crates.io publish (verification build failed without `web/dist/`).

## v1.1.0 (2026-04-06)

Security release closing 6 critical authentication and authorization vulnerabilities.

### Security Fixes

- **Privilege escalation via missing auth check**: `require_admin` and `require_project_lead` returned success when no user was associated with the request (OAuth tokens, legacy API keys). Any unauthenticated but authorized request had full admin privileges. Now default-deny.
- **OAuth PKCE bypass**: The `plain` PKCE method was accepted despite OAuth 2.1 requiring S256 only. Sending an empty challenge and verifier with `method=plain` fully bypassed PKCE. Removed `plain` and reject empty values.
- **OAuth redirect_uri not validated at token exchange**: The `redirect_uri` from the token request was never compared against the one stored with the authorization code. An attacker who intercepted an auth code could exchange it from any URI. Now validated per OAuth 2.1.
- **OAuth access tokens stored plaintext**: OAuth tokens were stored and looked up by raw value. A database leak exposed all active tokens. Now stored as SHA-256 hashes, with the raw token returned only once at issuance.
- **MCP identity confusion under concurrency**: A global mutex stored the current MCP user. Concurrent requests could overwrite each other's identity, and a panic would poison the mutex permanently. Replaced with serialized request handling and poison recovery.
- **Database errors leaked to clients**: Raw SQLite error messages (table names, column names, constraint details, file paths) were returned directly in API responses. Now returns a generic error and logs details server-side.

### Upgrade Notes

- **OAuth tokens are invalidated**: Existing plaintext OAuth tokens will no longer validate since the lookup now expects SHA-256 hashes. Clients will need to re-authorize. This is intentional.
- No database migration required. No config changes.
