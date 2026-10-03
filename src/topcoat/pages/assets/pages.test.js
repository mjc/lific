const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');

const source=fs.readFileSync(`${__dirname}/pages.js`,'utf8');
const scope={globalThis:{},URLSearchParams,Set,Map,Blob,FormData,Option:class{}};
vm.runInNewContext(source,scope);
const {markdown}=scope.globalThis.LificTopcoatPages;

test('page attachment fragment and query targets retain and normalize their selected lines',async()=>{
 for(const location of [{search:'',hash:'#att12-L4-2'},{search:'?att=att12-L2-4',hash:''},{search:'?att=att99-L1',hash:'#att12-L2-4'}]){
  const selected=[],rows=[];let scrolled=false;
  const viewer={replaceChildren(){rows.length=0;},append:node=>rows.push(node)};
  const status={textContent:''};const row={scrollIntoView(){},classList:{add(){}}};
  const doc={getElementById:id=>id==='attachment-12'?row:null,createElement:tag=>({dataset:{},children:[],append(node){this.children.push(node);},setAttribute(name,value){if(name==='data-selected'&&value==='true')selected.push(this.dataset.line);},scrollIntoView(){scrolled=true;}})};
  const app=Object.create(scope.globalThis.LificTopcoatPages.PagesController.prototype);
  Object.assign(app,{win:{location},doc,root:{querySelector:selector=>selector==='[data-page-attachment-viewer]'?viewer:status},generation:1,current:()=>true,comments:[],attachments:[{id:12,mime:'text/plain'}],commentAttachments:new Map(),previewGeneration:0,attachmentClient:{text:async()=>({ok:true,text:'one\ntwo\nthree\nfour\nfive'})}});
  await app.followDeepLink(1);
  assert.deepEqual(selected,['2','3','4']);assert.equal(scrolled,true);
  assert.equal(rows[0].children.map(line=>line.textContent).join(''),'one\ntwo\nthree\nfour\nfive');
 }
});

test('markdown escapes raw HTML and keeps links, mentions and fenced code inert',()=>{
 const html=markdown('# Page\n\n<script>alert(1)</script> [docs](https://example.test/a) @riley and `@hidden`\n\n```html\n<img src=x onerror=alert(1)>\n@hidden\n```');
 assert.match(html,/<h1>Page<\/h1>/);assert.doesNotMatch(html,/<script>/);assert.doesNotMatch(html,/<img/);
 assert.match(html,/rel="nofollow noopener"/);assert.match(html,/<span class="tc-page-mention" data-page-mention="riley">@riley<\/span>/);
 assert.doesNotMatch(html,/data-page-mention="hidden"/);
 assert.match(html,/&lt;img src=x onerror=alert\(1\)&gt;/);
});

test('markdown retains tables, task lists, nested list indentation and code spans',()=>{
 const html=markdown('| Name | State |\n| --- | --- |\n| Page | active |\n\n- [x] Published\n  - `reader` access');
 assert.match(html,/<table>/);assert.match(html,/<th>Name<\/th>/);assert.match(html,/<td>Page<\/td>/);
 assert.match(html,/<input type="checkbox" disabled checked/);assert.match(html,/<ul>[\s\S]*<li><code>reader<\/code> access<\/li>/);
});

test('markdown preserves blank lines in fenced code and mixed paragraphs/lists',()=>{
 const html=markdown('```rust\nfn main() {\n\n    println!("<safe>");\n}\n```\n\nBefore list\n- first\n1. second\nAfter list');
 assert.match(html,/<pre><code>fn main\(\) \{\n\n    println!\(&quot;&lt;safe&gt;&quot;\);\n\}<\/code><\/pre>/);
 assert.match(html,/<p>Before list<\/p><ul><li>first<\/li><\/ul><ol><li>second<\/li><\/ol><p>After list<\/p>/);
});

test('markdown links do not double escape query separators',()=>{
 const html=markdown('[Search](https://example.test/?a=1&b=2)');
 assert.match(html,/href="https:\/\/example\.test\/\?a=1&amp;b=2"/);
 assert.doesNotMatch(html,/amp;amp/);
});

test('page list and detail links keep the routing deployment prefix',()=>{
 const app=Object.create(scope.globalThis.LificTopcoatPages.PagesController.prototype);
 Object.assign(app,{win:{LificTopcoatRouting:{href:route=>`/app${route}`}},public:false,projectName:'ENG'});
 assert.equal(app.detailHref(22),'/app/ENG/pages/22');assert.equal(app.listHref(),'/app/ENG/pages');
 app.public=true;assert.equal(app.detailHref(22),'/app/public/ENG/pages/22');
});

test('page fragment targets supersede both cold-load query target kinds',async()=>{
 const app=Object.create(scope.globalThis.LificTopcoatPages.PagesController.prototype);const scrolled=[];
 Object.assign(app,{win:{location:{search:'?comment=9&att=att12',hash:'#comment-10'}},doc:{getElementById:id=>({scrollIntoView(){scrolled.push(id);},classList:{add(){}}})},comments:[{id:9},{id:10}],hasOlderComments:false,current:()=>true,attachments:[],commentAttachments:new Map()});
 await app.followDeepLink(1);assert.deepEqual(scrolled,['comment-10']);
});
