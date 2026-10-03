const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, 'collaboration.js'), 'utf8');
const context = {globalThis:{lificSession:{state:{user:{id:3}}}}, document:undefined, window:undefined, URLSearchParams, CustomEvent:class {}};
vm.runInNewContext(source, context);
const ui = context.globalThis.LificTopcoatIssueCollaboration;

test('direct comment query targets scroll to the loaded comment',async()=>{
  context.globalThis.location={search:'?comment=9',hash:''};
  let scrolled=false,highlighted=false;
  const target={classList:{add:name=>{highlighted=name==='tc-comment--target';}},scrollIntoView:()=>{scrolled=true;}};
  const root={isConnected:true,_comments:[{id:9}],querySelector:selector=>selector==='#comment-9'?target:{hidden:true}};
  assert.equal(await ui.resolveCommentHash(root),true);
  assert.equal(scrolled,true);assert.equal(highlighted,true);
});

test('issue attachment fragment and query links open text at the requested lines',async()=>{
  for(const location of [{search:'',hash:'#att12-L4-2'},{search:'?att=att12-L2-4',hash:''}]){
    context.globalThis.location=location;
    const rows=[],selected=[];let scrolled=false;
    const output={hidden:true,replaceChildren(){rows.length=0;},append:node=>rows.push(node)};
    const doc={createElement:()=>({dataset:{},setAttribute(name,value){if(name==='data-selected'&&value==='true')selected.push(this.dataset.line);},scrollIntoView(){scrolled=true;}})};
    const card={dataset:{attachmentKind:'text'},ownerDocument:doc,classList:{add(){}},querySelector:()=>output,scrollIntoView(){}};
    const root={isConnected:true,_collabGeneration:1,_attachmentClient:{text:async()=>({ok:true,text:'one\ntwo\nthree\nfour\nfive'})},querySelector:selector=>selector==='[data-attachment-id="12"]'?card:null};
    assert.equal(await ui.resolveAttachmentTarget(root,1),true);
    assert.equal(output.hidden,false);assert.equal(scrolled,true);
    assert.deepEqual(selected,['2','3','4']);
    assert.equal(rows.map(row=>row.textContent).join(''),'one\ntwo\nthree\nfour\nfive');
  }
});

test('an attachment preview cannot publish after its issue changes',async()=>{
  context.globalThis.location={search:'?att=att12-L2-4',hash:''};
  let release,rendered=false;
  const card={dataset:{attachmentKind:'text'},classList:{add(){}},scrollIntoView(){},querySelector(){rendered=true;}};
  const root={isConnected:true,_collabGeneration:1,_attachmentClient:{text:()=>new Promise(resolve=>{release=()=>resolve({ok:true,text:'old content'});})},querySelector:()=>card};
  const pending=ui.resolveAttachmentTarget(root,1);
  root._collabGeneration++;release();
  assert.equal(await pending,false);assert.equal(rendered,false);
});

test('comment rows escape server content and only offer author controls when enabled', () => {
  const comment = {id:7,user_id:3,author:'<script>',author_display_name:'<Admin>',created_at:'invalid',content:'<img src=x onerror=alert(1)>\n@sam'};
  const owner = ui.commentMarkup(comment, true);
  assert.match(owner, /&lt;Admin&gt;/);
  assert.match(owner, /&lt;img src=x onerror=alert\(1\)&gt;\n@sam/);
  assert.match(owner, /data-comment-edit="7"/);
  assert.match(owner, /id="comment-7"/);
  assert.match(owner, /href="#comment-7"/);
  assert.doesNotMatch(ui.commentMarkup(comment, false), /data-comment-delete/);
});

test('relation labels preserve each direction and render an empty state', () => {
  const list = {innerHTML:'',get children(){return {length:(this.innerHTML.match(/<li/g)||[]).length};}};
  const root = {dataset:{identifier:'ENG-2',editEnabled:'true',blockedBy:'ENG-1',blocks:'ENG-3',relatesTo:'',duplicates:'ENG-4',duplicatedBy:''},querySelector:()=>list};
  ui.renderRelations(root);
  assert.match(list.innerHTML, /blocked by: ENG-1/);
  assert.match(list.innerHTML, /blocks: ENG-3/);
  assert.match(list.innerHTML, /duplicates: ENG-4/);
  assert.equal((list.innerHTML.match(/data-relation-remove/g)||[]).length,3);
  root.dataset.blockedBy=''; root.dataset.blocks=''; root.dataset.duplicates='';
  ui.renderRelations(root);
  assert.match(list.innerHTML,/No relations/);
});

test('wait rows distinguish user/date blockers and expose clear only with edit access', () => {
  const list = {innerHTML:'',get children(){return {length:(this.innerHTML.match(/<li/g)||[]).length};}};
  const root = {dataset:{editEnabled:'false'},querySelector:()=>list};
  ui.renderWaits(root,[{id:9,kind:'user',username:'sam',display_name:'Sam',state:'holding',note:'Review'},{id:10,kind:'date',earliest:'2026-10-04',latest:'2026-10-05',state:'due',note:''}]);
  assert.match(list.innerHTML,/Waiting for Sam/);
  assert.match(list.innerHTML,/2026-10-04 through 2026-10-05/);
  assert.doesNotMatch(list.innerHTML,/data-wait-clear/);
  root.dataset.editEnabled='true'; ui.renderWaits(root,[{id:9,kind:'user',username:'sam',state:'holding'}]);
  assert.match(list.innerHTML,/data-wait-clear="9"/);
});

test('new comment fragment supersedes a cold-load comment query',async()=>{
 context.globalThis.location={search:'?comment=9',hash:'#comment-10'};let selected;
 const root={isConnected:true,_comments:[{id:9},{id:10}],querySelector:selector=>({classList:{add(){}},scrollIntoView(){selected=selector;}})};
 assert.equal(await ui.resolveCommentHash(root),true);assert.equal(selected,'#comment-10');
});
test('new attachment fragment supersedes the cold-load attachment query',async()=>{
 context.globalThis.location={search:'?att=att12',hash:'#att13'};let selected;
 const root={isConnected:true,querySelector:selector=>({dataset:{attachmentKind:'file'},classList:{add(){}},scrollIntoView(){selected=selector;}})};
 assert.equal(await ui.resolveAttachmentTarget(root),true);assert.equal(selected,'[data-attachment-id="13"]');
});

test('issue fragment navigation suppresses the other query target kind',async()=>{
 const root={isConnected:true,querySelector(){assert.fail('outdated query target selected');}};
 context.globalThis.location={search:'?comment=9',hash:'#att13'};assert.equal(await ui.resolveCommentHash(root),false);
 context.globalThis.location={search:'?att=att12',hash:'#comment-10'};assert.equal(await ui.resolveAttachmentTarget(root),false);
});
