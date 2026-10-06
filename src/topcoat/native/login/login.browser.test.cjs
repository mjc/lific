const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {tmpdir} = require('node:os');
const {launchBrowser, mountedProxy, settleScroll} = require('../browser_fixture.cjs');

const [origin, username = 'viewer', password = 'testpassword1'] = process.argv.slice(2);
const output = path.join(tmpdir(), 'lific-native-login-browser');

async function capture(page, name) {
  await page.evaluate(() => document.fonts.ready);
  await settleScroll(page);
  await page.screenshot({path:path.join(output, name), fullPage:true, animations:'disabled'});
}

test('anonymous native root offers Login and signs in through mounted framework procedures', async () => {
  assert.ok(origin, 'Pass the production fixture origin.');
  fs.mkdirSync(output, {recursive:true});
  const browser = await launchBrowser();
  try {
    for (const prefix of ['', '/app', '/app/login']) {
      const proxy = await mountedProxy(new URL(origin), prefix);
      const context = await browser.newContext({viewport:{width:1440, height:900},
        reducedMotion:'reduce', colorScheme:'light', locale:'en-US'});
      const errors = [], requests = [], bodies = [];
      try {
        const page = await context.newPage();
        page.setDefaultTimeout(15000);
        page.on('pageerror', error => errors.push(error.message));
        page.on('console', message => {
          if (message.type() === 'error') errors.push(message.text());
        });
        page.on('request', request => requests.push({method:request.method(), url:request.url()}));
        await page.route(`${proxy.origin}${prefix}/__native_login/sign_in`, async route => {
          const response = await route.fetch();
          const body = await response.text();
          bodies.push(body);
          await route.fulfill({response, body});
        });
        await page.goto(`${proxy.origin}${prefix}/`);
        await page.waitForURL(`${proxy.origin}${prefix}/login`);
        await page.getByRole('heading', {name:'Welcome back.', exact:true}).waitFor();
        await page.getByText('Sign in to continue on this instance.', {exact:true}).waitFor();
        await page.getByText('Not signed in', {exact:true}).waitFor();
        const identity = page.getByLabel('Username or email', {exact:true});
        const secret = page.getByLabel('Password', {exact:true});
        const submit = page.locator('form').getByRole('button', {name:'Sign in', exact:true});
        assert.equal(await identity.getAttribute('autocomplete'), 'username');
        assert.equal(await secret.getAttribute('autocomplete'), 'current-password');
        assert.equal(await secret.getAttribute('type'), 'password');
        assert.equal(await submit.isDisabled(), true, 'Empty credentials cannot submit.');
        assert.ok(!(await page.content()).includes(password), 'The initial document contains no fixture password.');
        assert.equal((await context.cookies()).some(cookie => cookie.name === 'lific_token'), false);
        const name = prefix.replaceAll('/', '_') || 'root';
        await capture(page, `${name}-login.png`);

        await identity.fill(username);
        const wrongPassword = 'definitely-wrong-password';
        await secret.fill(wrongPassword);
        await submit.click();
        const error = page.locator('form').getByRole('alert');
        await error.waitFor({state:'visible'});
        assert.equal((await error.textContent()).trim(), 'invalid username/email or password',
          'Rejected credentials use the established login error.');
        assert.equal(new URL(page.url()).pathname, `${prefix}/login`);
        assert.equal((await context.cookies()).some(cookie => cookie.name === 'lific_token'), false,
          'A rejected login cannot create a session.');
        assert.ok(!(await page.content()).includes(wrongPassword), 'Password state is not serialized into markup.');
        assert.ok(!page.url().includes(wrongPassword));
        await capture(page, `${name}-rejected.png`);

        await secret.fill(password);
        await page.getByRole('button', {name:'Show password', exact:true}).click();
        assert.equal(await secret.getAttribute('type'), 'text');
        await page.getByRole('button', {name:'Hide password', exact:true}).click();
        assert.equal(await secret.getAttribute('type'), 'password');
        assert.ok(!(await page.content()).includes(password), 'Visibility changes keep secrets out of serialized HTML.');
        await submit.click();
        await page.waitForURL(`${proxy.origin}${prefix}/`);
        await page.locator('#native-home-greeting').waitFor();
        const cookie = (await context.cookies()).find(cookie => cookie.name === 'lific_token');
        assert.ok(cookie?.value, 'The established authentication service sets a session cookie.');
        assert.equal(cookie.httpOnly, true);
        assert.equal(cookie.sameSite, 'Lax');
        assert.equal(await page.evaluate(() => document.cookie.includes('lific_token')), false);
        assert.equal(await page.evaluate(() => localStorage.getItem('lific_token')), null,
          'Native login stores no token in browser application state.');
        assert.ok(!(await page.content()).includes(password));
        assert.ok(!(await page.content()).includes(cookie.value));
        assert.ok(!page.url().includes(password));
        await capture(page, `${name}-home.png`);
        await page.goto(`${proxy.origin}${prefix}/login`);
        await page.waitForURL(`${proxy.origin}${prefix}/`);
        await page.locator('#native-home-greeting').waitFor();
        assert.equal(await page.getByRole('heading', {name:'Welcome back.', exact:true}).count(), 0,
          'An authenticated Login GET redirects to the mounted Home.');
        assert.equal(bodies.length, 2, 'Both attempts use the real native sign-in procedure.');
        for (const body of bodies) {
          assert.ok(!body.includes(password));
          assert.ok(!body.includes(wrongPassword));
          assert.ok(!body.includes(cookie.value), 'The native procedure never returns the JSON session token.');
        }
        assert.deepEqual(requests.filter(request => /(?:^|\/)api\//.test(new URL(request.url).pathname)), [],
          'Login uses native framework transport rather than frontend REST calls.');
        assert.deepEqual(requests.filter(request => request.method === 'POST'
          && !new URL(request.url).pathname.startsWith(`${prefix}/__native_`)
          && !new URL(request.url).pathname.startsWith(`${prefix}/__topcoat`)), []);
        assert.deepEqual(errors, []);
      } finally {
        await context.close();
        await proxy.close();
      }
    }
  } finally {
    await browser.close();
  }
});
