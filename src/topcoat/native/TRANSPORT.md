# Mounted Topcoat transport

Topcoat 0.9.0 emits logical procedure URLs in serialized `Procedure` values
and logical shard URLs in HTML comment markers. Its `BaseUrl` application
context does not reach those constructors. The procedure macro constructs a
static `ProcedureSurrogate`; shard markers write their static endpoint URL.
There is no request-aware URL attribute visitor in the pinned view renderer.

The vendored framework runtime therefore has one transport helper and three
changed call sites: procedure requests, shard requests, and shard socket URLs.
It also includes the tuple, mount lifecycle, comment decoding, and
connected-render patches described below. The rest of the upstream runtime is preserved. Page request URLs already
use the current mounted browser location.

## Rust document boundary

Register `Arc<[crate::ratelimit::IpNetwork]>` in Topcoat's application context.
Before Tower dispatch, preserve the verified direct peer address as
`topcoat::router::RemoteAddr` in request extensions. `trusted_mount(cx)` only
accepts `X-Forwarded-Prefix` when that direct peer matches a configured trusted
proxy, then uses the existing `session::forwarded_prefix` validator.

Render `data-topcoat-runtime-prefix` on the document's `<html>` element using
`trusted_mount(cx)`. An absent attribute means the root mount. This attribute
contains a validated URL path, never an origin, user credential, or routing
guess derived from the page's first segment.

Use `mounted_url(cx, logical_path)` for Rust-rendered root-relative links,
form actions, and asset URLs. Inputs are always logical URLs. A mount `/ACC`
and logical project path `/ACC/overview` produce `/ACC/ACC/overview`; checking
whether the input already starts with the mount would lose the project segment.
Protocol-relative and absolute URLs, fragments, queries, and relative URLs are
preserved.

Procedure and shard paths remain logical inside framework objects, markers,
and serialized values. The helper resolves the mount at each request. Passing
or returning nested procedure values therefore does not accumulate prefixes.
No global `fetch` interception, fetch options, credentials, or origin policy
changes are introduced. No HTML response buffering is required.

## Rust tuple compatibility

The pinned Rust tuple surrogate implementation serializes tuples as untagged
JSON arrays. The pinned browser hydration function instead expects every
object to have a surrogate tag, so real tuple procedure responses fail with
`Unknown surrogate type: undefined`.

`topcoatHydrateTuple` recursively hydrates each array element and adds a
non-enumerable `dehydrate()` method to the resulting array. Index access stays
normal tuple index access. One array guard in the existing hydration function
selects this helper. One array guard in the existing dehydration function
recursively serializes arrays, including tuple literals constructed in runtime
expressions. Nested Result, Option, integer, collection, and Procedure values
continue to use their existing framework conversion. Stored procedure URLs
remain logical. Tagged Vec, Array, and Slice surrogates retain their existing
implementations.

Together these patches prepend 27 helper/comment lines and change eight
sites in the pinned asset. To reconstruct upstream, remove those first 27
lines and reverse the following substitutions:

| Patched expression | Upstream expression | Occurrences |
| --- | --- | --- |
| `function pe(t){let e=new DOMParser().parseFromString(t.replaceAll("<","&lt;"),"text/html")` | `function pe(t){let e=new DOMParser().parseFromString(t,"text/html")` | 1 |
| `fetch(topcoatMountedEndpoint(this.path)` | `fetch(this.path` | 2 |
| `url(){return topcoatMountedEndpoint(this.path)}` | `url(){return this.path}` | 1 |
| `function V(t,e){if(Array.isArray(t))return topcoatHydrateTuple(t,e);if(t!==null)` | `function V(t,e){if(t!==null)` | 1 |
| `function f(t){if(t==null)return null;if(Array.isArray(t))return t.map(f);` | `function f(t){if(t==null)return null;` | 1 |
| ``let r=e.name.substring(ke.length);if(r==="mount"){topcoatMount(t,()=>T(e.value,`event @${r}`)(Object.assign(Object.create(n.runtime.context),{abortSignal:n.abortSignal})),n);return}let o=T(e.value,`event @${r}`)(n.runtime.context);`` | ``let r=e.name.substring(ke.length),o=T(e.value,`event @${r}`)(n.runtime.context);`` | 1 |
| `refresh(){if(this.isDisposed)return Promise.resolve();if(this.connection!==null)return this.connection.requestRun(),Promise.resolve();if(this.requiresConnection){for(let n of this.ancestors())if(n.connection!==null\|\|n.requiresConnection)return n.refresh();return this.connectIfRequired(),Promise.resolve()}` | `refresh(){if(this.connection?.isOpen)return this.connection.requestRun(),Promise.resolve();if(this.requiresConnection){for(let n of this.ancestors())if(n.connection?.isOpen)return n.refresh()}` | 1 |

The reconstructed bytes match the upstream SHA-256 recorded in
`../assets/runtime.LICENSE.txt`.

## Comment decoding

Rust signal declarations encode `>`, `&`, and quotes inside inert HTML
comments. Literal `<` remains data. The pinned decoder passed that data to an
HTML parser directly, which truncated tag-like strings and broke JSON parsing
when persisted issue text was hydrated after reload.

The decoder now escapes literal `<` before its existing entity-decoding step.
The parser reads text, and JSON receives the original value exactly once.
No application values, state, or handlers are added to the runtime. Real native
editor tests persist markup and comment terminators, reload, and check exact
visible text and edit values without created elements or effects.

## Browser initialization

The pinned runtime treats every event name as an ordinary DOM listener. It has
no mount lifecycle. Native components need one to obtain browser-owned clock,
locale and storage inputs without a separate application controller.

The event setup site delegates `@mount` to a generic scope-owned helper. Its
factory and callback run in a microtask after hydration and watcher setup.
An element mounts once per owning scope; a new scope can mount a retained DOM
element after a render. Released scopes and detached elements never start.
A transient listener receives only the runtime's own native event, preserving
event targets while preventing early or bubbling synthetic events from
consuming initialization. Callback errors go through the runtime reporter.

Each mount callback receives a private context exposing the owning scope’s
`abortSignal`. The shared runtime context remains unchanged. Global listeners
registered with this signal stop when their owner is disposed; replacement
scopes register their own listeners.

Rust consumers retain an initialization sentinel across renders for one-time
primitive input reads. Subscriptions use the owning scope lifetime instead. The primitive
adapter reads one supplied storage key, epoch time, local timezone offset and
browser collation locale into a string signal. It catches denied storage as
absence. Consumers must bound and validate these browser values before using
them; they confer no authority. Rust owns parsing, projections and rendering.

Callbacks pass the owning abort signal to browser operations that support
cancellation. Asynchronous work without cancellation support still needs its
own cleanup.

## Connected render selection

The pinned render unit selects a WebSocket only after it opens. Signal changes
while document loading delays the connection, during its handshake, or during
reconnect therefore fall back to HTTP, even for content requiring a connection.
An initialization mount callback can trigger that fallback before the socket
starts.

The render unit's `refresh()` now selects an existing connection regardless of
its socket state. A connection-required child delegates to an ancestor with
either an existing connection or a connection marker. Otherwise it schedules
its own required connection and waits without an HTTP render. The existing
socket open handler reads the current render inputs on every initial connection
and reconnect; intermediate updates need no separate queue or saved arguments.
Disposed units stop immediately, and the existing lifetime abort signal cancels
load listeners, socket ownership, and reconnect timers. Shards without a
connection requirement continue to use their existing HTTP transport.

## Tests and integration scope

`transport.browser.test.cjs` loads the actual vendored runtime in headless
Chromium against an HTTP/WebSocket protocol fixture. It covers root, `/app`,
and a mount matching the project identifier; nested and returned procedures;
logical dehydration; framed shard updates; actual socket URLs and protocol;
absolute URLs; same-origin cookies; and an unchanged global `fetch` function.
The tuple case covers indexing and explicit dehydration, nested tuple values
inside tagged collections, exact large integer values, returned procedures,
and passing hydrated or constructed tuples back to a procedure.
The connected-render case holds document loading and socket handshakes for both
own and ancestor connections. It proves that mount and later signal updates
wait without HTTP fallback, that initial open and reconnect send the freshest
inputs, and that HTTP-only shards continue to POST their current arguments.
The reconstruction test reverses all declared substitutions and checks the
exact pinned upstream SHA-256. Lifecycle browser tests cover later signal
declarations, one refresh with a persistent sentinel, reused DOM elements,
scope cancellation, error isolation and early synthetic event dispatch. They
also prove that mount contexts expose their real owning abort signal, that
normal event contexts remain unchanged, and that global subscriptions stop
across retained-element scope replacements.

Rust unit tests exercise trusted proxy context, prefix validation and missing
configuration, and logical URL mounting. These framework-focused tests do not
establish domain authorization, actual native endpoint registration, or
streaming middleware behavior. Those require the executable integration tests
owned by the server integration work.

The assembled native fixture uses the production server factory, real session
cookies and database. It checks initial authorized HTML, successive native
saves, typed conflicts and revoked sessions, framed HTTP shards, mounted socket
reruns, cross-origin refusal, and zero browser REST requests. Its read boundary
maps credential denial, permission denial and missing records to framework HTTP
errors; raw `LificError` conversion would otherwise produce a generic 500.
Production native handlers need the same classification. The fixture does not
establish product UI parity, socket recovery, or complete authentication
acceptance.
The assembled probe also uses the Rust-authored input adapter at root, `/app`
and `/ACC`. A fixed browser clock and timezone prove primitive values, denied
storage remains nonfatal, and hostile stored text stays text. This fixture does
not establish production Home initialization or recents behavior.
