//! Native project Files rendering and interaction state.
use super::super::collation::BrowserCollation;
use super::super::{context, dates, icons, session, transport};
use super::actions::delete as delete_file;
use super::model;
use crate::db::models::{PendingOrphan, ProjectAttachment, ProjectAttachmentPage};
use std::collections::HashSet;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, emit, live, view},
};

const MIME_FILTERS: [(&str, &str); 8] = [
    ("", "All"),
    ("image", "Images"),
    ("video", "Video"),
    ("audio", "Audio"),
    ("text", "Text"),
    ("pdf", "PDF"),
    ("archive", "Archives"),
    ("other", "Other"),
];
const SORT_OPTIONS: [(&str, &str); 3] = [
    ("created_at", "Newest first"),
    ("size", "Largest first"),
    ("filename", "Name A to Z"),
];

type History = (
    Signal<String>,
    Signal<String>,
    Signal<usize>,
    Signal<bool>,
    Signal<i64>,
    Signal<i64>,
);
type Controls = (
    Signal<Option<String>>,
    Signal<String>,
    Signal<String>,
    Signal<i64>,
    Signal<Option<i64>>,
    Signal<usize>,
    Signal<usize>,
    Signal<Option<i64>>,
    Signal<bool>,
    (Signal<String>, Signal<bool>, Signal<String>, Signal<String>),
);
type FilesInput = (
    Option<String>,
    String,
    String,
    i64,
    Option<i64>,
    usize,
    usize,
    Option<i64>,
    bool,
    String,
    bool,
    String,
);

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CachedEntity {
    entity_type: String,
    entity_id: i64,
    identifier: Option<String>,
    title: String,
    page_id: Option<i64>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedAttachment {
    id: i64,
    filename: String,
    mime: String,
    mime_class: String,
    size_bytes: i64,
    uploader_id: Option<i64>,
    uploader: Option<String>,
    uploader_display_name: Option<String>,
    created_at: String,
    entities: Vec<CachedEntity>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CachedDuplicate {
    attachment_id: i64,
    filename: String,
    entities: Vec<CachedEntity>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CachedWhereUsed {
    entities: Vec<CachedEntity>,
    duplicates: Vec<CachedDuplicate>,
}

fn cached_entity(entity: &crate::db::models::LinkedEntity) -> CachedEntity {
    CachedEntity {
        entity_type: entity.entity_type.clone(),
        entity_id: entity.entity_id,
        identifier: entity.identifier.clone(),
        title: entity.title.clone(),
        page_id: entity.page_id,
    }
}

fn linked_entity(entity: CachedEntity) -> crate::db::models::LinkedEntity {
    crate::db::models::LinkedEntity {
        entity_type: entity.entity_type,
        entity_id: entity.entity_id,
        identifier: entity.identifier,
        title: entity.title,
        page_id: entity.page_id,
    }
}

fn cached_where_used(value: &crate::services::files::WhereUsed) -> CachedWhereUsed {
    CachedWhereUsed {
        entities: value.entities.iter().map(cached_entity).collect(),
        duplicates: value
            .duplicates
            .iter()
            .map(|duplicate| CachedDuplicate {
                attachment_id: duplicate.attachment_id,
                filename: duplicate.filename.clone(),
                entities: duplicate.entities.iter().map(cached_entity).collect(),
            })
            .collect(),
    }
}

fn linked_where_used(value: CachedWhereUsed) -> crate::services::files::WhereUsed {
    crate::services::files::WhereUsed {
        entities: value.entities.into_iter().map(linked_entity).collect(),
        duplicates: value
            .duplicates
            .into_iter()
            .map(|duplicate| crate::services::files::DuplicateFile {
                attachment_id: duplicate.attachment_id,
                filename: duplicate.filename,
                entities: duplicate.entities.into_iter().map(linked_entity).collect(),
            })
            .collect(),
    }
}

pub(super) fn content<'a>(
    cx: &'a Cx,
    account: i64,
    snapshot: crate::services::files::Snapshot,
) -> BoxView<'a> {
    let project = snapshot.project;
    let identifier = project.identifier.clone();
    let target = (
        account,
        project.id,
        identifier.clone(),
        snapshot.user.id,
        snapshot.user.is_admin,
        snapshot.authority.can_edit_content,
    );
    let owner = cx.keyed(format!("native-files-{account}-{}", project.id));
    let mime = signal(&owner, || None::<String>);
    let uploader = signal(&owner, String::new);
    let sort = signal(&owner, || "created_at".to_owned());
    let offset = signal(&owner, || 0_i64);
    let expanded = signal(&owner, || None::<i64>);
    let revision = signal(&owner, || 0_usize);
    let orphan_revision = signal(&owner, || 0_usize);
    let confirming = signal(&owner, || None::<i64>);
    let deleting = signal(&owner, || false);
    let delete_error = signal(&owner, String::new);
    let orphan_open = signal(&owner, || false);
    let history_rows = signal(&owner, String::new);
    let history_key = signal(&owner, String::new);
    let has_more = signal(&owner, || snapshot.page.has_more);
    let total_count = signal(&owner, || snapshot.page.total_count);
    let total_bytes = signal(&owner, || snapshot.page.total_bytes);
    let links_cache = signal(&owner, || "{}".to_owned());
    let collation = signal(&owner, || {
        r#"{"locale":"en-US","usage":"sort","sensitivity":"variant","ignorePunctuation":false,"collation":"default","numeric":false,"caseFirst":"false"}"#.to_owned()
    });
    let clock = signal(&owner, || chrono::Utc::now().timestamp_millis() as f64);
    let clock_attrs = dates::clock_mount(cx, clock);
    let live = live_refresh(
        cx,
        account,
        project.id,
        revision.clone(),
        offset.clone(),
        deleting.clone(),
    );
    let mount = mount_refresh(cx, revision.clone(), offset.clone(), deleting.clone());
    let target_for_body = target;
    let history = (
        history_rows,
        history_key,
        signal(&owner, || 0_usize),
        has_more,
        total_count.clone(),
        total_bytes.clone(),
    );
    let controls = (
        mime.clone(),
        uploader.clone(),
        sort.clone(),
        offset.clone(),
        expanded.clone(),
        revision.clone(),
        orphan_revision.clone(),
        confirming.clone(),
        orphan_open.clone(),
        (
            collation.clone(),
            deleting.clone(),
            links_cache.clone(),
            delete_error,
        ),
    );
    view! {
        cx =>
        <div
            class="native-files h-full min-h-0 flex flex-col leading-[1.6] text-[var(--text)]"
            data-native-files=""
            (mount)
        >
            (live)
            <span hidden="hidden" (clock_attrs)></span>
            <div class="flex items-center gap-3 px-8 py-3 border-b border-[var(--border)]">
                <span class="text-body-sm font-mono font-medium text-[var(--text-muted)]">(identifier.clone())</span>
                <span class="text-body-sm font-medium text-[var(--text)]">"Files"</span>
                <span class="text-micro text-[var(--text-faint)] tabular-nums">$(total_count.get())</span>
                <span class="text-caption text-[var(--text-faint)] tabular-nums">(format_bytes(total_bytes.get())) " total"</span>
            </div>
            files_body(
                target: target_for_body,
                history: history,
                controls: controls,
                input: $({
                    (
                        mime.get(),
                        uploader.get(),
                        sort.get(),
                        offset.get(),
                        expanded.get(),
                        revision.get(),
                        orphan_revision.get(),
                        confirming.get(),
                        orphan_open.get(),
                        collation.get(),
                        deleting.get(),
                        links_cache.get(),
                    )
                })
            )
        </div>
    }.boxed()
}

#[shard("/__native_files/body")]
async fn files_body(
    cx: &Cx,
    target: (i64, i64, String, i64, bool, bool),
    history: History,
    controls: Controls,
    input: FilesInput,
) -> topcoat::Result<impl View> {
    let (account, project_id, identifier, _viewer_id, _is_admin, _can_edit) = target;
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(topcoat::router::error::forbidden().into());
    }
    let (
        mime_filter,
        uploader_filter,
        selected_sort,
        page_offset,
        expanded_id,
        revision_value,
        _orphan_revision_value,
        confirming_id,
        orphans_open,
        collation_wire,
        _deleting_value,
        links_cache_wire,
    ) = input;
    if !["created_at", "size", "filename"].contains(&selected_sort.as_str()) {
        return Err(topcoat::router::error::bad_request("invalid Files sort").into());
    }
    let key = format!(
        "{}|{}|{}",
        mime_filter.as_deref().unwrap_or(""),
        uploader_filter,
        selected_sort
    );
    let previous_key = history.1.get_untracked();
    let previous_rows = parse_rows(&history.0.get_untracked());
    let previous_revision = history.2.get_untracked();
    let append = previous_key == key && page_offset > 0 && previous_revision == revision_value;
    let query = crate::db::models::ProjectAttachmentQuery {
        mime_class: mime_filter,
        uploader: (!uploader_filter.is_empty()).then_some(uploader_filter),
        sort: Some(selected_sort),
        limit: Some(model::PAGE_SIZE),
        offset: Some(page_offset),
        ..Default::default()
    };
    let page = match crate::services::files::list_project_files(
        context::db(cx),
        &caller.identity,
        project_id,
        &query,
    ) {
        Ok(page) => page,
        Err(error) => {
            return Ok(error_state(cx, &identifier, &error.to_string(), controls.5));
        }
    };
    let mut rows = if append { previous_rows } else { Vec::new() };
    let existing = rows.iter().map(|row| row.id).collect::<HashSet<_>>();
    rows.extend(
        page.items
            .iter()
            .filter(|row| !existing.contains(&row.id))
            .cloned(),
    );
    let orphans = match crate::services::files::list_project_orphans(
        context::db(cx),
        &caller.identity,
        project_id,
    ) {
        Ok(orphans) => Some(orphans),
        Err(error) => {
            tracing::warn!(error=%error,"native Files orphan list failed");
            None
        }
    };
    let mut cache = serde_json::from_str::<
        std::collections::BTreeMap<String, Option<CachedWhereUsed>>,
    >(&links_cache_wire)
    .unwrap_or_default();
    let where_used = expanded_id.and_then(|id| {
        let key = id.to_string();
        if let Some(value) = cache.get(&key) {
            return value.as_ref().map(|item| linked_where_used(item.clone()));
        }
        match crate::services::files::where_used(context::db(cx), &caller.identity, id) {
            Ok(value) => {
                cache.insert(key, Some(cached_where_used(&value)));
                Some(value)
            }
            Err(error) => {
                tracing::debug!(error=%error,"native Files where-used detail unavailable");
                cache.insert(key, None);
                None
            }
        }
    });
    let collator =
        BrowserCollation::from_wire(&collation_wire).and_then(|value| value.comparator());
    let mut uploaders = rows
        .iter()
        .filter_map(|row| row.uploader.clone())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    uploaders.sort_by(|left, right| match &collator {
        Ok(collator) => collator.compare(left, right),
        Err(_) => left.cmp(right),
    });
    uploaders.dedup();
    let authority = match crate::services::project_authority::load(
        context::db(cx),
        &caller.identity,
        project_id,
    ) {
        Ok(authority) => authority,
        Err(error) => {
            return Ok(error_state(cx, &identifier, &error.to_string(), controls.5));
        }
    };
    let now = signal(cx, || chrono::Utc::now().timestamp_millis() as f64);
    let body = files_page(
        cx,
        account,
        project_id,
        &identifier,
        user.id,
        user.is_admin,
        authority.can_edit_content,
        &rows,
        &page,
        orphans.as_ref(),
        &uploaders,
        expanded_id,
        where_used.as_ref(),
        confirming_id,
        orphans_open,
        controls.9.1.clone(),
        now,
        controls.0.clone(),
        controls.1.clone(),
        controls.2.clone(),
        controls.3.clone(),
        controls.4.clone(),
        controls.5,
        controls.6.clone(),
        controls.7.clone(),
        controls.8.clone(),
        controls.9.0.clone(),
        controls.9.3.clone(),
    );
    let rows_wire = serde_json::to_string(
        &rows
            .iter()
            .map(|item| CachedAttachment {
                id: item.id,
                filename: item.filename.clone(),
                mime: item.mime.clone(),
                mime_class: item.mime_class.clone(),
                size_bytes: item.size_bytes,
                uploader_id: item.uploader_id,
                uploader: item.uploader.clone(),
                uploader_display_name: item.uploader_display_name.clone(),
                created_at: item.created_at.clone(),
                entities: item
                    .entities
                    .iter()
                    .map(|entity| CachedEntity {
                        entity_type: entity.entity_type.clone(),
                        entity_id: entity.entity_id,
                        identifier: entity.identifier.clone(),
                        title: entity.title.clone(),
                        page_id: entity.page_id,
                    })
                    .collect(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".to_owned());
    let cache_wire = serde_json::to_string(&cache).unwrap_or_else(|_| "{}".to_owned());
    let page_has_more = page.has_more;
    let page_total_count = page.total_count;
    let page_total_bytes = page.total_bytes;
    let links_cache = controls.9.2;
    let persist = view! {
        cx =>
        <span hidden="hidden" data-native-files-complete=""
            @mount=$(|_event: Event| {
                history.0.set(rows_wire.clone());
                history.1.set(key.clone());
                history.2.set(revision_value);
                if history.3.get() != page_has_more { history.3.set(page_has_more); }
                if history.4.get() != page_total_count { history.4.set(page_total_count); }
                if history.5.get() != page_total_bytes { history.5.set(page_total_bytes); }
                if links_cache.get() != cache_wire { links_cache.set(cache_wire.clone()); }
            })
        ></span>
        (body)
    }
    .boxed();
    Ok(persist)
}

fn parse_rows(value: &str) -> Vec<ProjectAttachment> {
    serde_json::from_str::<Vec<CachedAttachment>>(value)
        .unwrap_or_default()
        .into_iter()
        .map(|item| ProjectAttachment {
            id: item.id,
            filename: item.filename,
            mime: item.mime,
            mime_class: item.mime_class,
            size_bytes: item.size_bytes,
            uploader_id: item.uploader_id,
            uploader: item.uploader,
            uploader_display_name: item.uploader_display_name,
            created_at: item.created_at,
            entities: item
                .entities
                .into_iter()
                .map(|entity| crate::db::models::LinkedEntity {
                    entity_type: entity.entity_type,
                    entity_id: entity.entity_id,
                    identifier: entity.identifier,
                    title: entity.title,
                    page_id: entity.page_id,
                })
                .collect(),
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn files_page<'a>(
    cx: &'a Cx,
    account: i64,
    project_id: i64,
    project: &str,
    viewer_id: i64,
    is_admin: bool,
    can_edit: bool,
    rows: &[ProjectAttachment],
    page: &ProjectAttachmentPage,
    orphans: Option<&crate::db::models::PendingOrphanList>,
    uploaders: &[String],
    expanded_id: Option<i64>,
    where_used: Option<&crate::services::files::WhereUsed>,
    confirming_id: Option<i64>,
    orphans_open: bool,
    deleting: Signal<bool>,
    now: Signal<f64>,
    mime: Signal<Option<String>>,
    uploader: Signal<String>,
    sort: Signal<String>,
    offset: Signal<i64>,
    expanded: Signal<Option<i64>>,
    revision: Signal<usize>,
    orphan_revision: Signal<usize>,
    confirming: Signal<Option<i64>>,
    orphan_open: Signal<bool>,
    collation: Signal<String>,
    delete_error: Signal<String>,
) -> BoxView<'a> {
    let chips = MIME_FILTERS.iter().map(|(value, label)| {
        let selected = mime.clone();
        let chip_offset = offset.clone();
        let chip_confirming = confirming.clone();
        let value = value.to_string();
        let active = mime.get().as_deref().unwrap_or("") == value;
        let class = if active {
            "text-caption px-2.5 py-1 rounded-full border border-[var(--accent)] text-[var(--accent)] bg-[var(--accent-subtle)] font-medium"
        } else {
            "text-caption px-2.5 py-1 rounded-full border border-[var(--border)] text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)]"
        };
        view! { cx =>
            <button type="button" class=(class) :aria-pressed=$(if active { "true" } else { "false" })
                @click=$(move |_event: Event| {
                    selected.set(if value.is_empty() { None } else { Some(value.clone()) });
                    chip_offset.set(0_i64);
                    chip_confirming.set(None);
                })
            >(label)</button>
        }.boxed()
    }).collect::<Vec<_>>();
    let uploader_options = uploaders
        .iter()
        .map(|name| {
            let name = name.clone();
            view! { cx => <option value=(name.clone())>(name.clone())</option> }.boxed()
        })
        .collect::<Vec<_>>();
    let file_rows = rows
        .iter()
        .map(|row| {
            let detail = if expanded_id == Some(row.id) {
                where_used
            } else {
                None
            };
            file_row(
                cx,
                account,
                project,
                viewer_id,
                is_admin,
                can_edit,
                row,
                expanded_id,
                detail,
                confirming_id,
                deleting.clone(),
                now.clone(),
                expanded.clone(),
                confirming.clone(),
                revision.clone(),
                orphan_revision.clone(),
                offset.clone(),
                delete_error.clone(),
            )
        })
        .collect::<Vec<_>>();
    let orphan_rows = orphans
        .map(|list| {
            list.items
                .iter()
                .map(|orphan| {
                    orphan_row(
                        cx,
                        account,
                        project_id,
                        viewer_id,
                        is_admin,
                        can_edit,
                        orphan,
                        confirming_id,
                        deleting.clone(),
                        confirming.clone(),
                        revision.clone(),
                        orphan_revision.clone(),
                        offset.clone(),
                        delete_error.clone(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let total_count = page.total_count;
    let no_rows = rows.is_empty();
    let total_bytes = format_bytes(page.total_bytes);
    let count_label = format!(
        "{total_count} file{}",
        if total_count == 1 { "" } else { "s" }
    );
    let more = page.has_more;
    let next_offset = offset.get() + model::PAGE_SIZE;
    let current_sort = sort.get();
    let current_uploader = uploader.get();
    let expanded_class = if orphans_open { "block" } else { "hidden" };
    let orphan_count = orphans.map_or(0, |value| value.items.len());
    let orphan_bytes = orphans.map_or(0, |value| value.total_bytes);
    let orphan_error = orphans.is_none();
    let cache_signal = collation;
    let delete_error_text = delete_error.get();
    let sort_options = SORT_OPTIONS
        .iter()
        .map(|(value, label)| {
            let value = (*value).to_owned();
            let label = (*label).to_owned();
            view! { cx => <option value=(value)>(label)</option> }.boxed()
        })
        .collect::<Vec<_>>();
    view! {
        cx =>
        <div class="h-full flex flex-col">
            <div class="flex-1 overflow-y-auto">
                <div class="px-8 py-6 max-w-[1100px] mx-auto">
                    <div class="flex flex-wrap items-center gap-2 mb-5">
                        for chip in chips { (chip) }
                        <div class="flex-1"></div>
                        <select class="h-7 px-2 rounded-md text-caption bg-[var(--bg)] border border-[var(--border)] text-[var(--text-muted)]"
                            aria-label="Filter by uploader" value=(current_uploader)
                            @change=$(|event: Event| {
                                uploader.set(event.target.value);
                                offset.set(0_i64);
                                confirming.set(None);
                            })>
                            <option value="">"All uploaders"</option>
                            for option in uploader_options { (option) }
                        </select>
                        <select class="h-7 px-2 rounded-md text-caption bg-[var(--bg)] border border-[var(--border)] text-[var(--text-muted)]"
                            aria-label="Sort files" value=(current_sort)
                            @change=$(|event: Event| {
                                sort.set(event.target.value);
                                offset.set(0_i64);
                                confirming.set(None);
                            })>
                            for option in sort_options { (option) }
                        </select>
                    </div>
                    <div class="flex items-baseline gap-2 mb-3">
                        <span class="text-body-sm text-[var(--text)] font-medium tabular-nums">(count_label)</span>
                        <span class="text-caption text-[var(--text-faint)] tabular-nums">(total_bytes)</span>
                    </div>
                    if !delete_error_text.is_empty() {
                        <p class="text-caption text-[var(--error)] mb-3" role="status">(delete_error_text)</p>
                    }
                    <div class="flex flex-col divide-y divide-[var(--border)]">for row in file_rows { (row) }</div>
                    if no_rows {
                        <div class="flex flex-col items-center py-20 gap-3 text-center">
                            (icons::ui_icon(cx, icons::UiIcon::Files, 32))
                            <p class="text-body-lg text-[var(--text-muted)]">"No files here yet"</p>
                            <p class="text-body-sm text-[var(--text-faint)] max-w-[420px]">"Files appear once they are attached to an issue, page, or comment in this project."</p>
                        </div>
                    }
                    if more {
                        <div class="flex justify-center py-4">
                            <button class="text-body-sm text-[var(--text-muted)] border border-[var(--border)] px-3 py-1.5 rounded-md hover:bg-[var(--bg-subtle)] transition-colors"
                                type="button" @click=$(move |_event: Event| offset.set(next_offset))>"Load more"</button>
                        </div>
                    }
                    <section class="mt-10 border-t border-[var(--border)] pt-4">
                        <button class="w-full flex items-center gap-2 text-left" type="button"
                            :aria-expanded=$(if orphans_open { "true" } else { "false" })
                            @click=$(move |_event: Event| orphan_open.set(!orphans_open))>
                            (icons::ui_icon(cx, if orphans_open { icons::UiIcon::Expand } else { icons::UiIcon::Next }, 14))
                            (icons::ui_icon(cx, icons::UiIcon::Warning, 14))
                            <span class="text-body-sm font-medium text-[var(--text)]">"Pending cleanup"</span>
                            <span class="text-caption text-[var(--text-faint)] tabular-nums">(orphan_count.to_string())</span>
                            if orphan_count > 0 { <span class="text-caption text-[var(--text-faint)] tabular-nums">"· "(format_bytes(orphan_bytes))</span> }
                        </button>
                        <div class=(expanded_class)>
                            <p class="text-caption text-[var(--text-muted)] mt-2 mb-3 max-w-[560px]">"Uploads by this project's members that were never attached to anything. The server deletes them automatically once their grace window runs out."</p>
                            if orphan_error {
                                <div class="flex items-center gap-2 text-caption text-[var(--error)]">
                                    <span>"Pending cleanup could not be loaded."</span>
                                    <button type="button" class="underline" @click=$(move |_event: Event| orphan_revision.increment())>"Retry"</button>
                                </div>
                            }
                            if orphan_count == 0 && !orphan_error { <p class="text-caption text-[var(--text-faint)]">"Nothing waiting to be swept."</p> }
                            <div class="flex flex-col divide-y divide-[var(--border)]">for row in orphan_rows { (row) }</div>
                        </div>
                    </section>
                    <span hidden="hidden" data-native-files-collation=""
                        @mount=$(|_event: Event| {
                            let resolved = raw!(
                                "cx.hydrate((() => { const r = new Intl.Collator(undefined,{usage:'sort'}).resolvedOptions(); return JSON.stringify({locale:r.locale,usage:r.usage,sensitivity:r.sensitivity,ignorePunctuation:r.ignorePunctuation,collation:r.collation,numeric:r.numeric,caseFirst:r.caseFirst}); })())",
                                String::new(),
                            );
                            if cache_signal.get() != resolved { cache_signal.set(resolved); }
                        })
                    ></span>
                </div>
            </div>
        </div>
    }.boxed()
}

#[allow(clippy::too_many_arguments)]
fn file_row<'a>(
    cx: &'a Cx,
    account: i64,
    project: &str,
    viewer_id: i64,
    is_admin: bool,
    can_edit: bool,
    row: &ProjectAttachment,
    expanded_id: Option<i64>,
    detail: Option<&crate::services::files::WhereUsed>,
    confirming_id: Option<i64>,
    deleting: Signal<bool>,
    now: Signal<f64>,
    expanded: Signal<Option<i64>>,
    confirming: Signal<Option<i64>>,
    revision: Signal<usize>,
    orphan_revision: Signal<usize>,
    offset: Signal<i64>,
    delete_error: Signal<String>,
) -> BoxView<'a> {
    let id = row.id;
    let filename = row.filename.clone();
    let expanded_value = expanded_id == Some(id);
    let destination = transport::mounted_url(cx, &format!("/__native_files/download/{id}"));
    let entities = detail.map_or(row.entities.as_slice(), |detail| detail.entities.as_slice());
    let row_entity_chips = entity_chips(cx, project, entities);
    let expanded_entity_chips = entity_chips(cx, project, entities);
    let can_delete = model::can_delete(row.uploader_id, Some(viewer_id), is_admin, can_edit);
    let label = if expanded_value {
        "Hide where this is used"
    } else {
        "Show where this is used"
    };
    let mime_icon = mime_icon(cx, &row.mime_class);
    let size = format_bytes(row.size_bytes);
    let uploader = row.uploader.clone().unwrap_or_else(|| "unknown".into());
    let row_mime = row.mime.clone();
    let time = dates::relative_time_view(cx, &row.created_at, now);
    let deleting_value = deleting.get();
    let confirming_signal = confirming.clone();
    let busy_signal = deleting.clone();
    let error_signal = delete_error.clone();
    let failed_busy = deleting.clone();
    let failed_confirming = confirming.clone();
    let failed_error = delete_error;
    let success_busy = deleting;
    let success_confirming = confirming.clone();
    let success_expanded = expanded.clone();
    let success_offset = offset;
    let success_revision = revision;
    let success_orphan_revision = orphan_revision;
    let delete_handler = expr!(|_event: Event| {
        if !busy_signal.get() {
            busy_signal.set(true);
            error_signal.set("".to_owned());
            let _failed = || {
                failed_busy.set(false);
                failed_confirming.set(None);
                failed_error.set("Couldn't delete this file. Try again.".to_owned());
            };
            let _delete = async || {
                delete_file(account, id).await;
                success_busy.set(false);
                success_confirming.set(None);
                success_expanded.set(None);
                success_offset.set(0_i64);
                success_revision.increment();
                success_orphan_revision.increment();
            };
            raw!(
                "Promise.resolve().then(()=>${_delete}()).catch(()=>${_failed}());",
                ()
            );
        }
    });
    let mut delete_handler_attrs = Attributes::with_capacity(1);
    delete_handler_attrs.insert(
        cx,
        "data-topcoat-on:click",
        delete_handler.into_evaluated_and_js().1,
    );
    let duplicates = detail.map(|data| data.duplicates.iter().map(|duplicate| {
            let duplicate_name = duplicate.filename.clone();
            let duplicate_chips = entity_chips(cx, project, &duplicate.entities);
            view! { cx =>
            <div class="flex items-center gap-2"><span class="text-caption text-[var(--text-muted)] truncate">(duplicate_name)</span><div class="flex flex-wrap items-center gap-1">for chip in duplicate_chips { (chip) }</div></div>
            }.boxed()
        }).collect::<Vec<_>>()).unwrap_or_default();
    let has_duplicates = !duplicates.is_empty();
    let is_confirming = confirming_id == Some(id);
    let delete_confirmation_message = model::delete_confirm_message(entities.len());
    view! { cx =>
        <div class="mt-2 ml-8 flex flex-col gap-2">
            if expanded_value {
            <div class="flex flex-col gap-1">
                <span class="text-micro uppercase tracking-widest text-[var(--text-faint)] font-semibold">"Used by"</span>
                <div class="flex flex-wrap items-center gap-1">for chip in expanded_entity_chips { (chip) }</div>
            </div>
                if has_duplicates {
                    <div class="flex flex-col gap-1"><span class="text-micro uppercase tracking-widest text-[var(--text-faint)] font-semibold">"Identical file also attached to"</span>for duplicate in duplicates { (duplicate) }</div>
                }
                <span class="text-micro text-[var(--text-faint)]">(row_mime)</span>
            }
            if is_confirming {
            <div class="flex flex-wrap items-center gap-2 mt-2 ml-8 pl-3 border-l-2 border-[var(--error)]">
                <span class="text-caption text-[var(--text-muted)]">(delete_confirmation_message)</span>
                <button type="button" class="text-caption font-medium px-2 py-1 rounded-md text-[var(--error-text)] bg-[var(--error)] hover:opacity-90"
                    :disabled=$(deleting_value)
                    (delete_handler_attrs)>
                    $(if deleting_value { "Deleting…" } else { "Delete" })
                </button>
                <button type="button" class="text-caption text-[var(--text-muted)] px-2 py-1 rounded-md hover:bg-[var(--bg-subtle)]"
                    @click=$(move |_event: Event| confirming_signal.set(None))>"Cancel"</button>
            </div>
            }
        <div class="py-2" data-native-files-row=(id.to_string())>
            <div class="flex items-center gap-3">
                <button type="button" class="size-5 flex items-center justify-center rounded text-[var(--text-faint)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors shrink-0"
                    title=(label) :aria-expanded=$(if expanded_value { "true" } else { "false" })
                    @click=$(move |_event: Event| {
                        expanded.set(if expanded_value { None } else { Some(id) });
                    })>
                    (icons::ui_icon(cx, if expanded_value { icons::UiIcon::Expand } else { icons::UiIcon::Next }, 14))
                </button>
                (mime_icon)
                <a class="min-w-0 flex-1 text-left text-body-sm text-[var(--text)] truncate hover:text-[var(--accent)] transition-colors"
                    href=(destination) download=(filename.clone()) title=(format!("Download {filename}"))>(filename.clone())</a>
                <div class="hidden sm:block shrink-0"><div class="flex flex-wrap items-center gap-1">for chip in row_entity_chips { (chip) }</div></div>
                <span class="text-caption text-[var(--text-faint)] tabular-nums w-16 text-right shrink-0">(size)</span>
                <span class="hidden md:block text-caption text-[var(--text-muted)] w-24 truncate shrink-0">(uploader)</span>
                <span class="text-caption text-[var(--text-faint)] w-16 text-right shrink-0">(time)</span>
                if can_delete {
                    <button type="button" class="size-6 flex items-center justify-center rounded shrink-0 text-[var(--text-faint)] hover:text-[var(--error)] hover:bg-[var(--bg-subtle)]"
                        title=(format!("Delete {filename}")) :disabled=$(deleting_value)
                        @click=$(move |_event: Event| confirming.set(if is_confirming { None } else { Some(id) }))>
                        (icons::ui_icon(cx, icons::UiIcon::Delete, 14))
                    </button>
                }
            </div>
        </div>
        </div>
    }.boxed()
}

fn entity_chips<'a>(
    cx: &'a Cx,
    project: &str,
    entities: &[crate::db::models::LinkedEntity],
) -> Vec<BoxView<'a>> {
    entities.iter().map(|entity| {
        let label = model::entity_chip_label(entity);
        let path = model::entity_href(project, entity);
        let title = entity.title.clone();
        match path {
            Some(path) => {
                let attributes = super::super::navigation::attrs(cx, &path);
                view! { cx => <a class="text-micro font-mono px-1.5 py-0.5 rounded bg-[var(--bg-subtle)] text-[var(--text-muted)] hover:text-[var(--accent)] transition-colors" (attributes) title=(title)>(label)</a> }.boxed()
            }
            None => view! { cx => <span class="text-micro font-mono px-1.5 py-0.5 rounded bg-[var(--bg-subtle)] text-[var(--text-muted)]" title=(title)>(label)</span> }.boxed(),
        }
    }).collect()
}

#[allow(clippy::too_many_arguments)]
fn orphan_row<'a>(
    cx: &'a Cx,
    account: i64,
    project_id: i64,
    viewer_id: i64,
    is_admin: bool,
    can_edit: bool,
    orphan: &PendingOrphan,
    confirming_id: Option<i64>,
    deleting: Signal<bool>,
    confirming: Signal<Option<i64>>,
    revision: Signal<usize>,
    orphan_revision: Signal<usize>,
    offset: Signal<i64>,
    delete_error: Signal<String>,
) -> BoxView<'a> {
    let id = orphan.id;
    let can_delete = model::can_delete(orphan.uploader_id, Some(viewer_id), is_admin, can_edit);
    let filename = orphan.filename.clone();
    let size_bytes = orphan.size_bytes;
    let countdown = model::sweep_countdown(orphan.seconds_until_sweep);
    let uploader = orphan.uploader.clone().unwrap_or_else(|| "unknown".into());
    let confirm = confirming_id == Some(id);
    let delete = confirming.clone();
    let busy = deleting.get();
    let delete_busy = deleting.clone();
    let error_signal = delete_error.clone();
    let failed_busy = deleting.clone();
    let failed_confirming = confirming.clone();
    let failed_error = delete_error;
    let success_busy = deleting;
    let success_confirming = confirming.clone();
    let success_offset = offset;
    let success_revision = revision;
    let success_orphan_revision = orphan_revision;
    let delete_handler = expr!(|_event: Event| {
        if !delete_busy.get() {
            delete_busy.set(true);
            error_signal.set("".to_owned());
            let _failed = || {
                failed_busy.set(false);
                failed_confirming.set(None);
                failed_error.set("Couldn't delete this file. Try again.".to_owned());
            };
            let _delete = async || {
                delete_file(account, id).await;
                success_busy.set(false);
                success_confirming.set(None);
                success_offset.set(0_i64);
                success_revision.increment();
                success_orphan_revision.increment();
            };
            raw!(
                "Promise.resolve().then(()=>${_delete}()).catch(()=>${_failed}());",
                ()
            );
        }
    });
    let mut delete_handler_attrs = Attributes::with_capacity(1);
    delete_handler_attrs.insert(
        cx,
        "data-topcoat-on:click",
        delete_handler.into_evaluated_and_js().1,
    );
    let row = view! { cx =>
        <div class="flex items-center gap-3 py-2">
            (icons::ui_icon(cx, icons::UiIcon::Warning, 14))
            <span class="min-w-0 flex-1 text-body-sm text-[var(--text-muted)] truncate">(filename.clone())</span>
            <span class="text-caption text-[var(--text-faint)] tabular-nums w-16 text-right shrink-0">(format_bytes(size_bytes))</span>
            <span class="hidden md:block text-caption text-[var(--text-muted)] w-24 truncate shrink-0">(uploader)</span>
            <span class="text-caption w-40 text-right shrink-0 text-[var(--text-muted)]">(countdown)</span>
            if can_delete {
                <button type="button" class="size-6 flex items-center justify-center rounded shrink-0 text-[var(--text-faint)] hover:text-[var(--error)]"
                    title=(format!("Delete {filename} now")) :disabled=$(busy)
                    @click=$(move |_event: Event| delete.set(if confirm { None } else { Some(id) }))>
                    (icons::ui_icon(cx, icons::UiIcon::Delete, 14))
                </button>
            }
        </div>
        if confirm {
            <div class="flex flex-wrap items-center gap-2 py-2 pl-3 border-l-2 border-[var(--error)]">
                <span class="text-caption text-[var(--text-muted)]">(model::delete_confirm_message(0))</span>
                <button type="button" class="text-caption font-medium px-2 py-1 rounded-md text-[var(--error-text)] bg-[var(--error)]"
                    :disabled=$(busy)
                    (delete_handler_attrs)>$(if busy { "Deleting…" } else { "Delete" })</button>
                <button type="button" class="text-caption text-[var(--text-muted)] px-2 py-1 rounded-md"
                    @click=$(move |_event: Event| confirming.set(None))>"Cancel"</button>
            </div>
        }
    };
    let _ = project_id;
    row.boxed()
}

fn mime_icon<'a>(cx: &'a Cx, mime_class: &str) -> BoxView<'a> {
    match mime_class {
        "image" | "video" | "audio" => icons::ui_icon(cx, icons::UiIcon::Files, 16),
        "text" | "pdf" => icons::ui_icon(cx, icons::UiIcon::Page, 16),
        "archive" => icons::ui_icon(cx, icons::UiIcon::Module, 16),
        _ => icons::ui_icon(cx, icons::UiIcon::Entity, 16),
    }
}

fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    if bytes < 1024 * 1024 * 1024 {
        return format!("{:.1} MB", bytes as f64 / 1_048_576.0);
    }
    format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
}

fn error_state<'a>(
    cx: &'a Cx,
    project: &str,
    message: &str,
    revision: Signal<usize>,
) -> BoxView<'a> {
    let label = format!("Couldn't load files for {project}: {message}");
    view! { cx =>
        <div class="flex flex-col items-center py-14 gap-3 text-center" data-native-files-error="">
            <p class="text-body-sm text-[var(--text-muted)]">(label)</p>
            <button type="button" class="text-body-sm font-medium text-[var(--btn-success-text)] bg-[var(--btn-success)] px-3 py-1.5 rounded-md hover:bg-[var(--btn-success-hover)]"
                @click=$(move |_event: Event| revision.increment())>"Try again"</button>
        </div>
    }.boxed()
}

fn mount_refresh(
    cx: &Cx,
    revision: Signal<usize>,
    offset: Signal<i64>,
    deleting: Signal<bool>,
) -> topcoat::view::Attributes {
    let handler = topcoat::runtime::expr!(|_event: Event| {
        let _refresh = || {
            let visible = raw!("cx.hydrate(!document.hidden)", false);
            if visible {
                if !deleting.get() {
                    offset.set(0_i64);
                    revision.increment();
                }
            }
        };
        raw!(
            "window.addEventListener('focus',()=>${_refresh}(),{signal:cx.abortSignal});document.addEventListener('visibilitychange',()=>${_refresh}(),{signal:cx.abortSignal});",
            ()
        );
    });
    let mut attrs = topcoat::view::Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:mount",
        handler.into_evaluated_and_js().1,
    );
    attrs
}

fn live_refresh(
    cx: &Cx,
    account: i64,
    project: i64,
    revision: Signal<usize>,
    offset: Signal<i64>,
    deleting: Signal<bool>,
) -> BoxView<'_> {
    let context = cx.clone();
    let mut events = topcoat::context::app_context::<crate::realtime::RealtimeHub>(cx).subscribe();
    let connected = super::super::super::runtime::connected(cx);
    live! {
        cx =>
        let token = emit! { <span hidden="hidden" data-native-files-live=""></span> }?;
        if !connected { return Ok(token); }
        loop {
            let relevant = match events.recv().await {
                Ok(message) => matches!(message.event, crate::realtime::RealtimeEvent::ResyncRequired)
                    || message.event.project_id() == Some(project),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => true,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(token),
            };
            if relevant {
                let caller = super::super::session::read_for_refresh(&context, super::super::context::caller(&context))?;
                let user = super::super::session::read_for_refresh(&context, crate::api::require_user(&caller.identity))?;
                if user.id != account { return Err(topcoat::router::error::forbidden().into()); }
                let _changed = emit! {
                    <span hidden="hidden" @mount=$(|_event: Event| {
                        if !deleting.get() {
                            offset.set(0_i64);
                            revision.increment();
                        }
                    })></span>
                }?;
            }
        }
    }.boxed()
}
