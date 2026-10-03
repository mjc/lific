const vm=require('node:vm');
const {read,lexical}=require('./source.js');
function sessionFixture(fetcher=(...args)=>globalThis.fetch(...args)) {
 const events=new EventTarget();const document={readyState:'loading',body:{dataset:{lificRequireSession:'false'}},querySelectorAll:()=>[],addEventListener(){}};
 const storage=globalThis.localStorage??{getItem:()=>null,setItem(){},removeItem(){}};
 const location={origin:'http://localhost',href:'http://localhost/',pathname:'/',hash:'',replace(){}};
 const window=Object.assign(events,{document,location});
 const source=read('session.rs').split('pub(crate) const BROWSER_SCRIPT: &str = r#"')[1].split('"#;')[0];
 vm.runInNewContext(source,{window,document,location,localStorage:storage,Headers,FormData,URLSearchParams,CustomEvent,fetch:(url,options)=>fetcher(url,{...options,headers:Object.fromEntries([...options.headers].map(([key,value])=>[key==='authorization'?'Authorization':key,value]))})});
 const session=window.lificSession;return {session,window,document,storage};
}
const identity=lexical('identity/assets/identity.js',['controller','recentAuth'],{location:{origin:'http://localhost'}});
function controllerFixture() {
 const fixture=sessionFixture();
 const app=identity.controller({session:fixture.session,navigate(){},storage:fixture.storage});
 return {...fixture,app};
}
module.exports={sessionFixture,controllerFixture,identity};
