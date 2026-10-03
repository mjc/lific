const {controllerFixture,identity}=require('./session-harness.js');
const needsReauth=identity.recentAuth;
async function refresh(password,userId,passwordless) {
 const {app}=controllerFixture();app.state.user={id:userId};app.state.instance={web_auto_login:passwordless};
 app.state.pendingAction={kind:'action',action:async()=>({ok:true})};
 const result=await app.reauthenticate(password);
 // The old helper returned a prompt hint; today the pending action owns the
 // prompt. A refused automatic refresh keeps that prompt available. A human's
 // failed password is reported by the submitted form rather than auto-retried.
 return result.ok?result:{ok:false,error:result.error,recoverable:passwordless&&app.state.pendingAction!==null};
}
async function retryOnceAfterReauth(action,reauth) {
 const fixture=controllerFixture();fixture.app.state.instance={web_auto_login:false};
 const original=fixture.session.request;let first,verified;
 fixture.session.request=async(path,options)=>{
  if(path==='/auth/keys'&&options?.method==='POST')return first=await action();
  if(path==='/auth/me/refresh'){
   verified=await reauth();
   return verified.ok?{ok:true,data:{token:'lific_sess_verified'}}:verified;
  }
  return original(path,options);
 };
 const result=await fixture.app.createKey('port');
 if(!result.pending)return first??result;
 const retried=await fixture.app.reauthenticate('hunter2');
 if(verified?.ok===false)return {ok:false,error:verified.recoverable&&fixture.app.state.pendingAction?'recent authentication required':retried.error,status:403};
 return retried;
}
module.exports={needsReauth,retryOnceAfterReauth,reauthenticateWithPassword:(password,id)=>refresh(password,id,false),reauthenticateWithoutPassword:id=>refresh('',id,true)};
