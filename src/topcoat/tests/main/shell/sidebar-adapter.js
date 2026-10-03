const vm=require('node:vm');
const {read}=require('./source.js');
function element(){return {dataset:{},style:{setProperty(){}},setAttribute(){},addEventListener(){}};}
function fixture(fontSize=16,preferred) {
 const shell=element(),handle=element(),toggle=element();shell.querySelector=selector=>selector==='[data-sidebar-toggle]'?toggle:handle;
 const document={body:{dataset:{}},documentElement:{},querySelector:()=>shell};
 const location={href:'http://localhost/',hash:'',pathname:'/',search:''};
 const window={location,addEventListener(){},history:{length:2}};
 let localStorage;try{localStorage=globalThis.localStorage;}catch{localStorage={getItem(){throw Error('Unavailable')},setItem(){throw Error('Unavailable')},removeItem(){throw Error('Unavailable')}};}
 const code=read('shell/assets/shell.js').replace(/\}\)\(\);\s*$/,`window.testSidebar={read,persist,size,getPreferred:()=>preferred,getMetrics:()=>metrics,setPreferred:value=>{preferred=value}};})();`);
 vm.runInNewContext(code,{window,location,document,localStorage,getComputedStyle:()=>({fontSize:String(fontSize)}),ResizeObserver:class{observe(){}},URL});
 if(preferred!==undefined){window.testSidebar.setPreferred(preferred);window.testSidebar.size(fontSize);}
 return window.testSidebar;
}
module.exports={loadSidebarWidthPreference:()=>fixture().getPreferred(),saveSidebarWidth:value=>fixture().persist('lific:sidebar:width',value===null?null:String(value)),sidebarSizing:(preferred,fontSize)=>fixture(fontSize,preferred).getMetrics()};
