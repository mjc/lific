// Review artifacts from capture.cjs. Uses Playwright's already installed PNG reader.
const fs = require('node:fs');
const path = require('node:path');
const {PNG} = require('../../../e2e/node_modules/playwright-core/lib/utilsBundle.js');
const source = process.argv[2];
const output = process.argv[3];
if (!source || !output) throw new Error('Usage: node compare.cjs CAPTURE_DIRECTORY OUTPUT_DIRECTORY');
fs.mkdirSync(output, {recursive: true});
const pairs = [];
for (const file of fs.readdirSync(source).filter(file => file.endsWith('-svelte.png'))) {
  const name = file.replace('-svelte.png', '');
  const current = `${name}-topcoat.png`;
  if (!fs.existsSync(path.join(source, current))) continue;
  const original = PNG.sync.read(fs.readFileSync(path.join(source, file)));
  const topcoat = PNG.sync.read(fs.readFileSync(path.join(source, current)));
  if (original.width !== topcoat.width || original.height !== topcoat.height) throw new Error(`Viewport mismatch: ${name}`);
  const width = original.width, height = original.height;
  const pair = new PNG({width: width * 2, height});
  PNG.bitblt(original, pair, 0, 0, width, height, 0, 0);
  PNG.bitblt(topcoat, pair, 0, 0, width, height, width, 0);
  const overlay = new PNG({width, height});
  const difference = new PNG({width, height});
  for (let offset = 0; offset < original.data.length; offset += 4) {
    for (let channel = 0; channel < 3; channel++) {
      overlay.data[offset + channel] = Math.round((original.data[offset + channel] + topcoat.data[offset + channel]) / 2);
      difference.data[offset + channel] = Math.abs(original.data[offset + channel] - topcoat.data[offset + channel]);
    }
    overlay.data[offset + 3] = difference.data[offset + 3] = 255;
  }
  for (const [suffix, png] of [['pair', pair], ['overlay', overlay], ['difference', difference]]) {
    fs.writeFileSync(path.join(output, `${name}-${suffix}.png`), PNG.sync.write(png));
  }
  fs.copyFileSync(path.join(source, file), path.join(output, file));
  fs.copyFileSync(path.join(source, current), path.join(output, current));
  pairs.push(name);
}
const html = `<!doctype html><html lang="en"><meta charset="utf-8"><title>Lific visual comparison</title>
<meta name="viewport" content="width=device-width, initial-scale=1"><style>
body{margin:24px;font:14px system-ui;background:#eef4f1;color:#18201d}h1{font-size:24px}nav{display:flex;gap:12px;flex-wrap:wrap}section{margin:32px 0}img{display:block;max-width:100%;height:auto;border:1px solid #d0dcd6}summary{cursor:pointer;padding:12px 0}a{color:#5746a0}figure{margin:12px 0}figcaption{margin:8px 0;color:#495a53}
</style><h1>Original Svelte and Topcoat</h1><p>Each pair uses the same scratch backend, seeded data, viewport and appearance preferences. Original is on the left; Topcoat is on the right. Overlays blend both captures equally. Differences show absolute RGB channel differences; time, activity refresh windows and font rendering can contribute.</p>
<nav>${pairs.map(name => `<a href="#${name}">${name}</a>`).join('')}</nav>
${pairs.map(name => `<section id="${name}"><h2>${name}</h2><figure><img src="${name}-pair.png" alt="Original and Topcoat ${name}"><figcaption>Original Svelte (left), Topcoat (right)</figcaption></figure><details><summary>Overlay and pixel differences</summary><img src="${name}-overlay.png" alt="Equal blend of ${name}"><img src="${name}-difference.png" alt="Absolute RGB differences for ${name}"></details></section>`).join('')}
</html>`;
fs.writeFileSync(path.join(output, 'index.html'), html);
if (fs.existsSync(path.join(source, 'geometry.json'))) fs.copyFileSync(path.join(source, 'geometry.json'), path.join(output, 'geometry.json'));
console.log(`${pairs.length} comparison pairs: ${output}`);
