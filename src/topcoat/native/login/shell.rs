//! Main's signed-out chrome, theme cycle, and recessed login panel.
use super::super::{icons, transport};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, signal},
    view::{BoxView, ViewExt, view},
};

fn theme_button<'a>(cx: &'a Cx, theme: Signal<String>, mobile: bool) -> BoxView<'a> {
    view! {cx => <button type="button" class=(if mobile {"size-8 shrink-0 grid place-items-center rounded-md text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors lg:hidden bg-transparent border-0"} else {"size-8 shrink-0 grid place-items-center rounded-md text-[var(--text-muted)] hover:text-[var(--text)] hover:bg-[var(--bg-subtle)] transition-colors bg-transparent border-0"})
        :title=$({let preference=theme.get();raw!("cx.hydrate('Theme: '+${preference}.toString())",format!("Theme: {preference}"))})
        :aria-label=$({let preference=theme.get();raw!("cx.hydrate('Cycle theme, current: '+${preference}.toString())",format!("Cycle theme, current: {preference}"))})
        @click=$(|_event:Event| {
            theme.set(if theme.get()=="light" {"dark".to_owned()}else if theme.get()=="dark" {"system".to_owned()}else{"light".to_owned()});
            let preference=theme.get();
            if preference=="system" {raw!("try {localStorage.removeItem('lific_theme');} catch {}",());}
            else {raw!("try {localStorage.setItem('lific_theme',${preference}.toString());} catch {}",());}
            raw!("document.documentElement.setAttribute('data-theme',${preference}.toString());",());
        })>
        <span :hidden=$(theme.get()!="system")>(icons::project_icon(cx,Some("lucide:Monitor"),15))</span>
        <span :hidden=$(theme.get()!="dark")>(icons::project_icon(cx,Some("lucide:Moon"),15))</span>
        <span :hidden=$(theme.get()!="light")>(icons::project_icon(cx,Some("lucide:Sun"),15))</span>
    </button>}.boxed()
}

pub(super) fn content<'a>(
    cx: &'a Cx,
    instance_name: Option<String>,
    login_message: Option<String>,
    allow_signup: bool,
    auto: bool,
) -> BoxView<'a> {
    let host = signal(cx, String::new);
    let theme = signal(cx, || "system".to_owned());
    let name = instance_name.filter(|name| !name.is_empty());
    let message = login_message.filter(|message| !message.is_empty());
    let theme_desktop = theme_button(cx, theme.clone(), false);
    let theme_mobile = theme_button(cx, theme.clone(), true);
    let form = super::form::content(cx, auto);
    let logo = transport::mounted_url(cx, "/logo.webp");
    let signup = transport::mounted_url(cx, "/signup");
    let login = transport::mounted_url(cx, "/login");
    let mascot = transport::mounted_url(cx, "/__native_login/mascot.png");
    view! {cx => <div class="h-dvh flex overflow-hidden bg-[var(--chrome)]" data-native-login="" @mount=$(|_event:Event| {
        host.set(raw!("cx.hydrate(window.location.host)",String::new()));
        let preference=raw!("cx.hydrate((() => {try {return localStorage.getItem('lific_theme')??'';} catch {return '';}})())",String::new());
        theme.set(if preference=="light" {"light".to_owned()}else if preference=="dark" {"dark".to_owned()}else{"system".to_owned()});
    })>
        <aside class="hidden lg:flex w-[230px] shrink-0 flex-col bg-[var(--chrome)] select-none">
            <div class="px-3 pt-3 pb-2"><div class="flex items-center gap-2.5 px-1 py-1"><img src=(logo.clone()) alt="" width="26" height="26" class="rounded-md shrink-0"/><span class="font-display text-heading tracking-tight text-[var(--text)] leading-none flex-1">"Lific"</span><span class="font-mono text-micro tracking-tight text-[var(--text-faint)] px-1.5 py-0.5 rounded-md bg-[var(--bg-subtle)]">(concat!("v",env!("CARGO_PKG_VERSION")))</span></div></div>
            <div class="flex-1"></div>
            <div class="p-2 flex items-center gap-1"><div class="flex-1 min-w-0 flex items-center gap-2.5 px-2 py-1.5 rounded-md"><div class="size-7 rounded-full border border-solid border-[var(--border)] bg-[var(--bg-subtle)] grid place-items-center shrink-0">(icons::project_icon(cx,Some("lucide:LogIn"),13))</div><div class="flex-1 min-w-0"><div class="text-body-sm text-[var(--text-muted)] truncate leading-tight">"Not signed in"</div><div class="text-micro text-[var(--text-faint)] leading-tight mt-0.5">"Sign in to continue"</div></div></div>(theme_desktop)</div>
        </aside>
        <div class="flex-1 min-w-0 flex flex-col">
            <div class="shrink-0 flex items-center gap-3 px-4 sm:px-6 py-2 bg-[var(--chrome)]">
                <div class="flex items-center gap-1.5 min-w-0"><img src=(logo) alt="" width="20" height="20" class="rounded shrink-0 lg:hidden"/>
                    if let Some(name)=name {<span class="hidden sm:inline text-body-sm font-medium text-[var(--text-muted)] truncate max-w-[16rem]" :title=$(host.get())>(name)</span>}
                    else {<span class="hidden sm:inline font-mono text-body-sm font-medium text-[var(--text-muted)] truncate max-w-[16rem]" :title=$(host.get())>$(host.get())</span>}
                    <span class="hidden sm:inline text-[var(--text-faint)] shrink-0">(icons::project_icon(cx,Some("lucide:ChevronRight"),12))</span><span class="text-body-sm font-medium text-[var(--text)]">"Sign in"</span>
                </div>
                <div class="ml-auto flex items-center gap-2"><div class="flex items-center gap-0.5 p-0.5 rounded-md bg-[var(--bg)] shadow-[inset_0_1px_2px_rgba(0,0,0,0.10)]"><a href=(login) aria-current="page" class="flex items-center gap-1 px-2.5 py-1 rounded text-caption font-medium transition-all bg-[var(--surface)] text-[var(--text)] shadow-[0_1px_2px_rgba(0,0,0,0.16),0_1px_1px_rgba(0,0,0,0.10)] no-underline">(icons::project_icon(cx,Some("lucide:LogIn"),11))"Sign in"</a>
                    if allow_signup {<a href=(signup) class="flex items-center gap-1 px-2.5 py-1 rounded text-caption font-medium transition-all text-[var(--text-muted)] hover:text-[var(--text)] no-underline">(icons::project_icon(cx,Some("lucide:UserPlus"),11))"Create account"</a>}
                </div>(theme_mobile)</div>
            </div>
            <div class="relative flex-1 min-w-0 lg:rounded-tl-xl overflow-hidden"><main class="absolute inset-0 bg-[var(--bg)] overflow-y-auto"><div class="min-h-full flex flex-col"><div class="flex-1 px-6 sm:px-10 lg:px-14 py-10 lg:py-14"><div class="w-full max-w-[28rem]">
                <h1 class="font-display text-display font-semibold tracking-[-0.02em] text-[var(--text)] leading-[1.08] animate-reveal">"Welcome back."</h1><p class="text-body-lg text-[var(--text-muted)] leading-relaxed mt-2.5 max-w-[40ch] animate-reveal delay-100">"Sign in to continue on this instance."</p>
                if let Some(message)=message {<div class="mt-5 text-body-sm text-[var(--text-muted)] leading-relaxed bg-[var(--bg-subtle)] border border-solid border-[var(--border)] rounded-lg px-3.5 py-2.5 max-w-[40ch] animate-reveal delay-100">(message)</div>}
                <div class="mt-8 animate-reveal delay-150">(form)</div>
            </div></div><div class="pointer-events-none flex justify-end px-6 lg:px-10 pb-6 opacity-90"><img src=(mascot) width="78" height="114" alt="" class="select-none"/></div></div></main>
                <div class="pointer-events-none absolute top-0 left-0 right-0 h-6 z-10 bg-gradient-to-b from-[var(--tc-shadow-recess)] to-transparent"></div><div class="pointer-events-none absolute top-0 left-0 bottom-0 w-6 z-10 bg-gradient-to-r from-[var(--tc-shadow-recess)] to-transparent hidden lg:block"></div>
            </div>
        </div>
    </div>}.boxed()
}
