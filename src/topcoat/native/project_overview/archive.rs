//! Whole-project archives use native HTTP adapters with fresh human-cookie fences.
use super::super::{context, icons, session, transport};
use topcoat::{
    context::{Cx, app_context},
    router::{Body, path_param, response::Response, route},
    runtime::{Event, signal},
    view::{BoxView, ViewExt, view},
};
path_param!(owner);
path_param!(project);
path_param!(identifier);
fn ids(cx: &Cx) -> topcoat::Result<(i64, i64)> {
    let owner = path_param::<Owner>(cx)
        .parse()
        .map_err(|_| topcoat::router::error::bad_request("invalid account"))?;
    let project = path_param::<Project>(cx)
        .parse()
        .map_err(|_| topcoat::router::error::bad_request("invalid project"))?;
    Ok((owner, project))
}
#[route(GET "/__native_overview/archive/{owner}/{project}/{identifier}")]
async fn download(cx: &Cx) -> topcoat::Result<Response> {
    let (owner, project) = ids(cx)?;
    let caller = session::read(cx, context::caller(cx))?;
    let headers = session::read(cx, caller.session_headers())?;
    session::read(
        cx,
        crate::services::project_archive_export::verify_owner(
            context::db(cx),
            &caller.identity,
            &headers,
            owner,
            project,
        ),
    )?;
    let response = session::read(
        cx,
        crate::services::project_archive_export::download(
            context::db(cx).clone(),
            app_context::<crate::storage::AttachmentStore>(cx).clone(),
            &caller.identity,
            path_param::<Identifier>(cx).to_owned(),
            headers,
            Some(project),
            crate::project_archive::Limits::WEB,
        )
        .await,
    )?;
    Ok(response.map(Body::new))
}
#[route(GET "/__native_overview/archive_owner/{owner}/{project}")]
async fn verify(cx: &Cx) -> topcoat::Result<Response> {
    let (owner, project) = ids(cx)?;
    let caller = session::read(cx, context::caller(cx))?;
    let headers = session::read(cx, caller.session_headers())?;
    session::read(
        cx,
        crate::services::project_archive_export::verify_owner(
            context::db(cx),
            &caller.identity,
            &headers,
            owner,
            project,
        ),
    )?;
    Ok(Response::builder()
        .status(204)
        .header("cache-control", "no-store")
        .body(Body::empty())?)
}
pub(super) fn panel<'a>(
    cx: &'a Cx,
    account: i64,
    project: i64,
    identifier: &str,
) -> topcoat::Result<BoxView<'a>> {
    let caller = session::read(cx, context::caller(cx))?;
    let headers = session::read(cx, caller.session_headers())?;
    // The legacy panel's eligibility is the live-browser capabilities call,
    // independently of its parent's publication affordance. Refusal hides it.
    if crate::services::project_archive_export::capability(
        context::db(cx),
        &caller.identity,
        &headers,
    )
    .is_err()
    {
        return Ok(view! {cx=>""}.boxed());
    }
    let confirmed = signal(cx, || false);
    let busy = signal(cx, || false);
    let error = signal(cx, String::new);
    let endpoint = transport::mounted_url(
        cx,
        &format!("/__native_overview/archive/{account}/{project}/{identifier}"),
    );
    let verify_endpoint = transport::mounted_url(
        cx,
        &format!("/__native_overview/archive_owner/{account}/{project}"),
    );
    Ok(view!{cx=><section class="native-overview__archive" aria-labelledby="native-overview-archive-heading"><h2 id="native-overview-archive-heading">"Project archive"</h2>
        <p>"Copy this whole project to another Lific instance. The archive includes linked files, history, deleted content and author names. It can contain sensitive text no longer visible in the project. Accounts and permissions are not included; author names transfer as text only."</p>
        <p>"Downloading leaves this project untouched. Importing creates a private project, never a merge."</p>
        <label><input type="checkbox" :checked=$(confirmed.get()) :disabled=$(busy.get()) @change=$(|event:Event|confirmed.set(event.target.checked)) />"I understand this archive includes history and deleted content."</label>
        <p class="native-overview__error" role="alert" :hidden=$(error.get().is_empty())>$(error.get())</p>
        <button type="button" class="toolbar-pill" :disabled=$(if !confirmed.get(){true}else{busy.get()}) @click=$(|_event:Event|{
            if if confirmed.get(){!busy.get()}else{false}{busy.set(true);error.set("".to_owned());
                let _completed=|failure:topcoat::runtime::StringSurrogate|{if !raw!("cx.hydrate(cx.abortSignal.aborted)",false){error.set(failure);busy.set(false);}};
                // Fetch, abort, blob and download are browser primitives. Both
                // responses are native routes; application gates run in Rust.
                raw!(r#"void (async()=>{
                    try{
                        const response=await fetch(${endpoint}.toString(),{signal:cx.abortSignal,redirect:'error',cache:'no-store'});
                        if(!response.ok)return 'HTTP '+response.status;
                        const filename=response.headers.get('content-disposition')?.match(/filename="([^"]+)"/)?.[1]||'project.lific.tar.gz';
                        const blob=await response.blob();
                        if(cx.abortSignal.aborted)return '';
                        const owner=await fetch(${verify_endpoint}.toString(),{signal:cx.abortSignal,redirect:'error',cache:'no-store'});
                        if(!owner.ok)return 'Your account changed. Download the archive again after signing in.';
                        if(cx.abortSignal.aborted)return '';
                        const url=URL.createObjectURL(blob),anchor=document.createElement('a');
                        try{anchor.href=url;anchor.download=filename;document.body.appendChild(anchor);anchor.click();}
                        finally{anchor.remove();URL.revokeObjectURL(url);}
                        return '';
                    }catch(failure){return failure instanceof Error?failure.message:String(failure);}
                })().then(failure=>${_completed}(cx.hydrate(failure)))"#,());
            }
        })>(icons::project_icon(cx,Some("lucide:Download"),14))$(if busy.get(){"Preparing archive..."}else{"Download project archive"})</button>
    </section>}.boxed())
}
