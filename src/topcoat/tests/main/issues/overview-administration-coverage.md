# Native administration coverage

The native implementation is the executable under test. Pinned main (`9683d38af8e1e6f9b076439fe90d9519109b2218`) supplies the product behavior. The removed administration case names below came from this branch’s hybrid controller tests; they are not pinned-main test names. A mapped test is not a claimed pass: run the actual native suite and retain failures for unfinished behavior.

Native browser replacements live in [overview-administration.browser.test.js](overview-administration.browser.test.js). Existing Overview production smoke covers name/description saves, successful group assignment, label creation, publish/unpublish, viewer controls, draft retention and disclosures. It does not execute GitHub import, archive download, member mutation or project deletion. Paired geometry tests prove geometry and drafts, not completed mutations.

## Removed administration unit cases

Names below are the exact removed hybrid case names. They identify regression assertions around main features, sometimes with stronger branch-specific behavior. **Partial** and **open** identify assertions still needing actual native coverage; branch-specific assertions are marked separately.

| Removed hybrid case | Native counterpart and remaining assertions |
| --- | --- |
| identifier validation matches reserved word and server grammar | Native create model/form normalization. **Partial:** exact reserved-word, length and hyphen rejection matrix. |
| custom label colors normalize while legacy unsafe colors never reach CSS | `label_hex_and_safe_legacy_colors_preserve_source_input_rules` and `label_unsafe_stored_color_uses_native_component_fallback`. Preserve exact mixed-case hex and malicious-CSS fallback assertions. |
| archive selection rejects nonarchives empty and oversized files | Retained archive import unit validation. |
| project created before failed group assignment remains usable with warning | `native_project_create_foreign_group_failure_keeps_created_project_and_notice_at_overview` and shared creation/assignment event and audit test. |
| viewer cannot mutate project or member roles; label edits remain maintainer gated | Native viewer smoke and `overview_label_mutations_keep_legacy_lead_and_enforced_maintainer_gates`. **Partial:** rendered maintainer membership controls matrix. |
| membership uses PATCH role endpoint and surfaces forbidden failures | Native member metadata/write/rollback browser case replaces obsolete REST verb with native command and persisted-role checks. **Partial:** permission-forbidden UI refusal distinct from last-lead conflict. |
| label merge retains target color and removes source only after server success | `overview_label_merge_reassigns_issue_and_page_usage_once_and_refuses_other_projects`. **Partial:** distinct source/target colors, target color unchanged, browser refusal retains source. |
| failed project reorder keeps canonical order and permits retry | **Open:** real native sidebar refusal, canonical order and successful retry. |
| late mutation cannot update a different signed-in account | Native pending confirmation and archive account-replacement cases. **Branch regression goal:** ordinary pending name-write response after replacement. Main’s archive flow has explicit account/session guards; its ordinary Overview field save does not establish the same race contract. |
| archive upload distinguishes progress processing and uncertain connection loss | Retained archive import progress/processing/unknown-outcome unit case. |
| invalid archive can be retried and imported report preserves external references | Retained archive import refusal/retry/report unit case. |
| archive download is revoked after an account switch while preparing | Native archive account-replacement browser case requires zero download events. |
| project group assignment failure rolls back selection and successful order uses server response | Native successful assignment plus `assignment_refusals_keep_all_rows_and_publish_nothing_including_stale_admin_snapshot`. **Partial:** actual rendered selection rollback and authoritative order. |
| GitHub preview freezes the repository token and mapping for confirmed import | Native live importer browser case: preview writes nothing, confirmed request retains repo/mapping, persisted source/status. **Partial:** nonempty token. **Branch regression goal:** attempted pending-preview edit cannot alter a frozen snapshot. Main hides configuration after preview but reads live fields for each request and permits field edits while preview is pending. Genuine public upstream may independently fail within existing timing. |
| recent-auth refusal preserves action and retries once after verified refresh | Native wrong-password/retry browser case asserts frozen member/role, unchanged rejected cookie, successful rotation and one member row. |
| unknown archive import survives page reload until project list is checked | Retained archive import unknown-outcome persistence unit case. |
| project ZIP export requests a format matching its saved filename | Native ZIP browser case checks actual response/download bytes, PK signature and acc-export.zip filename. |
| archive export requires confirmation and verified account before saving | Native acknowledgement, actual archive bytes/filename, post-Blob owner request and account-replacement browser cases. |
| project and membership events refresh authoritative identity and role | Service stale-authority tests retain guards. **Branch regression goal:** native project/member events refresh identity/controls; unrelated project events do not. Pinned-main Overview and Members have no corresponding event subscriptions, so this is not evidence of missing main behavior. |
| reconnect and catalog refresh update permissions without losing archive outcome | Retained archive unknown-outcome state covers archive half. **Branch regression goal:** native reconnect discovers missed demotion without losing actionable state. Main archive session-change cancellation is a separate, required behavior; this combined reconnect contract originated in the hybrid controller. |
| successful identifier rename adopts and navigates to the returned identifier | Native normalization model only. **Open:** genuine returned-identifier navigation. |
| membership role and add writes retain display metadata absent from server write DTO | Native member add/change browser case reads joined display name and username after each actual write. |
| archive download rejects viewer and maintainer even when general authorization is off | Native enforcement-off viewer/maintainer browser case requires hidden controls and zero downloads. |
| same-account refresh cancels stale reauthentication with an actionable retry notice | **Branch regression goal:** actual same-account refresh interruption and usable retry notice. Main freezes pending membership/lead grants and retries after reauthentication; this specific refresh-interruption contract is from the hybrid controller. |
| refresh queued during a refused mutation does not leave an unusable password confirmation | **Branch regression goal:** actual queued realtime refusal/confirmation recovery. Main does not queue these Overview/Members realtime refreshes. |

## Audit against pinned main

All 29 removed hybrid unit case names were checked against main source. These source groups account for all 25 non-binding cases above:

| Main source | Removed hybrid assertions grounded in that feature |
| --- | --- |
| `ProjectNew.svelte`, `ProjectForm.svelte` | Identifier validation; created project survives failed group assignment. |
| `LabelManager.svelte`, `labelColors.ts` | Safe custom/stored colors; maintainer label gate; merge success retains target and removes source. |
| `ProjectSettings.svelte`, `ProjectMembers.svelte`, `projectRole.svelte.ts`, `reauth.ts` | Viewer management gate; role write/refusal; group assignment rollback; recent-auth grant retry; returned identifier navigation; joined member metadata. General late-account mutation guards and the four event/refresh contracts are stronger hybrid assertions, rather than proof main implements each race. |
| `Layout.svelte` | Optimistic project reorder restores canonical order after refusal. |
| `archiveImport.svelte.ts` | File validation; upload progress/processing/uncertain outcome; retry/report references; persisted unknown outcome until project list check. |
| `ArchiveTransferPanel.svelte`, `api.ts`, `projectArchives.test.ts` | Archive account/session cancellation; ZIP filename/format; acknowledgement and fresh owner verification; archive capability gate even when general authorization is off. |
| `ImportPanel.svelte` | GitHub configure/preview/confirmed import with repository, optional token and status mapping. Immutable pending-preview snapshots are a stronger hybrid assertion. |

The actual main frontend inventory below retains its own original source case names. The administration tables instead preserve the names of tests removed with the hybrid controller in branch commit `3941adf9`.

## Branch-only repository binding disposition

The binding controls were introduced by branch commit `a75fc57c` in `src/topcoat/project_settings/assets/project-settings.js`, then removed in `3941adf9`. The four unit cases below came from that controller’s `project-settings.test.js`. Pinned main has no repository binding references in `web/src`, `web/tests` or `e2e`; its Overview source is `web/src/routes/ProjectSettings.svelte`. Backend repository APIs and tables do not establish an Overview UI parity requirement.

| Removed hybrid case | Disposition |
| --- | --- |
| repository bindings preserve conflicts and use canonical records for successful add and removal | Branch-only removed binding controls; no pinned-main frontend parity requirement. |
| viewer binding writes and stale responses cannot change repository state | Branch-only removed binding controls; no pinned-main frontend parity requirement. |
| root binding preserves the canonical first-parent root commit alias | Branch-only removed binding controls; no pinned-main frontend parity requirement. |
| binding writes require a lead or admin even when general role enforcement is off | Branch-only removed binding controls; no pinned-main frontend parity requirement. |

## Removed administration browser cases

| Removed hybrid case | Disposition |
| --- | --- |
| project downloads retain their object URL until deferred cleanup | Native archive and ZIP cases require actual URL revocation at least 1000ms after creation and no remaining download anchor. Immediate revocation must fail. |
| headless project administration edits the shared identity and rolls back failed group assignment | Native smoke retains actual name save and successful group assignment. **Partial:** failed assignment selection rollback and pending response after account replacement. |
| headless archive import keeps uncertain outcome blocked until the project list is checked | Retained archive import browser case. |
| project publication, repository bindings, archive transfer and deletion complete through the mounted browser UI | Publication acknowledgement/success in native smoke; archive download in new native cases. The binding portion was branch-only UI and has no main parity requirement. The retained archive import browser case covers authenticated multipart submission, report links and Import another. **Open:** publication refusal/retry and actual deletion refusal then success/navigation. Exact delete confirmation/cancel remains in native smoke. |

## Main frontend inventory translations

Keep each original source and every original case name in its inventory. Retarget individual cases rather than loading removed JavaScript.

- `preserves hex colors and rejects CSS source`: native label model must preserve `#12aBcF` exactly and turn `red; background-image: url(https://example.test)` into `#6B7280`.
- `renders unsafe stored label colors through the component fallback`: native malicious-color test retains all three original port assertions: fallback equality, injected value absent, `background-image` absent. This maps the model boundary assertion; it does not claim rendered DOM coverage.
- `archive fetch returns bytes without saving and forwards cancellation and session`: **partial** mapping to native archive bytes/filename/acknowledgement/owner checks. The actual mounted fetch observes AbortSignal, no-store, current cookie and no save while fetching. Document disposal cancellation, same-user session replacement and no save during owner verification remain incomplete.
- `session observers see same-tab refresh and logout and unsubscribe cleanly`: retained live shared session adapter on unfinished routes. First/refreshed/null notifications and no further notifications after unsubscribe remain asserted.

### Archive transport assertions still required

A genuine native browser replacement must observe the real fetch and forward its actual response. It must prove:

1. The mounted native archive URL is correct and actual HTTP-only session cookie authenticates it. This replaces the old REST URL and Bearer-header transport; neither may be silently omitted.
2. The actual fetch receives `cache: 'no-store'` and a real signal; disposing the document aborts that same signal and the actual request.
3. No Blob URL, anchor click or download event occurs while archive bytes or fresh owner verification are pending.
4. Project navigation, account replacement and same-user session replacement cancel or refuse a stale save; each results in zero download events and no leftover anchor.
5. Successful verified bytes match the real native HTTP response and archive filename, then the URL survives through click and receives deferred cleanup.

The archive transport case observes the native mounted URL, actual current cookie, no-store, a mounted AbortSignal and no save while fetching. The successful archive case compares saved bytes with an observation of the real fetch response and proves deferred cleanup and a fresh owner request. The account-replacement case is a separate executable assertion. Document disposal aborting the same signal/request, navigation and same-user session replacement, and no save throughout pending owner verification remain unproven. Session-store unit observers do not establish native download cancellation. Existing pinned-main archive cancellation tests do not establish native implementation behavior.
