# Mounted Topcoat transport

The target runtime is Topcoat 0.10.0. Its browser package now handles tuple
surrogates and a shared document WebSocket with per-target Run IDs, Stop
messages, and `{run, frame}` envelopes. Lific retains only generic transport
patches that are not supplied upstream. No session, authorization, or product
policy is added to the browser runtime.

Topcoat keeps procedure and shard endpoint paths logical. HTTP requests resolve
the validated document mount at send time. A page rerun starts from the browser
URL, so its shared-socket Run path removes one mount prefix at a path boundary.
Shard Run paths are already logical and remain unchanged. This preserves
`/ACC/ACC/...` when the project identifier equals the mount and avoids
double-prefixing serialized procedures.
The one shared socket handshakes at the mounted document path; its Run paths
are routed after the proxy has stripped the mount.

The local runtime patch also preserves the connected-render contract while the
shared socket is loading, handshaking, or reconnecting: connected units join or
wait for the shared connection rather than falling back to HTTP. The open
callback reads current inputs. Run-scoped redirects, runless connection-retirement
redirects, and redirected render HTTP responses share one document navigation
claim.

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
Mount resolution uses the existing credentials and origin policy. It does not
intercept global `fetch` or buffer HTML responses.

## Procedure redirects

Typed procedure calls require JSON responses. Ordinary and keepalive calls use
Fetch's `redirect: 'manual'` so an authentication redirect rejects the procedure
before requesting an unrelated login document. The existing non-OK response
check also rejects the browser's opaque redirect response without decoding it.
Callers retain their existing failure recovery and navigation revision checks.
The server's HTTP redirect status and document navigation remain unchanged.

The packaged Procedure request adds `redirect:"manual"` and exposes an opt-in
keepalive call plus a typed adapter. Ordinary calls preserve their existing
Fetch options. The reconstruction oracle reverses the method to the pinned
upstream implementation.
The native redirect browser regression observes real 303 responses, zero login
GET requests, and recovery using the replacement cookie for both authentication
modes and all three mounts. This proves the RPC redirect contract; it does not
identify the suppressed callback in the earlier intermittent suite failure.
Upstream submission is tracked by LIF-246.

## Upstream 0.10 transport and compatibility

Topcoat 0.10 handles tuple surrogate hydration/dehydration and one shared
WebSocket connection per document. Run IDs identify each target, Stop messages
cancel superseded work, and replies are `{run, frame}` envelopes. Those
protocol features remain upstream. The application patch does not restore the
old tuple adapter or old per-target socket transport.

The asset patch keeps a small reversible prefix helper and patches mount-time
HTTP requests, page Run paths, generic procedure keepalive/manual redirects,
connection wait behavior, scoped browser mount events, native Event access,
vector signal writes, Unicode string helpers, render-error notifications, and
redirect arbitration. The test oracle strips the helper prefix, reverses each
substitution, and compares bytes with the exact package asset. The version,
commit, source digest, and patched digest live in `../assets/runtime.LICENSE.txt`.

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

Every event factory and callback receives a private context exposing the owning scope’s
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

## Shared connected render selection

Topcoat 0.10 shares one connection and distinguishes target Runs by ID. Lific
keeps the no-HTTP-fallback behavior for connected units while the document is
loading or the socket is handshaking/reconnecting. A connected refresh sends a
fresh Run when the socket is open; otherwise it joins the shared connection and
returns. The open handler reads the latest render inputs. HTTP-only shards
continue to use mounted POST requests.

The connection handshake uses the mounted document URL. A page Run path is
logical for the internal router, so the browser removes exactly one validated
mount prefix. Shard paths are already logical and are sent unchanged. This
keeps project identifiers that repeat the mount prefix intact.

## Document navigation

Sibling connected render units can receive redirects for the same document.
The document Runtime claims its first redirect before calling `location.assign`.
Socket protocol redirects and redirected HTTP render responses share that claim,
so a second connection cannot initiate another navigation while the first is
pending. The claim remains terminal for that Runtime; a new document creates a
new Runtime. Error reporting and transport lifetimes retain their existing paths.
This coordination contains no application session, destination or route policy.

## Native links and page ownership

Rust renders internal anchors with Topcoat's `link_attrs` helper. Ordinary
links prefetch on intent; one-shot resume and notice URLs use `Never` so a
hover cannot consume their token. Programmatic successful actions call the
same navigation controller through `cx.navigate`. Authentication retirement
still requires a fresh document and cookie admission.

The framework fetches destination HTML with the current signals and hydrates
its initial server render. Route-owned signal keys include the account and
logical destination; the shared sidebar and same-project deferred-delete
owner keep their stable keys. A committed page closes the old physical socket
before joining destination render targets, so the new handshake admits its
own credentials and public/private scope. Queued frames from the retired
socket cannot affect the replacement.

Two generic lifecycle events let mounted owners participate without adding an
application router. `topcoat:before-navigation-commit` carries the URL, history
mode, destination document, abort signal and `waitUntil` function. Navigation
awaits registered work before writing history. Rejection, cancellation or a
superseding navigation leaves the current document in place.
The native shell registers a fresh authorization request against the incoming
account and admin baseline before a cached destination can commit. It checks
current project membership and detail-resource ownership; denied access or an
identity change loads the full destination URL through normal document admission.
Aborted navigation ignores late authorization results.
`topcoat:before-page-replace` supplies the destination document synchronously
before the old scope aborts. Deferred deletion uses that boundary to transfer
timers and pending outcomes within the same project, or claim pending deletes
once when leaving it. Rust signals retain domain state; the browser owns DOM
listeners, timer handles and host lifetime tokens.

Pages write outcomes use Topcoat records with named fields. The session
procedure returns the existing domain shape, `Option<(i64, bool)>`, through
upstream tuple support instead of flattening absence into a separate flag.

## Private raw socket admission

The production router registers Rust `SocketAdmission` after `.runtime()`.
Topcoat executes pathless layers in reverse registration order, so admission
runs before `RuntimeLayer` can return an upgrade response. Only `GET` requests
asking for the exact `topcoat-runtime` subprotocol enter this boundary. Ordinary
documents, procedures, shard HTTP requests, and other socket protocols keep
their existing dispatch.

The existing Rust route parser identifies published `Public` scope before any
private caller lookup. Runtime socket admission accepts only supported published
routes after a fresh publication check, then acquires a global-only permit from
the shared 1024-slot quota. Cookies and optional operator identity are not
resolved for this scope. Missing, private, unpublished or unsupported routes
return 404 before upgrade.

Private requests resolve their current credentials through the existing caller,
require a user, and acquire the same per-user/global `RealtimeHub::SocketPermit`
used by REST events sockets. Genuine configured private local-operator behavior
remains in that caller; invalid or revoked credentials cannot select its
fallback. Quota refusals return HTTP 429 with the existing socket-limit JSON
message. Admission does not grant project access; every native read and write
keeps its own current authorization checks.

The generic socket driver and matching connected-render helpers live in
`../runtime`. Both checkout and packaged applications compile those sources with
pinned framework signal, shard, procedure, router and view types. The standalone vendor
runtime tests compile those same files. The packaged source includes its MIT
license and upstream provenance. Admission runs before this socket layer; HTTP
reruns continue through the pinned Git framework runtime layer.

The driver retains the incoming `Cx` in the connection target shared by its
render tasks. Admission installs an `Arc<SocketPermit>` and, for private sockets, one
request-owned authority lifetime. That lifetime captures its parent context
before being installed on a child, avoiding a reference cycle. Revocation and
the existing 60-second database revalidation run independently of renders and
socket input. Home's content-event subscription belongs to its render; authority
belongs to the shared physical document connection.

Retirement cancels the pumps and aborts/awaits the active render before a bounded
framework Redirect/close. The framework document runtime claims navigation once
across sibling socket and HTTP redirects, preserving one fresh document request
with the browser's current cookie. No permit is acquired by synthetic renders.
Disconnect and failed upgrades release the captured context and permit.

These admission and lifecycle guarantees do not complete native published
application rendering or the full product port. Published domain reads and
procedures still need their route-specific Rust scope and recovery contracts.
Document navigation and static resources retain normal same-origin browser
cookie behavior.

## Tests and integration scope

`transport.browser.test.cjs` loads the packaged runtime against HTTP and shared
WebSocket fixtures. It covers root, `/app`, and `/ACC` mounts; mounted procedure
requests and logical dehydration; shared document socket URLs; page and shard
Run paths; Run/Stop envelopes; stale-run suppression; cookies; and unchanged
global `fetch`. Separate browser-free Node VM tests cover the exact reconstruction
checksum, path-boundary mount conversion (including `/ACC/ACC`), procedure
keepalive opt-in, pending-connection no-fallback behavior, Unicode width and
trimming, redirect arbitration for run-scoped and runless frames, page socket
ownership, owner barriers and cancellation, and programmatic navigation.

Lifecycle browser tests cover one mount initialization per owning scope, real
abort-signal exposure, ordinary event lifetime, retained-element replacement,
cleanup, and error isolation. Render-failure tests cover scoped bubbling events,
mounted render paths, disposed owners, and continued error logging.

Rust unit tests exercise trusted proxy context, prefix validation and missing
configuration, and logical URL mounting. Framework-focused tests do not establish
domain authorization, actual endpoint registration, or streaming middleware
behavior; those are covered by the executable server integration tests. The
assembled native fixture uses the production server factory, real session cookies
and database, and checks authorized HTML, native saves, conflicts, revoked
sessions, framed shards, mounted shared-socket reruns, cross-origin refusal, and
zero browser REST requests.

## Native event adapter

`Context.event(nativeEvent)` wraps native DOM events with the existing framework
Event surrogate. Rust expressions can read keys, targets and modifiers or cancel
the original event. Serialized wire hydration keeps its original contract.

## Generic procedure keepalive

The procedure transport exposes `call_keepalive` and a callable
`with_keepalive` adapter for typed Rust expressions. Both use the existing
argument array, endpoint mounting, hydration and lazy Future behavior.
Keepalive adds only the Fetch flag; ordinary calls keep their original options
and same-origin cookie defaults. The adapter does not set an abort signal or
an Authorization header. Browser keepalive remains subject to browser request
limits and does not promise successful delivery. Calling the method constructs
a lazy Future; consuming it starts the request. This framework transport owns
no application deletion, timer, authorization, or toast policy.


## Scoped render failures

The generic runtime logs a render error and emits `topcoat:render-error` from
the current shard marker parent, or from document for a page. The event bubbles
and carries the mounted render URL in `detail.path`. Disposed owners keep error
logging but do not notify; superseded connected runs and canceled HTTP requests
retain their existing rejection rules. HTTP request failures use the owning
render unit's reporter.

Rust Home scheduling uses this completion event to release its in-flight state
while retaining the old rendered body. It applies the same completion path as
a successful snapshot. The framework notification owns no application retry
policy or Home state.

The packaged runtime adds reversible generic render-error reporting and HTTP
request delegation. The reconstruction test reverses those substitutions and
all remaining transport adaptations to the exact upstream package hash. The source and packaged tests cover
owner paths, retained logging, disposal, stale connected errors, and actual
scheduled HTTP failures.

## ECMAScript whitespace

The generic `StrEcmaTrimExt::trim_ecmascript` operation returns an existing
owned String wire value. It preserves interfaces using ECMAScript trim:
BOM is removed at the edges and NEL is retained. The packaged Str operation
calls `String.prototype.trim`; the Rust host enumerates exactly the ECMAScript
WhiteSpace and LineTerminator set. Existing Rust `trim` behavior remains
Unicode White_Space. The package regression executes eighteen actual Rust
expression sources and compares their authoritative surrogate wires.

The additional reverse substitution removes the ECMAScript trim method from the
owned String implementation. Its exact expression is declared in the reconstruction
oracle and the pinned upstream digest remains unchanged.
