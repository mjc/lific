# Changelog

## v1.6.0 (2026-06-15)

Lific gets a planning layer. Plans turn a goal into an ordered, arbitrarily-nestable tree of steps that persists across sessions and context compaction — the thing that separates an issue tracker from a project planner. Steps can mirror issues, so closing an issue checks its step and completing a step closes its issue, all recorded in the audit log.

### Plans

- **Persisted, nestable step trees.** A plan is a first-class, project-scoped tree of steps (steps containing steps, any depth) that survives across agent sessions and compaction. Issues stay flat and lateral; the hierarchy lives on the plan.
- **Steps mirror issues, both ways.** Link a step to an issue and the two stay in sync: closing the issue checks the step (anywhere it appears), and marking a step done closes its issue. Reopening an issue reopens its steps in active plans, stamped with the reason. Closing a plan's anchor issue auto-archives the plan. Done flows down from issues, never silently up from plans.
- **Authored in one call.** Four MCP tools: `create_plan` builds a full nested tree at once, `get_plan` rehydrates it for the next session, and `edit_plan_step` / `update_plan_step` handle surgical edits, done toggles, issue links, and structure changes — with every side effect reported back in the result.
- **First-class in the web UI.** A Plans tab alongside Issues, Board, Modules, and Pages: a list grouped by status and a detail view with a real nested tree — done toggles, per-step markdown descriptions, issue chips with provenance, an anchor issue, a progress bar, and an activity timeline. Built on the same shell as the issue and page views.
- **Fully audited.** Every plan and step mutation lands in the audit log with actor attribution, including the issue-driven cascades (recorded as system-driven via the triggering issue). A new `/api/plans/{id}/activity` surface and plan support across `list_resources` and `delete`.
- **REST + CLI.** Full `/api/plans` CRUD plus step operations, identifiers as `PROJ-PLAN-n`.

### Issue list

- **Accurate per-status tallies in the topbar.** The count was previously `filteredIssues.length` over a fetch capped at 200, so it silently undercounted once a project grew past that. A new `count_issues_by_status` query (a single indexed `GROUP BY`) and `GET /api/projects/{id}/issue-counts` endpoint return true per-status counts and a real total.
- **Click a status count to toggle that filter**, with narrowed views rendering "shown of total" so the number is always honest.
- **List fetch limit raised 200 → 1000** so rows don't truncate as early.

## v1.5.0 (2026-06-10)

Lific learns to remember and to listen. Every change is now recorded in an audit log — who did it, what changed, and whether it came through the web UI, an agent over MCP, the API, or the CLI — with activity surfaces across the app to read that history. A command palette puts every issue, page, project, and action one keystroke away. The issue list gains multi-select with bulk editing, connected tools get much richer query controls, and a sweep of UI fixes lands across every view.

### Audit log and activity

- **Every mutation is recorded**: issue, page, project, module, label, folder, and comment changes land in an append-only audit log with per-field old → new values. Edits to titles, descriptions, statuses, priorities, modules, labels, relations, and more are captured individually — no opaque blobs.
- **Full actor attribution**: each entry records who acted and through which door — a person in the web UI, an agent over MCP (shown as its bot identity, e.g. `opencode-blake · agent · via mcp`), a direct API call, or the CLI. Trustworthy answers to "did the agent do this, or did I?"
- **Capture is at the database layer**, so every write path is covered uniformly — including future ones. History survives entity deletion (deleted issues keep their identifier in the log), module/folder/lead changes record names rather than ids, and rolled-back transactions are never recorded.
- **Project Activity page**: a new "Activity" view in each project's sidebar shows everything that happened, newest first, grouped by day. Entries link to their entities, expand to show exact timestamps (local and UTC), full old → new values, and the actor's standing in the project ("412 actions · 2nd most active · last seen 3m ago"). An actor rail ranks everyone who has touched the project — humans and agents — by action count, each a one-click feed filter. The feed updates live.
- **Activity timelines on issue and page detail**: a quiet history between the description and comments — status and priority changes with their icons, expandable description-diff blocks, label and relation events, agent badges, and "via web/mcp/api/cli" attribution. Updates immediately after your own edits.
- **For integrations**: a new `get_activity` tool answers "what changed while I was gone" for any issue, page, or whole project, and the REST API gains `/activity` endpoints for issues, pages, and projects plus a per-project actor rollup.

### Command palette

- **`Cmd+K` or `Ctrl+P` from anywhere** opens a jump-to-anything palette covering projects, issues, pages, modules, and folders.
- **It understands identifiers**: `OMN156`, `omn 156`, and `OMN-156` all resolve to issue OMN-156; `lif doc 3` finds the page; a bare `156` is probed across every project and lists all hits.
- Free text searches issues and pages full-text, merged with fuzzy matches over projects, modules, and folders. The best-matching group leads the list, typing a project's name takes you to it, and an empty query doubles as a project switcher.
- **Context actions**: on an issue, the palette offers Set status, Set priority, Set module, Add or remove label (with current values shown), Rename, Edit description, and Add comment — submenus are filterable, rename turns the palette into a prefilled prompt, and every action lands in the audit log like any other edit. Pages get their lifecycle status and labels. Creating a project is available from every view.

### Issue list: multi-select and bulk editing

- Select with `x`, extend with `shift+↑/↓` (or `shift+j/k`), shift-click for ranges, ctrl/cmd-click to toggle — then apply status, priority, module, or a label to everything at once from a floating action bar, or delete behind a confirm. Triage that used to be N round-trips through the detail page is now one pass.
- Selection is keyboard-cheatsheet documented, pauses auto-refresh while active, and survives background updates.
- The board's per-column "+" now creates the issue in that column's status instead of silently defaulting to backlog.

### Integrations

- `search` supports filtering by result type (issue or page), relevance or most-recent sorting, and offset paging with has-more hints.
- `list_issues` supports created/updated date windows (`created_since`, `updated_until`, …) and explicit ordering by sort order, sequence, created, or updated — ascending or descending.
- Page listings gain the same ordering controls plus the status filter; page lines and `get_page` now include status, folder, and timestamps.
- `list_comments` can filter by author and sort in either direction.
- All ordering values are strictly whitelisted — invalid values error instead of being interpolated.

### Web fixes and polish

- Issue status icons are now one shared vocabulary everywhere — the new-issue form's mismatched colored dots are gone, and module pages use the same glyphs as the rest of the app.
- The high-priority orange and destructive-action colors are theme-aware tokens: "high" reads correctly in both modes, and red Delete buttons are no longer unreadable in dark mode.
- An issue's status now shows in the detail-page breadcrumb.
- Clicking a title to rename it shows the intended accent underline again, and priority icons in issue rows are properly sized.
- Pages list: the count matches what's shown when archived pages are hidden, the status pill only appears for non-default stages (Draft/Complete/Archived) instead of on every row, and the updated date is always visible — without jittering the status pill's position.
- Folders can no longer be dragged into each other — the move looked successful but was never persisted. Page drag-and-drop is unchanged.
- The breadcrumb says "Board" on the board view, board column visibility pills show their counts correctly, and shift-click range selection no longer sweeps text selection across rows.
- Signing in goes straight to Settings without a redirect flash, and ~450 lines of dead pre-1.4 UI code are gone.

### Upgrading

- The database upgrades itself automatically on first launch (one new migration). Upgrading from any 1.x is safe and needs no manual steps. Audit history begins at the moment of upgrade — earlier changes were not recorded and cannot be backfilled.

## v1.4.1 (2026-06-09)

A maintenance release: a sweep of correctness and security fixes across the database, auth, and MCP layers, plus server and web improvements that landed after v1.4.0.

### Fixes

- Creating an issue is now atomic — a failed label attach can no longer leave a half-created issue behind.
- Rotating an API key keeps its user binding, so rotated bot/tool keys no longer lose their comment attribution.
- Empty or whitespace-only search queries return no results instead of a database error.
- Project identifiers are validated on create and update: uppercase letters and digits, at most 5 characters, starting with a letter. Hyphenated, lowercase, or empty identifiers (which silently broke issue lookups) and the reserved word `DOC` are rejected.
- An issue can no longer be linked to itself — a self-"blocks" previously made it permanently unworkable.
- Board columns follow workflow order (backlog → todo → active → done → cancelled) and priority severity, instead of alphabetical order.
- Auto-refresh no longer stacks duplicate fetches when navigating between views.
- OAuth protected-resource metadata advertises the `/mcp`-qualified resource so claude.ai web accepts issued tokens.

### Server and web

- Responses are gzip/brotli compressed and content-hashed assets are cached immutably, dramatically cutting first-load time on slow links.
- Issue list, board, and page views auto-refresh to reflect changes without a manual reload.
- Optional authless MCP endpoint at `/mcp/<token>` to work around claude.ai web's broken OAuth connector flow.
- Priority icons are now consistent across the UI.
- The root URL lands on Settings instead of the first project's issue list.

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

- New `edit_issue` and `edit_page` tools let agents make targeted find-and-replace edits to an issue description or page body, instead of resending the whole field.
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
