//! Client configuration help for the native tool connection flow.
use super::templates::{
    API_KEY_MARKER, MCP_URL_MARKER, OsPathGroup, SetupStep, ToolTemplate, config_template,
    export_template, path_for_os, path_groups, persistence_hint,
};
use topcoat::{
    context::Cx,
    runtime::{Event, Js, Signal, expr, signal},
    view::{Attributes, BoxView, ViewExt, view},
};

const OS_SELECTED: &str = "rounded-md bg-[var(--surface)] px-2.5 py-1 text-caption font-medium text-[var(--text)] shadow-sm";
const OS_UNSELECTED: &str =
    "rounded-md px-2.5 py-1 text-caption text-[var(--text-muted)] hover:text-[var(--text)]";
const COPY_BUTTON: &str = "rounded-md border border-[var(--border)] px-2.5 py-1 text-caption text-[var(--text)] hover:bg-[var(--bg-subtle)]";

/// Build the shared clipboard event handler used by both setup details and the dialog.
/// `source` may contain the URL and API key markers from the static Rust templates.
pub(super) fn copy_action(
    cx: &Cx,
    source: Signal<String>,
    key: Signal<String>,
    copied: Signal<bool>,
    failed: Signal<bool>,
) -> Js {
    let mounted = super::super::transport::mounted_url(cx, "/mcp");
    expr!(|_event: Event| {
        let _source = source.get();
        let _key = key.get();
        let _copy = async || {
            raw!(
                r#"const value=${_source}.toString()
                    .replaceAll(${MCP_URL_MARKER}.toString(), window.location.origin + ${mounted}.toString())
                    .replaceAll(${API_KEY_MARKER}.toString(), ${_key}.toString());
                try {
                    if (navigator.clipboard && navigator.clipboard.writeText) {
                        await navigator.clipboard.writeText(value);
                    } else {
                        const field=document.createElement('textarea');
                        field.value=value;field.setAttribute('readonly','');
                        field.style.position='fixed';field.style.opacity='0';
                        document.body.appendChild(field);field.select();
                        const ok=document.execCommand('copy');field.remove();
                        if(!ok)throw new Error('copy failed');
                    }
                    if(cx.abortSignal.aborted)return;
                    ${copied}.set(cx.hydrate(true));${failed}.set(cx.hydrate(false));
                    setTimeout(()=>{if(!cx.abortSignal.aborted)${copied}.set(cx.hydrate(false))},1500);
                } catch {
                    if(cx.abortSignal.aborted)return;
                    ${failed}.set(cx.hydrate(true));${copied}.set(cx.hydrate(false));
                }"#,
                ()
            );
        };
        raw!("${_copy}();", ());
    })
    .into_evaluated_and_js()
    .1
}

/// Render one reusable copy button around the shared clipboard helper.
pub(super) fn copy_button<'a>(
    cx: &'a Cx,
    label: &'static str,
    source: Signal<String>,
    key: Signal<String>,
    copied: Signal<bool>,
    failed: Signal<bool>,
    marker: &'static str,
) -> BoxView<'a> {
    let handler = copy_action(cx, source, key, copied.clone(), failed.clone());
    copy_button_with_handler(cx, label, handler, copied, failed, marker)
}

fn copy_button_with_handler<'a>(
    cx: &'a Cx,
    label: &'static str,
    handler: Js,
    copied: Signal<bool>,
    failed: Signal<bool>,
    marker: &'static str,
) -> BoxView<'a> {
    let mut attrs = Attributes::with_capacity(2);
    attrs.insert(cx, "data-topcoat-on:click", handler);
    attrs.insert(cx, marker, "");
    view! { cx =>
        <button type="button" class=(COPY_BUTTON) (attrs)>
            $(if failed.get() { "Copy failed" } else if copied.get() { "Copied" } else { label })
        </button>
    }
    .boxed()
}

fn export_copy_action(
    commands: [String; 3],
    selected_os: Signal<String>,
    key: Signal<String>,
    copied: Signal<bool>,
    failed: Signal<bool>,
) -> Js {
    expr!(|_event: Event| {
        let current_os = selected_os.get();
        let _source = if current_os == "windows" {
            commands[2].clone()
        } else if current_os == "linux" {
            commands[0].clone()
        } else {
            commands[1].clone()
        };
        let _key = key.get();
        let _copy = async || {
            raw!(
                r#"const value=${_source}.toString().replaceAll(${API_KEY_MARKER}.toString(), ${_key}.toString());
                try {
                    if (navigator.clipboard && navigator.clipboard.writeText) {
                        await navigator.clipboard.writeText(value);
                    } else {
                        const field=document.createElement('textarea');
                        field.value=value;field.setAttribute('readonly','');
                        field.style.position='fixed';field.style.opacity='0';
                        document.body.appendChild(field);field.select();
                        const ok=document.execCommand('copy');field.remove();
                        if(!ok)throw new Error('copy failed');
                    }
                    if(cx.abortSignal.aborted)return;
                    ${copied}.set(cx.hydrate(true));${failed}.set(cx.hydrate(false));
                    setTimeout(()=>{if(!cx.abortSignal.aborted)${copied}.set(cx.hydrate(false))},1500);
                } catch {
                    if(cx.abortSignal.aborted)return;
                    ${failed}.set(cx.hydrate(true));${copied}.set(cx.hydrate(false));
                }"#,
                ()
            );
        };
        raw!("${_copy}();", ());
    })
    .into_evaluated_and_js()
    .1
}

pub(super) fn view<'a>(
    cx: &'a Cx,
    template: ToolTemplate,
    key: Signal<String>,
    revealed: Signal<bool>,
    selected_os: Signal<String>,
) -> BoxView<'a> {
    let groups = path_groups(template);
    let active_os = selected_os.clone();
    let config = config_template(template);
    let mounted = super::super::transport::mounted_url(cx, "/mcp");
    let os_paths =
        ["linux", "mac", "windows"].map(|os| path_for_os(template, os).unwrap_or("").to_owned());
    let os_hints = ["linux", "mac", "windows"].map(|os| persistence_hint(os).to_owned());
    let export_templates =
        ["linux", "mac", "windows"].map(|os| export_template(template, os).unwrap_or_default());
    let export = if export_templates.iter().any(|command| !command.is_empty()) {
        let copied = signal(cx, || false);
        let failed = signal(cx, || false);
        Some((copied, failed))
    } else {
        None
    };
    let os_buttons = groups
        .iter()
        .map(|group| os_button(cx, group, selected_os.clone()))
        .collect::<Vec<_>>();
    let notes = template
        .notes
        .iter()
        .map(|step| note_view(cx, *step, key.clone(), mounted.clone()))
        .collect::<Vec<_>>();
    let export_view = export.as_ref().map(|(copied, failed)| {
        let export_key = key.clone();
        let export_revealed = revealed.clone();
        let export_active_os = active_os.clone();
        let copied_for_copy = copied.clone();
        let failed_for_copy = failed.clone();
        let copied_text = copied.clone();
        let failed_text = failed.clone();
        let copy_handler = export_copy_action(
            export_templates.clone(),
            export_active_os.clone(),
            export_key.clone(),
            copied_for_copy.clone(),
            failed_for_copy.clone(),
        );
        let copy = copy_button_with_handler(
            cx,
            "Copy command",
            copy_handler,
            copied_for_copy,
            failed_for_copy,
            "data-native-tool-export-copy",
        );
        view! { cx =>
            <div class="mt-4 rounded-lg border border-[var(--border)] bg-[var(--bg-subtle)] p-3">
                <div class="mb-2 flex items-center justify-between gap-2">
                    <h4 class="text-caption font-medium text-[var(--text)]">"Set the API key"</h4>
                    (copy)
                </div>
                <code class="block break-all font-mono text-caption text-[var(--text)]">
                    $({
                        let current_os = export_active_os.get();
                        let _command = if current_os == "windows" { export_templates[2].clone() } else if current_os == "linux" { export_templates[0].clone() } else { export_templates[1].clone() };
                        let _shown = if export_revealed.get() { export_key.get() } else { "••••••••••••••••".to_owned() };
                        raw!("cx.hydrate(${_command}.toString().replaceAll(${API_KEY_MARKER}.toString(), ${_shown}.toString()))", String::new())
                    })
                </code>
                <p class="mt-1 text-caption text-[var(--text-muted)]">$(if export_active_os.get() == "windows" { os_hints[2].clone() } else if export_active_os.get() == "macos" { os_hints[1].clone() } else if export_active_os.get() == "mac" { os_hints[1].clone() } else { os_hints[0].clone() })</p>
                <p class="mt-1 text-caption text-[var(--error)]" role="status">$(if failed_text.get() { "Could not copy the command." } else if copied_text.get() { "Command copied." } else { "" })</p>
            </div>
        }.boxed()
    });

    let config_source = signal(cx, || config.to_owned());
    let config_copied = signal(cx, || false);
    let config_failed = signal(cx, || false);
    let config_copy = copy_button(
        cx,
        "Copy config",
        config_source,
        key.clone(),
        config_copied.clone(),
        config_failed.clone(),
        "data-native-tool-config-copy",
    );

    view! { cx =>
        <section class="mt-4" data-native-tool-setup="" data-native-tool-setup-client=(template.id)>
            <h3 class="text-body-sm font-semibold text-[var(--text)]">"Set up " (template.name)</h3>
            <p class="mt-1 text-caption text-[var(--text-muted)]">(template.description)</p>
            if !groups.is_empty() {
                <div class="mt-3 inline-flex rounded-lg bg-[var(--bg-subtle)] p-1" role="group" aria-label="Operating system">
                    for button in os_buttons { (button) }
                </div>
                <p class="mt-2 break-all font-mono text-caption text-[var(--text-muted)]">
                    $(if active_os.get() == "windows" { os_paths[2].clone() } else if active_os.get() == "macos" { os_paths[1].clone() } else if active_os.get() == "mac" { os_paths[1].clone() } else { os_paths[0].clone() })
                </p>
            }
            for note in notes { (note) }
            <div class="mt-4 rounded-lg border border-[var(--border)] bg-[var(--bg-subtle)] p-3">
                <div class="mb-2 flex items-center justify-between gap-2">
                    <h4 class="text-caption font-medium text-[var(--text)]">"Configuration"</h4>
                    (config_copy)
                </div>
                <pre class="overflow-x-auto whitespace-pre-wrap break-all font-mono text-caption text-[var(--text)]"><code>
                    $({
                        let _source = config.to_owned();
                        let secret = key.get();
                        let _shown = if revealed.get() { secret } else { "••••••••••••••••".to_owned() };
                        raw!("cx.hydrate(${_source}.toString().replaceAll(${MCP_URL_MARKER}.toString(), window.location.origin + ${mounted}.toString()).replaceAll(${API_KEY_MARKER}.toString(), ${_shown}.toString()))", String::new())
                    })
                </code></pre>
                <p class="mt-1 text-caption text-[var(--error)]" role="status">$(if config_failed.get() { "Could not copy the configuration." } else if config_copied.get() { "Configuration copied." } else { "" })</p>
            </div>
            if let Some(export_view) = export_view { (export_view) }
        </section>
    }
    .boxed()
}

fn os_button<'a>(cx: &'a Cx, group: &OsPathGroup, selected: Signal<String>) -> BoxView<'a> {
    let value = group.key.to_owned();
    let choice = selected.clone();
    let selected_key = selected.clone();
    let mac_in_group = group.oses.contains(&"mac");
    let label = group.label.clone();
    let handler = expr!(|_event: Event| {
        choice.set(value.clone());
    });
    let mut attrs = Attributes::with_capacity(1);
    attrs.insert(
        cx,
        "data-topcoat-on:click",
        handler.into_evaluated_and_js().1,
    );
    attrs.insert(cx, "data-native-tool-os", value.clone());
    view! { cx =>
        <button type="button"
            :class=$(if selected_key.get() == value { OS_SELECTED } else if selected_key.get() == "macos" { if mac_in_group { OS_SELECTED } else { OS_UNSELECTED } } else if selected_key.get() == "mac" { if mac_in_group { OS_SELECTED } else { OS_UNSELECTED } } else { OS_UNSELECTED })
            :aria-pressed=$(if selected_key.get() == value { "true" } else if selected_key.get() == "macos" { if mac_in_group { "true" } else { "false" } } else if selected_key.get() == "mac" { if mac_in_group { "true" } else { "false" } } else { "false" })
            (attrs)>(label)</button>
    }
    .boxed()
}

fn note_view<'a>(cx: &'a Cx, step: SetupStep, key: Signal<String>, mounted: String) -> BoxView<'a> {
    match step {
        SetupStep::Text(text) => view! { cx => <p class="mt-3 text-caption leading-relaxed text-[var(--text-muted)]">(text)</p> }.boxed(),
        SetupStep::Command(command) => {
            let source = signal(cx, || command.to_owned());
            let copied = signal(cx, || false);
            let failed = signal(cx, || false);
            let copy = copy_button(cx, "Copy", source, key, copied, failed, "data-native-tool-command-copy");
            view! { cx =>
                <div class="mt-2 flex items-center justify-between gap-2 rounded-lg bg-[var(--bg-subtle)] p-2">
                    <code class="min-w-0 break-all font-mono text-caption text-[var(--text)]">
                        $(raw!("cx.hydrate(${command}.toString().replaceAll(${MCP_URL_MARKER}.toString(), window.location.origin + ${mounted}.toString()))", String::new()))
                    </code>
                    (copy)
                </div>
            }.boxed()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::home_fixture;
    use super::super::templates::{TOOL_TEMPLATES, path_groups};

    #[tokio::test]
    async fn export_copy_uses_os_selected_from_another_client_panel() {
        let fixture = home_fixture::fixture();
        let (status, html) =
            home_fixture::document(&fixture, "/app", "/settings", true, None).await;
        assert_eq!(status, axum::http::StatusCode::OK);
        let document = scraper::Html::parse_document(&html);
        let button = |selector: &str| {
            document
                .select(&scraper::Selector::parse(selector).unwrap())
                .next()
                .unwrap_or_else(|| panic!("missing button matching {selector}"))
        };
        let windows = button(
            "[data-native-tool-setup-client='claude'] button[data-native-tool-os='windows']",
        );
        let linux_mac =
            button("[data-native-tool-setup-client='codex'] button[data-native-tool-os='linux']");
        assert_eq!(linux_mac.value().attr("aria-pressed"), Some("true"));
        let copy =
            button("[data-native-tool-setup-client='codex'] button[data-native-tool-export-copy]");
        let config_copy =
            button("[data-native-tool-setup-client='codex'] button[data-native-tool-config-copy]");
        let mount = document
            .select(&scraper::Selector::parse("[data-native-tool-dialog]").unwrap())
            .next()
            .unwrap()
            .value()
            .attr("data-topcoat-on:mount")
            .unwrap();
        let copy_label_expression = copy
            .children()
            .find_map(|node| {
                let scraper::Node::Comment(comment) = node.value() else {
                    return None;
                };
                let encoded = comment
                    .strip_prefix("::topcoat::expr::start(\"")?
                    .strip_suffix("\")")?;
                Some(
                    scraper::Html::parse_fragment(encoded)
                        .root_element()
                        .text()
                        .collect::<String>(),
                )
            })
            .expect("copy button emits its reactive label expression");
        let codex_template = *TOOL_TEMPLATES
            .iter()
            .find(|template| template.id == "codex")
            .unwrap();
        let codex_group_oses = path_groups(codex_template)
            .into_iter()
            .find(|group| group.key == "linux")
            .unwrap()
            .oses;
        let result = home_fixture::evaluate_handler(
            "src/topcoat/native/settings/tool_setup_handler.test.cjs",
            &serde_json::json!({
                "signals": home_fixture::page_signals(&html),
                "mount": mount,
                "select_windows": windows.value().attr("data-topcoat-on:click").unwrap(),
                "copy": copy.value().attr("data-topcoat-on:click").unwrap(),
                "copy_label_expression": copy_label_expression,
                "config_copy": config_copy.value().attr("data-topcoat-on:click").unwrap(),
                "mac_group_binding": linux_mac
                    .value()
                    .attr("data-topcoat-bind:aria-pressed")
                    .unwrap_or(""),
                "codex_group_oses": codex_group_oses,
            }),
        );
        assert!(
            result["clipboard"]
                .as_str()
                .unwrap()
                .starts_with("setx LIFIC_API_KEY \"")
        );
    }
}
