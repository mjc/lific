const fs=require('node:fs');
const path=require('node:path');
const {FilesController}=require('../../../files/assets/files.js');
class Node {
 constructor(tag){this.tag=tag;this.children=[];this.dataset={};this.textContent='';}
 append(...children){this.children.push(...children);}
 replaceChildren(...children){this.children=children;}
 add(child){this.append(child);}
 setAttribute(){}
 get childElementCount(){return this.children.length;}
}
const document={body:{dataset:{}},createElement:tag=>new Node(tag),createTextNode:text=>Object.assign(new Node('#text'),{textContent:text})};
function entityChipLabel(entity){
 const controller=Object.create(FilesController.prototype);
 Object.assign(controller,{doc:document,win:{},projectName:'LIF'});
 const node=controller.renderLinks({entities:[entity]}, {entities:[entity]});
 return node.children[0]?.textContent;
}
function uploaderOptions(rows){
 const controller=Object.create(FilesController.prototype),nodes=new Map();
 const root={querySelector:selector=>{if(!nodes.has(selector))nodes.set(selector,new Node('div'));return nodes.get(selector);},querySelectorAll:()=>[]};
 const OriginalOption=globalThis.Option;
 globalThis.Option=function(text,value){return Object.assign(new Node('option'),{textContent:text,value});};
 try{
  Object.assign(controller,{root,doc:document,rows,uploader:'',totalCount:rows.length,totalBytes:0,hasMore:false,renderRow:()=>new Node('article'),renderOrphans(){}});
  controller.render();return nodes.get('[data-files-uploader]').children.slice(1).map(node=>node.value);
 }finally{globalThis.Option=OriginalOption;}
}
function mimeClassLabel(kind){
 const source=fs.readFileSync(path.join(__dirname,'../../../files/mod.rs'),'utf8');
 const match=source.match(new RegExp(`data-files-mime="${kind}">"([^"]+)"`));
 if(!match)throw new Error(`No rendered MIME filter for ${kind}`);return match[1];
}
async function deleteConfirmMessage(count){
 let message;
 const controller=Object.create(FilesController.prototype);
 Object.assign(controller,{generation:1,rows:[{id:1,filename:'shot.png',uploader_id:7}],orphans:[],viewerId:7,isAdmin:false,canEdit:false,
   current:()=>true,session:{request:async()=>({ok:true,data:{entities:Array.from({length:count},()=>({}))}})},
   win:{confirm:value=>{message=value;return false;}}});
 await controller.remove(1);return message;
}
module.exports={entityChipLabel,uploaderOptions,mimeClassLabel,deleteConfirmMessage};
