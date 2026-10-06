# Native comment coverage after private controller removal

Pinned main is `9683d38af8e1e6f9b076439fe90d9519109b2218`. Sources are `web/tests/comments.test.ts`, `web/src/lib/Comments.svelte`, `web/src/lib/commentState.ts`, and `web/src/routes/IssueDetail.svelte`. Main’s Comments section has a `Comments` heading, an ordered thread with `comment-{id}` anchors, `Comment {id} actions` menus, Edit/Delete/Save controls, and `Load older comments`. Tests derive these contracts from main rather than the removed branch controller’s namespaces or data attributes.

All 34 cases formerly in `comments.test.js` now retain their names in `comments-window.browser.test.js`. The suite starts the real server, creates authenticated production issues, seeds deterministic comment rows in SQLite, navigates the native issue route, and asserts production DOM and persisted outcomes. `comments-adapter.js` and its fabricated controller/root state are removed. The unit runner consequently has 34 fewer cases and the browser runner 34 more; inventory targets point to the new browser file. No case is skipped.

The native issue composition currently lacks Comments. A failure at the actual Comments section or a persisted comment anchor is an active product gap, tracked under #228. Assertions after that boundary remain unreachable until the native component is implemented; retaining a case name is not a claim that every original assertion already passes.

| Retained case group | Actual production assertions and remaining scope |
| --- | --- |
| Author actions, edited state | Authenticated author action menu exists; viewer account has no author menu; edited indicator changes only with stored update time. Null-account and unavailable-action matrices remain domain assertions for the future Rust model. |
| Idempotent fold and comparator | Stored edits preserve canonical `(created_at,id)` order across reload; timestamp ties/backfilled timestamps match SQLite; repeated reads contain unique ids. Direct same-event local fold and immutable input-array assertions need the native model. |
| Newest bounded page and overfetch | Three rows render chronologically; 1000-row thread shows only newest 50 and Older; short thread has no Older; 51-row thread keeps the lookahead out of DOM. Native direct reads replace the old REST request shape. |
| Byte budget and named cursor | Actual large SQLite bodies force the production API’s byte-limited response; its continuation header stays true and rendered rows match the response. Actual API cursor pair produces only older rows; mounted native Older renders 50 then 100. Contradictory/absent synthetic response headers are not fabricated; the fallback helper matrix needs a future Rust domain boundary. |
| Paging, concurrent writes, dedupe, oldest cursor | Large bodies require repeated Older; all canonical ids appear once; a real new comment while paging does not duplicate existing rows; oldest rendered id matches SQLite and no Older remains at the end. A server cannot naturally return overlapping committed keyset pages; defensive duplicate-input coverage needs the native model. |
| Refresh all loaded pages | Three loaded pages reconcile a real middle edit and oldest deletion without navigation. |
| Preserve loaded depth, exact window, exact boundary, short thread | External edits retain loaded ids; posting to a 50-row page makes a real 51-row loaded window whose subsequent refresh must remain exactly 51; 100-row boundary and short loaded thread preserve their sizes. |
| Refresh budget, caller bounds, capped reconciliation | An 800-row manually loaded thread keeps all ids and older rows when an edit within the recent window arrives; observed browser mutation requests cannot storm. Main’s precise automatic budget is **nine 51-row reads = 459 transferred rows, 450 refreshed rows**, below its 500-row transfer cap. Exact server read/transfer accounting and pathological numeric/fractional caller arguments remain unresolved native-domain coverage under #228, rather than being reimplemented in test JavaScript. |
| Fractional page arguments | The native mounted protocol uses fixed 50-row pages and a two-page read for 100 rows. Main’s explicit `7.9 -> 7` and `2.9 -> 2` helper argument matrix remains a native-domain coverage gap; no configurable browser control is invented. |
| Replacement and preserved ordering | Existing loaded rows stay canonical after external edits; a replacement refresh must not regress those rows. Precise apply/retry/abandon and bounded retry-token matrices require future native window ownership logic. |
| Failed refresh and empty thread | Require an observed aborted native HTTP read before asserting preserved ids; WebSocket-only reads remain an active fault-injection RED until their actual transport is observed; empty persisted thread shows No comments yet without Older. Mid-window fault timing and single-read accounting remain pending native implementation. |
| Newest operation and replacement outcome | Consecutive real writes must leave the newest body; an external mutation during a loaded-window lifetime cannot regress it. A deterministic held-refresh/older/navigation race remains required once the real component exposes that transport. |
| Unloaded anchor, failed anchor, page budget, navigation, manual Older | Deep links must load an older stored comment; refused native transport cannot retry endlessly; an 800-row thread stops automatic anchor work at 500 loaded rows; entering a second thread resets that scope; manual Older extends 500 to 550 without restarting automatic work. |

`comments.browser.test.js` also uses real mounted native issues for author/viewer controls, add/backfill/edit/delete revisions, mention roster updates, shared Mermaid budget and revisions, exact edited/original timestamps, keyboard actions, and actual persisted edit/delete outcomes. Browser-created application controllers and injected renderer scripts are gone. Main uses inline Delete confirmation, so tests exercise that flow rather than a branch-only browser dialog.

The later application-wide purge also removed the shared attachment helpers and
public parser. Their original assertions remain, but their adapters now report
an explicit native coverage gap. They are not test subjects or reference code.

The recorded unit counts above precede that broader purge and are historical.
Only original-source inventory checks establish mappings; they do not establish
passing behavior or completed adapter coverage.
