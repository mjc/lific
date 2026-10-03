// main/web/tests/pendingPatch.test.ts decision assertions translated to actual settings controller outcomes.
const {test, expect, product} = require('./harness');
const identity=product('identity/assets/identity.js','LificTopcoatIdentity');
const stale={ok:false,status:403,code:'recent_auth_required',error:'Sign in again'};
function fixture(){
 const sent=[],gates=[],stored={};
 const app=identity.controller({session:{saveSession(){},async request(route,options={}){
  if(route==='/auth/me/refresh')return {ok:true,data:{token:'new-session'}};
  if(route==='/instance/settings'&&options.method==='PATCH'){
   const patch=JSON.parse(options.body);sent.push(patch);
   const result=await new Promise(resolve=>gates.push(resolve));
   if(result.ok){Object.assign(stored,patch);return {...result,data:{...stored}};}return result;
  }
  if(route==='/instance/settings')return {ok:true,data:{...stored}};
  if(route==='/users')return {ok:true,data:[]};
  throw new Error(`Unexpected controller request ${route}`);
 }}});
 app.state.settings={};app.state.instance={auth_required:true};
 return {app,sent,stored,finish:(index,result={ok:true})=>gates[index](result),
  async park(patch={a:1}){const pending=app.saveSettings(patch);gates.at(-1)(stale);await pending;},
  parked:()=>app.state.pendingAction?.kind==='settings'?app.state.pendingPatch:null};
}
const turn=()=>new Promise(resolve=>setImmediate(resolve));
test('with nothing parked, a patch is sent',async()=>{const f=fixture(),p=f.app.saveSettings({a:1});expect(f.sent).toEqual([{a:1}]);f.finish(0);expect((await p).ok).toBe(true);});
test('with a patch parked, later ones merge instead of being sent',async()=>{const f=fixture();await f.park({allow_signup:true});await f.app.saveSettings({instance_name:'Lific'});expect(f.sent).toEqual([{allow_signup:true}]);expect(f.parked()).toEqual({allow_signup:true,instance_name:'Lific'});});
test('a later edit to the same field wins',async()=>{const f=fixture();await f.park({allow_signup:true});await f.app.saveSettings({allow_signup:false});expect(f.parked()).toEqual({allow_signup:false});});
test("the drain's own send is always dispatched",async()=>{const f=fixture();await f.park({a:1});await f.app.saveSettings({b:2});const p=f.app.reauthenticate('password');await turn();expect(f.sent[1]).toEqual({a:1,b:2});f.finish(1);expect((await p).ok).toBe(true);});
test('an edit during the replay parks instead of racing it',async()=>{const f=fixture();await f.park();const p=f.app.reauthenticate('password');await turn();expect(f.parked()).toEqual({});await f.app.saveSettings({d:4});expect(f.parked()).toEqual({d:4});await f.app.saveSettings({e:5});expect(f.parked()).toEqual({d:4,e:5});expect(f.sent).toHaveLength(2);f.finish(1);await turn();f.finish(2);await p;});
test('a merged patch names every field that has to be restored on cancel',async()=>{const f=fixture();await f.park({allow_signup:false});await f.app.saveSettings({instance_name:'Lific'});expect(Object.keys(f.parked()).sort()).toEqual(['allow_signup','instance_name']);});
test('a stale refusal plus two queued fields yields one merged patch and no more sends',async()=>{
 const f=fixture(),p=f.app.saveSettings({allow_signup:true});f.finish(0,stale);expect((await p).ok).toBe(false);expect((await f.app.saveSettings({instance_name:'Lific'})).ok).toBe(false);expect((await f.app.saveSettings({session_lifetime_days:14})).ok).toBe(false);
 expect(f.sent).toEqual([{allow_signup:true}]);expect(f.parked()).toEqual({allow_signup:true,instance_name:'Lific',session_lifetime_days:14});const replay=f.app.reauthenticate('password');await turn();expect(f.sent).toHaveLength(2);expect(f.sent[1]).toEqual(f.sent[0]&&{allow_signup:true,instance_name:'Lific',session_lifetime_days:14});f.finish(1);await replay;expect(f.parked()).toBeNull();
});
test('stops on a failure, leaving whatever is parked for the prompt',async()=>{const f=fixture();await f.park();const p=f.app.reauthenticate('password');await turn();await f.app.saveSettings({c:3});f.finish(1,stale);expect((await p).ok).toBe(false);expect(f.sent).toHaveLength(2);expect(f.parked()).toEqual({a:1,c:3});});
test('finishes when a send lands and nothing arrived meanwhile',async()=>{const f=fixture();await f.park();const p=f.app.reauthenticate('password');await turn();f.finish(1);expect((await p).ok).toBe(true);expect(f.sent).toHaveLength(2);expect(f.parked()).toBeNull();});
test('continues with whatever arrived while the send was in flight',async()=>{const f=fixture();await f.park();const p=f.app.reauthenticate('password');await turn();await f.app.saveSettings({c:3});f.finish(1);await turn();expect(f.sent[2]).toEqual({c:3});f.finish(2);await p;expect(f.parked()).toBeNull();});
test('taking a snapshot clears the slot so new edits start a fresh one',async()=>{const f=fixture();await f.park();const p=f.app.reauthenticate('password');await turn();expect(f.sent[1]).toEqual({a:1});expect(f.parked()).toEqual({});await f.app.saveSettings({c:3});expect(f.parked()).toEqual({c:3});f.finish(1);await turn();f.finish(2);await p;});
test('A refused, B merged, C edited mid-replay: all three land',async()=>{const f=fixture();await f.park({allow_signup:true});expect((await f.app.saveSettings({instance_name:'Lific'})).ok).toBe(false);expect(f.parked()).toEqual({allow_signup:true,instance_name:'Lific'});const p=f.app.reauthenticate('password');await turn();await f.app.saveSettings({session_lifetime_days:14});expect(f.parked()).toEqual({session_lifetime_days:14});f.finish(1);await turn();f.finish(2);await p;expect(f.sent.slice(1)).toEqual([{allow_signup:true,instance_name:'Lific'},{session_lifetime_days:14}]);expect(f.stored).toEqual({allow_signup:true,instance_name:'Lific',session_lifetime_days:14});expect(f.parked()).toBeNull();});
test('a failed replay retains an edit made during it',async()=>{const f=fixture();await f.park();const p=f.app.reauthenticate('password');await turn();await f.app.saveSettings({session_lifetime_days:14});f.finish(1,{ok:false,status:500,error:'offline'});await p;expect(Object.keys(f.app.state.pendingPatch)).toContain('session_lifetime_days');});
