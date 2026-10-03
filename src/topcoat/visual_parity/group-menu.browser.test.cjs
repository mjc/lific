const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {chromium} = require('../../../e2e/node_modules/playwright');

test('compact group actions retain native keyboard access, drafts, focus, and catalog commands', async () => {
  assert.ok(process.env.PLAYWRIGHT_EXECUTABLE_PATH, 'Use the repository e2e Chromium environment.');
  const browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
  try {
    const page = await browser.newPage({viewport: {width: 1440, height: 900}});
    await page.setContent('<aside style="width:230px"><div id="catalog"></div></aside>');
    for (const file of ['assets/base.css', 'assets/controls.css', 'shell/assets/projects.css']) {
      await page.addStyleTag({content: fs.readFileSync(path.join(__dirname, '..', file), 'utf8')});
    }
    await page.addScriptTag({path: path.join(__dirname, '../shell/assets/projects.js')});
    await page.evaluate(() => {
      window.commands = [];
      window.catalog = window.LificTopcoatProjects.attach(document.querySelector('#catalog'), {
        current: () => ({generation: 1, groups: [{id: 1, name: 'Frontend', project_ids: [1]},
          {id: 2, name: 'Other', project_ids: []}], projects: [{id: 1, name: 'UI', identifier: 'UI'}]}),
        command: async command => {window.commands.push(command); return {};},
      });
    });
    const group = page.locator('[data-group-id="1"]');
    const toggle = group.locator('summary[aria-label="Actions for group Frontend"]');
    assert.ok((await group.locator('.tc-projects__group-heading').boundingBox()).height <= 32,
      'Closed actions retain a single compact heading row.');
    assert.equal(await group.getByRole('textbox', {name: 'Rename Frontend'}).isVisible(), false);
    await toggle.focus();
    await page.keyboard.press('Enter');
    const input = group.getByRole('textbox', {name: 'Rename Frontend'});
    await input.fill('Frontend draft');
    await input.evaluate(el => el.setSelectionRange(4, 9));
    await page.evaluate(() => window.catalog.controller.accept({...window.catalog.controller.snapshot, generation: 2}));
    assert.equal(await input.inputValue(), 'Frontend draft');
    assert.equal(await input.evaluate(el => document.activeElement === el), true);
    assert.deepEqual(await input.evaluate(el => [el.selectionStart, el.selectionEnd]), [4, 9]);
    assert.equal(await group.locator('details').evaluate(el => el.open), true);
    for (const key of ['Escape', 'Tab']) {
      await page.keyboard.press(key);
      assert.equal(await group.locator('details').evaluate(el => el.open), false, `${key} dismisses the popup.`);
      assert.equal(await toggle.evaluate(el => document.activeElement === el), true, `${key} restores its trigger.`);
      await page.keyboard.press('Enter');
      assert.equal(await input.inputValue(), 'Frontend draft');
    }
    await page.locator('aside').click({position: {x: 5, y: 400}, force: true});
    assert.equal(await group.locator('details').evaluate(el => el.open), false, 'An outside click dismisses the popup.');
    await toggle.click();
    await page.locator('[data-group-id="2"] summary').focus();
    await page.keyboard.press('Enter');
    assert.equal(await group.locator('details').evaluate(el => el.open), false, 'Opening another group closes the first.');
    await toggle.focus();
    await page.keyboard.press('Enter');
    assert.equal(await page.locator('details[open]').count(), 1);
    await group.getByRole('button', {name: 'Rename', exact: true}).click();
    await page.waitForFunction(() => window.commands.length === 1 && !window.catalog.controller.pending);
    assert.deepEqual(await page.evaluate(() => window.commands[0]), {type: 'rename_group', id: 1, name: 'Frontend draft'});
    await group.getByRole('button', {name: 'Move Frontend draft down'}).click();
    await page.waitForFunction(() => window.commands.length === 2 && !window.catalog.controller.pending);
    assert.deepEqual(await page.evaluate(() => window.commands[1]), {type: 'reorder_groups', ids: [2, 1]});
    await group.getByRole('button', {name: 'Delete group', exact: true}).click();
    await page.waitForFunction(() => window.commands.length === 3 && !window.catalog.controller.pending);
    assert.deepEqual(await page.evaluate(() => window.commands[2]), {type: 'delete_group', id: 1});
    assert.equal(await page.getByRole('link', {name: 'Open UI'}).count(), 1,
      'Deleting a group preserves its project in the catalog.');
    const otherMenu = page.locator('[data-group-id="2"] details');
    await otherMenu.locator('summary').click();
    await page.evaluate(() => window.catalog.destroy());
    await page.keyboard.press('Escape');
    assert.equal(await otherMenu.evaluate(el => el.open), true, 'Destroy removes delegated dismissal listeners.');
  } finally {await browser.close();}
});
