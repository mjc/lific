const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const publicScript=()=>['vendor.marked.js','vendor.dompurify.js','public.js'].map(name=>fs.readFileSync(path.join(__dirname,name),'utf8')).join('\n');

test('headless public routes use only public reads, render scrubbed DTOs, and expose no mutations',{skip:!process.env.PLAYWRIGHT_EXECUTABLE_PATH},async()=>{
 const {chromium}=await import(path.resolve(__dirname,'../../../..','e2e/node_modules/playwright/index.mjs'));
 const browser=await chromium.launch({headless:true,executablePath:process.env.PLAYWRIGHT_EXECUTABLE_PATH});
 try{
  const page=await browser.newPage({viewport:{width:1100,height:850}});page.setDefaultTimeout(5000);const errors=[];page.on('pageerror',error=>errors.push(error.message));
  const js=publicScript();const css=fs.readFileSync(`${__dirname}/public.css`,'utf8');
  const attachments=fs.readFileSync(path.resolve(__dirname,'../../attachments/assets/attachments.js'),'utf8');
  const shell=`<style>${css}</style><main id="mount"></main>`;
  const issue={id:11,identifier:'ENG-1',title:'Public title',status:'active',priority:'high',description:'# Public body\n\n**safe** <script>bad()</script> ![chart](/attachments/31) [attachment download](/api/attachments/31)',seq:900,assignee_id:999,private_notes:'must not render'};
  const pageRow={id:22,title:'Public page',status:'active',content:'# Public page body\n\n*visible* <img src=x onerror=bad()> ![chart](/attachments/31)',private_owner_id:987};
  const comments=[{id:91,author:'writer',author_display_name:'Writer',content:'Public comment **visible** ![comment chart](/attachments/31)',created_at:'2026-01-02T00:00:00Z',user_id:800}];
  const requests=[];const transports=[];
  const mount=async(kind,identifier='')=>{
   await page.setContent(shell);await page.locator('#mount').evaluate((root,{kind,identifier})=>root.innerHTML=`<main class="tc-public" data-topcoat-public="${kind}" data-public-project="ENG" data-public-identifier="${identifier}" aria-busy="true" aria-readonly="true"><header><a href="/public/ENG/issues">ENG</a><h1>${kind}</h1></header><p data-public-status></p><div data-public-error hidden></div><section data-public-content hidden></section></main>`,{kind,identifier});
   await page.evaluate(({js,attachments})=>{
    Object.defineProperty(window,'localStorage',{configurable:true,value:{getItem(){return null;},setItem(){},removeItem(){}}});
    window.publicRequests=[];window.publicFetches=[];
    const project='ENG';
    window.lificSession={state:{publicProject:project,user:null},resolve(path,method='GET'){
      const allowed=method==='GET'&&(/^\/(projects|issues|pages|attachments|modules|folders)(\/|\?|$)/.test(path));
      return allowed?{kind:'public',url:`/public/api/projects/${project}${path==='/projects'?'':path}`}:{kind:'refused'};
    },request:async(path,options={})=>{
      window.publicRequests.push([path,options.method||'GET',options.credentials||'omit',options.headers||null,options.body||null]);
      const method=options.method||'GET';if(method!=='GET'||options.body||options.headers?.Authorization||options.headers?.Cookie)return {ok:false,status:403,error:'public write refused'};
      if(path==='/projects')return {ok:true,data:[{id:7,identifier:'ENG',name:'Engineering'}]};
      if(path.startsWith('/modules?')||path.startsWith('/folders?'))return {ok:true,data:[]};
      if(path==='/projects/7/index')return {ok:true,data:{issues:[{id:11,identifier:'ENG-1',title:'Public title',status:'active',priority:'high'}],pages:[{id:22,title:'Public page',status:'active',preview:'Public preview'}]}};
      if(path==='/issues/resolve/ENG-1')return {ok:true,data:{id:11,identifier:'ENG-1'}};
      if(path==='/issues/11')return {ok:true,data:{id:11,identifier:'ENG-1',title:'Public title',status:'active',priority:'high',description:'# Public body\n\n**safe** <script>bad()</script> ![chart](/attachments/31)'}};
      if(path.startsWith('/issues/11/comments?'))return {ok:true,data:[{id:91,author:'writer',author_display_name:'Writer',content:'Public comment **visible**',created_at:'2026-01-02T00:00:00Z'}],headers:new Headers({'x-comment-has-more':'false'})};
      if(path==='/pages/22')return {ok:true,data:{id:22,title:'Public page',status:'active',content:'# Public page body\n\n*visible* <img src=x onerror=bad()> ![chart](/attachments/31)'}};
      if(path.startsWith('/pages/22/comments?'))return {ok:true,data:[{id:91,author:'writer',author_display_name:'Writer',content:'Public comment **visible**',created_at:'2026-01-02T00:00:00Z'}],headers:new Headers({'x-comment-has-more':'false'})};
      if(path.startsWith('/attachments?'))return {ok:true,data:[{id:31,filename:'chart.png',mime:'image/png',size_bytes:4}]};
      return {ok:false,status:404,error:`Unexpected ${method} ${path}`};
    }};
    window.fetch=async(url,options={})=>{window.publicFetches.push([String(url),options.credentials,options.headers&&Array.from(new Headers(options.headers).entries())]);return String(url).includes('/thumbnail')?new Response(new Uint8Array([137,80,78,71]),{status:200,headers:{'Content-Type':'image/png','Content-Length':'4'}}):new Response('file bytes',{status:200,headers:{'Content-Type':'image/png','Content-Length':'10'}});};
    eval(attachments+js);
   },{js,attachments});
   await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
  };
  await mount('issues');
  assert.equal(await page.getByRole('link',{name:'ENG-1 · Public title'}).getAttribute('href'),'/public/ENG/issues/ENG-1');
  assert.equal(await page.locator('form,textarea,input[type=file],[draggable="true"]').count(),0);
  assert.equal(await page.evaluate(()=>publicRequests.every(([path,method])=>method==='GET'&&!path.includes('/my-role'))),true);

  const afterTables=await page.evaluate(()=>{
    const output=document.createElement('article');
    LificTopcoatPublic.renderMarkdown(document,output,'| Name | Count |\n| --- | --- |\n| Widget | 2 |\nParagraph immediately after the table.\n\n| Name | Count |\n| --- | --- |\n# Heading after an empty table\n- Following item','ENG');
    return {tables:output.querySelectorAll('table').length,firstTableRows:output.querySelectorAll('table tbody tr').length,firstTableContinuation:output.querySelector('table tbody tr:last-child td:first-child')?.textContent,paragraph:output.querySelector('p')?.textContent,heading:output.querySelector('h1')?.textContent,item:output.querySelector('li')?.textContent};
  });
  assert.deepEqual(afterTables,{tables:2,firstTableRows:2,firstTableContinuation:'Paragraph immediately after the table.',paragraph:undefined,heading:'Heading after an empty table',item:'Following item'});

  await mount('board');
  assert.equal(await page.locator('[data-public-lane="active"] a').innerText(),'ENG-1 · Public title');
  assert.equal(await page.locator('form,textarea,input[type=file]').count(),0);

  await page.setContent(shell);
  await page.locator('#mount').evaluate(root=>root.innerHTML='<main class="tc-public" data-topcoat-public="issue-detail" data-public-project="ENG" data-public-identifier="ENG-1" aria-busy="true"><p data-public-status></p><div data-public-error hidden></div><section data-public-content hidden></section></main>');
  await page.evaluate(()=>{location.hash='#comment-70';});
  await page.evaluate(({js,attachments})=>{
    Object.defineProperty(window,'localStorage',{configurable:true,value:{getItem(){return null;},setItem(){},removeItem(){}}});
    window.publicRequests=[];window.publicFetches=[];window.lificSession={state:{publicProject:'ENG',user:null},resolve(path,method='GET'){return method==='GET'?{kind:'public',url:`/public/api/projects/ENG${path}`}:{kind:'refused'};},request:async(path,options={})=>{
     publicRequests.push([path,options.method||'GET',options.credentials||'omit',options.headers||null,options.body||null]);if((options.method||'GET')!=='GET'||options.body)return {ok:false,status:403,error:'public write refused'};
     if(path==='/projects')return {ok:true,data:[{id:7,identifier:'ENG'}]};if(path==='/projects/7/index')return {ok:true,data:{issues:[],pages:[]}};
     if(path==='/issues/resolve/ENG-1')return {ok:true,data:{id:11,identifier:'ENG-1'}};
     if(path==='/issues/11')return {ok:true,data:{id:11,identifier:'ENG-1',title:'Public title',status:'active',priority:'high',description:'# Public body\n\n**safe** <script>bad()</script> ![chart](/attachments/31) [attachment download](/api/attachments/31)\n\n> quoted text\n\n| Name | Count |\n| --- | ---: |\n| Widget | 2 |\n\n- [x] Done\n- [ ] Open\n\n~~removed~~'}};
     if(path.startsWith('/issues/11/comments?')){const before=new URLSearchParams(path.split('?')[1]).get('before_id');const rows=before?[{id:70,author:'writer',author_display_name:'Writer',content:'Public comment **visible** ![comment chart](/attachments/31)',created_at:'2026-01-01T00:00:00Z'}]:Array.from({length:51},(_,index)=>({id:120-index,author:'writer',author_display_name:'Writer',content:`Comment ${120-index}`,created_at:`2026-01-${String(31-index%28).padStart(2,'0')}T00:00:00Z`}));return {ok:true,data:rows,headers:new Headers({'x-comment-has-more':before?'false':'true'})};}
     if(path.startsWith('/attachments?')){const params=new URLSearchParams(path.split('?')[1]);return {ok:true,data:params.get('entity_type')==='issue'||params.get('entity_id')==='70'?[{id:31,filename:'chart.png',mime:'image/png',size_bytes:4}]:[]};}
     return {ok:false,status:404,error:`Unexpected ${path}`};
    }};
    window.fetch=async(url,options={})=>{const original=String(url).endsWith('/attachments/31');if(original&&window.delayDownloadFetch){window.pendingDownloadFetch=true;await new Promise(resolve=>window.releaseDownloadFetch=resolve);}publicFetches.push([String(url),options.credentials,options.headers&&Array.from(new Headers(options.headers).entries())]);const response=new Response(new Uint8Array([137,80,78,71]),{status:200,headers:{'Content-Type':'image/png','Content-Length':'4'}});if(original&&window.delayDownloadFetch)window.downloadFetchComplete=true;return response;};eval(attachments+js);
  },{js,attachments});
  await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
  assert.equal(await page.locator('[data-public-content] h2').innerText(),'Public title');
  assert.equal(await page.locator('[data-public-content] script').count(),0);
  assert.equal(await page.locator('[data-public-content] img').count(),2);
  assert.equal(await page.locator('.tc-public__markdown [data-public-download="31"]').innerText(),'attachment download');
  assert.equal(await page.locator('.tc-public__markdown blockquote').innerText(),'quoted text');
  assert.equal(await page.locator('.tc-public__markdown table tbody tr').count(),1);
  assert.equal(await page.locator('.tc-public__markdown del').innerText(),'removed');
  assert.deepEqual(await page.locator('.tc-public__markdown input[type=checkbox]').evaluateAll(nodes=>nodes.map(node=>[node.checked,node.disabled])),[[true,true],[false,true]]);
  assert.equal(await page.locator('#comment-70').innerText().then(value=>value.includes('Public comment visible')),true);
  await page.waitForFunction(()=>document.querySelector('#comment-70 img')?.src.startsWith('blob:'));
  assert.equal(await page.locator('form,textarea,input[type=file]').count(),0);
  const publicCalls=await page.evaluate(()=>publicRequests);
  assert.ok(publicCalls.every(([,method,credentials,headers,body])=>method==='GET'&&credentials==='omit'&&!body&&!headers?.Authorization&&!headers?.Cookie));
  const publicFetches=await page.evaluate(()=>publicFetches);
  assert.ok(publicFetches.every(([,credentials,headers])=>credentials==='omit'&&!headers?.some(([name])=>name.toLowerCase()==='authorization')));
  await page.locator('#attachment-31 [data-public-download="31"]').click();
  await page.waitForFunction(()=>publicFetches.some(([url])=>url.endsWith('/attachments/31')));
  assert.equal(await page.evaluate(()=>publicFetches.filter(([url])=>url.endsWith('/attachments/31')).length),1);
  const readsBeforeRestore=await page.evaluate(()=>publicRequests.filter(([path])=>path==='/projects').length);
  await page.evaluate(()=>{
    window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true}));
    window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}));
  });
  await page.waitForFunction(before=>publicRequests.filter(([path])=>path==='/projects').length>before,readsBeforeRestore);
  await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
  assert.equal(await page.locator('[data-public-content] h2').innerText(),'Public title');
  await page.evaluate(()=>{
    window.delayDownloadFetch=true;window.pendingDownloadFetch=false;window.downloadFetchComplete=false;window.downloadObjectUrls=0;const create=URL.createObjectURL.bind(URL);
    URL.createObjectURL=blob=>{window.downloadObjectUrls++;return create(blob);};
  });
  await page.locator('#attachment-31 [data-public-download="31"]').click();await page.waitForFunction(()=>pendingDownloadFetch);
  await page.evaluate(()=>{dispatchEvent(new PageTransitionEvent('pagehide',{persisted:false}));releaseDownloadFetch();});await page.waitForFunction(()=>downloadFetchComplete);
  assert.equal(await page.evaluate(()=>downloadObjectUrls),0);
  assert.ok(errors.length===0,errors.join('\n'));

  // A public session refuses every mutating method before transport.
  const refused=await page.evaluate(()=>lificSession.resolve('/issues/11','POST').kind);
  assert.equal(refused,'refused');

  await page.setContent(shell);
  await page.locator('#mount').evaluate(root=>root.innerHTML='<main class="tc-public" data-topcoat-public="pages" data-public-project="ENG" data-public-identifier="" aria-busy="true"><p data-public-status></p><div data-public-error hidden></div><section data-public-content hidden></section></main>');
  await page.evaluate(({js,attachments})=>{
   Object.defineProperty(window,'localStorage',{configurable:true,value:{getItem(){return null;}}});
   window.lificSession={state:{publicProject:'ENG',user:null},resolve:(path,method='GET')=>method==='GET'?{kind:'public',url:`/public/api/projects/ENG${path}`}:{kind:'refused'},request:async path=>path==='/projects'?{ok:true,data:[{id:7,identifier:'ENG'}]}:path==='/projects/7/index'?{ok:true,data:{issues:[],pages:[{id:22,title:'Public page',status:'active',preview:'Public preview'}]}}:path.startsWith('/folders?')?{ok:true,data:[]}:{ok:false,status:404,error:path}};
   eval(attachments+js);
  },{js,attachments});
  await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
  assert.equal(await page.getByRole('link',{name:'Public page'}).getAttribute('href'),'/public/ENG/pages/22');

  await page.setContent(shell);
  await page.locator('#mount').evaluate(root=>root.innerHTML='<main class="tc-public" data-topcoat-public="page-detail" data-public-project="ENG" data-public-identifier="22" aria-busy="true"><p data-public-status></p><div data-public-error hidden></div><section data-public-content hidden></section></main>');
  await page.evaluate(({js,attachments})=>{
   Object.defineProperty(window,'localStorage',{configurable:true,value:{getItem(){return null;}}});
   window.publicRequests=[];window.publicFetches=[];
   window.lificSession={
    state:{publicProject:'ENG',user:null},
    resolve:(path,method='GET')=>method==='GET'?{kind:'public',url:`/public/api/projects/ENG${path}`}:{kind:'refused'},
    request:async path=>path==='/projects'?{ok:true,data:[{id:7,identifier:'ENG'}]}:
     path==='/projects/7/index'?{ok:true,data:{issues:[],pages:[]}}:
     path==='/pages/22'?{ok:true,data:{id:22,title:'Public page',status:'active',content:'# Public page body\n\n*visible* <img src=x onerror=bad()> ![chart](/attachments/31)'}}:
     path.startsWith('/pages/22/comments?')?{ok:true,data:[],headers:new Headers({'x-comment-has-more':'false'})}:
     path.startsWith('/attachments?')?{ok:true,data:[{id:31,filename:'chart.png',mime:'image/png',size_bytes:4}]}:
     {ok:false,status:404,error:path}
   };
   window.fetch=async(url,options={})=>{publicFetches.push([String(url),options.credentials]);return new Response(new Uint8Array([137,80,78,71]),{status:200,headers:{'Content-Type':'image/png','Content-Length':'4'}});};
   eval(attachments+js);
  },{js,attachments});
  await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
  assert.equal(await page.locator('[data-public-content] h2').innerText(),'Public page');
  assert.equal(await page.locator('[data-public-content] img').count(),1);
  assert.equal(await page.locator('form,textarea,input[type=file]').count(),0);
  assert.equal(await page.evaluate(()=>lificSession.resolve('/pages/22','DELETE').kind),'refused');
 }finally{await browser.close();}
});

async function publicFixture(kind, options = {}) {
 const {chromium}=await import(path.resolve(__dirname,'../../../..','e2e/node_modules/playwright/index.mjs'));
 const browser=await chromium.launch({headless:true,executablePath:process.env.PLAYWRIGHT_EXECUTABLE_PATH});
 const page=await browser.newPage();page.setDefaultTimeout(5000);
 const errors=[];page.on('pageerror',error=>errors.push(error.message));
 const mountHtml=`<main class="tc-public" data-topcoat-public="${kind}" data-public-project="ENG" data-public-identifier="ENG-1"><p data-public-status></p><div data-public-error hidden></div><section data-public-content hidden></section></main>`;
 await page.route('http://public.test/**',route=>route.fulfill({contentType:'text/html',body:mountHtml}));
 await page.goto(`http://public.test/public/ENG/${kind==='issue-detail'?'issues/ENG-1':kind}`);
 const sessionSource=fs.readFileSync(path.resolve(__dirname,'../../session.rs'),'utf8').split('pub(crate) const BROWSER_SCRIPT: &str = r#"')[1].split('"#;')[0];
 await page.evaluate(({sessionSource,options,js,attachments,files})=>{
  const store=new Map([['lific_token','private-token-must-not-travel']]);
  Object.defineProperty(window,'localStorage',{configurable:true,value:{getItem:key=>store.get(key)||null,setItem:(key,value)=>store.set(key,value),removeItem:key=>store.delete(key)}});
  document.body.dataset.lificPublicProject='ENG';document.body.dataset.lificRequireSession='false';
  window.calls=[];window.failNext=options.failNext||false;
  const index=options.index||{issues:[{id:11,identifier:'ENG-1',title:'Alpha',status:'active',priority:'low',module_id:5,labels:['bug'],preview:'needle',created_at:'2026-01-01',updated_at:'2026-01-02'},{id:12,identifier:'ENG-2',title:'Beta',status:'todo',priority:'high',module_id:null,labels:[],created_at:'2026-01-02',updated_at:'2026-01-03'}],pages:[{id:22,identifier:'ENG-DOC-1',title:'Guide',status:'active',folder_id:6,pinned:true,preview:'needle'},{id:23,identifier:'ENG-DOC-2',title:'Archive',status:'archived',folder_id:null,preview:'old'}]};
  window.fetch=async(url,opts={})=>{
   const headers=Array.from(new Headers(opts.headers).entries());calls.push({url:String(url),method:opts.method||'GET',credentials:opts.credentials,headers});
   if(failNext){failNext=false;return Response.json({error:'Temporary outage'},{status:503});}
   const route=String(url).replace(/^\/public\/api\/projects\/ENG/,'').split('?')[0];let data;
   if(route==='')data={id:7,identifier:'ENG',name:'Engineering'};
   else if(route==='/index')data=index;
   else if(route==='/modules')data=[{id:5,name:'Core'}];
   else if(route==='/folders')data=[{id:6,name:'Handbook',parent_id:null}];
   else if(route==='/labels')data=[{name:'bug'}];
   else if(route==='/issues/resolve/ENG-1')data={id:11};
   else if(route==='/issues/11')data={id:11,identifier:'ENG-1',title:'Alpha',status:'active',description:'ENG-2 ENG-2#comment-91 #91 ENG-DOC-1 OTHER-1 `ENG-2` [linked](#/ENG/issues/ENG-2) [private](/OTHER/issues/OTHER-1)'};
   else if(route.endsWith('/comments'))data=[];
   else if(route==='/attachments')data=options.attachments||[];
   else if(/^\/attachments\/\d+\/preview$/.test(route))data=options.preview||{kind:'zip',entries:[{name:'readme.txt',size:4}],total_entries:1};
   else if(/^\/attachments\/\d+$/.test(route)){
    if(window.deferPreview){window.previewPending=true;await new Promise((resolve,reject)=>{window.releasePreview=resolve;opts.signal?.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true});});}
    const payload=options.payloads?.[route.split('/').pop()];
    return new Response(payload?Uint8Array.from(atob(payload.base64),char=>char.charCodeAt(0)):options.bytes||'first\nsecond\nthird\nfourth',{headers:{'Content-Type':payload?.mime||options.mime||'text/plain'}});
   }
   else return Response.json({error:'Unexpected '+route},{status:404});
   return Response.json(data);
  };
  eval(sessionSource);eval(attachments);eval(files);eval(js);
 },{sessionSource,options,js:publicScript(),attachments:fs.readFileSync(path.resolve(__dirname,'../../attachments/assets/attachments.js'),'utf8'),files:fs.readFileSync(path.resolve(__dirname,'../../files/assets/files.js'),'utf8')});
 await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
 return {browser,page,errors};
}

const headless={skip:!process.env.PLAYWRIGHT_EXECUTABLE_PATH};
function wavPayload(){
 const bytes=Buffer.alloc(52);bytes.write('RIFF');bytes.writeUInt32LE(44,4);bytes.write('WAVEfmt ',8);bytes.writeUInt32LE(16,16);bytes.writeUInt16LE(1,20);bytes.writeUInt16LE(1,22);bytes.writeUInt32LE(8000,24);bytes.writeUInt32LE(8000,28);bytes.writeUInt16LE(1,32);bytes.writeUInt16LE(8,34);bytes.write('data',36);bytes.writeUInt32LE(8,40);bytes.fill(128,44);return {base64:bytes.toString('base64'),mime:'audio/wav'};
}

test('public browsing retains search, filters, sorting, layouts, folders and keyboard navigation',headless,async()=>{
 const {browser,page,errors}=await publicFixture('issues');
 try {
  assert.equal(await page.getByRole('link',{name:'Board',exact:true}).getAttribute('href'),'/public/ENG/board');
  await page.getByRole('searchbox',{name:'Search issues'}).fill('needle');
  assert.equal(await page.locator('[data-public-row]').count(),1);
  await page.getByRole('searchbox',{name:'Search issues'}).fill('Alpa');
  assert.equal(await page.locator('[data-public-row]').count(),1);
  await page.getByRole('searchbox',{name:'Search issues'}).fill('absent');
  assert.equal(await page.getByText('No matching issues.').count(),1);
  await page.getByRole('searchbox',{name:'Search issues'}).fill('');
  await page.getByLabel('Status', {exact:true}).selectOption('todo');
  assert.equal(await page.locator('[data-public-row]').innerText().then(value=>value.includes('Beta')),true);
  await page.getByLabel('Status', {exact:true}).selectOption('');
  await page.getByLabel('Module', {exact:true}).selectOption('5');
  assert.equal(await page.locator('[data-public-row]').count(),1);
  await page.getByLabel('Module', {exact:true}).selectOption('');
  await page.getByLabel('Sort', {exact:true}).selectOption('priority');
  assert.equal(await page.locator('[data-public-row] a').first().textContent(),'ENG-2 · Beta');
  await page.locator('body').evaluate(node=>{node.tabIndex=-1;node.focus();});await page.keyboard.press('j');
  assert.equal(await page.evaluate(()=>document.activeElement.dataset.publicIssue),'ENG-2');
  await page.keyboard.press('j');assert.equal(await page.evaluate(()=>document.activeElement.dataset.publicIssue),'ENG-1');
  await page.keyboard.press('/');assert.equal(await page.getByRole('searchbox',{name:'Search issues'}).evaluate(node=>node===document.activeElement),true);
  assert.equal(await page.locator('form,textarea,[draggable="true"],input[type=file]').count(),0);
  assert.equal(await page.evaluate(()=>calls.every(call=>call.method==='GET'&&call.credentials==='omit'&&call.url.startsWith('/public/api/projects/ENG')&&!call.headers.some(([key])=>/authorization|cookie/i.test(key)))),true);
  assert.deepEqual(errors,[]);
 }finally{await browser.close();}
 const pages=await publicFixture('pages');
 try {
  assert.equal(await pages.page.getByRole('link',{name:'Guide',exact:true}).count(),1);
  assert.equal(await pages.page.getByRole('link',{name:'Archive',exact:true}).count(),0);
  await pages.page.getByLabel('Status', {exact:true}).selectOption('all');
  assert.equal(await pages.page.getByRole('link',{name:'Archive',exact:true}).count(),1);
  await pages.page.getByLabel('Folder', {exact:true}).selectOption('6');
  assert.equal(await pages.page.locator('[data-public-row]').count(),1);
  await pages.page.getByRole('searchbox',{name:'Search pages'}).fill('missing');
  assert.equal(await pages.page.getByText('No matching pages.').count(),1);
 }finally{await pages.browser.close();}
 const board=await publicFixture('board');
 try {
  await board.page.getByRole('button',{name:'Hide done column',exact:true}).click();
  assert.equal(await board.page.locator('[data-public-lane="done"]').count(),0);
  await board.page.getByRole('searchbox',{name:'Search issues'}).fill('needle');
  assert.equal(await board.page.locator('.tc-public__card').count(),1);
  assert.equal(await board.page.getByRole('link',{name:'List',exact:true}).getAttribute('href'),'/public/ENG/issues');
 }finally{await board.browser.close();}
});

test('public reference links remain in scope and skip code and private targets',headless,async()=>{
 const {browser,page}=await publicFixture('issue-detail');
 try {
  assert.equal(await page.locator('.tc-public__markdown a[href="/public/ENG/issues/ENG-2"]').count(),2);
  assert.equal(await page.locator('.tc-public__markdown a[href="/public/ENG/issues/ENG-2?comment=91"]').count(),1);
  assert.equal(await page.locator('.tc-public__markdown a[href="#comment-91"]').count(),1);
  assert.equal(await page.locator('.tc-public__markdown a[href="/public/ENG/pages"]').count(),1);
  assert.equal(await page.locator('.tc-public__markdown code a,.tc-public__markdown a[href*="OTHER"],.tc-public__markdown a[href*="plans"]').count(),0);
 }finally{await browser.close();}
});

test('public read failures can be retried without duplicate rows or credentialed requests',headless,async()=>{
 const {browser,page}=await publicFixture('issues',{failNext:true});
 try {
  assert.equal(await page.getByText('Temporary outage').count(),1);
  await page.getByRole('button',{name:'Try again'}).click();
  await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
  assert.equal(await page.locator('[data-public-row]').count(),2);
  assert.equal(await page.locator('[data-public-error]').isVisible(),false);
  assert.equal(await page.evaluate(()=>calls.every(call=>call.credentials==='omit'&&call.method==='GET'&&!call.headers.some(([key])=>key==='authorization'))),true);
 }finally{await browser.close();}
});

test('public attachment viewers preserve line targets, data previews, media and download fallback',headless,async()=>{
 const {browser,page}=await publicFixture('issue-detail',{attachments:[{id:31,filename:'build.log',mime:'text/plain',size_bytes:25},{id:32,filename:'data.csv',mime:'text/csv',size_bytes:10},{id:33,filename:'archive.zip',mime:'application/zip',size_bytes:20},{id:34,filename:'clip.wav',mime:'audio/wav',size_bytes:52},{id:35,filename:'opaque.bin',mime:'application/octet-stream',size_bytes:30}],bytes:'Name,Count\nWidget,2\nAnother,3\nLast,4',payloads:{34:wavPayload()}});
 try {
  await page.evaluate(()=>{location.hash='#att31-L2-3';});
  await page.waitForFunction(()=>document.querySelector('#attachment-31 [data-public-line="2"]')?.classList.contains('tc-public__target'));
  assert.equal(await page.locator('#attachment-31 [data-public-line].tc-public__target').count(),2);
  await page.locator('#attachment-32').getByRole('button',{name:'Preview'}).click();
  assert.equal(await page.locator('#attachment-32 table tbody tr').count(),3);
  await page.locator('#attachment-33').getByRole('button',{name:'Preview'}).click();
  await page.locator('#attachment-33').getByText('readme.txt · 4 B').waitFor();
  assert.equal(await page.locator('#attachment-33').getByText('readme.txt · 4 B').count(),1);
  await page.locator('#attachment-34').getByRole('button',{name:'Preview'}).click();
  await page.locator('#attachment-34 audio[controls]').waitFor();
  assert.equal(await page.locator('#attachment-34 audio[controls]').evaluate(node=>node.src.startsWith('blob:')),true);
  assert.equal(await page.locator('#attachment-35').getByRole('button',{name:'Preview'}).count(),0);
  assert.equal(await page.locator('#attachment-35 [data-public-download]').count(),1);
  assert.equal(await page.evaluate(()=>calls.every(call=>call.credentials==='omit'&&call.method==='GET'&&call.url.startsWith('/public/api/projects/ENG')&&!call.headers.some(([key])=>key==='authorization'))),true);
 }finally{await browser.close();}
});

test('public image lightbox, JSON and SQLite previews are readable without author or uploader fields',headless,async()=>{
 const {browser,page}=await publicFixture('issue-detail',{attachments:[{id:31,filename:'pixel.png',mime:'image/png',size_bytes:68},{id:32,filename:'data.json',mime:'application/json',size_bytes:13},{id:33,filename:'data.sqlite',mime:'application/vnd.sqlite3',size_bytes:20}],bytes:'{"answer":42}',preview:{kind:'sqlite',tables:[{name:'widgets',rows:3}]},payloads:{31:{base64:'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aX1cAAAAASUVORK5CYII=',mime:'image/png'}}});
 try {
  await page.locator('#attachment-31').getByRole('button',{name:'Preview'}).click();
  const image=page.locator('#attachment-31 img');await image.waitFor();await image.focus();await page.keyboard.press('Enter');
  assert.equal(await page.getByRole('dialog',{name:'Image preview'}).count(),1);
  await page.getByRole('button',{name:'Close image'}).click();
  await page.locator('#attachment-32').getByRole('button',{name:'Preview'}).click();
  await page.locator('#attachment-32 pre').waitFor();assert.equal((await page.locator('#attachment-32 pre').textContent()).includes('"answer": 42'),true);
  await page.locator('#attachment-33').getByRole('button',{name:'Preview'}).click();await page.getByText('widgets · 3 rows').waitFor();
 }finally{await browser.close();}
});

test('public attachment preview discards in-flight content when its project scope changes',headless,async()=>{
 const {browser,page}=await publicFixture('issue-detail',{attachments:[{id:31,filename:'build.log',mime:'text/plain',size_bytes:20}]});
 try {
  await page.evaluate(()=>{window.deferPreview=true;});
  await page.locator('#attachment-31').getByRole('button',{name:'Preview'}).click();await page.waitForFunction(()=>previewPending);
  await page.evaluate(()=>{lificSession.setPublicProject('OTHER');releasePreview();});
  assert.equal(await page.locator('[data-public-content]').textContent(),'');
  assert.equal(await page.evaluate(()=>calls.every(call=>call.method==='GET'&&call.credentials==='omit'&&call.url.startsWith('/public/api/projects/ENG'))),true);
 }finally{await browser.close();}
});

test('headless public browse state persists by project and board swimlanes collapse without writes',{skip:!process.env.PLAYWRIGHT_EXECUTABLE_PATH},async t=>{
 const {chromium}=await import(path.resolve(__dirname,'../../../..','e2e/node_modules/playwright/index.mjs'));
 const browser=await chromium.launch({headless:true,executablePath:process.env.PLAYWRIGHT_EXECUTABLE_PATH});
 const page=await browser.newPage();page.setDefaultTimeout(3000);const errors=[];page.on('pageerror',error=>errors.push(error.message));
 const script=publicScript(),css=fs.readFileSync(`${__dirname}/public.css`,'utf8');
 async function mount(kind='issues',project='ENG'){
  await page.evaluate(()=>window.browseController?.dispose());
  await page.setContent(`<style>${css}</style><main data-topcoat-public="${kind}" data-public-project="${project}" data-mounted="true"><p data-public-status></p><div data-public-error hidden></div><section data-public-content hidden></section></main>`);
  await page.evaluate(({script,project})=>{
   window.browseStorage??=new Map();Object.defineProperty(window,'localStorage',{configurable:true,value:{getItem:key=>browseStorage.get(key)??null,setItem:(key,value)=>browseStorage.set(key,value),removeItem:key=>browseStorage.delete(key)}});
   window.browseCalls=[];
   const issues=[{id:1,identifier:`${project}-1`,title:'Core issue',status:'active',priority:'high',module_id:5,labels:['bug'],preview:'Context for core'},
    {id:2,identifier:`${project}-2`,title:'Unassigned issue',status:'todo',priority:'low',module_id:null,labels:['feature']},
    {id:3,identifier:`${project}-3`,title:'Finished issue',status:'done',priority:'high',module_id:6,labels:['bug']}];
   window.lificSession={state:{publicProject:project,user:null},resolve:path=>({kind:'public',url:`/public/api/projects/${project}${path}`}),request:async(path,options={})=>{
    browseCalls.push({path,...options});if(path==='/projects')return {ok:true,data:[{id:7,identifier:window.lificSession.state.publicProject}]};
    if(path==='/projects/7/index')return {ok:true,data:{issues,pages:[]}};
    if(path.startsWith('/modules?'))return {ok:true,data:[{id:5,name:'Core'},{id:6,name:'UI'}]};
    return {ok:false,error:`Unexpected ${path}`};
   }};
   eval(script);window.browseController=new LificTopcoatPublic.PublicController(document.querySelector('[data-topcoat-public]'));
  },{script,project});
  await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
 }
 try{
  await t.test('filters, grouping, density and sorting survive navigation and remain project scoped',async()=>{
   await mount();await page.getByLabel('Status',{exact:true}).selectOption('active');await page.getByLabel('Priority',{exact:true}).selectOption('high');
   await page.getByLabel('Module',{exact:true}).selectOption('5');await page.getByLabel('Label',{exact:true}).selectOption('bug');
   await page.getByLabel('Group by',{exact:true}).selectOption('module_id');await page.getByLabel('Density',{exact:true}).selectOption('comfortable');
   await page.getByLabel('Sort',{exact:true}).selectOption('number');await page.getByLabel('Direction',{exact:true}).selectOption('desc');
   await page.getByRole('searchbox',{name:'Search issues'}).fill('Core');assert.equal(await page.locator('[data-public-row]').count(),1);
   await mount('board');assert.equal(await page.getByLabel('Status',{exact:true}).inputValue(),'active');assert.equal(await page.getByLabel('Priority',{exact:true}).inputValue(),'high');
   assert.equal(await page.getByLabel('Module',{exact:true}).inputValue(),'5');assert.equal(await page.getByLabel('Label',{exact:true}).inputValue(),'bug');
   assert.equal(await page.getByLabel('Density',{exact:true}).inputValue(),'comfortable');assert.equal(await page.getByRole('searchbox',{name:'Search issues'}).inputValue(),'Core');
   assert.equal(await page.getByLabel('Sort',{exact:true}).inputValue(),'number');assert.equal(await page.getByLabel('Direction',{exact:true}).inputValue(),'desc');
   await mount('issues');assert.equal(await page.getByLabel('Group by',{exact:true}).inputValue(),'module_id');
   await mount('issues','OTHER');assert.equal(await page.getByLabel('Status',{exact:true}).inputValue(),'');assert.equal(await page.getByRole('searchbox',{name:'Search issues'}).inputValue(),'');
   await mount('issues');assert.equal(await page.getByLabel('Module',{exact:true}).inputValue(),'5');
   await page.getByRole('searchbox',{name:'Search issues'}).fill('');await page.getByRole('button',{name:'Collapse Core group'}).click();
   assert.equal(await page.locator('[data-public-row]').count(),0);await mount('issues');assert.equal(await page.getByRole('button',{name:'Expand Core group'}).getAttribute('aria-expanded'),'false');
  });
  await t.test('module and priority lanes retain columns, hide and collapse state across reloads',async()=>{
   await page.evaluate(()=>browseStorage.clear());await mount('board');await page.getByLabel('Swimlanes',{exact:true}).selectOption('module');
   assert.equal(await page.locator('[data-public-swimlane]').count(),3);assert.equal(await page.locator('[data-public-swimlane="5"] [data-public-lane="active"] a').textContent(),'ENG-1 · Core issue');
   assert.equal(await page.locator('[data-public-swimlane="none"] [data-public-lane="todo"] a').textContent(),'ENG-2 · Unassigned issue');
   await page.getByRole('button',{name:'Hide done column'}).click();assert.equal(await page.locator('[data-public-lane="done"]').count(),0);
   await page.locator('[data-public-swimlane="5"]').getByRole('button',{name:'Collapse active column'}).click();assert.equal(await page.locator('[data-public-lane="active"] a').count(),0);
   await page.getByRole('button',{name:'Collapse Core lane'}).click();assert.equal(await page.locator('[data-public-swimlane="5"] [data-public-lane]').count(),0);
   await mount('board');assert.equal(await page.getByLabel('Swimlanes',{exact:true}).inputValue(),'module');assert.equal(await page.getByRole('button',{name:'Expand Core lane'}).getAttribute('aria-expanded'),'false');
   assert.equal(await page.locator('[data-public-lane="done"]').count(),0);assert.equal(await page.locator('[data-public-lane="active"] a').count(),0);
   await page.getByRole('button',{name:'Expand Core lane'}).click();await page.locator('[data-public-swimlane="5"]').getByRole('button',{name:'Expand active column'}).click();
   await page.getByLabel('Swimlanes',{exact:true}).selectOption('priority');assert.equal(await page.locator('[data-public-swimlane="high"] a').count(),1);
   await page.getByRole('button',{name:'Collapse high lane'}).click();await mount('board');assert.equal(await page.getByRole('button',{name:'Expand high lane'}).getAttribute('aria-expanded'),'false');
   await page.getByLabel('Swimlanes',{exact:true}).selectOption('none');assert.equal(await page.locator('[data-public-lane="todo"] a').textContent(),'ENG-2 · Unassigned issue');
   assert.equal(await page.locator('form,textarea,input[type=file],[draggable="true"]').count(),0);
   assert.equal(await page.evaluate(()=>browseCalls.every(call=>call.method==='GET'&&call.credentials==='omit'&&!call.body)),true);
  });
  await t.test('a public scope transition resets and restores the same controller preferences',async()=>{
   await page.evaluate(()=>browseStorage.clear());await mount('issues');await page.getByRole('searchbox',{name:'Search issues'}).fill('Core');
   await page.evaluate(()=>{const root=document.querySelector('[data-topcoat-public]');root.dataset.publicProject='OTHER';lificSession.state.publicProject='OTHER';window.dispatchEvent(new Event('lific:scope-change'));});
   await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
   assert.equal(await page.getByRole('searchbox',{name:'Search issues'}).inputValue(),'');
   await page.getByRole('searchbox',{name:'Search issues'}).fill('Finished');
   await page.evaluate(()=>{const root=document.querySelector('[data-topcoat-public]');root.dataset.publicProject='ENG';lificSession.state.publicProject='ENG';window.dispatchEvent(new Event('lific:scope-change'));});
   await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
   assert.equal(await page.getByRole('searchbox',{name:'Search issues'}).inputValue(),'Core');
  });
  await t.test('legacy project preferences retain their keys and serialization',async()=>{
   await page.evaluate(()=>{browseStorage.clear();browseStorage.set('lific:list:state:ENG',JSON.stringify({filterModule:'@none',groupBy:'module',density:'comfortable'}));browseStorage.set('lific:board:lanes:ENG','priority');browseStorage.set('lific:board:hidden-statuses:ENG',JSON.stringify(['done']));browseStorage.set('lific:board:collapsed-lanes:ENG',JSON.stringify(['high']));});
   await mount('board');assert.equal(await page.getByLabel('Swimlanes',{exact:true}).inputValue(),'priority');assert.equal(await page.getByLabel('Module',{exact:true}).inputValue(),'none');
   assert.equal(await page.getByLabel('Density',{exact:true}).inputValue(),'comfortable');assert.equal(await page.getByRole('button',{name:'Expand high lane'}).getAttribute('aria-expanded'),'false');
   await page.getByLabel('Swimlanes',{exact:true}).selectOption('module');
   assert.equal(await page.evaluate(()=>browseStorage.get('lific:board:lanes:ENG')),'module');assert.equal(await page.evaluate(()=>JSON.parse(browseStorage.get('lific:list:state:ENG')).filterModule),'@none');
   await mount('issues');assert.equal(await page.getByLabel('Group by',{exact:true}).inputValue(),'module_id');assert.equal(await page.locator('[data-public-row]').count(),1);
  });
  assert.deepEqual(errors,[]);
 }finally{await browser.close();}
});
