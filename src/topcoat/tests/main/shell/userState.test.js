const {test}=require('node:test');const assert=require('node:assert/strict');
const {sessionFixture,identity}=require('./session-harness.js');
const original={id:1,username:'test',display_name:'Before',email:'test@example.com',is_admin:false};
const response=data=>({ok:true,status:200,json:async()=>data});
function setup(fetch){const values=new Map([['lific_token','session']]);globalThis.localStorage={getItem:key=>values.get(key)??null,setItem:(key,value)=>values.set(key,value),removeItem:key=>values.delete(key)};const f=sessionFixture(fetch);return {...f,app:identity.controller({session:f.session,navigate(){}})};}
test('successful profile publication reaches all subscribers immediately',async()=>{
 const f=setup(async(path,options)=>response(options.method==='PATCH'?{...original,display_name:'After'}:original));
 const shell=[''],settings=[''];const listener=()=>{shell.push(f.session.state.user?.display_name??'');settings.push(f.session.state.user?.display_name??'');};f.window.addEventListener('lific:account-change',listener);
 await f.session.refreshAccount();await f.app.changeProfile({display_name:'After'});assert.deepEqual(shell,['','Before','After']);assert.deepEqual(settings,shell);
});
test('late me response cannot replace a newer profile',async()=>{
 let release;const f=setup(async(path,options)=>options.method==='PATCH'?response({...original,display_name:'Saved'}):new Promise(resolve=>release=()=>resolve(response(original))));
 const pending=f.session.refreshAccount();await f.app.changeProfile({display_name:'Saved'});release();await pending;
 assert.equal(f.session.state.user?.display_name,'Saved');
});
test('unchanged revision accepts me, sign-out invalidates pending loads',async()=>{
 let release,held=false;const f=setup(async()=>held?new Promise(resolve=>release=()=>resolve(response(original))):response(original));
 await f.session.refreshAccount();assert.deepEqual(f.session.state.user,original);held=true;const pending=f.session.refreshAccount();f.session.clearSession();release();await pending;assert.equal(f.session.state.user,null);
});
test('direct store updates also invalidate stale loads',async()=>{
 let release;const f=setup(async()=>new Promise(resolve=>release=()=>resolve(response(original))));f.session.state.user=original;
 const pending=f.session.refreshAccount();f.session.state.user={...original,display_name:'Updated'};release();await pending;assert.equal(f.session.state.user?.display_name,'Updated');
});
