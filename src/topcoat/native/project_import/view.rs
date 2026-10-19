use topcoat::{
    context::Cx,
    runtime::{Event, expr, shard},
    view::{Attributes, BoxView, ViewExt, view},
};

use super::super::super::runtime::string::StrUnicodeExt;
use super::{
    model::{ImportResult, MAX_EXPANDED_BYTES, MAX_UPLOAD_BYTES},
    state::State,
};

pub(crate) fn region<'a>(
    cx: &'a Cx,
    _route: &super::super::super::shell::ParsedRoute<'_>,
    account: i64,
    caller: &super::super::context::Caller,
) -> topcoat::Result<BoxView<'a>> {
    let user = super::super::session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    let current_user = super::super::session::read(
        cx,
        crate::db::queries::users::get_user_by_id(&*super::super::context::db(cx).read()?, user.id),
    )?;
    let state = State::new(
        cx,
        account,
        super::state::session_fingerprint(caller.session_token.as_deref()),
    );
    let can_import = caller.session_token.is_some()
        && current_user.is_admin
        && !current_user.is_bot
        && current_user.is_active
        && !state.fingerprint.is_empty();
    let content = content(cx, &state, can_import);
    let content = view! {
        cx =>
        (content)
        (super::transport::owner(cx, state))
    }
    .boxed();
    let topbar = topbar(cx);
    Ok(super::super::home_shell::page_region(
        cx,
        content,
        Some(topbar),
        String::new(),
    ))
}

pub(crate) fn screen<'a>(
    cx: &'a Cx,
    route: &super::super::super::shell::ParsedRoute<'_>,
) -> topcoat::Result<BoxView<'a>> {
    super::super::workspace::common_screen(cx, route)
}

fn topbar(cx: &Cx) -> BoxView<'_> {
    let destination = super::super::transport::mounted_url(cx, "/projects/new");
    view! {
        cx =>
        <div class="flex flex-wrap items-center gap-3 px-6 py-2">
            <a class="toolbar-pill" href=(destination)>"← New project"</a>
            <span class="text-body-sm text-[var(--text-muted)]">"Import archive"</span>
        </div>
    }
    .boxed()
}

pub(super) fn content<'a>(cx: &'a Cx, state: &State, can_import: bool) -> BoxView<'a> {
    let state = state.clone();
    let phase = state.phase.clone();
    let result = state.result.clone();
    let max_upload_mib = pretty_mib_limit(MAX_UPLOAD_BYTES);
    let max_expanded_mib = pretty_mib_limit(MAX_EXPANDED_BYTES);
    let reset = reset_attributes(cx, &state);

    view! {
        cx =>
        <div class="h-full overflow-y-auto">
            <div class="mx-auto w-full max-w-[680px] px-6 py-8 space-y-5 break-words">
                <h1 class="text-xl font-semibold text-[var(--text)]">
                    "Import project archive"
                </h1>
                if !can_import {
                    <p role="alert" class="text-body-sm text-[var(--text-muted)]">
                        "Only a signed-in instance admin can import a project archive."
                    </p>
                } else {
                    <div
                        :hidden=$(if phase.get() == "success" {
                            true
                        } else {
                            phase.get() == "unknown"
                        })
                    >
                        (form(cx, &state, max_upload_mib, max_expanded_mib))
                    </div>
                    <div :hidden=$(phase.get() != "unknown")>(unknown(cx, &state))</div>
                    <div :hidden=$(phase.get() != "success")>
                        archive_import_report(
                            account: state.account,
                            fingerprint: state.fingerprint.clone(),
                            result: $(result.get())
                        )
                        <button class="toolbar-pill" type="button" (reset)>
                            "Import another archive"
                        </button>
                    </div>
                }
            </div>
        </div>
    }
    .boxed()
}

fn reset_attributes(cx: &Cx, state: &State) -> Attributes {
    let phase = state.phase.clone();
    let progress = state.progress.clone();
    let error = state.error.clone();
    let result = state.result.clone();
    let file_name = state.file_name.clone();
    let file_size = state.file_size.clone();
    let file_error = state.file_error.clone();
    let confirmed = state.confirmed.clone();
    let storage_key = format!("lific:native-archive-import-pending:{}", state.account);
    let handler = expr!(|_event: Event| {
        phase.set("idle".to_owned());
        progress.set(None);
        error.set("".to_owned());
        result.set(None::<ImportResult>);
        file_name.set("".to_owned());
        file_size.set(0_usize);
        file_error.set("".to_owned());
        confirmed.set(false);
        raw!(
            "try { sessionStorage.removeItem(${storage_key}.toString()); } catch {}",
            ()
        );
        raw!(
            "const file=document.querySelector('input[data-native-project-import-file]');if(file)file.value='';",
            ()
        );
        raw!(
            "const token=document.nativeArchiveImportTransfer;if(token){token.terminal=null;token.phase='idle';token.progress=undefined;}",
            ()
        );
    });
    let (_, handler) = handler.into_evaluated_and_js();
    let mut attributes = Attributes::with_capacity(1);
    attributes.insert(cx, "data-topcoat-on:click", handler);
    attributes
}

fn form<'a>(
    cx: &'a Cx,
    state: &State,
    max_upload_mib: String,
    max_expanded_mib: String,
) -> BoxView<'a> {
    let file_name = state.file_name.clone();
    let file_size = state.file_size.clone();
    let confirmed = state.confirmed.clone();
    let phase = state.phase.clone();
    let progress = state.progress.clone();
    let error = state.error.clone();
    let file_error = state.file_error.clone();
    let confirmation_text = "I understand this imports linked files, history and deleted content that may contain sensitive information.";
    let max_upload = MAX_UPLOAD_BYTES;
    let usize_width = usize::BITS;
    let max_upload_error = format!(
        "This archive exceeds the {} MiB web upload limit. Use the CLI for larger archives.",
        MAX_UPLOAD_BYTES / (1024 * 1024)
    );
    let change_file_name = file_name.clone();
    let change_file_size = file_size.clone();
    let change_confirmed = confirmed.clone();
    let change_error = error.clone();
    let change_phase = phase.clone();
    let change_file_error = file_error.clone();
    let file_change = expr!(|_event: Event| {
        let name = raw!(
            "cx.hydrate(${_event}.inner.target.files?.[0]?.name ?? '')",
            "".to_owned()
        );
        let bytes = raw!(
            "cx.hydrate({t:'usize',bits:Number(${usize_width}.toString()),v:String(${_event}.inner.target.files?.[0]?.size ?? 0)})",
            0_usize
        );
        change_file_name.set(name.clone());
        change_file_size.set(bytes);
        change_confirmed.set(false);
        change_error.set("".to_owned());
        if change_phase.get() == "error" {
            change_phase.set("idle".to_owned());
        }
        let validation = if name.is_empty() {
            "".to_owned()
        } else if !name.to_uppercase().ends_with(".TAR.GZ") {
            "Choose a Lific project archive ending in .tar.gz.".to_owned()
        } else if bytes == 0 {
            "This file is empty. Choose a Lific project archive.".to_owned()
        } else if bytes > max_upload {
            max_upload_error.clone()
        } else {
            "".to_owned()
        };
        change_file_error.set(validation);
    });
    let (_, file_change) = file_change.into_evaluated_and_js();
    let mut file_change_attributes = Attributes::with_capacity(1);
    file_change_attributes.insert(cx, "data-topcoat-on:change", file_change);

    view! {
        cx =>
        <p class="text-body-sm text-[var(--text-muted)]">
            "Create a private project from a Lific archive. You become its lead. Accounts and permissions do not transfer; imported author names are text only."
        </p>
        <p class="text-body-sm text-[var(--text-muted)]">
            "The source stays untouched. The archive's project identifier must be unused here: import never merges or overwrites a project."
        </p>
        <form data-native-project-import-form="" class="space-y-4">
            <div class="space-y-2">
                <label
                    for="project-archive"
                    class="block text-body-sm font-medium text-[var(--text)]"
                >
                    "Project archive (.tar.gz)"
                </label>
                <input
                    id="project-archive"
                    name="archive"
                    type="file"
                    accept=".tar.gz,application/gzip"
                    data-native-project-import-file=""
                    aria-describedby="archive-limits"
                    :disabled=$(if phase.get() == "uploading" {
                        true
                    } else {
                        phase.get() == "processing"
                    })
                    (file_change_attributes)
                    class="block w-full min-w-0 text-body-sm text-[var(--text-muted)] file:mr-3 file:rounded-md file:border file:border-[var(--border)] file:bg-[var(--bg-subtle)] file:px-3 file:py-2 file:text-[var(--text)]"
                />
                <p id="archive-limits" class="text-caption text-[var(--text-muted)]">
                    "Up to "
                    (max_upload_mib)
                    " MiB compressed, "
                    (max_expanded_mib)
                    " MiB expanded. Larger archives need the CLI."
                </p>
                <p
                    :hidden=$(file_name.get().is_empty())
                    class="text-body-sm text-[var(--text)] [overflow-wrap:anywhere]"
                >
                    $(file_name.get())
                    " ("
                    <span data-native-project-import-file-size="">
                        $({
                            let _bytes = file_size.get();
                            raw!(
                                "cx.hydrate((Number(${_bytes}.toString()) / 1048576).toLocaleString())",
                                "0".to_owned(),
                            )
                        })
                    </span>
                    " MiB)"
                </p>
                <p
                    role="alert"
                    :hidden=$(file_error.get().is_empty())
                    class="text-body-sm text-[var(--error)]"
                >
                    $(file_error.get())
                </p>
            </div>
            <label class="flex items-start gap-2 text-body-sm text-[var(--text)]">
                <input
                    type="checkbox"
                    :checked=$(confirmed.get())
                    :disabled=$(if phase.get() == "uploading" {
                        true
                    } else if phase.get() == "processing" {
                        true
                    } else if file_name.get().is_empty() {
                        true
                    } else {
                        !file_error.get().is_empty()
                    })
                    @change=$(|event: Event| confirmed.set(event.target.checked))
                    class="mt-1 accent-[var(--accent)]"
                />
                (confirmation_text)
            </label>
            <p
                role="alert"
                :hidden=$(error.get().is_empty())
                class="text-body-sm text-[var(--error)]"
            >
                $(error.get())
            </p>
            <div
                :hidden=$(if phase.get() == "uploading" {
                    false
                } else {
                    phase.get() != "processing"
                })
                role="status"
                class="space-y-2 text-body-sm text-[var(--text-muted)]"
            >
                <p>
                    <span :hidden=$(phase.get() != "processing")>"Importing..."</span>
                    <span :hidden=$(phase.get() == "processing")>
                        "Uploading... "
                        <span :hidden=$(progress.get().is_none())>
                            $(if progress.get().is_some() {
                                progress.get().unwrap()
                            } else {
                                0_usize
                            })
                            "%"
                        </span>
                    </span>
                </p>
                <progress
                    :hidden=$(phase.get() != "uploading")
                    aria-label="Archive upload"
                    max="100"
                    :value=$(if progress.get().is_some() {
                        progress.get().unwrap()
                    } else {
                        0_usize
                    })
                    class="w-full accent-[var(--accent)]"
                ></progress>
                <p>
                    "Once processing starts, it cannot be canceled. You can return to this page to see the result. If the connection is lost, check the project list before importing again."
                </p>
            </div>
            <button
                type="submit"
                :disabled=$(if phase.get() == "uploading" {
                    true
                } else if phase.get() == "processing" {
                    true
                } else if file_name.get().is_empty() {
                    true
                } else if !file_error.get().is_empty() {
                    true
                } else if !confirmed.get() {
                    true
                } else if phase.get() == "unknown" {
                    true
                } else {
                    phase.get() == "success"
                })
                class="bg-[var(--accent)] text-[var(--accent-text)] rounded-md px-3 py-2 text-body-sm font-medium disabled:opacity-40 disabled:cursor-not-allowed"
            >
                $(if phase.get() == "uploading" {
                    "Import in progress"
                } else if phase.get() == "processing" {
                    "Import in progress"
                } else {
                    "Import as private project"
                })
            </button>
        </form>
    }
    .boxed()
}

fn unknown<'a>(cx: &'a Cx, state: &State) -> BoxView<'a> {
    let settings = super::super::transport::mounted_url(cx, "/settings");
    let error = state.error.clone();
    let reset = reset_attributes(cx, state);
    view! {
        cx =>
        <p role="alert" class="text-body-sm text-[var(--error)]">
            $(if error.get().is_empty() {
                "An earlier import may have completed. Check the project list before importing again.".to_owned(

                )
            } else {
                error.get()
            })
        </p>
        <a class="toolbar-pill" href=(settings)>"Check project list"</a>
        <div>
            <button class="text-body-sm text-[var(--text-muted)] underline" (reset)>
                "I've checked the project list"
            </button>
        </div>
    }
    .boxed()
}

#[shard("/__native_project_import/report")]
async fn archive_import_report(
    cx: &Cx,
    account: i64,
    fingerprint: String,
    result: Option<ImportResult>,
) -> topcoat::Result<impl topcoat::view::View> {
    let caller = super::super::session::read(cx, super::super::context::caller(cx))?;
    let user = super::super::session::read(cx, crate::api::require_user(&caller.identity))?;
    let current_user = super::super::session::read(
        cx,
        crate::db::queries::users::get_user_by_id(&*super::super::context::db(cx).read()?, account),
    )?;
    if user.id != account
        || caller.session_token.is_none()
        || !current_user.is_admin
        || current_user.is_bot
        || !current_user.is_active
        || super::state::session_fingerprint(caller.session_token.as_deref()) != fingerprint
    {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    let Some(imported) = result else {
        return Ok(view! {
            cx =>
            <p role="alert">
                "The import result is unavailable. Check the project list."
            </p>
        }
        .boxed());
    };
    let total_rows = imported
        .report
        .rows
        .iter()
        .map(|row| row.count)
        .sum::<usize>();
    let project_url = super::super::transport::mounted_url(
        cx,
        &format!("/{}/overview", imported.project.identifier),
    );
    let identifier = imported.project.identifier.clone();
    let rows = imported.report.rows.clone();
    let references = imported.report.external_references.clone();
    let hidden_reference_count = imported
        .report
        .external_reference_count
        .saturating_sub(imported.report.external_references.len());
    let external_reference_count =
        super::super::numbers::count(cx, imported.report.external_reference_count as i64);
    let shown_count =
        super::super::numbers::count(cx, imported.report.external_references.len() as i64);
    let total_count = super::super::numbers::count(cx, total_rows as i64);
    let blobs_count = super::super::numbers::count(cx, imported.report.blobs as i64);
    let rows = rows
        .into_iter()
        .map(|row| {
            let table = row.table.replace('_', " ");
            let count = super::super::numbers::count(cx, row.count as i64);
            view! {
                cx =>
                <div class="flex justify-between gap-4">
                    <dt>(table)</dt>
                    <dd>(count)</dd>
                </div>
            }
            .boxed()
        })
        .collect::<Vec<_>>();
    let record_label = if total_rows == 1 { "record" } else { "records" };
    let file_label = if imported.report.blobs == 1 {
        "file"
    } else {
        "files"
    };

    Ok(view! {
        cx =>
        <div role="status" class="space-y-2">
            <h2 class="text-body-sm font-semibold text-[var(--text)]">
                (identifier)
                " imported"
            </h2>
            <p class="text-body-sm text-[var(--text-muted)]">
                "The new project is private. You are its lead. The source project is unchanged."
            </p>
            <p class="text-body-sm text-[var(--text)]">
                (total_count)
                " "
                (record_label)
                " and "
                (blobs_count)
                " "
                (file_label)
                " imported."
            </p>
        </div>
        <details class="text-body-sm text-[var(--text-muted)]">
            <summary class="cursor-pointer">"Record counts"</summary>
            <dl class="mt-2 space-y-1">
                for row in rows.into_iter() {
                    (row)
                }
            </dl>
        </details>
        if imported.report.external_reference_count > 0 {
            <section class="space-y-2" aria-labelledby="unresolved-heading">
                <h2
                    id="unresolved-heading"
                    class="text-body-sm font-semibold text-[var(--text)]"
                >
                    (external_reference_count)
                    " unresolved "
                    (if imported.report.external_reference_count == 1 {
                        "reference"
                    } else {
                        "references"
                    })
                </h2>
                <p class="text-body-sm text-[var(--text-muted)]">
                    "These references point outside the archive. Review them before relying on the imported links."
                </p>
                <ul
                    class="list-disc pl-5 space-y-1 text-body-sm text-[var(--text-muted)] [overflow-wrap:anywhere]"
                >
                    for reference in references.into_iter() {
                        <li>(reference)</li>
                    }
                </ul>
                if hidden_reference_count > 0 {
                    <p class="text-caption text-[var(--text-muted)]">
                        "Showing the first "
                        (shown_count)
                        ". The full report is retained with the project archive provenance."
                    </p>
                }
            </section>
        } else {
            <p class="text-body-sm text-[var(--text-muted)]">
                "No unresolved references."
            </p>
        }
        <div class="flex flex-wrap gap-3">
            <a
                class="bg-[var(--accent)] text-[var(--accent-text)] rounded-md px-3 py-2 text-body-sm font-medium"
                href=(project_url)
            >
                "Open imported project"
            </a>
        </div>
    }.boxed())
}

fn pretty_mib_limit(bytes: usize) -> String {
    (bytes / (1024 * 1024)).to_string()
}
