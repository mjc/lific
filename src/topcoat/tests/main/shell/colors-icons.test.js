const {test}=require('node:test');const assert=require('node:assert/strict');const {lexical}=require('./source.js');
const {projectLink}=lexical('shell/assets/projects.js',['projectLink'],{window:{LificProjectIcons:require('../../../shell/assets/project-icons.js')}});
function rendered(value){const document={createElement:tagName=>({tagName,children:[],dataset:{},attributes:{},setAttribute(key,value){this.attributes[key]=value;},append(...children){this.children.push(...children);}})};document.createElementNS=(_,tagName)=>document.createElement(tagName);return projectLink(document,{id:1,identifier:'ONE',name:'One',emoji:value},null);}
test('project icons / accepts installed icons, emoji sequences, and the logo',()=>{
 const installed=rendered('lucide:Terminal');assert.ok(!installed.textContent.includes('lucide:Terminal'),'Installed icon must render its icon, not the serialized token');assert.ok(installed.children.some(child=>['svg','img'].includes(child.tagName)),'Installed icon must produce an actual SVG or image');
 for(const emoji of ['🚀','👩‍💻'])assert.equal(rendered(emoji).textContent,`${emoji} One`);
 const logo=rendered('lific:logo');assert.ok(!logo.textContent.includes('lific:logo'),'Logo must render its asset, not the serialized token');assert.ok(logo.children.some(child=>['svg','img'].includes(child.tagName)),'Logo must produce an actual SVG or image');
});
test('project icons / rejects arbitrary text and inherited properties instead of rendering them',()=>{
 for(const value of [null,undefined,'','Lucide:Terminal','lucide:MissingIcon','lucide:constructor','lucide:__proto__','constructor','hello'.repeat(1000)])assert.equal(rendered(value).textContent,'One',`${value} must fall back to the project name`);
});
