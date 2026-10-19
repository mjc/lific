const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {startFixture} = require('../../../acceptance/server.js');

test('fits a large image into the viewport and never upscales a small one', {timeout: 60000}, async () => {
  const fixture = await startFixture({seed: false});
  try {
    const page = await fixture.browser.newPage({viewport: {width: 1250, height: 1540}});
    await page.setContent('<main></main>');
    await page.addStyleTag({path: path.resolve(__dirname, '../../../attachments/assets/attachments.css')});
    const source = require('../native-port-missing').unavailable('attachment annotation');
    assert.equal(source.split('globalThis.LificTopcoatAttachments = {').length, 2);
    await page.addScriptTag({content: source.replace('globalThis.LificTopcoatAttachments = {',
      'globalThis.LificTopcoatAttachments = {annotation,')});
    for (const [width, height, scale] of [[2000, 1000, .5], [200, 100, 1]]) {
      await page.evaluate(async ({width, height}) => {
        const canvas = document.createElement('canvas');
        canvas.width = width; canvas.height = height;
        const blob = await new Promise(resolve => canvas.toBlob(resolve, 'image/png'));
        window.annotationDone = LificTopcoatAttachments.annotation(new File([blob], 'test.png', {type: 'image/png'}),
          {win: window, signal: new AbortController().signal});
      }, {width, height});
      await page.getByRole('button', {name: 'Annotate', exact: true}).click();
      const canvas = page.locator('[data-attachment-annotation-canvas]');
      await canvas.waitFor();
      const box = await canvas.boundingBox();
      assert.equal(box.width / width, scale);
      assert.equal(box.height / height, scale);
      await page.getByRole('button', {name: 'Cancel annotation', exact: true}).click();
      await page.evaluate(() => window.annotationDone.then(() => undefined));
    }
  } finally {
    await fixture.close();
  }
});
