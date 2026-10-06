# Topcoat frontend

The Rust frontend builds without Vite. Native application
components are under `src/topcoat/native/`; `src/server.rs` serves their routes,
shared stylesheet, fonts, images, the framework runtime, and Rust-generated
handler assets.

New native views use Tailwind utilities with Main's typography tokens. The
pinned CLI scans Rust templates and generates `src/topcoat/assets/tailwind.css`,
which is embedded in the binary. Run `devenv tasks run lific:topcoat:styles`
after changing classes; `lific:topcoat:styles-check` detects stale output.
The compiler is a development dependency. Cargo builds and the application
need neither Node nor Vite for these styles.

Shared presentation helpers live in `native/avatar.rs` (display names and
picker/member initials) and `native/dates.rs` (localized dates, relative times,
and the clock that pauses while the document is hidden). Activity tooltips
combine the shared localized date signal with a transport label. Text matching
and trimming reuse
`runtime/whitespace.rs` so native views preserve Main's whitespace rules.

All intermediate JavaScript application controllers and their fallback screens
are removed. Unfinished features remain unavailable until they are implemented
in Rust; see `topcoat-migration.md`. Browser-only APIs are accessed through the
native Topcoat components, while Rust owns application decisions and state.

Home omits collapsed project destination trees, the unopened phone dialog, and
closed palette result handlers. The phone dialog initializes on first use and
retains its shared sidebar state through closing and browser history. Initial
HTML contains one project catalog; stale projection checks use its revision.
Shared browser handlers are generated from Rust and served in three versioned,
immutable assets. The runtime imports them before hydration, so connected
controls already have their listeners. The shared invocation bridge registers
only immutable functions; signal handles and request data stay in each owning
mount scope. Home refresh, recents, mobile navigation, and account checks reuse
those assets. Sidebar rows carry compact scalar event arguments, preserving
exact 64-bit IDs and JSON encoding for arbitrary editor text.
Lucide geometry is served as versioned, immutable assets for the selected icons.
Repeated sidebar chevrons and overflow icons use CSS pseudo-elements and shared
masks; other icons reuse SVG geometry.

The pinned framework source and distribution are retained for reproducible
runtime patches. Its tests compare the served runtime against the upstream
artifact plus documented fixes. Framework transport is distinct from an
application controller.

Standalone runtime tests cover a smaller signal-comment encoding. The
application still uses the published framework's signal renderer; adopting
that encoding requires an upstream change and browser hydration checks.

Use the repository's devenv profiles for compilation and headless browser
checks. Original main assertions and their current native adapter gaps are
recorded under `src/topcoat/tests/main/`.
