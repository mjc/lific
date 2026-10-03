// Original Mermaid units now exercise the actual browser renderer and vendors.
const {test}=require('node:test');const assert=require('node:assert/strict');
const fs=require('node:fs');const path=require('node:path');const {root}=require('./source.js');
const diagram=source=>'```mermaid\n'+source+'\n```';
test('main Mermaid limits and render lifecycle',{skip:!process.env.PLAYWRIGHT_EXECUTABLE_PATH,timeout:60000},async t=>{
 const {chromium}=await import(path.resolve(__dirname,'../../../../../e2e/node_modules/playwright/index.mjs'));
 const browser=await chromium.launch({headless:true,executablePath:process.env.PLAYWRIGHT_EXECUTABLE_PATH});
 const page=await browser.newPage();page.setDefaultTimeout(3000);
 try{
  await page.setContent('<!doctype html><html><body><article id="target"></article></body></html>');
  for(const file of ['vendor.marked.js','vendor.dompurify.js','vendor.mermaid.js','public.js'])await page.addScriptTag({content:fs.readFileSync(path.join(root,'public/assets',file),'utf8')});
  await page.evaluate(()=>{window.originalMermaid=mermaid;window.renders=0;window.deferred=[];window.failDiagram=false;window.holdDiagram=false;window.mermaid={initialize(){},render(){renders++;if(failDiagram)return Promise.reject(Error('invalid diagram'));if(holdDiagram)return new Promise(resolve=>deferred.push(resolve));return Promise.resolve({svg:'<svg><text>safe</text></svg>'});}};});
  const render=async(source,{hold=false,fail=false}={})=>{await page.evaluate(({source,hold,fail})=>{renders=0;holdDiagram=hold;failDiagram=fail;LificTopcoatPublic.renderMarkdown(document,document.querySelector('#target'),source,'ENG');},{source,hold,fail});await page.waitForTimeout(20);return page.locator('[data-public-diagram]').evaluateAll(nodes=>nodes.map(node=>({text:node.textContent,rendered:node.dataset.rendered,html:node.innerHTML})));};
  const skipped=async source=>{const nodes=await render(diagram(source));assert.equal(await page.evaluate(()=>renders),0);assert.match(nodes[0].text,/too complex/);};
  await t.test('accepts ordinary diagrams and rejects large or dense input',async()=>{
   await render(diagram('graph TD\nA-->B\nB-->C'));assert.equal(await page.evaluate(()=>renders),1);
   for(const source of ['x'.repeat(4097),Array(129).fill('node').join('\n'),`graph TD\nA${'-->A'.repeat(128)}`])await skipped(source);
  });
  await t.test('rejects tiny inputs that trigger known Mermaid resource exhaustion',async()=>{
   for(const source of ['xychart\n  x-axis 1 --> 1\n  line [1, 2]','xychart\n  x-axis score 1 --> 1\n  line [1, 2]','radar-beta\n  axis a, b\n  curve c {1,1}\n  ticks 1000000000'])await skipped(source);
  });
  await t.test('rejects Mermaid architecture prototype pollution keys',()=>skipped('architecture-beta\n  group __proto__(cloud)[Attacker controlled]'));
  for(const source of ['xychart; x-axis 1 --> 1; line [1, 2]','xychart\nx-axis 1 --> 10\nx-axis 2 --> 2\nline [1, 2]','radar-beta\naxis a, b\ncurve c {1,1}\nticks 5\nticks 1000000000','radar-beta\naxis a, b\ncurve c {1,1}\nticks 1000000000 %% comment'])await t.test(`rejects dangerous directives in valid Mermaid syntax: ${source}`,async()=>{
   assert.ok(await page.evaluate(async source=>{originalMermaid.initialize({startOnLoad:false,securityLevel:'strict'});return originalMermaid.parse(source);},source));await skipped(source);
  });
  await t.test('preserves safe ranges and quoted labels containing directive-like text',async()=>{
   for(const source of ['xychart; x-axis 1 --> 10; line [1, 2]','xychart\ntitle "Example; x-axis 1 --> 1"\nx-axis 1 --> 10\nline [1, 2]','xychart\nx-axis "Revenue; %% total" 1 --> 10\nline [1, 2]','radar-beta\naxis a, b\ncurve c {1,1}\nticks 128 %% allowed','architecture-beta\ngroup safe(cloud)[Safe]']){await render(diagram(source));assert.equal(await page.evaluate(()=>renders),1,source);}
  });
  await t.test('shares aggregate block and source limits',async()=>{const nodes=await render([diagram('x'.repeat(4096)),diagram('x'.repeat(4096)),diagram('A')].join('\n\n'));assert.equal(await page.evaluate(()=>renders),2);assert.match(nodes[2].text,/too many diagrams/);});
  await t.test('does not consume rejected source',async()=>{const nodes=await render([diagram('x'.repeat(8193)),diagram('A'),diagram('B')].join('\n\n'));assert.equal(await page.evaluate(()=>renders),2);assert.match(nodes[0].text,/too complex/);});
  await t.test('rejects a forged complex placeholder before rendering',async()=>{const nodes=await render(diagram(`graph TD\nA${'-->A'.repeat(128)}`));assert.equal(await page.evaluate(()=>renders),0);assert.match(nodes[0].text,/too complex/);assert.equal(nodes[0].rendered,'error');});
  await t.test('rejects malformed encoded source without throwing',async()=>{
   // Encoding moved from data-mermaid URI text to a renderer-owned numeric index.
   // Forge the equivalent invalid placeholder index through the renderer seam.
   await page.evaluate(()=>{const target=document.querySelector('#target');window.savedQuery=target.querySelectorAll;target.querySelectorAll=function(selector){const nodes=savedQuery.call(this,selector);if(selector==='[data-public-diagram-index]')for(const node of nodes)node.dataset.publicDiagramIndex='%invalid';return nodes;};});
   try{await render(diagram('graph TD\nA-->B'));assert.equal(await page.evaluate(()=>renders),0);assert.equal(await page.locator('[data-public-diagram-index]').count(),0);}finally{await page.evaluate(()=>document.querySelector('#target').querySelectorAll=savedQuery);}
  });
  await t.test('enforces one shared budget across blocks',async()=>{const nodes=await render(['A','B','C'].map(diagram).join('\n\n'));assert.equal(await page.evaluate(()=>renders),2);assert.match(nodes[2].text,/too many diagrams/);});
  await t.test('does not update after cancellation',async()=>{
   await render(diagram('graph TD\nA-->B'),{hold:true});await page.evaluate(()=>{window.stale=document.querySelector('[data-public-diagram]');stale.remove();for(const resolve of deferred.splice(0))resolve({svg:'<svg></svg>'});});await page.waitForTimeout(20);
   assert.equal(await page.evaluate(()=>stale.dataset.rendered),undefined);assert.ok(!(await page.evaluate(()=>stale.innerHTML)).includes('<svg'));
  });
  await t.test('a pass cancelled before it starts charges nothing and leaves the node alone',async()=>{
   const result=await page.evaluate(()=>{renders=0;const target=document.createElement('article');LificTopcoatPublic.renderMarkdown(document,target,'```mermaid\ngraph TD\nA'+'-->A'.repeat(128)+'\n```','ENG');const block=target.querySelector('[data-public-diagram]');return {renders,text:block.textContent,html:block.innerHTML,rendered:block.dataset.rendered};});
   assert.equal(result.renders,0);assert.equal(result.text,'');assert.equal(result.html,'');assert.equal(result.rendered,undefined);
  });
  await t.test('a rerender gets a fresh budget, so a cancelled pass costs the next one nothing',async()=>{
   await render(diagram('graph TD\nA-->B'),{hold:true});await page.evaluate(()=>{window.stale=document.querySelector('[data-public-diagram]');stale.remove();deferred.splice(0).forEach(resolve=>resolve({svg:'<svg></svg>'}));});
   const nodes=await render(['graph TD\nA-->B','graph TD\nC-->D'].map(diagram).join('\n\n'));assert.equal(await page.evaluate(()=>renders),2);assert.deepEqual(nodes.map(node=>node.rendered),['true','true']);assert.equal(await page.evaluate(()=>stale.dataset.rendered),undefined);
  });
  await t.test('a refreshed thread budget still counts the bodies already on screen',async()=>{
   await render([diagram('graph TD\nA0-->B0'),diagram('graph TD\nA1-->B1')].join('\n\n'));assert.equal(await page.evaluate(()=>renders),2);
   // Thread refresh remounts separate comment bodies, as in the old assertion.
   const result=await page.evaluate(()=>{renders=0;const thread=document.createElement('section');document.querySelector('#target').replaceChildren(thread);for(let i=0;i<3;i++){const body=document.createElement('article');thread.append(body);LificTopcoatPublic.renderMarkdown(document,body,`\`\`\`mermaid\ngraph TD\nA${i}-->B${i}\n\`\`\``,'ENG');}return renders;});
   await page.waitForTimeout(20);
   assert.equal(await page.evaluate(()=>renders),2,'All comment bodies must share the thread budget');
  });
  await t.test('a mention-roster rerender charges one shared budget, not one per body',async()=>{
   for(let pass=0;pass<2;pass++){const result=await page.evaluate(()=>{renders=0;const thread=document.createElement('section');document.querySelector('#target').replaceChildren(thread);for(let i=0;i<3;i++){const body=document.createElement('article');thread.append(body);LificTopcoatPublic.renderMarkdown(document,body,`\`\`\`mermaid\ngraph TD\nA${i}-->B${i}\n\`\`\``,'ENG');}return renders;});await page.waitForTimeout(20);assert.equal(await page.evaluate(()=>renders),2);}
  });
  await t.test('reports success and failure while active',async()=>{const success=await render(diagram('graph TD\nA-->B'));assert.match(success[0].html,/<svg/);assert.equal(success[0].rendered,'true');const failure=await render(diagram('graph TD\nA-->B'),{fail:true});assert.match(failure[0].text,/invalid diagram/);assert.equal(failure[0].rendered,'error');});
 }finally{await browser.close();}
});
