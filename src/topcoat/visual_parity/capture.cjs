// Live Svelte/Topcoat comparison against one disposable executable fixture.
// Run from the repository's devenv shell. This never touches the user's database.
const fs = require('node:fs');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const {startFixture} = require('../acceptance/server.js');

async function main() {
  const snapshot = process.env.LIFIC_SVELTE_SNAPSHOT || '/tmp/lific-main-frontend.Nd6XMH/web';
  const output = process.env.LIFIC_VISUAL_OUTPUT || '/tmp/lific-visual-parity';
  fs.mkdirSync(output, {recursive: true});
  const fixture = await startFixture();
  let vite;
  try {
    for (const [route, body] of [[`/issues/${fixture.issue.id}`, {status: 'active', priority: 'high'}], [`/pages/${fixture.page.id}`, {pinned: true}]]) {
      const response = await fixture.api(route, {method: 'PUT', body});
      if (!response.ok) throw new Error(`${route}: ${response.status} ${await response.text()}`);
    }
    if (process.env.LIFIC_VISUAL_SCENARIO === 'populated') {
      const create = async (route, body) => {
        const response = await fixture.api(route, {method: 'POST', body});
        if (!response.ok) throw new Error(`${route}: ${response.status} ${await response.text()}`);
        return response.json();
      };
      const extraProject = await create('/projects', {name: 'Interface work', identifier: 'UI'});
      const group = await create('/project-groups', {name: 'Frontend'});
      for (const project of [fixture.project, extraProject]) {
        const assigned = await fixture.api('/project-groups/assign', {method: 'PUT', body: {project_id: project.id, group_id: group.id}});
        if (!assigned.ok) throw new Error(await assigned.text());
        for (const [index, priority] of ['urgent', 'high', 'medium', 'low', 'none', 'high', 'urgent'].entries()) await create('/issues', {
          project_id: project.id, title: index % 2 ? 'Preserve sidebar density and keyboard navigation' : 'Restore compact issue rows with matching status and priority icons across all project views',
          status: index % 2 ? 'todo' : 'active', priority,
        });
      }
    }
    const {createServer} = await import(pathToFileURL(path.join(snapshot, 'node_modules/vite/dist/node/index.js')).href);
    vite = await createServer({root: snapshot, logLevel: 'silent', configFile: path.join(snapshot, 'vite.config.ts'), server: {
      port: 5178, strictPort: true, host: '127.0.0.1', proxy: {
        '/api': {target: fixture.origin, rewrite: p => `${fixture.prefix}${p}`, ws: true},
        '/public/api': {target: fixture.origin, rewrite: p => `${fixture.prefix}${p}`},
      },
    }});
    await vite.listen();
    const routes = process.env.LIFIC_VISUAL_ROUTES?.split(',') || ['home'];
    const routePaths = {home: '/', palette: '/', navigation: '/', overview: '/ACC/overview', issues: '/ACC/issues', board: '/ACC/board',
      issue: `/ACC/issues/${fixture.issue.identifier}`, pages: '/ACC/pages', page: `/ACC/pages/${fixture.page.id}`,
      files: '/ACC/files', modules: '/ACC/modules', plans: '/ACC/plans', settings: '/settings', public: '/public/ACC/issues'};
    if (routes.includes('public')) {
      const published = await fixture.api(`/projects/${fixture.project.id}`, {method: 'PUT', body: {is_public: true}});
      if (!published.ok) throw new Error(await published.text());
    }
    const modes = [{name: 'desktop', width: 1440, height: 900}, {name: 'phone', width: 390, height: 844}];
    const report = [];
    for (const viewport of modes) for (const theme of ['light', 'dark']) for (const route of routes) {
      if (route === 'navigation' && viewport.name !== 'phone') continue;
      for (const implementation of ['svelte', 'topcoat']) {
        const context = await fixture.browser.newContext({viewport, hasTouch: viewport.name === 'phone', isMobile: viewport.name === 'phone', colorScheme: theme, reducedMotion: 'reduce', locale: 'en-US', timezoneId: 'America/Denver'});
        if (route !== 'public') await context.addCookies([{name: 'lific_token', value: fixture.token, url: fixture.origin, httpOnly: true, sameSite: 'Lax'}]);
        await context.addInitScript(({token, theme, origins, fontScale, recents}) => {
          if (!origins.includes(location.origin)) return;
          if (token) localStorage.setItem('lific_token', token);
          localStorage.setItem('lific_theme', theme);
          localStorage.setItem('lific_motion', 'reduced');
          localStorage.setItem('lific_font_scale', fontScale);
          localStorage.setItem('lific_recents', JSON.stringify(recents));
        }, {token: route === 'public' ? null : fixture.token, theme, origins: [fixture.origin, 'http://127.0.0.1:5178'], fontScale: process.env.LIFIC_VISUAL_FONT_SCALE || 'md',
          recents: process.env.LIFIC_VISUAL_SCENARIO === 'populated' ? [{type: 'issue', project: 'ACC', routeId: fixture.issue.identifier, identifier: fixture.issue.identifier, title: fixture.issue.title, ts: Date.now()},
            {type: 'page', project: 'ACC', routeId: String(fixture.page.id), identifier: fixture.page.identifier, title: fixture.page.title, ts: Date.now() - 1000}] : []});
        const page = await context.newPage();
        const errors = [];
        page.on('pageerror', e => errors.push(e.message));
        await page.goto(implementation === 'svelte' ? `http://127.0.0.1:5178/#${routePaths[route]}` : fixture.url(routePaths[route]), {waitUntil: 'networkidle'});
        await page.locator('main').first().waitFor();
        await page.evaluate(() => document.fonts.ready);
        await page.waitForTimeout(700);
        if (route === 'navigation') await page.getByRole('button', {name: 'Open navigation', exact: true}).click();
        if (route === 'palette') await page.getByRole('button', {name: /Jump to/}).last().click();
        const name = `${route}-${viewport.name}-${theme}-${implementation}`;
        await page.screenshot({path: path.join(output, `${name}.png`)});
        const geometry = await page.evaluate(() => [...document.querySelectorAll('main, aside, h1, h2, header, button')].slice(0, 55).map(el => ({tag: el.tagName, text: el.textContent.trim().slice(0, 70), rect: el.getBoundingClientRect().toJSON(), font: getComputedStyle(el).font, background: getComputedStyle(el).backgroundColor})));
        report.push({name, errors, geometry});
        console.log(name, errors.length ? JSON.stringify(errors) : 'captured');
        await context.close();
      }
    }
    fs.writeFileSync(path.join(output, 'geometry.json'), JSON.stringify(report, null, 2));
  } finally { await vite?.close(); await fixture.close(); }
}
main().catch(error => {console.error(error); process.exitCode = 1;});
