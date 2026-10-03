const {FilesController}=require('../../../files/assets/files.js');
const {subject}=require('./subjects.js');
class Node {
 constructor(tag){this.tag=tag;this.children=[];this.dataset={};this.textContent='';this.style={cssText:''};this.listeners=new Map();this.attributes={};}
 append(...children){this.children.push(...children);}
 replaceChildren(...children){this.children=children;}
 showModal(){this.open=true;}
 setAttribute(name,value){this.attributes[name]=value;if(name.startsWith('data-'))this.dataset[name.slice(5).replace(/-([a-z])/g,(_,letter)=>letter.toUpperCase())]=value;}
 addEventListener(name,callback){const callbacks=this.listeners.get(name)||[];callbacks.push(callback);this.listeners.set(name,callbacks);}
 click(event={}){for(const callback of this.listeners.get('click')||[])callback({target:this,currentTarget:this,shiftKey:false,preventDefault(){},...event});}
}
async function preview(text,{id=12,location={origin:'https://host',pathname:'/LIF/issues/LIF-1',search:'',hash:''}}={}){
 const nodes=new Map(),dialog=new Node('dialog'),content=new Node('div');
 nodes.set('[data-files-viewer]',dialog);nodes.set('[data-files-viewer-content]',content);
 const querySelector=selector=>{if(!nodes.has(selector))nodes.set(selector,new Node('div'));return nodes.get(selector);};
 dialog.querySelector=querySelector;
 const doc={body:{dataset:{}},createElement:tag=>new Node(tag)};
 const win={location,navigator:{clipboard:{writeText:async value=>{win.copiedLink=value;}}},LificTopcoatAttachments:subject('attachments/production')};
 const controller=Object.create(FilesController.prototype);
 Object.assign(controller,{root:{querySelector},doc,win,rows:[{id,filename:'build.log',mime:'text/plain',size_bytes:text.length}],orphans:[],previewGeneration:0,
  session:{resolve:()=>({url:'/api/attachments/'+id})},attachmentClient:{text:async()=>({ok:true,text})}});
 await controller.preview(id);
 return {dialog,content,nodes,controller};
}
function spans(content){return content.children.flatMap(node=>node.children.length?node.children.map(child=>({text:child.textContent,style:child.style??{}})):[{text:node.textContent,style:{}}]);}
async function ansiLineToSpans(text){const rendered=await preview(text);return {spans:spans(rendered.content)};}
async function ansiToSpans(text){const result=await ansiLineToSpans(text);return result.spans.flatMap(span=>span.text.split('\n').map(text=>[{text,style:span.style}]));}
async function stripAnsi(text){const result=await ansiLineToSpans(text);return result.spans.map(span=>span.text).join('');}
async function hasAnsi(text){return await stripAnsi(text)!==text;}
async function ansiStyleToCss(style){
 let codes=[];
 for(const [key,prefix]of [['fg','38'],['bg','48']])if(style[key]){const hex=style[key].replace('#','');codes.push(`${prefix};2;${[0,2,4].map(offset=>parseInt(hex.slice(offset,offset+2),16)).join(';')}`);}
 for(const [key,code]of [['bold','1'],['underline','4'],['strike','9'],['inverse','7']])if(style[key])codes.push(code);
 const result=await preview(codes.length?`\u001b[${codes.join(';')}mx`:'x');
 return result.content.children[0]?.children[0]?.style.cssText??result.content.children[0]?.style.cssText??'';
}
module.exports={preview,ansiLineToSpans,ansiToSpans,stripAnsi,hasAnsi,ansiStyleToCss};
