# Mounted Topcoat transport

Topcoat 0.9.0 emits logical procedure URLs in serialized `Procedure` values
and logical shard URLs in HTML comment markers. Its `BaseUrl` application
context does not reach those constructors. The procedure macro constructs a
static `ProcedureSurrogate`; shard markers write their static endpoint URL.
There is no request-aware URL attribute visitor in the pinned view renderer.

The vendored framework runtime therefore has one transport helper and three
changed call sites: procedure requests, shard requests, and shard socket URLs.
The rest of the upstream runtime is preserved. Page reruns already use the
current mounted browser location and are unchanged.

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

## Tests and integration scope

`transport.browser.test.cjs` loads the actual vendored runtime in headless
Chromium against an HTTP/WebSocket protocol fixture. It covers root, `/app`,
and a mount matching the project identifier; nested and returned procedures;
logical dehydration; framed shard updates; actual socket URLs and protocol;
absolute URLs; same-origin cookies; and an unchanged global `fetch` function.

Rust unit tests exercise trusted proxy context, prefix validation and missing
configuration, and logical URL mounting. These framework-focused tests do not
establish domain authorization, actual native endpoint registration, or
streaming middleware behavior. Those require the executable integration tests
owned by the server integration work.
