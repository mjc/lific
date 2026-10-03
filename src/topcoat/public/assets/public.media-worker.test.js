const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const vm=require('node:vm');

function fixture(basePath, pagePath=`${basePath}/public/ENG/issues/ENG-1`) {
 const listeners={},calls=[];
 const self={location:new URL(`https://lific.test${basePath}/__topcoat-public-media.js`),
  addEventListener(name,handler){listeners[name]=handler;},
  clients:{get:async()=>({url:`https://lific.test${pagePath}`})}};
 vm.runInNewContext(fs.readFileSync(`${__dirname}/public.media-worker.js`,'utf8'),{
  self,URL,Headers,Response,fetch:async(url,options)=>{calls.push({url,options});return new Response('media');},
 });
 return {calls,async request(path,destination='audio') {
  let response;
  listeners.fetch({request:{url:`https://lific.test${path}`,method:'GET',destination,
   headers:new Headers({Range:'bytes=10-20',Authorization:'must-not-travel'}),signal:undefined},
   clientId:'reader',respondWith(value){response=value;}});
  return response;
 }};
}

test('media workers keep ranges and anonymous requests inside their own mount',async()=>{
 for(const base of ['', '/app']) {
  const f=fixture(base);
  const response=await f.request(`${base}/public/ENG/_media/31`);
  assert.equal(response?.status,200);
  assert.equal(f.calls[0].url,`${base}/public/api/projects/ENG/attachments/31`);
  assert.equal(f.calls[0].options.credentials,'omit');
  assert.equal(f.calls[0].options.headers.get('Range'),'bytes=10-20');
  assert.equal(f.calls[0].options.headers.has('Authorization'),false);
 }
});

test('a worker refuses other project clients and ignores resources outside its mount',async()=>{
 const mismatch=fixture('/app','/app/public/OTHER/issues/OTHER-1');
 assert.equal((await mismatch.request('/app/public/ENG/_media/31'))?.status,403);
 assert.equal(mismatch.calls.length,0);
 const outside=fixture('/app');
 assert.equal(await outside.request('/public/ENG/_media/31'),undefined);
 assert.equal(await outside.request('/app/public/ENG/_media/31',''),undefined);
 assert.equal(outside.calls.length,0);
 const wrongMount=fixture('/app','/public/ENG/issues/ENG-1');
 assert.equal((await wrongMount.request('/app/public/ENG/_media/31'))?.status,403);
 assert.equal(wrongMount.calls.length,0);
});
