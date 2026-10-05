const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const sessionSource=fs.readFileSync(path.join(__dirname,'../../../session.rs'),'utf8').split('pub(crate) const BROWSER_SCRIPT: &str = r#"')[1].split('"#;')[0];
let session,sessionWindow;
function browserSession(){
 if(sessionWindow!==globalThis.window){
  sessionWindow=globalThis.window;
  const document={readyState:'loading',body:{dataset:{lificRequireSession:'false'}},addEventListener(){},querySelectorAll(){return[];}};
  const context={window:sessionWindow,document,localStorage:globalThis.localStorage,location:sessionWindow.location,CustomEvent,Headers,URLSearchParams,fetch:(...args)=>globalThis.fetch(...args)};
  vm.runInNewContext(sessionSource,context);session=sessionWindow.lificSession;
 }
 return session;
}
function onSessionChange(listener){const session=browserSession();const win=globalThis.window;win.addEventListener('lific:session-change',listener);return()=>win.removeEventListener('lific:session-change',listener);}
function saveSession(token){browserSession().saveSession(token);}
function clearSession(){browserSession().clearSession();}
module.exports={onSessionChange,saveSession,clearSession};
