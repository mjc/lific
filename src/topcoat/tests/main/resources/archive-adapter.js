const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const settingsSource=fs.readFileSync(path.join(__dirname,'../../../project_settings/assets/project-settings.js'),'utf8');
const start=settingsSource.indexOf('download: async (path, filename) => {');
let position=settingsSource.indexOf('{',start),depth=1;
for(position++;depth;position++){if(settingsSource[position]==='{')depth++;if(settingsSource[position]==='}')depth--;}
const downloadSource=settingsSource.slice(start,position);
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
async function downloadProjectArchive(identifier,signal){
 const win={...globalThis.window,localStorage:globalThis.localStorage,fetch:(...args)=>globalThis.fetch(...args)};
 const context={win,identity:()=> 'private:archive-fixture',controller:{state:{project:{identifier}}}};
 const download=vm.runInNewContext(`({${downloadSource}}).download`,context);
 // The same route and filename are passed to the production download function.
 const result=await download(`/project-archives/${encodeURIComponent(identifier)}`,undefined,signal);
 return result.ok?{ok:true,...result.data}:result;
}
function onSessionChange(listener){const session=browserSession();const win=globalThis.window;win.addEventListener('lific:session-change',listener);return()=>win.removeEventListener('lific:session-change',listener);}
function saveSession(token){browserSession().saveSession(token);}
function clearSession(){browserSession().clearSession();}
module.exports={downloadProjectArchive,onSessionChange,saveSession,clearSession};
