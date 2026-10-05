//! Acknowledged publication is separate from the legacy management affordance.
use super::super::{context, icons, session, transport};
use crate::{db::models::UpdateProject, realtime::RealtimeHub};
use topcoat::{
    context::{Cx, app_context},
    runtime::{Event, procedure, signal},
    view::{BoxView, ViewExt, view},
};

#[procedure("/__native_overview/publish")]
async fn set_published(
    cx: &Cx,
    account: i64,
    project: i64,
    published: bool,
) -> topcoat::Result<Result<bool, String>> {
    let caller = session::read(cx, context::caller(cx))?;
    let user = session::read(cx, crate::api::require_user(&caller.identity))?;
    if user.id != account {
        return Ok(Err("Your account changed. Reload this page.".into()));
    }
    let result = caller
        .scope(async {
            crate::services::project_overview::update(
                context::db(cx),
                app_context::<RealtimeHub>(cx),
                &caller.identity,
                caller.session_token.as_deref(),
                project,
                UpdateProject {
                    is_public: Some(published),
                    ..Default::default()
                },
            )
        })
        .await;
    Ok(result
        .map(|project| project.is_public)
        .map_err(super::actions::error_message))
}
pub(super) fn panel<'a>(
    cx: &'a Cx,
    account: i64,
    project: &crate::db::models::Project,
) -> BoxView<'a> {
    let project_id = project.id;
    let published = signal(cx, || project.is_public);
    let acknowledged = signal(cx, || false);
    let busy = signal(cx, || false);
    let error = signal(cx, String::new);
    let notice = signal(cx, String::new);
    let path = transport::mounted_url(cx, &format!("/public/{}", project.identifier));
    let address = signal(cx, || path.clone());
    let failed_busy = busy.clone();
    let failed_error = error.clone();
    let save = topcoat::runtime::expr!(async |_event: Event| {
        if !busy.get() {
            let next = !published.get();
            if if published.get() {
                true
            } else {
                acknowledged.get()
            } {
                busy.set(true);
                error.set("".to_owned());
                let _failed = || {
                    failed_busy.set(false);
                    failed_error.set("Couldn't change public access. Try again.".to_owned());
                };
                let _save = async || {
                    let result = set_published(account, project_id, next).await;
                    busy.set(false);
                    if result.is_ok() {
                        published.set(result.unwrap());
                        acknowledged.set(false);
                        notice.set(if next {
                            "Project published".to_owned()
                        } else {
                            "Public access turned off".to_owned()
                        });
                    } else {
                        error.set(result.unwrap_err());
                    }
                };
                raw!(
                    "Promise.resolve().then(()=>${_save}()).catch(()=>${_failed}());",
                    ()
                );
            }
        }
    });
    view!{cx => <section class="native-overview-publish" data-native-overview-publish="" @mount=$(|_event:Event|address.set(raw!("cx.hydrate(window.location.origin+${path}.toString())",String::new())))>
  <header>(icons::project_icon(cx,Some("lucide:Globe"),15))<div><h2>"Public view"</h2><p :hidden=$(!published.get())>"This project's current issues and pages are readable by anyone, with no account."</p><p :hidden=$(published.get())>"Off. Only people with access to this project can see its issues and pages."</p></div><span class="native-overview-publish__badge" :data-published=$(published.get())><span :hidden=$(!published.get())>"Public"</span><span :hidden=$(published.get())>"Private"</span></span></header>
  <div class="native-overview-publish__body">
   <div :hidden=$(!published.get())><p>"Public address"</p><div class="native-overview-publish__address"><code>$(address.get())</code><button type="button" aria-label="Copy public link" @click=$(|_event:Event|{let _value=address.get();raw!("navigator.clipboard.writeText(${_value}.toString());",());})>(icons::project_icon(cx,Some("lucide:Copy"),14))</button><a :href=$(address.get()) target="_blank" rel="noopener noreferrer" aria-label="Open public view">(icons::project_icon(cx,Some("lucide:ExternalLink"),14))</a></div><p>"The address stays the same. Turning public access off and on again brings this exact link back."</p><p>"Closes the link immediately. Copies people already downloaded stay downloaded."</p></div>
   <div class="native-overview-publish__private" :hidden=$(published.get())>
    <div class="native-overview-publish__warning">(icons::project_icon(cx,Some("lucide:AlertTriangle"),15))<div><p><strong>"Publishing makes existing content public, not just future content."</strong></p><ul><li>"Every current issue and page in this project, including full descriptions, labels, modules and folders."</li><li>"Every comment on those issues and pages, shown with the commenter's display name."</li><li>"Every file attached to those issues, pages and comments, downloadable by anyone."</li></ul><p>"Plans, history, deleted items, usernames and your member list stay private. Nobody needs an account or a password, and there is no secret in the link, so search engines can find it. Turning this back off closes the link but cannot recall anything already downloaded."</p></div></div>
    <label><input type="checkbox" :checked=$(acknowledged.get()) :disabled=$(busy.get()) @change=$(|event:Event|acknowledged.set(event.target.checked))><span>"I've reviewed this project's issues, pages, comments and attachments, and they can be public."</span></label>
   </div>
   <p role="alert" :hidden=$(error.get().is_empty())>$(error.get())</p><p role="status" :hidden=$(notice.get().is_empty())>$(notice.get())</p>
   <div class="native-overview-publish__actions"><button type="button" :disabled=$(if busy.get(){true}else if !published.get(){!acknowledged.get()}else{false}) @click=(save)><span :hidden=$(!busy.get())>"Working…"</span><span :hidden=$(if busy.get(){true}else{published.get()})>"Publish issues"</span><span :hidden=$(if busy.get(){true}else{!published.get()})>"Turn off public access"</span></button></div>
  </div>
 </section>}.boxed()
}
