// Exact production Google Fonts response bytes; original reference contexts only.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createHash} = require('node:crypto');
const assets = path.resolve(__dirname, '../assets/fonts');
const stylesheet = 'https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=DM+Sans:ital,opsz,wght@0,9..40,300..700;1,9..40,300..700&display=swap';
const sources = [
  ['https://fonts.gstatic.com/s/dmsans/v17/rP2Fp2ywxg089UriCZa4ET-DNl0.woff2', 'dm-sans-italic-latin-ext.woff2', 'b72f7226650dcbc66e1e6ccdc9e53dc8c66107357eeac2d2a4ca18942c42f360'],
  ['https://fonts.gstatic.com/s/dmsans/v17/rP2Fp2ywxg089UriCZa4Hz-D.woff2', 'dm-sans-italic-latin.woff2', 'd5c53a50536536971ea27318a590dbf723a190dd2f608e7a92929a021cc0ebaa'],
  ['https://fonts.gstatic.com/s/dmsans/v17/rP2Hp2ywxg089UriCZ2IHSeH.woff2', 'dm-sans-normal-latin-ext.woff2', '5d18d31d23ada61ebee1d589b11d5da30db9e158f589d5e35221ba2b1a45de54'],
  ['https://fonts.gstatic.com/s/dmsans/v17/rP2Hp2ywxg089UriCZOIHQ.woff2', 'dm-sans-normal-latin.woff2', 'ca72d2bcea8f4daa783dbdfa2d9b46068c3ce38168e05918fb867aa453b4f890'],
  ['https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPb54C-s0.woff2', 'space-grotesk-normal-vietnamese.woff2', '8895e3d49825128bfa5238e4c96e9b432d7a3dadbe5c17c6081267398305dc71'],
  ['https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPb94C-s0.woff2', 'space-grotesk-normal-latin-ext.woff2', '952dddb45d2f96f71cbf3b7f510b24379afc3c89ea02fcf89d377b45d62c0166'],
  ['https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPbF4Cw.woff2', 'space-grotesk-normal-latin.woff2', '0640890476fc1198ab4de571fb658de443c4d85b66466ec09534a8737ab1ce9d'],
];
const fixtures = new WeakMap();
async function installOriginalFonts(context) {
  const responses = new Map();
  for (const [url, file, sha256] of [[stylesheet, 'google.css', '925c2de4cc33e2f391a588554333ddc178cdab61bd4ff5491b80d01cb1383b70'], ...sources]) {
    const body = fs.readFileSync(path.join(assets, file));
    assert.equal(createHash('sha256').update(body).digest('hex'), sha256, `Original font provenance: ${file}`);
    responses.set(url, {body, file, sha256});
  }
  const requested = [];
  fixtures.set(context, {requested, sources: [...responses].map(([url, {file, sha256}]) => ({url, file, sha256}))});
  await context.route(url => responses.has(url.href), async route => {
    const url = route.request().url(), response = responses.get(url);
    requested.push({url, file: response.file, sha256: response.sha256});
    await route.fulfill({status: 200, body: response.body,
      contentType: response.file.endsWith('.css') ? 'text/css' : 'font/woff2',
      headers: {'access-control-allow-origin': '*'}});
  });
}
async function captureOriginalFonts(page, output) {
  const evidence = fixtures.get(page.context());
  assert.ok(evidence, 'Original font fixture is installed on this reference context');
  const loaded = await page.evaluate(() => ({status: document.fonts.status,
    faces: [...document.fonts].map(face => ({family: face.family, style: face.style, weight: face.weight, status: face.status})),
    dmSans: document.fonts.check('400 14px "DM Sans"'),
    spaceGrotesk: document.fonts.check('400 18px "Space Grotesk"')}));
  fs.writeFileSync(output, JSON.stringify({...evidence, loaded}, null, 2));
  assert.equal(loaded.status, 'loaded');
  for (const family of ['DM Sans', 'Space Grotesk'])
    assert.ok(loaded.faces.some(face => face.family.replaceAll('"', '').replaceAll("'", '') === family && face.status === 'loaded'), `${family} actually loaded in pinned document`);
}
module.exports = {installOriginalFonts, captureOriginalFonts};
