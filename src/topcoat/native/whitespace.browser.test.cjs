// Framework parity: execute actual Rust-produced expression sources and wire cases.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const http=require('node:http');
const {launchBrowser}=require('./browser_fixture.cjs');
test('packaged ECMAScript trim matches eighteen actual Rust expression wires',async()=>{
  assert.ok(process.env.LIFIC_WHITESPACE_COHERENCE_OUTPUT,'Export the actual Rust whitespace cases first.');
  const cases=JSON.parse(fs.readFileSync(process.env.LIFIC_WHITESPACE_COHERENCE_OUTPUT,'utf8'));
  assert.equal(cases.length,18);
  const runtime=fs.readFileSync(path.join(__dirname,'../assets/runtime.js'),'utf8');
  const server=http.createServer((request,response)=>{
    if(request.url==='/runtime.js')return response.writeHead(200,{'content-type':'text/javascript'}).end(runtime);
    response.writeHead(200,{'content-type':'text/html'}).end('<html><body><span data-topcoat-on:mount="()=>{window.whitespaceCx=cx;}"></span><script type="module" src="/runtime.js"></script></body></html>');
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  let browser;
  try{
    browser=await launchBrowser();const page=await browser.newPage(),errors=[];
    page.on('pageerror',error=>errors.push(error.message));
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForFunction(()=>window.whitespaceCx!==undefined);
    const actual=await page.evaluate(cases=>cases.map(item=>{
      const value=new Function('cx',`return (${item.source});`)(window.whitespaceCx);
      return {wire:value.dehydrate(),roundTrip:window.whitespaceCx.hydrate(value.dehydrate()).dehydrate()};
    }),cases);
    actual.forEach((value,index)=>{
      assert.deepEqual(value.wire,cases[index].wire,`actual expression ${index}`);
      assert.deepEqual(value.roundTrip,cases[index].wire,`wire round trip ${index}`);
      assert.deepEqual(cases[index].expected,{kind:'Return',value:{type:'String',value:cases[index].wire}});
    });
    assert.deepEqual(errors,[]);
  }finally{if(browser)await browser.close();server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
});
