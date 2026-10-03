const {sessionFixture}=require('./session-harness.js');
const {session,window}=sessionFixture();
module.exports={getPublicProject:()=>session.state.publicProject,setPublicProject:project=>session.setPublicProject(project),scopedRoute:route=>session.scopedRoute(route),
 publicMirror:path=>{const r=session.resolve(path);return r.kind==='public'?r.url:null;},
 publicSynthetic:path=>{const r=session.resolve(path);return r.kind==='synthetic'?{status:r.status,body:r.body}:undefined;},
 onPublicScopeChange:callback=>{const listener=()=>callback(session.state.publicProject);window.addEventListener('lific:scope-change',listener);return()=>window.removeEventListener('lific:scope-change',listener);}};
