//! Chrome for one published project, independent of private account owners.
use super::super::{browser, chrome_controls, icons, navigation, preferences, public_route::Route};
use crate::db::models::Project;
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr, signal},
    view::{BoxView, ViewExt, view},
};

pub(super) fn shell<'a>(
    cx: &'a Cx,
    project: &Project,
    route: &Route,
    content: BoxView<'a>,
) -> BoxView<'a> {
    let theme = signal(cx, || "system".to_owned());
    let collapsed = signal(cx, || false);
    let mobile = signal(cx, || false);
    let sidebar_style = signal(cx, || "width:230px".to_owned());
    let preference_mount = preferences::mount(cx, &theme);
    let browser = browser::bindings();
    let mount_collapsed = collapsed.clone();
    let mount_style = sidebar_style.clone();
    let mount = expr!(|_event: Event| {
        mount_collapsed.set(browser.stored("lific:sidebar:collapsed".to_owned()) == "1");
        let _stored = browser.stored("lific:sidebar:width".to_owned());
        let valid = raw!(
            "cx.hydrate(${_stored}.toString().trim() !== '' && Number.isFinite(Number(${_stored}.toString())))",
            false
        );
        if valid {
            let parsed = raw!("cx.hydrate(Number(${_stored}.toString()))", 230.0f64);
            let _width = if parsed < 180.0 {
                180.0
            } else if parsed > 400.0 {
                400.0
            } else {
                parsed
            };
            let style = raw!(
                "cx.hydrate('width:' + ${_width}.toString() + 'px')",
                String::new()
            );
            mount_style.set(style);
        }
    });
    let mobile_toggle = mobile.clone();
    let toggle = expr!(|_event: Event| {
        mobile_toggle.set(!mobile_toggle.get());
    });
    let mobile_close = mobile.clone();
    let dismiss = expr!(|event: Event| {
        if event.key == "Escape" {
            mobile_close.set(false);
        }
    });
    let collapse = chrome_controls::collapse_attributes(cx, &collapsed);
    let expand = chrome_controls::collapse_attributes(cx, &collapsed);
    let theme_desktop = cycle_theme(cx, theme.clone());
    let theme_mobile = cycle_theme(cx, theme);
    let pages = matches!(route, Route::Pages { .. } | Route::PageDetail { .. });
    let section = if pages { "Pages" } else { "Issues" };
    let desktop_entries = entries(cx, &project.identifier, pages);
    let mobile_entries = entries(cx, &project.identifier, pages);
    let sign_in = navigation::attrs(cx, "/login");
    let mobile_sign_in = navigation::attrs(cx, "/login");
    let name = project.name.clone();
    let mobile_name = name.clone();
    let description = project.description.clone();
    let identifier = project.identifier.clone();
    let initials = identifier.chars().take(2).collect::<String>();
    let icon = icons::project_icon(cx, project.emoji.as_deref(), 16);
    let has_icon = project.emoji.as_ref().is_some_and(|icon| !icon.is_empty());
    let logo = super::super::preloads::image_url(cx, "/logo.webp");
    view! {
        cx =>
        <div
            class="h-dvh flex overflow-hidden bg-[var(--chrome)]"
            data-native-public-shell=(identifier)
            (preference_mount)
        >
            <div hidden=(true) data-topcoat-on:mount=(mount.into_evaluated_and_js().1)></div>
            <aside
                :class=$(if collapsed.get() {
                    "hidden"
                } else {
                    "hidden md:flex shrink-0 relative flex-col bg-[var(--chrome)] select-none"
                })
                :style=$(sidebar_style.get())
            >
                <div class="px-3 pt-3 pb-2 flex items-center gap-1.5">
                    <a
                        href="https://lific.dev"
                        target="_blank"
                        rel="noopener noreferrer"
                        title="Lific"
                        class="group flex flex-1 min-w-0 items-center gap-2.5 px-1 py-1 rounded-lg hover:bg-[var(--bg-subtle)]"
                    >
                        <img
                            src=(logo)
                            alt=""
                            width="26"
                            height="26"
                            class="rounded-md shrink-0"
                        >
                        <span
                            class="font-display text-heading tracking-tight text-[var(--text)] leading-none flex-1"
                        >
                            "Lific"
                        </span>
                        (public_badge(cx))
                    </a>
                    <button
                        type="button"
                        aria-label="Collapse sidebar"
                        title="Collapse sidebar"
                        class="size-7 grid place-items-center rounded-md text-[var(--text-faint)] hover:bg-[var(--bg-subtle)]"
                        (collapse)
                    >
                        (icons::ui_icon(cx, icons::UiIcon::CollapseSidebar, 15))
                    </button>
                </div>
                <nav
                    class="flex-1 px-2 py-1 overflow-y-auto"
                    aria-label="Published project"
                >
                    <div
                        class="px-2 pt-1.5 pb-1 text-micro font-semibold uppercase tracking-widest text-[var(--text-faint)]"
                    >
                        "Project"
                    </div>
                    <div
                        class="flex items-center gap-1.5 pl-1.5 pr-2 py-1.5 text-body-sm font-medium"
                    >
                        (icons::ui_icon(cx, icons::UiIcon::Expand, 13))
                        if has_icon {
                            (icon)
                        } else {
                            <span
                                class="size-5 rounded-md border border-solid border-[var(--border)] bg-[var(--bg-subtle)] grid place-items-center text-micro"
                            >
                                (initials)
                            </span>
                        }
                        <span class="truncate flex-1">(name)</span>
                    </div>
                    <div
                        class="ml-[1.125rem] pl-2.5 mt-0.5 mb-1.5 border-l border-solid border-[var(--border)] flex flex-col gap-px"
                    >
                        (desktop_entries)
                    </div>
                    if !description.is_empty() {
                        <p
                            class="px-2.5 pt-2 text-caption text-[var(--text-faint)] leading-snug line-clamp-6"
                        >
                            (description)
                        </p>
                    }
                </nav>
                <footer class="p-2 flex items-center gap-1">
                    <a
                        class="flex-1 flex items-center gap-2 px-2 py-1.5 rounded-md text-body-sm text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
                        title="Sign in to edit or comment"
                        (sign_in)
                    >
                        (icons::ui_icon(cx, icons::UiIcon::SignIn, 14))
                        "Sign in"
                    </a>
                    (theme_desktop)
                </footer>
            </aside>
            <div class="flex-1 min-w-0 flex flex-col">
                <header
                    class="md:hidden shrink-0 flex items-center gap-1 h-12 px-1 pt-[env(safe-area-inset-top)] box-content bg-[var(--chrome)]"
                >
                    <button
                        type="button"
                        class="size-11 grid place-items-center rounded-lg text-[var(--text-muted)]"
                        :aria-label=$(if mobile.get() {
                            "Close navigation"
                        } else {
                            "Open navigation"
                        })
                        :aria-expanded=$(mobile.get())
                        data-topcoat-on:click=(toggle.into_evaluated_and_js().1)
                    >
                        (icons::ui_icon(cx, icons::UiIcon::OpenNavigation, 20))
                    </button>
                    <span
                        class="flex-1 min-w-0 truncate font-display text-body-lg px-1.5"
                    >
                        (mobile_name)
                        <span class="text-body-sm text-[var(--text-faint)]">
                            (section)
                        </span>
                    </span>
                    (public_badge(cx))
                </header>
                <nav
                    class="md:hidden px-2 pb-2 bg-[var(--chrome)]"
                    :hidden=$(!mobile.get())
                    aria-label="Mobile published project"
                    data-topcoat-on:keydown=(dismiss.into_evaluated_and_js().1)
                >
                    (mobile_entries)
                    <div class="flex items-center gap-1 pt-1">
                        <a
                            class="flex-1 flex items-center gap-2 px-3 py-2.5 rounded-md text-body text-[var(--text-muted)]"
                            (mobile_sign_in)
                        >
                            (icons::ui_icon(cx, icons::UiIcon::SignIn, 16))
                            "Sign in"
                        </a>
                        (theme_mobile)
                    </div>
                </nav>
                <div
                    class="shrink-0 flex items-center min-h-10 px-3 bg-[var(--chrome)]"
                >
                    <button
                        type="button"
                        :hidden=$(!collapsed.get())
                        aria-label="Expand sidebar"
                        title="Expand sidebar"
                        class="hidden md:grid size-7 place-items-center rounded-md text-[var(--text-faint)] hover:bg-[var(--bg-subtle)]"
                        (expand)
                    >
                        (icons::ui_icon(cx, icons::UiIcon::CollapseSidebar, 15))
                    </button>
                    <span class="text-body-sm text-[var(--text-muted)] px-2">
                        (section)
                    </span>
                </div>
                <div class="relative flex-1 min-w-0 overflow-hidden md:rounded-tl-xl">
                    <div class="absolute inset-0 bg-[var(--bg)] overflow-y-auto">
                        (content)
                    </div>
                    <div
                        class="pointer-events-none absolute top-0 left-0 right-0 h-6 z-10 bg-gradient-to-b from-[var(--shadow-recess)] to-transparent"
                    ></div>
                    <div
                        :hidden=$(collapsed.get())
                        class="hidden md:block pointer-events-none absolute top-0 left-0 bottom-0 w-6 z-10 bg-gradient-to-r from-[var(--shadow-recess)] to-transparent"
                    ></div>
                </div>
            </div>
        </div>
    }.boxed()
}

fn public_badge(cx: &Cx) -> BoxView<'_> {
    view! {
        cx =>
        <span
            class="inline-flex items-center gap-1 font-mono text-micro text-[var(--text-faint)] px-1.5 py-0.5 rounded-md bg-[var(--bg-subtle)]"
            title="Anyone with the link can read this project"
        >
            (icons::ui_icon(cx, icons::UiIcon::PublicView, 10))
            "public"
        </span>
    }.boxed()
}

fn entries<'a>(cx: &'a Cx, project: &str, pages: bool) -> BoxView<'a> {
    let entries = [("issues", "Issues", icons::UiIcon::Issues, !pages), ("pages", "Pages", icons::UiIcon::Page, pages)].into_iter().map(|(path,label,icon,active)| {
        let href = navigation::attrs(cx, &format!("/public/{project}/{path}"));
        let class = if active { "flex items-center gap-2 px-2 py-1 rounded-md text-body-sm text-[var(--text)] bg-[var(--bg-subtle)] font-medium" } else { "flex items-center gap-2 px-2 py-1 rounded-md text-body-sm text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]" };
        view! {
            cx =>
            <a
                class=(class)
                aria-current=(if active { Some("page") } else { None })
                (href)
            >
                (icons::ui_icon(cx, icon, 14))
                (label)
            </a>
        }.boxed()
    }).collect::<Vec<_>>();
    view! {
        cx =>
        for entry in entries {
            (entry)
        }
    }
    .boxed()
}

fn cycle_theme(cx: &Cx, theme: Signal<String>) -> BoxView<'_> {
    let browser = browser::bindings();
    let toggle_theme = theme.clone();
    let click = expr!(|_event: Event| {
        let next = if toggle_theme.get() == "light" {
            "dark"
        } else if toggle_theme.get() == "dark" {
            "system"
        } else {
            "light"
        };
        toggle_theme.set(next.to_owned());
        if next == "system" {
            browser.remove_storage("lific_theme".to_owned());
        } else {
            browser.store("lific_theme".to_owned(), next.to_owned());
        }
        browser.broadcast_storage("lific_theme".to_owned(), next.to_owned());
    });
    view! {
        cx =>
        <button
            type="button"
            class="size-8 grid place-items-center rounded-md text-[var(--text-muted)] hover:bg-[var(--bg-subtle)]"
            :aria-label=$(if theme.get() == "light" {
                "Cycle theme, current: light"
            } else if theme.get() == "dark" {
                "Cycle theme, current: dark"
            } else {
                "Cycle theme, current: system"
            })
            data-topcoat-on:click=(click.into_evaluated_and_js().1)
        >
            <span :hidden=$(theme.get() != "system")>
                (icons::ui_icon(cx, icons::UiIcon::SystemTheme, 15))
            </span>
            <span :hidden=$(theme.get() != "light")>
                (icons::ui_icon(cx, icons::UiIcon::LightTheme, 15))
            </span>
            <span :hidden=$(theme.get() != "dark")>
                (icons::ui_icon(cx, icons::UiIcon::DarkTheme, 15))
            </span>
        </button>
    }.boxed()
}
