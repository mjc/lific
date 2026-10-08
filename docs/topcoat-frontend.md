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
Pages and Plans share `native/mascot.rs` for the original empty-state artwork.
Their private list/detail routes use the existing workspace owner and sidebar;
reads and mutations call shared Rust services also used by REST. Page list rows
contain bounded previews rather than full document bodies.

All intermediate JavaScript application controllers and their fallback screens
are removed. Unfinished features remain unavailable until they are implemented
in Rust; see `topcoat-migration.md`. Browser-only APIs are accessed through the
native Topcoat components, while Rust owns application decisions and state.

Home omits collapsed project destination trees, the unopened phone dialog, and
closed palette result handlers. The phone dialog initializes on first use and
retains its shared sidebar state through closing and browser history. Initial
HTML contains one project catalog; stale projection checks use its revision.
Shared browser handlers are generated from Rust and served in two versioned,
immutable assets. The runtime imports them before hydration, so connected
controls already have their listeners. The shared invocation bridge registers
only immutable functions; signal handles and request data stay in each owning
mount scope. Home refresh, recents, mobile navigation, and account checks reuse
those assets. Sidebar rows carry compact scalar event arguments, preserving
exact 64-bit IDs and JSON encoding for arbitrary editor text.

The Home shell composes separate palette, mobile navigation, and chrome
components. Palette projections carry authorized destinations and their revision;
selection and queued Enter do not read result counts or destinations from the
DOM. Mobile history enters through one typed record. Shared browser bindings
handle focus, storage, media queries, listeners, and disposal. Theme and collapse
handlers are emitted once and reused by desktop and mobile controls.

Lucide icons render their geometry inline. Shared styles supply SVG paint
defaults, and each instance contains only its selected glyph. Sizes, colors,
transforms and selectors remain valid. Sidebar controls use the same semantic
helpers. Glyphs make no separate HTTP requests, and the stylesheet contains no
embedded icon catalog.

Initial document responses send one mounted HTTP `Link` image preload header
for the selected logo. Embedded glyphs need no hints. Unopened picker choices,
redirects, API responses and WebSocket upgrades add no image hints.

Use `icons::ui_icon(cx, icons::UiIcon::Search, 16)` for application controls.
Semantic names map to approved Lucide glyphs in `native/icons/ui.rs`.
Add a `UiIcon` variant with its glyph mapping; aliases such as `ShowPassword`
and `Preview` resolve to the same geometry. Use `icons::project_icon` for
stored project values, emoji and the logo. Picker grids use
`icons::picker_choice_icon` so unopened logo choices do not add preload hints.

Run `node scripts/optimize-native-icons.mjs` to regenerate the inline catalog.
It invokes `nix run nixpkgs#svgo` with the checked-in configuration and six-digit
precision. Geometry tests compare all 1,937 generated glyphs with the original
approved nodes and attributes. Production embeds only the generated catalog.

The pinned framework source and distribution are retained for reproducible
runtime patches. Its tests compare the served runtime against the upstream
artifact plus documented fixes. Framework transport is distinct from an
application controller.

Standalone runtime tests cover a smaller signal-comment encoding. The
application still uses the pinned framework's signal renderer; adopting
that encoding requires an upstream change and browser hydration checks.

Topcoat 0.10 supplies native client navigation and intent prefetching through
`native/navigation.rs`. Rust-rendered internal links share its mounted URL
helper. Entry-token links disable prefetching. Pages outcomes use named records,
and session checks return nested optional tuples directly. See
`../src/topcoat/native/TRANSPORT.md` for page ownership, cancellation, and socket boundaries.

Run `devenv tasks run lific:topcoat:fmt` to format Rust and Topcoat macros with
the matching pinned CLI. `lific:topcoat:fmt-check` checks the same sources without
changing them.

Use the repository's devenv workflow for compilation and focused native tests.
Do not run headless browsers. Original main assertions and their current native adapter gaps are
recorded under `src/topcoat/tests/main/`.

## Framework distribution

The framework is pinned to the rebased fork, including its macro lowering fixes.
Source and binary builds retain that revision. Registry publication is disabled:
Cargo normalizes Git dependencies to registry versions when publishing, which
would discard those fixes. Restore publication and registry package verification
after the required fixes have a compatible published version.
