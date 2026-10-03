// The former configurable shared timer now belongs to live route controllers.
const {test}=require('node:test');const assert=require('node:assert/strict');
const {dashboard}=require('./activity-adapter.js');
const {read}=require('./source.js');
test('bounds refresh delay during a continuous realtime event burst',()=>{
 const match=read('dashboard/assets/dashboard.js').match(/Math\.min\((\d+), (\d+) - \(now - this\.firstInvalidation\)\)/);
 assert.ok(match,'Find the actual controller debounce and maximum wait');
 const debounce=Number(match[1]),maxWait=Number(match[2]),interval=debounce/2;
 let now=0,refreshCount=0,nextId=0;const timers=new Map();
 const controller=new dashboard.DashboardController({identity:()=>1,now:()=>now,
  delay:(callback,delay)=>{const id=++nextId;timers.set(id,{callback,at:now+delay});return id;},cancel:id=>timers.delete(id)});
 controller.load=()=>{refreshCount++;controller.firstInvalidation=null;};
 function advance(delta){const end=now+delta;for(;;){const due=[...timers.entries()].filter(([,timer])=>timer.at<=end).sort((a,b)=>a[1].at-b[1].at)[0];if(!due)break;now=due[1].at;timers.delete(due[0]);due[1].callback();}now=end;}
 try{
  for(let i=0;i<Math.ceil(maxWait/interval)+2;i++){
   controller.handleEvent({type:'issue.updated'});advance(interval);
   if((i+1)*interval>=maxWait)assert.ok(refreshCount>0,'A continuous burst must refresh by its maximum wait');
  }
  assert.ok(refreshCount>0);
 }finally{controller.dispose();timers.clear();}
});
