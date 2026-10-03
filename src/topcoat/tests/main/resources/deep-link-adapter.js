const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const {preview}=require('./viewer-adapter.js');
const source=fs.readFileSync(path.join(__dirname,'../../../issue_detail/collaboration/assets/collaboration.js'),'utf8');
class Line {
 constructor(){this.dataset={};this.children=[];this.textContent='';this.classList={add(){}};}
 setAttribute(name,value){if(name==='data-selected')this.dataset.selected=value;}
 append(node){this.children.push(node);}
 replaceChildren(){this.children=[];}
 scrollIntoView(){}
}
async function readTarget(hash){
 const url=new URL('https://host/LIF/issues/LIF-1');url.hash=hash;
 const document={createElement:()=>new Line()};
 const cards=new Map(Array.from({length:100},(_,index)=>{const id=index+1,output=new Line();return [String(id),{id,output,ownerDocument:document,dataset:{attachmentKind:'text'},classList:{add(){}},scrollIntoView(){},querySelector:()=>output}];}));
 let requested;
 const root={isConnected:true,_collabGeneration:1,querySelector:selector=>cards.get(selector.match(/data-attachment-id="(\d+)"/)?.[1]),
  _attachmentClient:{text:async id=>{requested=id;return {ok:true,text:Array.from({length:1000},(_,index)=>`line ${index+1}`).join('\n')};}}};
 const context={globalThis:{location:url},URLSearchParams,CustomEvent};vm.runInNewContext(source,context);
 const found=await context.globalThis.LificTopcoatIssueCollaboration.resolveAttachmentTarget(root);
 if(!found||requested===undefined)return null;
 const selected=cards.get(String(requested)).output.children.filter(line=>line.dataset.selected==='true');
 return selected.length?{attachmentId:requested,start:Number(selected[0].dataset.line),end:Number(selected.at(-1).dataset.line)}:null;
}
async function selectableViewer(operation,{attachmentId=12,start=1,end=start,location={origin:'https://host',pathname:'/LIF/issues/LIF-1',search:'',hash:''}}={}) {
 const view=await preview(Array.from({length:1000},(_,index)=>`line ${index+1}`).join('\n'),{id:attachmentId,location});
 const nodes=[];const walk=node=>{nodes.push(node);for(const child of node.children)walk(child);};walk(view.content);
 const controls=nodes.filter(node=>node.dataset.line||node.dataset.attachmentLine);
 assert(controls.length>0,`${operation}: the production text viewer rendered ${view.content.children.map(node=>node.tag).join(',')} with no selectable line controls.`);
 const first=controls.find(node=>Number(node.dataset.line??node.dataset.attachmentLine)===start);
 assert(first,`${operation}: line ${start} is selectable`);
 assert.equal(typeof first.click,'function',`${operation}: line control exposes its real click interaction`);
 first.click();
 if(end!==start){const last=controls.find(node=>Number(node.dataset.line??node.dataset.attachmentLine)===end);assert(last);last.click({shiftKey:true});}
 return view;
}
async function formatLineAnchor(id,start,end=start){const view=await selectableViewer('formatLineAnchor',{attachmentId:id,start,end});return view.controller.win.location.hash.replace(/^#/,'');}
async function parseLineAnchor(value){return readTarget(value);}
async function lineTargetFromHash(hash){return readTarget(hash);}
async function selectedLocation(operation,anchor,location) {
 const target=await readTarget(anchor||location.hash);
 const view=await selectableViewer(operation,{...target,location});
 if(anchor===null){
  const controls=[];const walk=node=>{controls.push(node);node.children.forEach(walk);};walk(view.content);
  const clear=controls.find(node=>/clear.*(line|selection)/i.test(node.textContent));
  assert(clear,`${operation}: production text viewer offers Clear line selection`);clear.click();
 }
 return view;
}
async function routeWithLineTarget(route,anchor){const view=await selectedLocation('routeWithLineTarget',anchor,{origin:'https://host',pathname:'/',search:'',hash:'#'+route});return view.controller.win.location.hash.slice(1);}
async function routeWithoutLineTarget(route){const view=await selectedLocation('routeWithoutLineTarget',null,{origin:'https://host',pathname:'/',search:'',hash:'#'+route});return view.controller.win.location.hash.slice(1);}
async function hashWithLineTarget(hash,anchor){const view=await selectedLocation('hashWithLineTarget',anchor,{origin:'https://host',pathname:'/',search:'',hash});return view.controller.win.location.hash;}
async function fullLineLink(anchor,location){
 const view=await selectedLocation('fullLineLink',anchor,location);
 const controls=[];const walk=node=>{controls.push(node);node.children.forEach(walk);};walk(view.content);
 const copy=controls.find(node=>/Copy link/i.test(node.textContent));assert(copy,'production text viewer offers Copy link');
 await copy.click();return view.controller.win.copiedLink;
}
module.exports={formatLineAnchor,parseLineAnchor,lineTargetFromHash,routeWithLineTarget,routeWithoutLineTarget,hashWithLineTarget,fullLineLink};
