const {test} = require('node:test');
const assert = require('node:assert/strict');
const {startFixture} = require('./server.js');

async function loaded(page, selector) {
  await page.waitForFunction(selector => document.querySelector(selector)?.getAttribute('aria-busy') === 'false', selector);
  const root = page.locator(selector);
  const alerts = await root.locator('[role="alert"]:visible').allTextContents();
  assert.deepEqual(alerts.filter(text => text.trim()), [], `${selector} showed an error`);
  return root;
}

async function contains(locator, text) {
  await locator.filter({hasText: text}).first().waitFor({state: 'visible'});
}

test('real server loads every private route family through a stripping proxy',
  {skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH, timeout: 120000}, async t => {
    const fixture = await startFixture();
    try {
      const page = await fixture.newPage();
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
      const project = fixture.project.identifier;
      const routes = [
        ['/', '[data-topcoat-dashboard="home"]', async root => {
          await contains(root.locator('[data-dashboard-content]'), 'All quiet here');
          await contains(page.locator('[data-topcoat-projects]'), fixture.project.name);
          assert.equal((await root.locator('[data-dashboard-errors]').textContent()).trim(), '');
          const mascot = root.locator('.tc-dashboard__mascot');
          assert.equal(await mascot.isVisible(), true);
          const mask = await mascot.evaluate(element => getComputedStyle(element).maskImage);
          const mascotPath = `${fixture.prefix}/__topcoat-dashboard-mascot.png`;
          assert.equal(new URL(mask.slice(5, -2), fixture.origin).pathname, mascotPath);
          const decoded = await page.evaluate(url => new Promise((resolve, reject) => {
            const image = new Image();
            image.onload = () => resolve(image.naturalWidth > 0);
            image.onerror = () => reject(new Error('The mascot asset could not be decoded'));
            image.src = url;
          }), `${fixture.origin}${mascotPath}`);
          assert.equal(decoded, true);
        }],
        ['/settings', '[data-topcoat-identity="settings"]', root => contains(root.locator('h1'), 'Account settings')],
        ['/settings/instance', '[data-topcoat-identity="instance"]', root => contains(root.locator('h1'), 'Instance settings')],
        ['/projects/new', '[data-topcoat-project-settings="new"]', async root => {
          assert.equal(await root.getByLabel('Name', {exact: true}).isVisible(), true);
          assert.equal(await root.getByRole('button', {name: 'Create project', exact: true}).isEnabled(), true);
        }],
        ['/projects/import', '[data-topcoat-project-settings="archive"]', async root => {
          assert.equal(await root.getByLabel('Project archive (.tar.gz)', {exact: true}).isVisible(), true);
        }],
        ...['overview', 'settings'].map(section => [`/${project}/${section}`, '[data-topcoat-dashboard="overview"]', async root => {
          await contains(root.locator('h1'), fixture.project.name);
          assert.equal(await root.getByRole('region', {name: 'Project progress'}).count(), 1);
          assert.equal((await root.locator('[data-dashboard-errors]').textContent()).trim(), '');
          await loaded(page, '[data-topcoat-project-settings="settings"]');
        }]),
        ...['issues', 'board'].map(section => [`/${project}/${section}`, '[data-topcoat-issue-list]', root => contains(root.locator('[data-issues-content]'), fixture.issue.title)]),
        [`/${project}/issues/new`, '[data-topcoat-issue-create]', async root => {
          assert.equal(await root.getByRole('heading', {name: 'New issue', exact: true}).isVisible(), true);
          assert.equal(await root.locator('[data-issue-create-form]').isVisible(), true);
        }],
        [`/${project}/issues/${fixture.issue.identifier}`, '[data-topcoat-issue-detail]', async root => {
          await contains(root.locator('[data-detail-title]'), fixture.issue.title);
          await contains(root.locator('[data-detail-identifier]'), fixture.issue.identifier);
        }],
        [`/${project}/pages`, '[data-topcoat-pages="list"]', root => contains(root.locator('[data-pages-content]'), fixture.page.title)],
        [`/${project}/pages/${fixture.page.id}`, '[data-topcoat-pages="detail"]', async root => {
          assert.equal(await root.locator('[data-page-title]').inputValue(), fixture.page.title);
          assert.equal(await root.locator('[data-page-content]').isVisible(), true);
        }],
        [`/${project}/files`, '[data-topcoat-files]', root => contains(root.locator('[data-files-list]'), 'No files here yet')],
        [`/${project}/modules`, '[data-topcoat-modules="list"]', async root => {
          await root.locator('[data-module-tab="all"]').click();
          await contains(root.locator('[data-modules-content]'), fixture.module.name);
        }],
        [`/${project}/modules/${fixture.module.id}`, '[data-topcoat-modules="detail"]', async root => {
          assert.equal(await root.locator('[data-module-name]').inputValue(), fixture.module.name);
        }],
        [`/${project}/plans`, '[data-topcoat-plans="list"]', root => contains(root.locator('[data-plans-content]'), fixture.plan.title)],
        [`/${project}/plans/${fixture.plan.id}`, '[data-topcoat-plans="detail"]', async root => {
          assert.equal(await root.locator('[data-plan-title]').inputValue(), fixture.plan.title);
        }],
        [`/${project}/activity`, '[data-topcoat-analytics="activity"]', root => contains(root.locator('[data-analytics-content]'), 'Activity')],
        [`/${project}/insights`, '[data-topcoat-analytics="insights"]', root => contains(root.locator('[data-analytics-content]'), 'Insights')],
        [`/${project}/graph`, '[data-topcoat-analytics="graph"]', root => contains(root.locator('[data-analytics-content]'), 'Dependency graph')],
      ];
      for (const [route, selector, check] of routes) {
        await t.test(route, async () => {
          errors.length = 0;
          const response = await page.goto(fixture.url(route));
          assert.equal(response.status(), 200);
          assert.equal(new URL(page.url()).pathname, `${fixture.prefix}${route}`);
          assert.equal(await page.locator('.tc-shell').getAttribute('data-layout'), 'private');
          assert.equal(await page.locator('.tc-shell__skip').getAttribute('href'), '#main-content');
          const root = await loaded(page, selector);
          await check(root);
          assert.equal(await page.locator('.tc-shell__placeholder').count(), 0);
          assert.deepEqual(errors, [], route);
        });
      }
    } finally {
      await fixture.close();
    }
  });

test('real server renders all five anonymous public route families under the proxy prefix',
  {skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH, timeout: 60000}, async t => {
    const fixture = await startFixture();
    try {
      const published = await fixture.api(`/projects/${fixture.project.id}`, {method: 'PUT', body: {is_public: true}});
      assert.equal(published.status, 200);
      const page = await fixture.newPage({authenticated: false});
      const errors = [];
      const privateReads = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
      page.on('request', request => {
        const pathname = new URL(request.url()).pathname;
        if (pathname.startsWith(`${fixture.prefix}/api/`) && pathname !== `${fixture.prefix}/api/instance`) privateReads.push(pathname);
      });
      const project = fixture.project.identifier;
      for (const [route, kind, title] of [
        [`/public/${project}/issues`, 'issues', fixture.issue.title],
        [`/public/${project}/board`, 'board', fixture.issue.title],
        [`/public/${project}/issues/${fixture.issue.identifier}`, 'issue-detail', fixture.issue.title],
        [`/public/${project}/pages`, 'pages', fixture.page.title],
        [`/public/${project}/pages/${fixture.page.id}`, 'page-detail', fixture.page.title],
      ]) {
        await t.test(route, async () => {
          errors.length = 0;
          privateReads.length = 0;
          const response = await page.goto(fixture.url(route));
          assert.equal(response.status(), 200);
          assert.equal(new URL(page.url()).pathname, `${fixture.prefix}${route}`);
          assert.equal(await page.locator('.tc-shell').getAttribute('data-layout'), 'public');
          await contains(page.locator('.tc-shell__hint'), 'Read only');
          const root = await loaded(page, `[data-topcoat-public="${kind}"]`);
          await contains(root.locator('[data-public-content]'), title);
          assert.equal(await root.getAttribute('aria-readonly'), 'true');
          assert.equal(await page.locator('[data-palette-open]').count(), 0);
          assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), null);
          assert.equal((await page.context().cookies()).some(cookie => cookie.name === 'lific_token'), false);
          assert.deepEqual(privateReads, [], route);
          assert.deepEqual(errors, [], route);
        });
      }
    } finally {
      await fixture.close();
    }
  });
