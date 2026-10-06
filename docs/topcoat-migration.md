# Native Topcoat migration

Pinned main (`9683d38af8e1e6f9b076439fe90d9519109b2218`) is the behavior and
visual reference. Application state, rendering, validation, and workflows belong
in Rust. Native components use shared services and Topcoat procedures rather
than frontend REST calls.

## Current implementation

Login, Signup, Home, the shared workspace/sidebar, query-free issue lists and
boards, issue editing, project creation, Project Overview, Project Insights, and
Project Activity have native implementations.

The intermediate JavaScript frontend is deleted, including controllers,
frontend API clients, vendor libraries used by those controllers, generated
controller fixtures, and dormant Rust screen scaffolds. The only production
JavaScript assets are Topcoat's framework runtime and Rust-generated bindings.

## Unfinished features

Settings, archive import, issue creation, filtered issue lists and boards, pages,
files, plans, modules, dependency graphs, and public readers have no intermediate
fallback. Their canonical routes return 404 until native ports are implemented.
Existing backend REST/MCP interfaces remain available.

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
