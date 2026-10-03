const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const root=path.resolve(__dirname,'../../..');
const read=file=>fs.readFileSync(path.join(root,file),'utf8');
// Extract a live production function, retaining its body and dependencies.
function declaration(source,name) {
  const begin=source.indexOf(`function ${name}(`);
  if(begin<0)throw new Error(`Production function ${name} was removed`);
  let depth=0,at=source.indexOf('{',begin),quote=null,escape=false;
  for(let i=at;i<source.length;i++) {
    const c=source[i];
    if(quote){if(escape)escape=false;else if(c==='\\')escape=true;else if(c===quote)quote=null;continue;}
    if(c==='"'||c==="'"||c==='`'){quote=c;continue;}
    if(c==='{')depth++;if(c==='}'&&!--depth)return source.slice(begin,i+1);
  }
  throw new Error(`Unclosed ${name}`);
}
function lexical(file,names,environment={}) {
 const code=read(file).replace(/\}\)\(\);\s*$/,`globalThis.testExports={${names.join(',')}};})();`);
 const context={console,URL,URLSearchParams,TextEncoder,setTimeout,clearTimeout,AbortController,...environment};
 vm.runInNewContext(code,context,{filename:file});return context.testExports;
}
module.exports={root,read,declaration,lexical};
