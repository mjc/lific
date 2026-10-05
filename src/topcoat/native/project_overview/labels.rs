//! Native label vocabulary: create, rename, recolor, delete, merge and usage.
use super::super::{context, icons, session, transport};
use super::labels_actions::{mutate, normalize_color, stamp_filter};
use super::{collation, labels_model as model};
use crate::db::models::Label;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, shard, signal},
    view::{Attributes, BoxView, View, ViewExt, view},
};
// Count, revision, error, edit/draft, confirmation/merge, busy row, creating.
type OwnerSignals = (
    Signal<usize>,
    Signal<usize>,
    Signal<String>,
    Signal<i64>,
    Signal<String>,
    Signal<i64>,
    Signal<String>,
    Signal<i64>,
    Signal<bool>,
);

type PaletteSignals = (Signal<String>, Signal<String>, Signal<bool>, Signal<bool>);

#[derive(Clone)]
struct State {
    account: i64,
    project: i64,
    count: Signal<usize>,
    revision: Signal<usize>,
    filter: Signal<String>,
    sort: Signal<String>,
    collation: Signal<String>,
    name: Signal<String>,
    color: Signal<String>,
    touched: Signal<bool>,
    duplicate: Signal<bool>,
    creating: Signal<bool>,
    busy_id: Signal<i64>,
    error: Signal<String>,
    editing: Signal<i64>,
    draft: Signal<String>,
    confirming: Signal<i64>,
    merge: Signal<String>,
}
#[allow(clippy::too_many_arguments)]
fn mutation(
    cx: &Cx,
    state: &State,
    command: &str,
    id: i64,
    value: Signal<String>,
    color: Signal<String>,
    touched: Signal<bool>,
    event: &str,
) -> Attributes {
    let account = state.account;
    let project = state.project;
    let command = command.to_owned();
    let keyboard = event == "keydown";
    let revision = state.revision.clone();
    let count = state.count.clone();
    let error = state.error.clone();
    let busy = state.busy_id.clone();
    let creating = state.creating.clone();
    let name = state.name.clone();
    let create_color = state.color.clone();
    let create_touched = state.touched.clone();
    let editing = state.editing.clone();
    let confirming = state.confirming.clone();
    let merge = state.merge.clone();
    let failed_busy = busy.clone();
    let failed_creating = creating.clone();
    let failed_error = error.clone();
    let handler = expr!(async |event: Event| {
        if if keyboard {
            event.key == "Escape"
        } else {
            false
        } {
            event.prevent_default();
            editing.set(0_i64);
        } else if if !keyboard {
            true
        } else {
            event.key == "Enter"
        } {
            let rename = command == "rename";
            let allowed = if rename { editing.get() == id } else { true };
            if if allowed {
                if !creating.get() {
                    busy.get() == 0_i64
                } else {
                    false
                }
            } else {
                false
            } {
                if keyboard {
                    event.prevent_default();
                }
                if rename {
                    editing.set(0_i64);
                }
                let original = value.get();
                let original_color = color.get();
                let original_touched = touched.get();
                error.set("".to_owned());
                busy.set(id);
                if if command == "create" {
                    true
                } else {
                    command == "presets"
                } {
                    creating.set(true);
                }
                let _failed = || {
                    failed_busy.set(0_i64);
                    failed_creating.set(false);
                    failed_error.set("Couldn't save the label. Try again.".to_owned());
                };
                let _run = async || {
                    let result = mutate(
                        account,
                        project,
                        command.clone(),
                        id,
                        original,
                        original_color,
                        original_touched,
                    )
                    .await;
                    busy.set(0_i64);
                    creating.set(false);
                    if result.0.is_ok() {
                        count.set(result.1);
                        revision.increment();
                        if if command == "create" {
                            id == 0_i64
                        } else {
                            false
                        } {
                            name.set("".to_owned());
                            create_color.set("#6B7280".to_owned());
                            create_touched.set(false);
                        }
                        if if command == "delete" {
                            true
                        } else {
                            command == "merge"
                        } {
                            confirming.set(0_i64);
                            merge.set("".to_owned());
                        }
                    } else {
                        error.set(result.0.unwrap_err());
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        format!("data-topcoat-on:{event}"),
        handler.into_evaluated_and_js().1,
    );
    attrs
}
pub(super) fn panel<'a>(
    cx: &'a Cx,
    account: i64,
    project: i64,
    identifier: &str,
    initial_count: usize,
    can_edit: bool,
) -> BoxView<'a> {
    let state = State {
        account,
        project,
        count: signal(cx, || initial_count),
        revision: signal(cx, || 0_usize),
        filter: signal(cx, String::new),
        sort: signal(cx, || "name".into()),
        collation: signal(cx, String::new),
        name: signal(cx, String::new),
        color: signal(cx, || "#6B7280".into()),
        touched: signal(cx, || false),
        duplicate: signal(cx, || false),
        creating: signal(cx, || false),
        busy_id: signal(cx, || 0_i64),
        error: signal(cx, String::new),
        editing: signal(cx, || 0_i64),
        draft: signal(cx, String::new),
        confirming: signal(cx, || 0_i64),
        merge: signal(cx, String::new),
    };
    let count = state.count.clone();
    let revision = state.revision.clone();
    let filter = state.filter.clone();
    let sort = state.sort.clone();
    let collation = state.collation.clone();
    let name = state.name.clone();
    let color = state.color.clone();
    let touched = state.touched.clone();
    let creating = state.creating.clone();
    let duplicate = state.duplicate.clone();
    let error = state.error.clone();
    let editing = state.editing.clone();
    let draft = state.draft.clone();
    let confirming = state.confirming.clone();
    let merge = state.merge.clone();
    let busy = state.busy_id.clone();
    let click = mutation(
        cx,
        &state,
        "create",
        0,
        name.clone(),
        color.clone(),
        touched.clone(),
        "click",
    );
    let enter = mutation(
        cx,
        &state,
        "create",
        0,
        name.clone(),
        color.clone(),
        touched.clone(),
        "keydown",
    );
    let identifier = identifier.to_owned();
    view!{cx=><section class="native-overview__labels" @mount=$(|_event:Event|collation.set(raw!("cx.hydrate(JSON.stringify(new Intl.Collator().resolvedOptions()))",String::new())))>
        <div class="native-overview__heading"><h2>(icons::project_icon(cx,Some("lucide:Tag"),14))" Labels "<span :hidden=$(count.get()==0_usize)>$(count.get())</span></h2>
            <div class="native-overview__label-toolbar" :hidden=$(count.get()<=1_usize)><input aria-label="Filter labels" placeholder="Filter…" :hidden=$(count.get()<=8_usize) :value=$(filter.get()) @input=$(|event:Event|filter.set(event.target.value.to_owned())) />
            <span>"Sort"</span>for (value,label) in [("name","A–Z"),("usage","Most used"),("newest","Newest")] {<button type="button" :aria-pressed=$(sort.get()==value) @click=$(|_event:Event|sort.set(value.to_owned()))>(label)</button>}
            </div>
        </div>
        <div class="native-overview__label-card">
            if can_edit{<div class="native-overview__label-create">
                native_overview_label_create_color(account:account,project:project,name:$(name.get()),color:color.clone(),touched:touched.clone(),chosen:$(color.get()),locked:$(touched.get()))
                <input maxlength="40" aria-label="New label name" placeholder="New label name…" :value=$(name.get()) @input=$(|event:Event|name.set(event.target.value.to_owned())) (enter) />
                native_overview_label_preview(account:account,project:project,name:$(name.get()),color:$(color.get()),touched:$(touched.get()),revision:$(revision.get()),duplicate:duplicate.clone())
                <button type="button" class="native-overview__success" :disabled=$(if creating.get(){true}else if name.get().trim().is_empty(){true}else{duplicate.get()}) (click)>(icons::project_icon(cx,Some("lucide:Plus"),14))$(if creating.get(){"Adding…"}else{"Add"})</button>
            </div>}
            <div role="alert" class="native-overview__label-error" :hidden=$(error.get().is_empty())>$(error.get())</div>
            native_overview_label_rows(account:account,project:project,identifier:identifier,can_edit:can_edit,revision:$(revision.get()),filter:$(filter.get()),sort:$(sort.get()),collation:$(collation.get()),owner_state:(count,revision,error,editing,draft,confirming,merge,busy,creating))
        </div>
    </section>}.boxed()
}
fn read(
    cx: &Cx,
    account: i64,
    project: i64,
) -> topcoat::Result<crate::services::project_overview::OverviewReads> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Err(crate::error::LificError::Forbidden(
            "Your account changed. Reload this page.".into(),
        )
        .into());
    }
    session::read(
        cx,
        crate::authz::require_role(
            context::db(cx),
            &caller.identity,
            project,
            crate::db::models::Role::Viewer,
        ),
    )?;
    let project = session::read(cx, {
        let conn = context::db(cx).read()?;
        crate::db::queries::get_project(&conn, project)
    })?;
    session::read(
        cx,
        crate::services::project_overview::load(
            context::db(cx),
            &caller.identity,
            &project.identifier,
        ),
    )
}
use shards::{
    native_overview_label_create_color, native_overview_label_preview, native_overview_label_rows,
};

#[allow(
    clippy::too_many_arguments,
    reason = "Topcoat emits shard handlers with an extra context argument and drops function lint attributes"
)]
mod shards {
    use super::*;

    #[shard("/__native_overview/label_preview")]
    pub(super) async fn native_overview_label_preview(
        cx: &Cx,
        account: i64,
        project: i64,
        name: String,
        color: String,
        touched: bool,
        revision: usize,
        duplicate: Signal<bool>,
    ) -> topcoat::Result<impl View> {
        let _ = revision;
        let reads = read(cx, account, project)?;
        let labels = reads.labels?;
        let name = name.trim_matches(model::js_whitespace).to_owned();
        let color = if touched {
            model::safe_color(&color).to_owned()
        } else {
            model::color_for_name(if name.is_empty() { "label" } else { &name }).to_owned()
        };
        let is_duplicate = model::name_taken(&labels, &name, None);
        Ok(
            view! {cx=><span class="native-overview__label-preview" @mount=$(|_event:Event|duplicate.set(is_duplicate))>if !name.is_empty(){<span class="native-overview__label-chip" style=(format!("color:{color};border-color:{color}40;background:{color}10"))><span style=(format!("background:{color}"))></span>(name.clone())</span>if is_duplicate{<span class="native-overview__duplicate" role="status">(format!("“{name}” already exists."))</span>}}</span>},
        )
    }
    #[shard("/__native_overview/label_create_color")]
    pub(super) async fn native_overview_label_create_color(
        cx: &Cx,
        account: i64,
        project: i64,
        name: String,
        color: Signal<String>,
        touched: Signal<bool>,
        chosen: String,
        locked: bool,
    ) -> topcoat::Result<impl View> {
        read(cx, account, project)?;
        let name = name.trim_matches(model::js_whitespace);
        let display = if locked {
            model::safe_color(&chosen).to_owned()
        } else {
            model::color_for_name(if name.is_empty() { "label" } else { name }).into()
        };
        Ok(color_picker(
            cx,
            (account, project),
            0,
            display,
            color,
            touched,
            None,
        ))
    }
    #[shard("/__native_overview/label_rows")]
    pub(super) async fn native_overview_label_rows(
        cx: &Cx,
        account: i64,
        project: i64,
        identifier: String,
        can_edit: bool,
        revision: usize,
        filter: String,
        sort: String,
        collation: String,
        owner_state: OwnerSignals,
    ) -> topcoat::Result<impl View> {
        let (count, owner_revision, owner_error, editing, draft, confirming, merge, busy, creating) =
            owner_state;
        let _ = revision;
        let reads = read(cx, account, project)?;
        let labels = reads.labels?;
        let pages = reads.pages?;
        let issues = reads.issues?;
        // Fresh capabilities control rendering too; a retained DOM permission is never trusted.
        let can_edit = can_edit
            && super::super::model::capabilities(
                reads.enforced,
                reads.user.is_admin,
                reads.role,
                reads.project.lead_user_id == Some(account),
            )
            .edit;
        let usages = model::usage(&issues, &pages);
        let visible = if collation.is_empty() {
            Vec::new()
        } else {
            collation::visible(
                &labels,
                &usages,
                &filter,
                collation::Sort::parse(&sort)?,
                &collation::BrowserCollation::from_wire(&collation)?,
            )?
        };
        let state = State {
            account,
            project,
            count,
            revision: owner_revision,
            filter: signal(cx, || filter.clone()),
            sort: signal(cx, || sort.clone()),
            collation: signal(cx, || collation.clone()),
            name: signal(cx, String::new),
            color: signal(cx, || model::DEFAULT_COLOR.into()),
            touched: signal(cx, || true),
            duplicate: signal(cx, || false),
            creating,
            busy_id: busy,
            error: owner_error,
            editing,
            draft,
            confirming,
            merge,
        };
        let all = mutation(
            cx,
            &state,
            "presets",
            0,
            signal(cx, String::new),
            signal(cx, String::new),
            signal(cx, || true),
            "click",
        );
        let rows = visible
            .into_iter()
            .map(|label| {
                row(
                    cx,
                    &state,
                    label,
                    &labels,
                    usages.get(&label.name).copied().unwrap_or_default(),
                    &identifier,
                    can_edit,
                )
            })
            .collect::<Vec<_>>();
        Ok(view! {cx=>
            if collation.is_empty(){for _ in 0..3 {<div class="native-overview__label-skeleton" aria-hidden="true"><span></span><span></span><span></span></div>}}
            else if labels.is_empty(){if can_edit{<div class="native-overview__label-empty"><p>"No labels yet. Start from a common set, or create your own above."</p><div>for (name,color) in model::PRESETS {<button type="button" class="native-overview__preset" style=(format!("color:{color};border-color:{color}55;background:{color}12")) (mutation(cx,&state,"create",-1,signal(cx,||name.into()),signal(cx,||color.into()),signal(cx,||true),"click"))>(icons::project_icon(cx,Some("lucide:Plus"),11))(name)</button>}<button type="button" class="native-overview__label-add-all" (all)>"Add all"</button></div></div>}else{<div class="native-overview__empty">"No labels yet."</div>}}
            else if rows.is_empty(){<div class="native-overview__empty">(format!("No labels match “{}”.",filter.trim_matches(model::js_whitespace)))</div>}
            else{for rendered in rows{(rendered)}}
        })
    }
}

fn row<'a>(
    cx: &'a Cx,
    state: &State,
    label: &Label,
    labels: &[Label],
    usage: model::Usage,
    identifier: &str,
    can_edit: bool,
) -> BoxView<'a> {
    let id = label.id;
    let name = label.name.clone();
    let initial = name.clone();
    let color = label.color.clone();
    let color_safe = model::safe_color(&color).to_owned();
    let editing = state.editing.clone();
    let draft = state.draft.clone();
    let confirming = state.confirming.clone();
    let merge = state.merge.clone();
    let busy = state.busy_id.clone();
    let rename_enter = mutation(
        cx,
        state,
        "rename",
        id,
        draft.clone(),
        signal(cx, String::new),
        signal(cx, || true),
        "keydown",
    );
    let rename_blur = mutation(
        cx,
        state,
        "rename",
        id,
        draft.clone(),
        signal(cx, String::new),
        signal(cx, || true),
        "blur",
    );
    let remove = mutation(
        cx,
        state,
        "delete",
        id,
        signal(cx, String::new),
        signal(cx, String::new),
        signal(cx, || true),
        "click",
    );
    let merger = mutation(
        cx,
        state,
        "merge",
        id,
        merge.clone(),
        signal(cx, String::new),
        signal(cx, || true),
        "click",
    );
    let picker = color_picker(
        cx,
        (state.account, state.project),
        id,
        color,
        signal(cx, || color_safe.clone()),
        signal(cx, || true),
        Some(state),
    );
    let account = state.account;
    let project = state.project;
    let key = format!("lific:list:state:{identifier}");
    let destination = transport::mounted_url(cx, &format!("/{identifier}/issues"));
    let error = state.error.clone();
    let _input = format!("native-label-edit-{id}");
    let others = labels
        .iter()
        .filter(|other| other.id != id)
        .map(|other| (other.id.to_string(), other.name.clone()))
        .collect::<Vec<_>>();
    view!{cx=><div class="native-overview__label-row">
        <div class="native-overview__label-display" :hidden=$(confirming.get()==id)>
            if can_edit{(picker)}else{<span class="native-overview__label-dot" style=(format!("background:{color_safe}"))></span>}
            if can_edit{<button type="button" class="native-overview__label-name" :hidden=$(editing.get()==id) @click=$(|_event:Event|{draft.set(initial.clone());editing.set(id);confirming.set(0_i64);raw!("requestAnimationFrame(()=>document.getElementById(${_input}.toString())?.select());",());})>(name.clone())</button>
            <input id=(format!("native-label-edit-{id}")) maxlength="40" aria-label="Rename label" :hidden=$(editing.get()!=id) :value=$(draft.get()) @input=$(|event:Event|draft.set(event.target.value.to_owned())) (rename_enter) (rename_blur) />}
            else{<span class="native-overview__label-name">(name.clone())</span>}
            <button type="button" class="native-overview__label-usage" title=(usage.text()) @click=$(async |_event:Event|{
                let _stored=raw!("cx.hydrate((()=>{try{return localStorage.getItem(${key}.toString())||''}catch{return ''}})())",String::new());
                let _failed=||error.set("Couldn't open the label filter. Try again.".to_owned());
                let _stamp=async ||{let _value=stamp_filter(account,project,_stored,name.clone()).await;raw!("if(!cx.abortSignal.aborted){try{localStorage.setItem(${key}.toString(),${_value}.toString())}catch{}window.location.assign(${destination}.toString())}",());};
                raw!("Promise.resolve().then(()=>${_stamp}()).catch(()=>${_failed}());",());
            })>(if usage.total()>0 {usage.text()}else{"unused".into()})</button>
            if can_edit{<button type="button" aria-label=(format!("Delete {name}")) class="native-overview__label-trash" @click=$(|_event:Event|{confirming.set(id);editing.set(0_i64);merge.set("".to_owned());})>(icons::project_icon(cx,Some("lucide:Trash2"),13))</button>}
        </div>
        if can_edit{<div class="native-overview__label-confirm" :hidden=$(confirming.get()!=id)><span class="native-overview__label-dot" style=(format!("background:{color_safe}"))></span><div><p>"Delete "<strong>(name.clone())</strong>"? "if (usage.total()>0){<span>(format!("Detaches from {}.",usage.text()))</span>}</p>
            if !others.is_empty(){<div class="native-overview__label-merge"><span>"or merge into"</span><select aria-label="Merge into label" :value=$(merge.get()) @change=$(|event:Event|merge.set(event.target.value.to_owned()))><option value="">"Choose label…"</option>for (value,name) in others{<option value=(value)>(name)</option>}</select><button type="button" class="native-overview__accent" :disabled=$(if merge.get().is_empty(){true}else{busy.get()==id}) (merger)>"Merge"</button></div>}
        </div><button type="button" class="native-overview__destructive" :disabled=$(busy.get()==id) (remove)>"Delete"</button><button type="button" @click=$(|_event:Event|confirming.set(0_i64))>"Cancel"</button></div>}
    </div>}.boxed()
}
fn color_picker<'a>(
    cx: &'a Cx,
    owner: (i64, i64),
    id: i64,
    display: String,
    color: Signal<String>,
    touched: Signal<bool>,
    state: Option<&State>,
) -> BoxView<'a> {
    let (account, project) = owner;
    let display = model::safe_color(&display).to_owned();
    let shown = signal(cx, || display.to_ascii_lowercase());
    let palette = model::PALETTE
        .into_iter()
        .map(|(name, value)| (name, value.to_ascii_lowercase()))
        .collect::<Vec<_>>();
    let open = signal(cx, || false);
    let hex = signal(cx, || display.trim_start_matches('#').to_owned());
    let bad = signal(cx, || false);
    let busy = state.map_or_else(|| signal(cx, || 0_i64), |s| s.busy_id.clone());
    let count = state.map_or_else(|| signal(cx, || 0_usize), |s| s.count.clone());
    let revision = state.map_or_else(|| signal(cx, || 0_usize), |s| s.revision.clone());
    let error = state.map_or_else(|| signal(cx, String::new), |s| s.error.clone());
    let root = format!("native-label-color-{project}-{id}");
    let mount_root = root.clone();
    let color_name = model::color_name(&display);
    let picker_state = State {
        account,
        project,
        count,
        revision,
        filter: signal(cx, String::new),
        sort: signal(cx, String::new),
        collation: signal(cx, String::new),
        name: signal(cx, String::new),
        color: color.clone(),
        touched: touched.clone(),
        duplicate: signal(cx, || false),
        creating: signal(cx, || false),
        busy_id: busy,
        error,
        editing: signal(cx, || 0_i64),
        draft: signal(cx, String::new),
        confirming: signal(cx, || 0_i64),
        merge: signal(cx, String::new),
    };
    // Hex normalization executes as Rust; palette choices are already validated.
    let selected = color.clone();
    let dirty = touched.clone();
    let picker_error = picker_state.error.clone();
    let picker_revision = picker_state.revision.clone();
    let picker_count = picker_state.count.clone();
    let picker_busy = picker_state.busy_id.clone();
    let failed_busy = picker_busy.clone();
    let failed_error = picker_error.clone();
    let failed_revision = picker_revision.clone();
    let set = expr!(|event: Event| {
        if event.key == "Escape" {
            event.prevent_default();
            open.set(false);
        } else if if event.key == "Enter" {
            true
        } else {
            raw!("cx.hydrate(${event}.type === 'click')", false)
        } {
            event.prevent_default();
            if picker_busy.get() == 0_i64 {
                let entered = hex.get();
                let _failed = || {
                    failed_busy.set(0_i64);
                    failed_error.set("Couldn't save the color. Try again.".to_owned());
                    if id > 0_i64 {
                        failed_revision.increment();
                    }
                };
                let _run = async || {
                    let value = normalize_color(entered).await;
                    if value.is_ok() {
                        let value = value.unwrap();
                        selected.set(value.clone());
                        shown.set(value.clone());
                        dirty.set(true);
                        open.set(false);
                        bad.set(false);
                        if id > 0_i64 {
                            picker_busy.set(id);
                            let result = mutate(
                                account,
                                project,
                                "color".to_owned(),
                                id,
                                value,
                                "".to_owned(),
                                true,
                            )
                            .await;
                            picker_busy.set(0_i64);
                            if result.0.is_ok() {
                                picker_count.set(result.1);
                            } else {
                                picker_error.set(result.0.unwrap_err());
                            }
                            picker_revision.increment();
                        }
                    } else {
                        bad.set(true);
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    })
    .into_evaluated_and_js()
    .1;
    let mut click = Attributes::with_capacity(1);
    click.insert(cx, "data-topcoat-on:click", set.clone());
    let mut keydown = Attributes::with_capacity(1);
    keydown.insert(cx, "data-topcoat-on:keydown", set);
    view!{cx=><div id=(root) class="native-overview__color" @mount=$(|_event:Event|{let _outside=|_event:Event|{if !raw!("cx.hydrate(document.getElementById(${mount_root}.toString())?.contains(${_event}.target)??false)",false){open.set(false);}};raw!("window.addEventListener('click',${_outside},{signal:cx.abortSignal});",());})>
        <button type="button" class="native-overview__color-trigger" :style=$({let _chosen=shown.get();raw!("cx.hydrate('background:'+${_chosen}.toString())",String::new())}) aria-label=(format!("Color: {color_name}. Click to change.")) title=(format!("{color_name} · {display}")) :aria-expanded=$(open.get()) @click=$(|event:Event|{event.stop_propagation();open.set(!open.get());bad.set(false);})></button>
        <div class="native-overview__color-panel" :hidden=$(!open.get()) @keydown=$(|event:Event|event.stop_propagation()) @click=$(|event:Event|event.stop_propagation())><div class="native-overview__color-palette">for (name,value) in palette{<button type="button" style=(format!("background:{value}")) aria-label=(name) title=(name) :aria-pressed=$(shown.get()==value) (palette_choice(cx,&picker_state,id,&value,(color.clone(),shown.clone(),touched.clone(),open.clone())))><span :hidden=$(shown.get()!=value)>(icons::project_icon(cx,Some("lucide:Check"),12))</span></button>}</div><div class="native-overview__color-hex"><span>"#"</span><input aria-label="Custom label hex color" maxlength="7" spellcheck="false" placeholder="hex" :value=$(hex.get()) :aria-invalid=$(bad.get()) @input=$(|event:Event|{hex.set(event.target.value.to_owned());bad.set(false);}) (keydown) /><button type="button" (click)>"Set"</button></div></div>
    </div>}.boxed()
}

fn palette_choice(
    cx: &Cx,
    state: &State,
    id: i64,
    value: &str,
    signals: PaletteSignals,
) -> Attributes {
    let (color, shown, touched, open) = signals;
    let account = state.account;
    let project = state.project;
    let value = value.to_owned();
    let busy = state.busy_id.clone();
    let count = state.count.clone();
    let revision = state.revision.clone();
    let error = state.error.clone();
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let handler = expr!(|_event: Event| {
        color.set(value.clone());
        shown.set(value.clone());
        touched.set(true);
        open.set(false);
        if if id > 0_i64 {
            busy.get() == 0_i64
        } else {
            false
        } {
            busy.set(id);
            error.set("".to_owned());
            let _failed = || {
                failed_busy.set(0_i64);
                failed_error.set("Couldn't save the color. Try again.".to_owned());
            };
            let _run = async || {
                let result = mutate(
                    account,
                    project,
                    "color".to_owned(),
                    id,
                    value,
                    "".to_owned(),
                    true,
                )
                .await;
                busy.set(0_i64);
                if result.0.is_ok() {
                    count.set(result.1);
                    revision.increment();
                } else {
                    error.set(result.0.unwrap_err());
                    revision.increment();
                }
            };
            raw!(
                "Promise.resolve().then(()=>${_run}()).catch(()=>${_failed}());",
                ()
            );
        }
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs
}
