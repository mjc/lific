const {subject}=require('./subjects.js');
const attachments=subject('attachments/production');
class Node extends EventTarget {
 constructor(doc){super();this.ownerDocument=doc;this.dataset={};this.children=[];}
 append(...nodes){this.children.push(...nodes);}
 replaceChildren(...nodes){this.children=nodes;}
 setAttribute(){}
 querySelector(){return null;}
 remove(){}
}
const settle=()=>new Promise(resolve=>setImmediate(resolve));
function createConcurrencyQueue(limit) {
 const win=new EventTarget();
 Object.assign(win,{localStorage:{getItem:()=>null},AbortController,CustomEvent});
 const doc={defaultView:win,activeElement:null,createElement(){return new Node(doc);}};
 const root=new Node(doc),entries=new Map();let id=0;
 const client={audience:()=> 'private:queue-fixture',upload(file){
   const entry=entries.get(file.name);
   let operation;
   try{operation=Promise.resolve(entry.task());}catch(error){operation=Promise.reject(error);}
   const result=operation.then(value=>({ok:true,data:{id:entry.id,filename:file.name,mime:'text/plain',size:1,value}}),error=>({ok:false,error:error.message}));
   operation.then(value=>settle().then(()=>entry.resolve(value)),error=>settle().then(()=>entry.reject(error)));
   return {result,abort(){}};
 }};
 const composer=attachments.createComposer({root,client,concurrency:limit,win});
 return {limit,get active(){return composer.items.filter(item=>item.status==='uploading').length;},
  get waiting(){return composer.items.filter(item=>['queued','preparing'].includes(item.status)).length;},
  add(task){const current=++id,name=`task-${current}.txt`;let resolve,reject;
   const promise=new Promise((res,rej)=>{resolve=res;reject=rej;});
   entries.set(name,{id:current,task,resolve,reject});
   void composer.enqueue([new File(['x'],name,{type:'text/plain'})]);return promise;
  },dispose(){composer.dispose();}};
}
module.exports={createConcurrencyQueue,settle};
