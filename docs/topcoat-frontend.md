# Topcoat frontend

The Topcoat feature builds the Rust frontend without Vite. Native application
components are under `src/topcoat/native/`; `src/server.rs` serves their routes,
shared stylesheet, fonts, images, and the framework runtime.

All intermediate JavaScript application controllers and their fallback screens
are removed. Unfinished features remain unavailable until they are implemented
in Rust; see `topcoat-migration.md`. Browser-only APIs are accessed through the
native Topcoat components, while Rust owns application decisions and state.

Home omits collapsed project destination trees, the unopened phone dialog, and
closed palette result handlers. The phone dialog initializes on first use and
retains its shared sidebar state through closing and browser history. Initial
HTML contains one project catalog; stale projection checks use its revision.
The shell binds each shared signal handle once within its owning mount scope.
Lucide geometry is served as versioned, immutable assets for the selected icons;
repeated instances reuse cached geometry and share presentation CSS.

The pinned framework source and distribution are retained for reproducible
runtime patches. Its tests compare the served runtime against the upstream
artifact plus documented fixes. Framework transport is distinct from an
application controller.

Use the repository's devenv profiles for compilation and headless browser
checks. Original main assertions and their current native adapter gaps are
recorded under `src/topcoat/tests/main/`.
