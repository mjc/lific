const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const http=require('node:http');
const os=require('node:os');
const {spawnSync}=require('node:child_process');

const asset=name=>fs.readFileSync(path.join(__dirname,name),'utf8');
const attachments=fs.readFileSync(path.resolve(__dirname,'../../attachments/assets/attachments.js'),'utf8');
const vendor=name=>fs.existsSync(path.join(__dirname,name))?asset(name):'';
const skip=!process.env.PLAYWRIGHT_EXECUTABLE_PATH;

function wave(bytes=12*1024*1024){
 const data=Buffer.alloc(bytes+44,128);data.write('RIFF');data.writeUInt32LE(bytes+36,4);data.write('WAVEfmt ',8);data.writeUInt32LE(16,16);data.writeUInt16LE(1,20);data.writeUInt16LE(1,22);data.writeUInt32LE(48000,24);data.writeUInt32LE(48000,28);data.writeUInt16LE(1,32);data.writeUInt16LE(8,34);data.write('data',36);data.writeUInt32LE(bytes,40);return data;
}

async function fixture(media=wave(),mime='audio/wav',projectPath='ENG'){
 const calls=[];
 const server=http.createServer((request,response)=>{
  const call={url:request.url,method:request.method,headers:request.headers,bytes:0};calls.push(call);
  const send=(body,type='text/html',status=200,headers={})=>{response.writeHead(status,{'Content-Type':type,...headers});response.end(body);};
  if(request.url==='/__topcoat-public-media.js')return send(vendor('public.media-worker.js'),'text/javascript');
  if(request.url==='/public/api/projects/ENG/attachments/31'){
   const range=request.headers.range?.match(/^bytes=(\d+)-(\d*)$/);
   const start=range?Number(range[1]):0,end=range&&range[2]?Math.min(Number(range[2]),media.length-1):media.length-1;
   if(start>=media.length)return send('',mime,416,{'Content-Range':`bytes */${media.length}`});
   response.writeHead(range?206:200,{'Content-Type':mime,'Content-Length':String(end-start+1),'Accept-Ranges':'bytes',...(range?{'Content-Range':`bytes ${start}-${end}/${media.length}`}:{})});
   let offset=start;const transfer=()=>{const next=Math.min(offset+65536,end+1);response.write(media.subarray(offset,next));call.bytes+=next-offset;offset=next;if(offset>end){clearInterval(timer);response.end();}};
   const timer=setInterval(transfer,15);response.on('close',()=>clearInterval(timer));transfer();return;
  }
  if(/^\/public\/ENG\/issues\//i.test(request.url||''))return send('<main id="mount"></main>');
  send('unproxied','text/plain',404);
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const origin=`http://127.0.0.1:${server.address().port}`;
 const {chromium}=await import(path.resolve(__dirname,'../../../..','e2e/node_modules/playwright/index.mjs'));
 const browser=await chromium.launch({headless:true,executablePath:process.env.PLAYWRIGHT_EXECUTABLE_PATH});
 const context=await browser.newContext();await context.addCookies([{name:'private_session',value:'must-not-travel',url:origin}]);
 const page=await context.newPage();page.setDefaultTimeout(10000);const errors=[];page.on('pageerror',error=>errors.push(error.message));
 await page.goto(`${origin}/public/${projectPath}/issues/ENG-1`);
 await page.evaluate(({js,attachments,vendors,mime,size})=>{
  localStorage.setItem('lific_token','private-bearer-must-not-travel');window.transportCalls=[];
  const nativeFetch=window.fetch;window.fetch=(url,options={})=>{transportCalls.push([String(url),options.credentials,Array.from(new Headers(options.headers).entries())]);return nativeFetch(url,options);};
  window.lificSession={state:{publicProject:'ENG',user:null},resolve(path,method='GET'){return method==='GET'?{kind:'public',url:`/public/api/projects/ENG${path}`}:{kind:'refused'};},request:async(path)=>{
   if(path==='/projects')return {ok:true,data:[{id:7,identifier:'ENG'}]};
   if(path==='/projects/7/index')return {ok:true,data:{issues:[],pages:[]}};
   if(path==='/issues/resolve/ENG-1')return {ok:true,data:{id:11}};
   if(path==='/issues/11')return {ok:true,data:{id:11,title:'Public media',description:'ENG-2 #91 `ENG-2`'}};
   if(path.startsWith('/issues/11/comments?'))return {ok:true,data:[]};
   if(path.startsWith('/attachments?'))return {ok:true,data:[{id:31,filename:mime.startsWith('video/')?'movie.webm':'recording.wav',mime,size_bytes:size}]};
   return {ok:false,error:`Unexpected ${path}`};
  }};
  document.querySelector('#mount').innerHTML='<section data-topcoat-public="issue-detail" data-public-project="ENG" data-public-identifier="ENG-1" aria-busy="true"><p data-public-status></p><div data-public-error hidden></div><section data-public-content hidden></section></section>';
  (0,eval)(vendors+'\n'+attachments+js);
 },{js:asset('public.js'),attachments,vendors:vendor('vendor.marked.js')+vendor('vendor.dompurify.js')+vendor('vendor.mermaid.js'),mime,size:media.length});
 await page.waitForFunction(()=>document.querySelector('[data-topcoat-public]').getAttribute('aria-busy')==='false');
 return {page,calls,errors,origin,async close(){await browser.close();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}};
}

test('public Markdown nests child lists inside their parent items like legacy marked',{skip},async()=>{
 const {marked}=await import(path.resolve(__dirname,'../../../../web/node_modules/marked/lib/marked.esm.js'));
 const sources=[
  '- Parent\n  - Child',
  '- Parent\n  - Child\n    1. Grandchild\n    2. Grandchild two\n  - Child two\n- Sibling\n\nAfter list.',
  '1. Parent\n   - Child\n2. Sibling',
  '- First\n1. Second',
  '- Parent\n - Sibling',
  '- Parent\n\n  - Child',
  '- Parent\n  continuation\n  - Child',
  '- First\n- Second\n\n- Third',
  '- Parent\n\n  one paragraph\n\n  another paragraph',
  '- Parent\nnot indented continuation',
  '- Parent\n  - Child\n\n  parent paragraph',
  '- Parent\n\n  ## Heading\n\n  - Child',
  '- Parent\n\n  ```js\n  x\n  ```',
  '10. Parent\n    - Child\n11. Sibling',
  '| Name | Count |\n| --- | --- |\n| Widget | 2 |\nParagraph immediately after the table.\n\n| Name | Count |\n| --- | --- |\n# Heading after an empty table\n- Following item',
 ];
 const f=await fixture();try{
  const cases=sources.map(source=>({source,legacy:marked.parse(source,{breaks:true,gfm:true})}));
  const results=await f.page.evaluate(cases=>{
   const tree=node=>({tag:node.tagName.toLowerCase(),start:node.getAttribute('start'),language:node.localName==='code'?node.className:null,text:Array.from(node.childNodes).filter(child=>child.nodeType===Node.TEXT_NODE).map(child=>child.textContent.trim()).filter(Boolean).join(' '),children:Array.from(node.children).map(tree)});
   return cases.map(({source,legacy})=>{
    const actual=document.createElement('article'),expected=document.createElement('article');
    LificTopcoatPublic.renderMarkdown(document,actual,source,'ENG');expected.innerHTML=DOMPurify.sanitize(legacy);
    return {source,actual:tree(actual),expected:tree(expected)};
   });
  },cases);
  for(const {source,actual,expected} of results)assert.deepEqual(actual,expected,source);
  assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('public Markdown preserves attachment hooks and scoped prose without author-controlled resource loads',{skip},async()=>{
 const f=await fixture();try{
  const requests=[];f.page.on('request',request=>requests.push(request.url()));
  await f.page.evaluate(()=>{
   const target=document.createElement('article');target.id='markdown-hooks';document.body.append(target);
   LificTopcoatPublic.renderMarkdown(document,target,[
    '[**ENG-2**](#/ENG/issues/ENG-2) ENG-2#comment-91 #91 OTHER-1 `ENG-2`',
    '', '![inline](/api/attachments/31) [file](/api/attachments/32)',
    '<img alt="raw" src="/api/attachments/33">',
    '<img alt="tracker" src="https://tracker.test/pixel" srcset="https://tracker.test/pixel 2x" onerror="window.markdownExecuted=true">',
    '<input type="image" src="https://tracker.test/input">',
    '<div style="background:url(https://tracker.test/style)">Safe</div>',
    '<video poster="https://tracker.test/poster" src="https://tracker.test/video"></video>',
    '<svg><image href="https://tracker.test/svg"></image></svg>',
    '<script>window.markdownExecuted=true</script>',
    '', '```js', 'ENG-2 #91', '```',
   ].join('\n'),'ENG');
  });
  assert.equal(await f.page.locator('#markdown-hooks a[href="/public/ENG/issues/ENG-2"] strong').count(),1);
  assert.equal(await f.page.locator('#markdown-hooks a a, #markdown-hooks code a').count(),0);
  assert.equal(await f.page.locator('#markdown-hooks a[href="/public/ENG/issues/ENG-2?comment=91"]').count(),1);
  assert.equal(await f.page.locator('#markdown-hooks a[href="#comment-91"]').count(),1);
  assert.equal(await f.page.locator('#markdown-hooks img[data-public-image="31"]').count(),1);
  assert.equal(await f.page.locator('#markdown-hooks img[data-public-image="33"]').count(),1);
  assert.equal(await f.page.locator('#markdown-hooks img').count(),2);
  assert.match(await f.page.locator('#markdown-hooks').textContent(),/tracker/);
  assert.equal(await f.page.locator('#markdown-hooks a[data-public-download="32"]').count(),1);
  assert.equal(await f.page.locator('#markdown-hooks [src], #markdown-hooks [srcset], #markdown-hooks [style], #markdown-hooks [poster], #markdown-hooks [onerror], #markdown-hooks script, #markdown-hooks svg, #markdown-hooks video').count(),0);
  assert.equal(await f.page.locator('#markdown-hooks pre code.language-js').textContent(),'ENG-2 #91\n');
  assert.equal(await f.page.evaluate(()=>window.markdownExecuted),undefined);
  assert.deepEqual(requests,[]);
  assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('public Mermaid fences render sanitized SVG and preserve scoped inline references',{skip},async()=>{
 const f=await fixture();try{
  await f.page.evaluate(()=>{
   const target=document.createElement('article');target.id='diagrams';document.body.append(target);
   LificTopcoatPublic.renderMarkdown(document,target,'```mermaid\nflowchart LR\n A[Public] --> B[Diagram]\n```\n\n```js\nENG-2\n```\n\nENG-2 #91 `ENG-2`','ENG');
  });
  await f.page.waitForFunction(()=>document.querySelector('#diagrams svg'));
  assert.match(await f.page.locator('#diagrams svg').textContent(),/Public/);
  assert.equal(await f.page.locator('#diagrams svg image, #diagrams svg foreignObject, #diagrams svg a, #diagrams svg script, #diagrams svg use, #diagrams svg [href], #diagrams svg [style]').count(),0);
  assert.equal(await f.page.locator('#diagrams a[href="/public/ENG/issues/ENG-2"]').count(),1);
  assert.equal(await f.page.locator('#diagrams a[href="#comment-91"]').count(),1);
  assert.equal(await f.page.locator('#diagrams pre code').textContent(),'ENG-2\n');
  assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('public audio larger than 10 MiB plays and seeks through anonymous HTTP ranges',{skip},async()=>{
 const f=await fixture();try{
  await f.page.locator('[data-public-preview="31"]').click();
  await f.page.waitForFunction(()=>document.querySelector('audio')?.readyState>=1);
  const duration=await f.page.locator('audio').evaluate(audio=>audio.duration);assert.ok(duration>200);
  await f.page.locator('audio').evaluate(async audio=>{await audio.play();});
  await f.page.waitForFunction(()=>document.querySelector('audio').currentTime>0);
  await f.page.locator('audio').evaluate(audio=>{audio.pause();audio.currentTime=audio.duration-2;});
  await f.page.waitForFunction(()=>{const audio=document.querySelector('audio');return !audio.seeking&&audio.currentTime>audio.duration-3;});
  const mediaCalls=f.calls.filter(call=>call.url==='/public/api/projects/ENG/attachments/31');
  assert.ok(mediaCalls.length>=2,'seeking must issue another range request');
  assert.ok(mediaCalls.some(call=>/^bytes=[1-9]\d*-/.test(call.headers.range||'')),'seek must fetch a later range');
  assert.ok(mediaCalls.every(call=>call.method==='GET'&&!call.headers.cookie&&!call.headers.authorization));
  assert.ok(mediaCalls.reduce((total,call)=>total+call.bytes,0)<10*1024*1024,'playback and seek must not require the complete file');
  assert.equal(await f.page.locator('audio').evaluate(audio=>audio.src.startsWith(location.origin+'/public/ENG/_media/')),true);
  assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('public SVG sanitization strips active elements and external SVG/CSS fetches',{skip},async()=>{
 const f=await fixture();try{
  const foreign=[];await f.page.route('https://attacker.test/**',route=>{foreign.push(route.request().url());return route.abort();});
  await f.page.evaluate(()=>{
   mermaid.render=async()=>({svg:'<svg xmlns="http://www.w3.org/2000/svg"><style>@import url("https://attacker.test/css");rect{fill:url(https://attacker.test/paint)}</style><image href="https://attacker.test/image"/><a href="javascript:bad()"><text>link</text></a><foreignObject><div>html</div></foreignObject><script>bad()</script><rect fill="url(https://attacker.test/paint)" onload="bad()"/><text>Safe</text></svg>'});
   const target=document.createElement('article');target.id='unsafe-diagram';document.body.append(target);LificTopcoatPublic.renderMarkdown(document,target,'```mermaid\nflowchart LR\n A --> B\n```','ENG');
  });
  await f.page.waitForFunction(()=>document.querySelector('#unsafe-diagram [data-rendered="true"]'));
  assert.equal(await f.page.locator('#unsafe-diagram svg image, #unsafe-diagram svg a, #unsafe-diagram svg foreignObject, #unsafe-diagram svg script, #unsafe-diagram svg [href], #unsafe-diagram svg [onload]').count(),0);
  assert.equal(await f.page.locator('#unsafe-diagram').textContent(),'linkSafe');
  assert.deepEqual(foreign,[]);
 }finally{await f.close();}
});

test('media worker proxies only media in the requesting public project and leaves unrelated requests alone',{skip},async()=>{
 const f=await fixture(wave(),'audio/wav','eng');try{
  await f.page.locator('[data-public-preview="31"]').click();await f.page.waitForFunction(()=>document.querySelector('audio')?.readyState>=1);
  const results=await f.page.evaluate(async()=>{
   const responses=await Promise.all([
    fetch('/public/ENG/_media/31'),
    fetch('/public/ENG/_media/31',{method:'POST'}),
    fetch('/api/attachments/31',{headers:{Authorization:'Bearer unrelated-private-request',Range:'bytes=10-20'}}),
   ]);return responses.map(response=>response.status);
  });
  assert.deepEqual(results,[404,404,404]);
  const count=f.calls.filter(call=>call.url==='/public/api/projects/ENG/attachments/31').length;
  await f.page.evaluate(()=>{const media=document.createElement('audio');media.id='other-project-media';media.src='/public/OTHER/_media/31';document.body.append(media);});
  await f.page.waitForFunction(()=>document.querySelector('#other-project-media')?.error);
  assert.equal(f.calls.filter(call=>call.url==='/public/api/projects/ENG/attachments/31').length,count);
  assert.equal(f.calls.some(call=>call.url.startsWith('/public/api/projects/OTHER/')),false);
  assert.equal(f.calls.some(call=>call.url==='/public/OTHER/_media/31'),false,'mismatched media is refused before network');
  assert.equal(f.calls.find(call=>call.url==='/api/attachments/31')?.headers.authorization,'Bearer unrelated-private-request');
  const mediaCalls=f.calls.filter(call=>call.url==='/public/api/projects/ENG/attachments/31');assert.ok(mediaCalls.every(call=>!call.headers.cookie&&!call.headers.authorization));
 }finally{await f.close();}
});

test('public media stops when its project scope changes',{skip},async()=>{
 const f=await fixture();try{
  await f.page.locator('[data-public-preview="31"]').click();await f.page.waitForFunction(()=>document.querySelector('audio')?.readyState>=1);
  const state=await f.page.evaluate(async()=>{
   const audio=document.querySelector('audio');await audio.play();lificSession.state.publicProject='OTHER';window.dispatchEvent(new CustomEvent('lific:scope-change'));
   return {paused:audio.paused,source:audio.getAttribute('src'),content:document.querySelector('[data-public-content]').textContent};
  });
  assert.deepEqual(state,{paused:true,source:null,content:''});
  assert.equal(f.calls.some(call=>call.url.startsWith('/public/api/projects/OTHER/')),false);
 }finally{await f.close();}
});

test('invalid, image-bearing and excessive Mermaid blocks have readable fallbacks',{skip},async()=>{
 const f=await fixture();try{
  await f.page.evaluate(()=>{
   const target=document.createElement('article');target.id='diagram-fallbacks';document.body.append(target);
   LificTopcoatPublic.renderMarkdown(document,target,'```mermaid\nnot a diagram\n```\n\n```mermaid\nflowchart LR\n A@{ img: "https://attacker.test/image" }\n```\n\n```mermaid\nflowchart LR\n A --> B\n```\n\n```mermaid\nflowchart LR\n C --> D\n```','ENG');
  });
  await f.page.waitForFunction(()=>document.querySelector('#diagram-fallbacks')?.textContent.includes('Diagram could not be rendered.'));
  const text=await f.page.locator('#diagram-fallbacks').textContent();assert.match(text,/unsupported media/);assert.match(text,/too many diagrams/);
  assert.equal(await f.page.locator('#diagram-fallbacks svg').count(),1);
  assert.deepEqual(f.errors,[]);
 }finally{await f.close();}
});

test('public video larger than 10 MiB plays and seeks without downloading its padding or sending credentials',{skip},async()=>{
 const directory=fs.mkdtempSync(path.join(os.tmpdir(),'lific-public-video-'));
 const filename=path.join(directory,'large.webm');
 try{
  // A seekable WebM with a large index reservation is deterministic and fast
  // to generate; the player must skip that 12 MiB gap with HTTP ranges.
  const encoded=spawnSync('ffmpeg',['-v','error','-f','lavfi','-i','color=c=blue:s=160x90:r=5','-t','6','-c:v','libvpx','-g','5','-reserve_index_space',String(12*1024*1024),filename],{timeout:30000});
  assert.equal(encoded.status,0,encoded.stderr?.toString()||encoded.error?.message);
  const data=fs.readFileSync(filename);assert.ok(data.length>10*1024*1024);
  const f=await fixture(data,'video/webm');try{
   await f.page.locator('[data-public-preview="31"]').click();await f.page.waitForFunction(()=>document.querySelector('video')?.readyState>=1);
   await f.page.locator('video').evaluate(async video=>{await video.play();});await f.page.waitForFunction(()=>document.querySelector('video').currentTime>0);
   await f.page.locator('video').evaluate(video=>{video.pause();video.currentTime=4;});await f.page.waitForFunction(()=>{const video=document.querySelector('video');return !video.seeking&&video.currentTime>=4;});
   const calls=f.calls.filter(call=>call.url==='/public/api/projects/ENG/attachments/31');
   assert.ok(calls.some(call=>/^bytes=[1-9]\d*-/.test(call.headers.range||'')));
   assert.ok(calls.every(call=>call.method==='GET'&&!call.headers.cookie&&!call.headers.authorization));
   assert.ok(calls.reduce((total,call)=>total+call.bytes,0)<10*1024*1024,'playback must skip the 12 MiB index reservation');
   assert.deepEqual(f.errors,[]);
  }finally{await f.close();}
 }finally{fs.rmSync(directory,{recursive:true,force:true});}
});
