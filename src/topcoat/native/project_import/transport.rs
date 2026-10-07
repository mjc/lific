//! The browser owns only the File/FormData/XHR boundary. State and policy stay in Rust.

use topcoat::{
    context::Cx,
    runtime::{
        BoolSurrogate, Event, I64Surrogate, Js, SignalSurrogate, StringSurrogate, Surrogated,
        UsizeSurrogate, expr, procedure, record,
    },
    view::{BoxView, ViewExt, view},
};

use super::{model::ImportResult, state::State};

#[record]
#[derive(Clone)]
pub(crate) struct SessionResult {
    pub(crate) current: bool,
    pub(crate) fingerprint: String,
}

#[procedure("/__native_project_import/current_session")]
async fn current_session(cx: &Cx, account: i64) -> topcoat::Result<SessionResult> {
    let caller = super::super::session::read(cx, super::super::context::caller(cx))?;
    let user = super::super::session::read(cx, crate::api::require_user(&caller.identity))?;
    let live_user =
        crate::db::queries::users::get_user_by_id(&*super::super::context::db(cx).read()?, account);
    let current = match live_user {
        Ok(live_user) => {
            caller.session_token.is_some()
                && user.id == account
                && live_user.is_admin
                && !live_user.is_bot
                && live_user.is_active
        }
        Err(_) => false,
    };
    Ok(SessionResult {
        current,
        fingerprint: super::state::session_fingerprint(caller.session_token.as_deref()),
    })
}

type Handles<'a> = (
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<bool>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<Option<usize>>,
    &'a SignalSurrogate<String>,
    &'a SignalSurrogate<Option<ImportResult>>,
);

pub(crate) fn owner(cx: &Cx, state: State) -> BoxView<'_> {
    let handles = (
        &state.file_name,
        &state.file_error,
        &state.confirmed,
        &state.phase,
        &state.progress,
        &state.error,
        &state.result,
    )
        .into_surrogate();
    let account = state.account.into_surrogate();
    let fingerprint = state.fingerprint.clone().into_surrogate();
    let upload_url = super::super::transport::mounted_url(
        cx,
        &format!("/__native_project_import/upload/{}", state.account),
    )
    .into_surrogate();
    let args = Js::builder()
        .raw("[")
        .surrogate(&handles)
        .raw(",")
        .surrogate(&account)
        .raw(",")
        .surrogate(&fingerprint)
        .raw(",")
        .surrogate(&upload_url)
        .raw("]")
        .build();
    let owner_key = format!("{}:{}", state.account, state.fingerprint);
    let key = format!(
        "{}#project-archive-import",
        super::super::home_shell::handler_url()
    );
    let attrs = super::super::handler_asset::mount(cx, &key, args);
    view! {
        cx =>
        <div
            id="native-project-import-owner"
            data-native-archive-owner=(owner_key)
            class="hidden"
            (attrs)
        ></div>
    }
    .boxed()
}

pub(crate) fn handler_factory() -> Js {
    let usize_width = usize::BITS;
    let handler = expr!(|_event: Event,
                         handles: Handles<'_>,
                         account: I64Surrogate,
                         fingerprint: StringSurrogate,
                         _upload_url: StringSurrogate| {
        let file_name = handles.0;
        let file_error = handles.1;
        let confirmed = handles.2;
        let phase = handles.3;
        let progress = handles.4;
        let error = handles.5;
        let result = handles.6;
        let _storage_key = raw!(
            "cx.hydrate('lific:native-archive-import-pending:'+${account}.toString())",
            "".to_owned()
        );

        let _set_progress = |percent: UsizeSurrogate| {
            progress.set(Some(percent));
        };
        let _set_uploading = || {
            phase.set("uploading".to_owned());
            error.set("".to_owned());
        };
        let _set_processing = || {
            phase.set("processing".to_owned());
            progress.set(None);
            error.set("".to_owned());
        };
        let _mark_unknown = || {
            let disposed = raw!("cx.hydrate(cx.abortSignal.aborted)", false);
            if disposed {
                return;
            }
            phase.set("unknown".to_owned());
            error.set("The import outcome could not be checked. Check the project list before importing again.".to_owned());
            progress.set(None);
        };
        let _terminal = |status: UsizeSurrogate,
                         body: StringSurrogate,
                         network_error: BoolSurrogate| {
            let _finish = async || {
                let fresh = current_session(account.clone()).await;
                let disposed = raw!("cx.hydrate(cx.abortSignal.aborted)", false);
                if disposed {
                    return;
                }
                if !fresh.current {
                    phase.set("unknown".to_owned());
                    error.set("The session changed while importing. Check the project list before importing again.".to_owned());
                    progress.set(None);
                    return;
                }
                if fresh.fingerprint != fingerprint {
                    phase.set("unknown".to_owned());
                    error.set("The session changed while importing. Check the project list before importing again.".to_owned());
                    progress.set(None);
                    return;
                }
                if network_error {
                    phase.set("unknown".to_owned());
                    error.set("The connection was lost while importing. Check the project list before importing again.".to_owned());
                    progress.set(None);
                } else {
                    if status == 201 {
                        let decoded = raw!(
                            "cx.hydrate(JSON.parse(${body}.toString()))",
                            ImportResult::default()
                        );
                        raw!(
                            "try { sessionStorage.removeItem(${_storage_key}.toString()); } catch {}",
                            ()
                        );
                        result.set(Some(decoded));
                        phase.set("success".to_owned());
                        error.set("".to_owned());
                    } else {
                        raw!(
                            "try { sessionStorage.removeItem(${_storage_key}.toString()); } catch {}",
                            ()
                        );
                        phase.set("error".to_owned());
                        error.set(body.to_owned());
                        progress.set(None);
                    }
                }
            };
            raw!("void ${_finish}().catch(()=>${_mark_unknown}());", ());
        };
        let _begin = |_submit_event: Event| {
            if phase.get() != "idle" {
                if phase.get() != "error" {
                    return;
                }
            }
            if !confirmed.get() {
                error.set("Confirm that this archive may contain sensitive content.".to_owned());
                return;
            }
            if file_name.get().is_empty() {
                error.set("Choose a Lific project archive.".to_owned());
                return;
            }
            if !file_error.get().is_empty() {
                error.set(file_error.get());
                return;
            }
            let has_file = raw!(
                "cx.hydrate(Boolean(${_submit_event}.inner.target.closest('form[data-native-project-import-form]')?.querySelector('input[type=file]')?.files?.[0]))",
                false
            );
            if !has_file {
                error.set("Choose a Lific project archive.".to_owned());
                return;
            }
            phase.set("uploading".to_owned());
            progress.set(Some(0));
            error.set("".to_owned());
            result.set(None);
            raw!(
                "try { sessionStorage.setItem(${_storage_key}.toString(),'1'); } catch {}",
                ()
            );
            raw!(
                r#"const form=${_submit_event}.inner.target.closest('form[data-native-project-import-form]');
                   if(!form)return;
                   const data=new FormData(form);
                   for(const field of [...data.keys()]) if(field!=='archive') data.delete(field);
                   if(!(data.get('archive') instanceof File))return;
                   const ownerElement=document.getElementById('native-project-import-owner');
                   if(!ownerElement)return;
                   const transfer=document.nativeArchiveImportTransfer;
                   const token=transfer?.key===ownerElement.dataset.nativeArchiveOwner
                     ? transfer : {key:ownerElement.dataset.nativeArchiveOwner};
                   token.host=ownerElement;
                   document.nativeArchiveImportTransfer=token;
                   const request=new XMLHttpRequest();
                   request.open('POST',${_upload_url}.toString());
                   request.setRequestHeader('X-Lific-Import-Session',${fingerprint}.toString());
                   request.upload.addEventListener('progress',event=>{
                     if(event.lengthComputable){
                       const percent=Math.floor(event.loaded*100/event.total);
                       if(request.nativeLastProgress!==percent){
                         request.nativeLastProgress=percent;
                         token.progress=percent;
                         token.host?.nativeArchiveProgress?.(percent);
                       }
                     }
                   });
                   request.upload.addEventListener('load',()=>{token.phase='processing';token.host?.nativeArchiveProcessing?.()});
                   request.addEventListener('load',()=>{token.terminal={status:request.status,body:request.responseText,network:false};token.host?.nativeArchiveTerminal?.(request.status,request.responseText,false)});
                   request.addEventListener('error',()=>{token.terminal={status:0,body:'',network:true};token.host?.nativeArchiveTerminal?.(0,'',true)});
                   request.addEventListener('abort',()=>{token.terminal={status:0,body:'',network:true};token.host?.nativeArchiveTerminal?.(0,'',true)});
                   token.phase='uploading';token.progress=0;token.terminal=null;token.request=request;
                   token.host?.nativeArchiveUploading?.();
                   request.send(data);"#,
                ()
            );
        };
        let _restore_pending = || {
            let pending = raw!(
                "cx.hydrate((()=>{try{return sessionStorage.getItem(${_storage_key}.toString())==='1'}catch{return false}})())",
                false
            );
            if pending {
                if phase.get() == "idle" {
                    phase.set("unknown".to_owned());
                    error.set("An earlier import may have completed. Check the project list before importing again.".to_owned());
                }
            }
        };
        raw!("${_restore_pending}();", ());
        let _leave_import_page = || {
            if phase.get() == "idle" {
                file_name.set("".to_owned());
                file_error.set("".to_owned());
                confirmed.set(false);
            } else if phase.get() == "error" {
                file_name.set("".to_owned());
                file_error.set("".to_owned());
                confirmed.set(false);
            }
        };
        raw!(
            r#"const owner=document.getElementById('native-project-import-owner');
               const transferred=document.nativeArchiveImportTransfer;
               delete document.nativeArchiveImportTransfer;
               const ownerKey=owner.dataset.nativeArchiveOwner;
               const token=transferred?.key===ownerKey
                 ? transferred : {key:ownerKey,host:owner};
               token.host=owner;
               document.nativeArchiveImportTransfer=token;
               owner.nativeArchiveProgress=percent=>${_set_progress}(cx.hydrate({t:'usize',bits:Number(${usize_width}.toString()),v:String(percent)}));
               owner.nativeArchiveUploading=()=>${_set_uploading}();
               owner.nativeArchiveProcessing=()=>${_set_processing}();
               owner.nativeArchiveTerminal=(status,body,network)=>${_terminal}(cx.hydrate({t:'usize',bits:Number(${usize_width}.toString()),v:String(status)}),cx.hydrate(body),cx.hydrate(network));
               if(token.phase==='uploading'){owner.nativeArchiveUploading();${_set_progress}(cx.hydrate({t:'usize',bits:Number(${usize_width}.toString()),v:String(token.progress??0)}));}
               if(token.phase==='processing')${_set_processing}();
               if(token.terminal)owner.nativeArchiveTerminal(token.terminal.status,token.terminal.body,token.terminal.network);
               document.addEventListener('topcoat:before-page-replace',event=>{
                 const next=event.detail.nextDocument.getElementById('native-project-import-owner');
                 const transferring=next?.dataset.nativeArchiveOwner===ownerKey;
                 if(transferring)document.nativeArchiveImportTransfer=token;
                 token.host=null;
                 const nextForm=event.detail.nextDocument.querySelector('[data-native-project-import-form]');
                 if(!nextForm) ${_leave_import_page}();
               },{signal:cx.abortSignal});
               cx.abortSignal.addEventListener('abort',()=>{
                 token.host=null;
                 delete owner.nativeArchiveProgress;
                 delete owner.nativeArchiveUploading;
                 delete owner.nativeArchiveProcessing;
                 delete owner.nativeArchiveTerminal;
               },{once:true});"#,
            ()
        );
        raw!(
            "document.addEventListener('submit',event=>{if(event.target.closest('[data-native-project-import-form]')){event.preventDefault();${_begin}(cx.event(event));}},{signal:cx.abortSignal});",
            ()
        );
    });
    handler.into_evaluated_and_js().1
}
