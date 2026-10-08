# Native Topcoat migration

Upstream master is the behavior and visual reference. The recent native batches
use `1be6617aa21f70ee4e6ad355efc147f3e537085b`; earlier ports used
`9683d38af8e1e6f9b076439fe90d9519109b2218`. Application state, rendering,
validation, and workflows belong
in Rust. Native components use shared services and Topcoat procedures rather
than frontend REST calls.

## Current implementation

Login, Signup, Home, the shared workspace/sidebar, issue lists and
boards, issue editing, project creation, Project Overview, Project Insights, and
Project Activity have native implementations.

Settings has native Profile, Security, Connected tools, Appearance, and initial
Instance administration routes. Archive import uses a native form and streaming
upload backed by the shared Rust service. These features still need the
remaining Main interactions and parity checks recorded in their family tickets.

Instance administration supports human roster promotion, demotion, deactivation,
and reactivation. The native procedures and existing API share the same
transactions, including last-active-admin protection, recent authentication for
granting access, and credential/socket revocation after deactivation commits.
Destructive row actions require inline confirmation; own-row actions are hidden.

Pages and Plans have native private list and detail routes. Pages supports
search, tabs and filters, creation, Markdown editing, lifecycle status changes,
pinning, explicit save with sequence conflicts, and confirmed deletion. Metadata
changes share the editor's sequence and busy state and preserve unsaved
title/body drafts.
Plans supports status tabs, creation, nested steps, title and description edits,
done toggles, step issue linking and detaching, anchor assignment and clearing,
and deletion. Nested steps have independent collapse controls for viewers and
editors. Plan detail reuses the shared activity timeline for the latest 100 entries,
with six initially visible and expandable description changes. Shared plan actions
ignore retired owners and late replies.
Both reuse the workspace/sidebar and shared authorized services. This is a
partial port of those feature families, not a completed parity claim.

Issue creation and Modules have native private routes. Issue creation supports
title, description, status, priority, module assignment, labels, and inline label
creation. Modules supports lifecycle tabs, unbounded issue counts, progress,
creation, detail, scalar edits, and inline-confirmed deletion. Descriptions start
in Markdown read mode, with Main's Edit/Preview, Save, and Cancel transitions.
Lifecycle tabs persist under Main's project-identifier storage key. Both use shared
Rust services and the workspace/sidebar. Viewers receive read-only content. Cached navigation
checks the destination's rendered project permissions against current records.

The intermediate JavaScript frontend is deleted, including controllers,
frontend API clients, vendor libraries used by those controllers, generated
controller fixtures, and dormant Rust screen scaffolds. The only production
JavaScript assets are Topcoat's framework runtime and Rust-generated bindings.

## Unfinished features

Issue lists and boards share a complete authorized candidate loader, native
filter/search/sort projection, list subtabs, and project-scoped stored view state.
Query strings are ignored as they are in Main; they do not select filters.
Status tally shortcuts, filter badges, list tab counts, and visible result counts
share the same Rust projection. Board column visibility is reversible even when
all columns were hidden in saved preferences.
The candidate loader retains Main's skinny previews and does not stop at the
REST pagination limit. Saved views, bulk actions, drag/drop, clickable group and
board folds, realtime reconciliation, and remaining toolbar interactions still
need ports. Restored fold state is rendered. Main's board column predicate
currently hides every column under the Unresolved filter; this port preserves
that reference behavior.

Public readers have no intermediate fallback and return 404 until implemented.
Existing backend REST/MCP interfaces remain available.

Files and dependency graphs now have native private routes and reuse the
workspace/sidebar. Files includes pagination, filters, sorting, downloads,
where-used details, duplicate references, deletion, and pending cleanup.
Dependency graphs render nodes and relations and expose authorized relation
actions. Shared services enforce current permissions for downloads, deletion,
and both relation endpoints. Graph pan, centered zoom, Fit, initial fitting,
reduced motion, and account/project-scoped state are implemented. Drag/connect
gestures, remaining refresh behavior, and hover/touch integration are unfinished.
Files still needs complete interaction and visual parity checks.

The shared Rust issue preview provides hover content and a touch panel with
authorized edits. Loading/error recovery, close transitions, global undo toasts,
and full gesture parity remain unfinished. It is not yet wired into graph nodes.

Pages still needs folder management, other metadata editing, autosave, comments,
attachments, and realtime recovery. Plans still needs the full metadata/editor
workflow and realtime recovery. Keep the family
tickets open until their remaining main assertions and visual parity are met.

Issue creation still needs the attachment composer and Main's picker and input
interactions. Modules still needs the shared icon picker and
Markdown editor, realtime updates, and remaining mobile,
keyboard, error, and visual parity. These family tickets remain open.

Main has no step reordering controls. The backend's reordering operations do
not establish a missing frontend interaction.

Each feature must still match main's behavior, text, visual layout, permissions,
keyboard/touch interactions, mounted URLs, conflicts, and realtime recovery.
Use main's assertions and screenshots; the removed controllers are not a
reference. Shared contracts must follow the live native components.

## Tests

The original 48 frontend source mappings and assertions remain. Adapters that
need native subjects report an explicit native-port coverage gap. A preserved
case name does not prove assertion equivalence or passing functionality.
Controller-only generated tests are removed. Native tests cover actual services,
rendered controls, auth/socket boundaries, CSS/fonts/images, and retired assets.
Unported feature failures and missing test adapters remain separate in the
frontend failure ledger.

Pages and Plans tests use real database records, shared services, and the
authenticated production router. They cover list pagination, bounded page
previews, search candidates, initial hydration, mounted URLs, hidden records,
revoked membership, mutation permissions, conflicts, and audit attribution.
No browser is run for this batch.

Issue creation and Modules tests cover fresh permissions, account changes,
module ownership, complete scalar payloads, Web audit attribution, post-commit
events, tombstones, and counts beyond 500 issues. Production-router tests cover
read-only and editable initial hydration, hidden resources, revoked membership,
query defaults, mounted navigation, and permission changes during cached
navigation. Node executes the emitted module-save handlers with the packaged
runtime to check mounted procedure and navigation URLs without a browser.

Collection tests cover composed filters, fuzzy search ranking and limits,
stable sorting, tabs, grouping, swimlanes, candidate sets beyond 500 records,
initial rendering, ignored queries, and revoked authorization. The packaged
runtime executes the emitted filter, search, sort, and visibility controls;
their signals also drive authenticated render requests. Module tests cover
stored tabs and inline deletion success, cancellation, failure, and disposal.
Plan regressions cover independent folds, retained descendant fold state,
and collapse attempts during pending mutations.

Plan activity integration tests cover the 100-entry limit, newest-first ordering,
scope isolation, six initially visible entries, emitted expansion handlers and
revoked route access. Module description tests exercise initial read and empty
states, Viewer access, emitted Edit/Cancel/Preview/Save handlers, canonical content
after failure, and a real database write through the emitted procedure payload.
