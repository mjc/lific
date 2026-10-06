//! Shared owned project/group rows. Layout affects presentation, never actions.
use super::super::{icons, transport};
use super::{
    model::{Catalog, DESTINATIONS, Destination, EditTarget, Group, Project, State},
    state::{self, Signals},
};
use topcoat::{
    context::Cx,
    runtime::{Event, expr},
    view::{Attributes, BoxView, ViewExt, view},
};

#[derive(Clone, Copy)]
pub(super) enum Layout {
    Desktop,
    Phone,
}

pub(super) fn destinations<'a>(
    cx: &'a Cx,
    project: &Project,
    path: &str,
    layout: Layout,
) -> BoxView<'a> {
    let links = DESTINATIONS.map(|destination| {
        let (slug, label, icon) = destination.row();
        (
            transport::mounted_url(cx, &format!("/{}/{slug}", project.identifier)),
            label,
            icon,
            destination.active(&project.identifier, path),
        )
    });
    let class = match layout {
        Layout::Desktop => "sidebar-destination native-sidebar-destination",
        Layout::Phone => "sidebar-destination native-sidebar-mobile-destination",
    };
    view!{cx=>for(href,label,icon,active)in links{<a class=(class) href=(href) aria-current=(active.then_some("page"))>(icons::project_icon(cx,Some(icon),if matches!(layout,Layout::Phone){20}else{14}))(label)</a>}}.boxed()
}
fn mark<'a>(cx: &'a Cx, project: &Project, size: u32) -> BoxView<'a> {
    if let Some(icon) = project.emoji.as_deref().filter(|icon| !icon.is_empty()) {
        icons::project_icon(cx, Some(icon), size)
    } else {
        let initials = project.identifier.chars().take(2).collect::<String>();
        view! {cx=><span class="sidebar-initials native-sidebar-initials">(initials)</span>}.boxed()
    }
}
fn project_row<'a>(
    cx: &'a Cx,
    project: &Project,
    model: &State,
    owners: (&Signals, &super::recents_state::Signals),
    path: &str,
    layout: Layout,
    phone_action: &dyn Fn(&str) -> Attributes,
) -> BoxView<'a> {
    let (signals, recents) = owners;
    let id = project.id;
    let name = project.name.clone();
    let identifier = project.identifier.clone();
    let current = model
        .catalog
        .projects
        .iter()
        .find(|p| {
            Destination::Overview.active(&p.identifier, path)
                || DESTINATIONS.iter().any(|d| d.active(&p.identifier, path))
        })
        .is_some_and(|p| p.id == id);
    let actions = format!("Actions for {name}");
    let trigger = format!(
        "native-sidebar-project-actions-{id}-{}",
        if matches!(layout, Layout::Phone) {
            "phone"
        } else {
            "desktop"
        }
    );
    let menu = state::open_menu(cx, signals, "project", id, "click");
    let context_menu = state::open_menu(cx, signals, "project", id, "contextmenu");
    let keyboard_menu = state::open_menu(cx, signals, "project", id, "keydown");
    let icon = mark(
        cx,
        project,
        if matches!(layout, Layout::Phone) {
            20
        } else {
            16
        },
    );
    match layout {
        Layout::Desktop => {
            let open = model.expanded.contains(&id) && !model.dragging;
            let disclosure = format!("{} {name}", if open { "Collapse" } else { "Expand" });
            let panel = format!("project-nav-{id}");
            let toggle = state::invoke(cx, signals, "toggle_project", id, String::new(), "click");
            let contents = open.then(|| {
                (
                    destinations(cx, project, path, layout),
                    super::recents_view::slot(cx, recents, project, path, layout),
                )
            });
            let overview = transport::mounted_url(cx, &format!("/{identifier}/overview"));
            let overview_active = path.eq_ignore_ascii_case(&format!("/{identifier}/overview"));
            view!{cx=><div data-native-sidebar-project=(id.to_string())>
                <div class="sidebar-row sidebar-project native-sidebar-project-row">
                    <button class="native-sidebar-project-toggle" aria-label=(disclosure) aria-expanded=(open.to_string()) aria-controls=(panel.clone()) (toggle)>(icons::compact_icon(cx,icons::CompactIcon::ChevronRight,13))</button>
                    <a id=(format!("native-sidebar-project-link-{id}")) href=(overview) data-sidebar-project=(id.to_string()) title=(name.clone()) aria-current=(overview_active.then_some("page")) class="sidebar-project-link native-sidebar-project-link" (context_menu) (keyboard_menu)><span class="sidebar-project-icon">(icon)</span><span>(name.clone())</span></a>
                    <button id=(trigger) class="sidebar-overflow native-sidebar-overflow" aria-label=(actions) aria-haspopup="menu" (menu)>(icons::compact_icon(cx,icons::CompactIcon::Ellipsis,15))</button>
                </div>
                <div id=(panel) hidden=(!open) class="project-subnav">if let Some((links,recent))=contents{(links)(recent)}</div>
            </div>}.boxed()
        }
        Layout::Phone => {
            let open = phone_action(&identifier);
            let label = format!("Open {name} navigation");
            view!{cx=><div class="native-sidebar-phone-row"><button id=(format!("native-sidebar-phone-project-{id}")) class="mobile-project-row native-sidebar-mobile-project" data-current-project=(current.then_some("true")) data-mobile-project-trigger=(identifier.clone()) data-native-project-trigger=(identifier.clone()) aria-label=(label) (open) (context_menu) (keyboard_menu)><span class="mobile-project-icon">(icon)</span><span><span>(name)</span><small>(identifier)</small></span>(icons::compact_icon(cx,icons::CompactIcon::ChevronRight,17))</button><button id=(trigger) class="native-sidebar-phone-actions" aria-label=(actions) aria-haspopup="menu" (menu)>(icons::compact_icon(cx,icons::CompactIcon::Ellipsis,18))</button></div>}.boxed()
        }
    }
}
fn editor<'a>(cx: &'a Cx, model: &State, signals: &Signals, layout: Layout) -> BoxView<'a> {
    let Some(edit) = &model.edit else {
        return view! {cx=>}.boxed();
    };
    let id = if matches!(layout, Layout::Phone) {
        "native-sidebar-phone-group-name"
    } else {
        "native-sidebar-desktop-group-name"
    };
    let error_id = format!("{id}-error");
    let has_error = !edit.error.is_empty();
    let error = edit.error.clone();
    let saving = edit.saving;
    let busy = signals.busy.clone();
    let waiting = expr!(if saving { true } else { busy.get() });
    let draft = signals.draft.clone();
    let save = state::invoke(cx, signals, "save_group", 0, String::new(), "submit");
    let keyboard = state::invoke(cx, signals, "cancel_edit", 0, String::new(), "keydown");
    let cancel = state::invoke(cx, signals, "cancel_edit", 0, String::new(), "click");
    let restore = state::restore_editor_focus(cx, signals, id.to_owned(), !saving);
    view!{cx=><form class="native-sidebar-group-editor" (save) (keyboard)>
        <input id=(id) aria-label="Group name" placeholder="Group name" autocomplete="off" aria-invalid=(has_error.to_string()) aria-describedby=(has_error.then_some(error_id.clone())) :disabled=$(waiting) (restore) :value=$(draft.get()) @input=$(|event:Event|draft.set(event.target.value))>
        if has_error{<p id=(error_id) role="alert">(error)</p>}
        <div><button type="submit" :disabled=$(waiting)> (if saving{"Saving…"}else{"Save"}) </button><button type="button" :disabled=$(waiting) (cancel)>"Cancel"</button></div>
    </form>}.boxed()
}
fn group<'a>(
    cx: &'a Cx,
    group: &Group,
    model: &State,
    owners: (&Signals, &super::recents_state::Signals),
    path: &str,
    layout: Layout,
    phone_action: &dyn Fn(&str) -> Attributes,
) -> BoxView<'a> {
    let (signals, recents) = owners;
    let id = group.id;
    let name = group.name.clone();
    let collapsed = model.collapsed_groups.contains(&id);
    let panel = format!(
        "group-{id}-{}",
        if matches!(layout, Layout::Phone) {
            "phone"
        } else {
            "desktop"
        }
    );
    let editing = model
        .edit
        .as_ref()
        .is_some_and(|edit| edit.target == EditTarget::Existing(id));
    let field = editor(cx, model, signals, layout);
    let toggle = state::invoke(cx, signals, "toggle_group", id, String::new(), "click");
    let context_menu = state::open_menu(cx, signals, "group", id, "contextmenu");
    let menu = state::open_menu(cx, signals, "group", id, "click");
    let actions = format!(
        "Actions for {}{name}",
        if matches!(layout, Layout::Desktop) {
            "group "
        } else {
            ""
        }
    );
    let trigger = format!(
        "native-sidebar-group-actions-{id}-{}",
        if matches!(layout, Layout::Phone) {
            "phone"
        } else {
            "desktop"
        }
    );
    let current = group
        .project_ids
        .iter()
        .find_map(|id| {
            model.catalog.projects.iter().find(|project| {
                project.id == *id
                    && DESTINATIONS
                        .iter()
                        .any(|destination| destination.active(&project.identifier, path))
            })
        })
        .map(|project| project.name.clone());
    let mut rows = Vec::new();
    for id in model.catalog.siblings(Some(id)) {
        if let Some(project) = model
            .catalog
            .projects
            .iter()
            .find(|project| project.id == id)
        {
            rows.push(project_row(
                cx,
                project,
                model,
                (signals, recents),
                path,
                layout,
                phone_action,
            ));
        }
    }
    let show_hint = matches!(layout, Layout::Desktop) && collapsed;
    let hint_toggle = state::invoke(cx, signals, "toggle_group", id, String::new(), "click");
    view!{cx=><section data-native-sidebar-group=(id.to_string())>
        if editing{(field)}else{<div class="sidebar-row native-sidebar-group-heading"><button id=(format!("native-sidebar-group-toggle-{panel}")) class="native-sidebar-group-toggle" aria-expanded=((!collapsed).to_string()) aria-controls=(panel.clone()) title=(name.clone()) (toggle) (context_menu)>(icons::compact_icon(cx,icons::CompactIcon::ChevronRight,if matches!(layout,Layout::Phone){15}else{13}))<span>(name)</span></button><button id=(trigger) class="sidebar-overflow native-sidebar-overflow" aria-label=(actions) aria-haspopup="menu" data-sidebar-group-actions=(id.to_string()) (menu)>(icons::compact_icon(cx,icons::CompactIcon::Ellipsis,if matches!(layout,Layout::Phone){18}else{15}))</button></div>}
        if show_hint{if let Some(name)=current{<button class="native-sidebar-current" (hint_toggle)>(format!("Current: {name}"))</button>}}
        <div id=(panel) class="sidebar-group-projects native-sidebar-group-projects" hidden=(collapsed)>for row in rows{(row)}</div>
    </section>}.boxed()
}
pub(super) fn projects<'a>(
    cx: &'a Cx,
    model: &State,
    signals: &Signals,
    path: &str,
    layout: Layout,
    phone_action: &dyn Fn(&str) -> Attributes,
    recents: &super::recents_state::Signals,
) -> BoxView<'a> {
    let create = state::open_menu(cx, signals, "create", 0, "click");
    let trigger = if matches!(layout, Layout::Phone) {
        "native-sidebar-create-phone"
    } else {
        "native-sidebar-create-desktop"
    };
    let new = model
        .edit
        .as_ref()
        .is_some_and(|edit| matches!(edit.target, EditTarget::New { .. }));
    let field = editor(cx, model, signals, layout);
    let mut groups = Vec::new();
    for row in &model.catalog.groups {
        groups.push(group(
            cx,
            row,
            model,
            (signals, recents),
            path,
            layout,
            phone_action,
        ));
    }
    let mut ungrouped = Vec::new();
    for id in model.catalog.siblings(None) {
        if let Some(project) = model
            .catalog
            .projects
            .iter()
            .find(|project| project.id == id)
        {
            ungrouped.push(project_row(
                cx,
                project,
                model,
                (signals, recents),
                path,
                layout,
                phone_action,
            ));
        }
    }
    let empty = model.catalog.projects.is_empty() && model.catalog.groups.is_empty() && !new;
    let has_groups = !model.catalog.groups.is_empty();
    let error = model.error.clone();
    let transport_error = signals.error.clone();
    let pending_receipt = signals.frozen.clone();
    let confirm = state::invoke(cx, signals, "recover", 0, String::new(), "click");
    let create_group = state::invoke(cx, signals, "new_group", 0, String::new(), "click");
    let create_url = transport::mounted_url(cx, "/projects/new");
    let region = if matches!(layout, Layout::Phone) {
        "phone"
    } else {
        "desktop"
    };
    view!{cx=><div data-native-sidebar-projects="" data-native-sidebar-layout=(region)><div class="sidebar-projects-heading native-sidebar-projects-heading"><span class="sidebar-section-label">"Projects"</span><button id=(trigger) aria-label="New project or group" title="New project or group" aria-haspopup="menu" (create)>(icons::project_icon(cx,Some("lucide:Plus"),if matches!(layout,Layout::Phone){18}else{13}))</button></div>
        <div :hidden=$(transport_error.get().is_empty()) class="native-sidebar-order-error"><p role="alert">$(transport_error.get())</p><button type="button" :hidden=$(pending_receipt.get().is_empty()) (confirm)>"Confirm change"</button><button type="button" @click=$(|_event:Event|{raw!("window.location.reload();",());})>"Reload page"</button></div>
        if new{(field)}if !error.is_empty(){<p role="alert" class="native-sidebar-order-error">(error)</p>}
        for group in groups{(group)}
        <div data-native-sidebar-drag-zone="" class=(if has_groups{"sidebar-ungrouped native-sidebar-ungrouped"}else{"native-sidebar-ungrouped"})>for project in ungrouped{(project)}</div>
        if empty{<div class="native-sidebar-empty"><p>"No projects yet."</p><a href=(create_url)>"Create a project"</a>if matches!(layout,Layout::Desktop){<button (create_group)>"Create a group"</button>}</div>}
    </div>}.boxed()
}

pub(super) fn menu<'a>(
    cx: &'a Cx,
    model: &State,
    signals: &Signals,
    kind: &str,
    id: i64,
) -> BoxView<'a> {
    let mut items = Vec::new();
    let catalog: &Catalog = &model.catalog;
    if kind == "create" {
        items.push((
            "New group".to_owned(),
            "lucide:FolderPlus",
            "new_group",
            0,
            String::new(),
            false,
        ));
    }
    if kind == "group"
        && let Some(group) = catalog.groups.iter().find(|group| group.id == id)
    {
        let order = catalog.group_ids();
        let at = order.iter().position(|group| *group == id).unwrap_or(0);
        items.extend([
            (
                String::from("Move up"),
                "lucide:ArrowUp",
                "group_up",
                id,
                String::new(),
                model.pending() || at == 0,
            ),
            (
                String::from("Move down"),
                "lucide:ArrowDown",
                "group_down",
                id,
                String::new(),
                model.pending() || at + 1 == order.len(),
            ),
            (
                String::from("Rename"),
                "lucide:Pencil",
                "rename_group",
                id,
                group.name.clone(),
                false,
            ),
            (
                String::from("Delete group"),
                "lucide:Trash2",
                "delete_group",
                id,
                String::new(),
                false,
            ),
        ]);
    }
    if kind == "project" && catalog.projects.iter().any(|project| project.id == id) {
        let containing = catalog.containing(id);
        let siblings = catalog.siblings(containing);
        let at = siblings
            .iter()
            .position(|project| *project == id)
            .unwrap_or(0);
        items.extend([
            (
                String::from("Move up"),
                "lucide:ArrowUp",
                "project_up",
                id,
                String::new(),
                model.pending() || at == 0,
            ),
            (
                String::from("Move down"),
                "lucide:ArrowDown",
                "project_down",
                id,
                String::new(),
                model.pending() || at + 1 == siblings.len(),
            ),
        ]);
        for group in &catalog.groups {
            if Some(group.id) != containing {
                items.push((
                    format!("Move to {}", group.name),
                    "lucide:Folder",
                    "assign",
                    id,
                    group.id.to_string(),
                    false,
                ));
            }
        }
        if containing.is_some() {
            items.push((
                String::from("Remove from group"),
                "lucide:FolderMinus",
                "assign",
                id,
                String::new(),
                false,
            ));
        }
        items.push((
            String::from("New group…"),
            "lucide:FolderPlus",
            "new_group",
            id,
            String::new(),
            false,
        ));
    }
    let mut rows = Vec::new();
    for (label, icon, command, target, value, disabled) in items {
        let invoke = state::invoke(cx, signals, command, target, value, "click");
        rows.push(view!{cx=><button role="menuitem" disabled=(disabled) (invoke)>(icons::project_icon(cx,Some(icon),14))(label)</button>}.boxed());
    }
    let new_project = transport::mounted_url(cx, "/projects/new");
    let menu_attributes = state::menu_attributes(cx, signals);
    let x = signals.menu_x.clone();
    let y = signals.menu_y.clone();
    let create = kind == "create";
    view!{cx=><div id="native-sidebar-menu" role="menu" data-native-sidebar-menu="" data-context-menu="" aria-label="Context menu" tabindex="-1" class="native-sidebar-menu" (menu_attributes) :style=$(raw!("cx.hydrate('left:'+${x}.get().toString()+'px;top:'+${y}.get().toString()+'px')",String::new()))>if create{<a role="menuitem" href=(new_project)>(icons::project_icon(cx,Some("lucide:Plus"),14))"New project"</a>}for row in rows{(row)}</div>}.boxed()
}

/// The phone project pane reads the same authorized model as the root tree.
pub(super) fn phone_panels<'a>(
    cx: &'a Cx,
    model: &State,
    path: &str,
    navigation: &super::super::home_shell::MobileNavigation,
    recents: &super::recents_state::Signals,
) -> BoxView<'a> {
    let selected = navigation.handles().2;
    let mut panels = Vec::new();
    for project in &model.catalog.projects {
        let identifier = project.identifier.clone();
        let name = project.name.clone();
        let id = format!("native-mobile-project-{identifier}");
        let back = super::super::home_shell::mobile_action(cx, navigation, "back", String::new());
        let close = super::super::home_shell::mobile_action(cx, navigation, "close", String::new());
        let links = destinations(cx, project, path, Layout::Phone);
        let recent = super::recents_view::slot(cx, recents, project, path, Layout::Phone);
        let selected = selected.clone();
        panels.push(view!{cx=><div id=(id) data-native-mobile-project="" :hidden=$(selected.get()!=identifier)><header class="native-home-mobile-nav-header"><button class="native-home-icon-button" aria-label="Back to projects" (back)>(icons::project_icon(cx,Some("lucide:ArrowLeft"),20))</button><strong>(name)</strong><button class="native-home-icon-button" aria-label="Close navigation" (close)>(icons::project_icon(cx,Some("lucide:X"),20))</button></header>(links)(recent)</div>}.boxed());
    }
    view! {cx=>for panel in panels{(panel)}}.boxed()
}
