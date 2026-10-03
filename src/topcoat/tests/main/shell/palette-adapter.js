// Translate the old helper API to the code executed by the Topcoat palette.
// Expressions below are read from production; no old scoring implementation is retained.
const vm=require('node:vm');
const {read,lexical}=require('./source.js');
const source=read('palette/assets/palette.js');
const live=lexical('palette/assets/palette.js',['parseRefQuery','quality','distance','searchDocuments','searchLocalDocuments','catalogResults','localScoreToPaletteScore']);
const expression=(pattern)=>{const m=source.match(pattern);if(!m)throw new Error(`Palette expression missing: ${pattern}`);return m[1];};
const tokenizeExpression=expression(/const terms = ([^;]+);/);
const localExpression='localScoreToPaletteScore(score)';
const qualify=expression(/const identifier = (`[^`]+`);/);
const localScoreToPaletteScore=live.localScoreToPaletteScore;
const tokenize=query=>vm.runInNewContext(tokenizeExpression,{query});
const scoreDoc=(terms,doc)=>live.searchDocuments(terms.join(' '),[doc])[0]?.score??0;
const searchLocalDocs=(query,docs,limit=0)=>{const result=live.searchLocalDocuments(query,docs);return limit?result.slice(0,limit):result;};
const searchLocalDocsPerKind=(query,docs)=>live.searchDocuments(query,docs);
const refIdentifier=(project,ref)=>vm.runInNewContext(qualify,{ref,project:{identifier:project}});
const numberExpression=expression(/String\(doc\.identifier \?\? ''\)(\.match\([^)]*\)\?\.\[0\])/);
const refNumber=identifier=>vm.runInNewContext(`String(identifier??'')${numberExpression}??''`,{identifier});
const resultExpression=expression(/const results = ([\s\S]*?);\n        const groups/);
const publish=rows=>vm.runInNewContext(resultExpression,{local:rows,remote:[],catalog:{projects:[...new Set(rows.map(r=>r.route?.split('/')[1]))].map(identifier=>({identifier}))},scopedResults:results=>results,seen:new Set(),counts:new Map()});
const dedupeByKey=(rows,key)=>publish(rows.map((value,i)=>({route:key(value),kind:'test',score:0,value}))).map(row=>row.value);
const dedupeByIdentifier=(rows,taken)=>{
 const result=publish([...taken.filter(Boolean).map(identifier=>({route:identifier.toLowerCase(),kind:'test',score:0,taken:true})),...rows.map((value,i)=>({route:value.identifier?value.identifier.toLowerCase():`missing:${i}`,kind:'test',score:0,value}))]);
 return result.filter(row=>!row.taken).map(row=>row.value);
};
const catalogChanged=expression(/const changed = ([\s\S]*?);\n          catalog =/);
const projectCatalogChanged=(previous,projects)=>vm.runInNewContext(catalogChanged,{catalog:{projects:previous},projects});
const selectionExpression=expression(/selected = (next >= 0[^;]+);/);
const preserveSelection=(old,keys,previous)=>vm.runInNewContext(selectionExpression,{old,previous,items:keys,next:old?keys.indexOf(old):-1});
// Mount the actual controller and dispatch its actual route listeners. Only
// expose its generation gate/state initialization; do not implement invalidation.
function isStaleSearch(issued,now) {
 function element(){const node=Object.assign(new EventTarget(),{dataset:{},children:[],value:'',isConnected:true,style:{},setAttribute(){},removeAttribute(){},focus(){},scrollIntoView(){},close(){},showModal(){}});node.append=(...children)=>node.children.push(...children);node.replaceChildren=(...children)=>node.children=children;node.querySelectorAll=()=>[];return node;}
 const input=element(),list=element(),dialog=element(),nodes=new Map([['[data-palette-input]',input],['[data-palette-results]',list]]);
 dialog.querySelector=selector=>{if(!nodes.has(selector))nodes.set(selector,element());return nodes.get(selector);};
 const doc=Object.assign(new EventTarget(),{activeElement:null,querySelectorAll:()=>[],querySelector:selector=>selector==='[data-topcoat-palette]'?dialog:null,createElement:element,getElementById:()=>null});
 const route=project=>project?`/${project}/issues`:'/';
 const win=Object.assign(new EventTarget(),{location:{pathname:route(issued.projectIdent)},setTimeout,clearTimeout,localStorage:{getItem:()=>null},CustomEvent});
 const session={state:{publicProject:null,user:{id:1}},request:async()=>({ok:true,data:[]}),affordances:()=>({})};
 const instrumented=source.replace('      open: show, close: hide,','      open: show, close: hide, testBegin: generation=>{open=true;searchGeneration=generation;}, testCurrent: generation=>searchCurrent(generation,epoch),');
 if(instrumented===source)throw Error('Palette controller observation seam changed');
 const context={module:{exports:{}},console,URL,Date,Event,EventTarget,CustomEvent,setTimeout,clearTimeout};vm.runInNewContext(instrumented,context);
 const app=context.module.exports.mount({window:win,document:doc,session});app.testBegin(now.gen);
 try{if(issued.projectIdent!==now.projectIdent){win.location.pathname=route(now.projectIdent);win.dispatchEvent(new Event('popstate'));}return !app.testCurrent(issued.gen);}finally{app.dispose();}
}
module.exports={...live,tokenize,matchQuality:live.quality,boundedEditDistance:live.distance,scoreDoc,searchLocalDocs,searchLocalDocsPerKind,refIdentifier,refNumber,localScoreToPaletteScore,publish,dedupeByKey,dedupeByIdentifier,projectCatalogChanged,preserveSelection,isStaleSearch,
 PREFIX_MIN_TERM:Number(expression(/term.length < (\d+)/)),FUZZY_MIN_TERM:Number(expression(/fuzzy && term.length >= (\d+)/)),QUALITY:{exact:live.quality('palette','palette'),prefix:live.quality('pal','palette search'),wordPrefix:live.quality('sea','palette search'),substring:live.quality('ear','palette search'),fuzzy:live.quality('wark','warm read model')},LOCAL_SCORE_FLOOR:localScoreToPaletteScore(0),LOCAL_SCORE_CEIL:localScoreToPaletteScore(1),SERVER_SCORE_MAX:Number(expression(/score: (\d+) - index/)),LOCAL_HIT_SERVER_THRESHOLD:Number(expression(/wantFts = local\.filter[^;]+\.length < (\d+)/)),EXACT_REF_SCORE:Number(expression(/project.id === active\?\.id \? \d+ : (\d+)/)),CURRENT_PROJECT_REF_SCORE:Number(expression(/project.id === active\?\.id \? (\d+) :/))};
