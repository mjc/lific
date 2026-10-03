const vm=require('node:vm');const {read}=require('./source.js');
// Inspect the live counter's buckets without substituting the old implementation.
const source=read('dashboard/assets/dashboard.js').replace('    seed(value, now)', '    inspect() { return {baseline,baselineAt,events}; },\n    seed(value, now)');
const context={console,AbortController};vm.runInNewContext(source,context);
const dashboard=context.LificTopcoatDashboard;
function createActivityRateCounter(){
 const actual=dashboard.createActivityCounter();
 return {seed:({dayCount},now)=>actual.seed(dayCount,now),record:now=>actual.record(now),reset:()=>actual.reset(),counts(now){
  const {events,baseline,baselineAt}=actual.inspect();
  // These expressions are the ones used by production rate(), evaluated for
  // each requested window so all original counter assertions remain visible.
  const count=source.match(/const value = ([^;]+);\n        if \(value >= 2\)/)[1];
  const day=source.match(/return \{value: (events.length[^,]+), unit: 'updates\/day'/)[1];
  const result={};for(const [name,duration]of [['perSecond',1000],['perMinute',60000],['perHour',3600000]])result[name]=vm.runInNewContext(count,{events,now,duration});
  result.perDay=vm.runInNewContext(day,{events:events.filter(at=>at>=now-86400000),baseline,baselineAt,now});return result;
 }};
}
function controller(){return new dashboard.DashboardController({identity:()=>1,now:()=>0,delay:()=>0,cancel(){}});}
function parseActivityBaseline(dayCount){const c=controller();c.handleEvent({type:'activity.baseline',day_count:dayCount});return c.activityReady?{dayCount:c.counter.inspect().baseline}:null;}
function isActivityRealtimeEvent(type){const c=controller();c.handleEvent({type});return c.counter.inspect().events.length>0;}
function selectActivityRate(counts){const c=dashboard.createActivityCounter();c.seed(counts.perDay-counts.perHour,0);const limits=[counts.perSecond,counts.perMinute,counts.perHour],times=[0,-1001,-60001];let previous=0;for(let i=0;i<3;i++){for(let j=previous;j<limits[i];j++)c.record(times[i]);previous=limits[i];}return c.rate(0);}
module.exports={createActivityRateCounter,parseActivityBaseline,isActivityRealtimeEvent,selectActivityRate,dashboard};
