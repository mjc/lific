const {test}=require('node:test');
const assert=require('node:assert/strict');
const publicUi=require('./public.js');

test('public byte formatting is stable for scrubbed attachment DTOs',()=>{
 assert.equal(publicUi.formatBytes(4),'4 B');
 assert.equal(publicUi.formatBytes(1536),'1.5 KB');
 assert.equal(publicUi.formatBytes(undefined),'');
});

test('renderer exports no DOM-dependent controller construction on the Node path',()=>{
 assert.equal(typeof publicUi.PublicController,'function');
 assert.equal(typeof publicUi.renderMarkdown,'function');
 assert.equal(typeof publicUi.attach,'function');
});

test('public navigation rewrites only supported routes within the current project',()=>{
 assert.equal(publicUi.publicHref('#/ENG/issues/ENG-2?comment=91','ENG'),'/public/ENG/issues/ENG-2?comment=91');
 assert.equal(publicUi.publicHref('/ENG/pages/22','ENG'),'/public/ENG/pages/22');
 assert.equal(publicUi.publicHref('#comment-91','ENG'),'#comment-91');
 for(const href of ['/ENG/issues/new','/ENG/plans/1','/ENG/settings','/OTHER/issues/OTHER-1','/public/ENG/files','javascript:bad()']) assert.equal(publicUi.publicHref(href,'ENG'),null,href);
});

test('attachment targets preserve single lines and reversed ranges from fragment and query links',()=>{
 assert.deepEqual(publicUi.attachmentTarget('#att31-L10-12'),{id:31,start:10,end:12});
 assert.deepEqual(publicUi.attachmentTarget('att31-L12-10'),{id:31,start:10,end:12});
 assert.deepEqual(publicUi.attachmentTarget('#att31-L10'),{id:31,start:10,end:10});
 assert.equal(publicUi.attachmentTarget('#att31-L0'),null);
});

test('public search matches abbreviated words and ranks exact names over preview matches',()=>{
 assert.ok(publicUi.searchScore('Alpa','Alpha')>=.25);
 assert.ok(publicUi.searchScore('alpha','Alpha')>publicUi.searchScore('alpha','Long alpha explanation'));
 assert.equal(publicUi.searchScore('missing','Alpha'),0);
});

test('public navigation retains prefixes and scope while leaving external and fragment links intact',()=>{
 const win={LificTopcoatRouting:{href:route=>`/app${route}`,path:route=>route.replace(/^\/app(?=\/|$)/,'')||'/'}};
 assert.equal(publicUi.publicHref('/ENG/pages/22','ENG',win),'/app/public/ENG/pages/22');
 assert.equal(publicUi.publicHref('/ENG/issues/ENG-2?comment=91','ENG',win),'/app/public/ENG/issues/ENG-2?comment=91');
 assert.equal(publicUi.publicHref('#comment-91','ENG',win),'#comment-91');
 assert.equal(publicUi.publicHref('https://example.test/docs','ENG',win),'https://example.test/docs');
 assert.equal(publicUi.publicHref('/OTHER/issues/OTHER-1','ENG',win),null);
});

test('public fragment navigation overrides both cold-load query target kinds',async()=>{
 const selected=[];const app=Object.create(publicUi.PublicController.prototype);
 Object.assign(app,{win:{location:{search:'?comment=9&att=att31-L1',hash:'#comment-10'}},doc:{getElementById:id=>({id,scrollIntoView(){selected.push(id);},classList:{add(){}}})},hasOlder:false,current:()=>true});
 await app.followDeepLink(1);assert.deepEqual(selected,['comment-10']);
 selected.length=0;app.win.location.hash='#att32-L2';let target;
 app.preview=async(row,lines)=>{target=lines;};await app.followDeepLink(1);
 assert.deepEqual(selected,['attachment-32']);assert.deepEqual(target,{id:32,start:2,end:2});
});
