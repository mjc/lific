# Mounted Topcoat transport

Topcoat 0.9.0 emits logical procedure URLs in serialized `Procedure` values
and logical shard URLs in HTML comment markers. Its `BaseUrl` application
context does not reach those constructors. The procedure macro constructs a
static `ProcedureSurrogate`; shard markers write their static endpoint URL.
There is no request-aware URL attribute visitor in the pinned view renderer.

The vendored framework runtime therefore has one transport helper and three
changed call sites: procedure requests, shard requests, and shard socket URLs.
It also has the tuple compatibility patch described below. The rest of the
upstream runtime is preserved. Page reruns already use the current mounted
browser location and are unchanged.

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

Together these patches prepend five helper/comment lines and change five
sites in the pinned asset. To reconstruct upstream, remove those first five
lines and reverse the following substitutions:

| Patched expression | Upstream expression | Occurrences |
| --- | --- | --- |
| `fetch(topcoatMountedEndpoint(this.path)` | `fetch(this.path` | 2 |
| `url(){return topcoatMountedEndpoint(this.path)}` | `url(){return this.path}` | 1 |
| `function V(t,e){if(Array.isArray(t))return topcoatHydrateTuple(t,e);if(t!==null)` | `function V(t,e){if(t!==null)` | 1 |
| `function f(t){if(t==null)return null;if(Array.isArray(t))return t.map(f);` | `function f(t){if(t==null)return null;` | 1 |

The reconstructed bytes match the upstream SHA-256 recorded in
`../assets/runtime.LICENSE.txt`.

## Tests and integration scope

`transport.browser.test.cjs` loads the actual vendored runtime in headless
Chromium against an HTTP/WebSocket protocol fixture. It covers root, `/app`,
and a mount matching the project identifier; nested and returned procedures;
logical dehydration; framed shard updates; actual socket URLs and protocol;
absolute URLs; same-origin cookies; and an unchanged global `fetch` function.
The tuple case covers indexing and explicit dehydration, nested tuple values
inside tagged collections, exact large integer values, returned procedures,
and passing hydrated or constructed tuples back to a procedure.

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
