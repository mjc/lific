// Removed administration contracts, exercised only through the real mounted native UI.
// Missing native features deliberately fail and remain in the failure ledger.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {createHash} = require('node:crypto');
const {gunzipSync} = require('node:zlib');
const {DatabaseSync} = require('node:sqlite');
const {startFixture} = require('../../../acceptance/server.js');
const {settleScroll} = require('../../../native/browser_fixture.cjs');
const rootAlias = 'v1:0123456789abcdef0123456789abcdef01234567';
const remoteAlias = 'v1:github.com/acme/app';

async function ready(page) {
  await page.locator('nav.native-home-palette-results[data-native-home-connected="true"]').waitFor({state:'attached'});
}

function database(fixture, action) {
  const db = new DatabaseSync(fixture.database);
  try { return action(db); } finally { db.close(); }
}
function rows(fixture, sql, ...args) {
  return database(fixture, db => db.prepare(sql).all(...args).map(row => ({...row})));
}
async function withPage(run) {
  const fixture = await startFixture();
  try {
    database(fixture, db => db.exec('UPDATE instance_settings SET web_auto_login=0, authz_enforced=1'));
    const page = await fixture.newPage();
    page.setDefaultTimeout(5000); page.setDefaultNavigationTimeout(7000);
    const requests = [];
    page.on('request', request => requests.push(request));
    await page.goto(fixture.url('/ACC/overview'));
    await page.locator('.native-overview').waitFor({state:'visible'});
    await ready(page);
    await run({fixture, page, requests});
    assert.equal(requests.some(request => new URL(request.url()).pathname.includes('/api/')), false,
      'Administration uses actual native routes and procedures.');
  } finally { await fixture.close(); }
}
async function user(fixture, username, admin = false) {
  fixture.cli(['user','create','--username',username,'--email',`${username}@example.test`,
    '--password',fixture.credentials.password,...(admin ? ['--admin'] : []),'--json']);
  return rows(fixture, 'SELECT id, username FROM users WHERE username=?', username)[0];
}
async function login(fixture, identity) {
  const response = await fixture.api('/auth/login',{method:'POST',token:null,
    body:{identity,password:fixture.credentials.password}});
  assert.equal(response.status,200);
  return (await response.json()).token;
}
async function replaceCookie(page, fixture, token) {
  await page.context().addCookies([{name:'lific_token',value:token,url:fixture.origin,httpOnly:true,sameSite:'Lax'}]);
}
async function choose(page, id, label) {
  const trigger=page.locator(`#${id}-trigger`);
  await trigger.scrollIntoViewIfNeeded();await settleScroll(page);await trigger.click();
  await page.locator(`#${id}-menu`).getByRole('option').filter({hasText:label}).first().click();
}
function member(page, username) {
  return page.locator('.native-overview__member-row').filter({hasText:`@${username}`});
}
function bindingRows(fixture) {
  return rows(fixture, 'SELECT b.id,b.project_id,i.kind,i.value FROM repo_bindings b JOIN repo_identities i ON i.binding_id=b.id WHERE b.project_id=? ORDER BY i.id', fixture.project.id);
}

test('native repository binding conflict, canonical remote/root aliases, and cancelled or completed removal', async () => {
  await withPage(async ({fixture,page}) => {
    fixture.cli(['project','create','--name','Conflicting repository','--identifier','OTHER','--json']);
    database(fixture, db => {
      const other = db.prepare("SELECT id FROM projects WHERE identifier='OTHER'").get();
      const owner = db.prepare("SELECT id FROM users WHERE is_admin=1 LIMIT 1").get();
      const inserted = db.prepare('INSERT INTO repo_bindings(project_id,created_by) VALUES(?,?)').run(other.id,owner.id);
      db.prepare('INSERT INTO repo_identities(binding_id,kind,value) VALUES(?,?,?)').run(inserted.lastInsertRowid,'remote',remoteAlias);
    });
    const alias = page.getByLabel('Repository alias',{exact:true});
    await alias.fill(` ${remoteAlias} `);
    await page.getByRole('button',{name:'Bind repository',exact:true}).click();
    await page.getByText(/already (?:bound|claimed)/).first().waitFor();
    assert.deepEqual(bindingRows(fixture),[],'Conflict does not publish a local binding.');
    database(fixture, db => db.exec("DELETE FROM repo_identities; DELETE FROM repo_bindings"));
    await page.getByRole('button',{name:'Bind repository',exact:true}).click();
    await page.getByText(`remote: ${remoteAlias}`,{exact:true}).waitFor();
    const [canonical] = bindingRows(fixture);
    assert.equal(canonical.value,remoteAlias);assert.equal(canonical.kind,'remote');
    page.once('dialog',dialog => dialog.dismiss());
    await page.getByRole('button',{name:'Remove binding',exact:true}).click();
    assert.deepEqual(bindingRows(fixture),[canonical],'Cancelled removal retains canonical record.');
    page.once('dialog',dialog => dialog.accept());
    await page.getByRole('button',{name:'Remove binding',exact:true}).click();
    await page.getByText('No repositories are bound to this project.',{exact:true}).waitFor();
    assert.deepEqual(bindingRows(fixture),[]);
    await page.getByLabel('Repository alias type',{exact:true}).selectOption('root');
    await alias.fill(` ${rootAlias} `);
    await page.getByRole('button',{name:'Bind repository',exact:true}).click();
    await page.getByText(`root: ${rootAlias}`,{exact:true}).waitFor();
    assert.equal(bindingRows(fixture)[0].value,rootAlias,'Canonical first-parent root alias is unchanged.');
  });
});

test('native binding readers have no mutation controls even when role enforcement is off', async () => {
  await withPage(async ({fixture,page,requests}) => {
    const reader = await user(fixture,'binding-reader');
    const token = await login(fixture,reader.username);
    database(fixture, db => {
      db.exec('UPDATE instance_settings SET authz_enforced=0');
      const inserted=db.prepare('INSERT INTO repo_bindings(project_id,created_by) VALUES(?,?)').run(fixture.project.id,reader.id);
      db.prepare('INSERT INTO repo_identities(binding_id,kind,value) VALUES(?,?,?)').run(inserted.lastInsertRowid,'root',rootAlias);
    });
    await replaceCookie(page,fixture,token);
    for (const role of ['viewer','maintainer',null]) {
      database(fixture, db => {
        db.prepare('DELETE FROM project_members WHERE project_id=? AND user_id=?').run(fixture.project.id,reader.id);
        if(role)db.prepare('INSERT INTO project_members(project_id,user_id,role) VALUES(?,?,?)').run(fixture.project.id,reader.id,role);
      });
      const before=requests.length;
      await page.goto(fixture.url('/ACC/overview'));await ready(page);
      await page.getByText(`root: ${rootAlias}`,{exact:true}).waitFor();
      assert.equal(await page.getByRole('button',{name:'Bind repository',exact:true}).count(),0);
      assert.equal(await page.getByRole('button',{name:'Remove binding',exact:true}).count(),0);
      assert.equal(requests.slice(before).filter(request=>request.method()!=='GET'&&/bind/.test(request.url())).length,0);
    }
  });
});

// Observers delegate to the browser's real Blob URL and anchor primitives.
// They record lifecycle timing and never replace a result or an application action.
async function observeDownloads(page) {
  await page.addInitScript(() => {
    window.nativeDownloadLifecycle=[];
    window.nativeDownloadBodies=[];
    const originalFetch=window.fetch.bind(window);
    window.fetch=async (input,options)=>{
      const response=await originalFetch(input,options);
      const url=typeof input==='string'?input:input.url;
      if(/\/__native_overview\/(?:archive|export)\//.test(url)) {
        window.nativeDownloadBodies.push({url,bytes:response.clone().arrayBuffer().then(buffer=>[...new Uint8Array(buffer)])});
      }
      return response;
    };
    const create=URL.createObjectURL.bind(URL), revoke=URL.revokeObjectURL.bind(URL);
    URL.createObjectURL=blob=>{const url=create(blob);window.nativeDownloadLifecycle.push({kind:'create',url,time:performance.now()});return url;};
    URL.revokeObjectURL=url=>{window.nativeDownloadLifecycle.push({kind:'revoke',url,time:performance.now()});return revoke(url);};
  });
}
async function responseBytes(page,url) {
  return Buffer.from(await page.evaluate(async url=>await window.nativeDownloadBodies.find(body=>body.url===url).bytes,url));
}
async function downloadedBytes(download) {
  const chunks=[];for await(const chunk of await download.createReadStream())chunks.push(chunk);
  return Buffer.concat(chunks);
}

test('archive fetch returns bytes without saving and forwards cancellation and session', async () => {
  await withPage(async ({fixture,page}) => {
    await page.addInitScript(() => {
      const fetchArchive=window.fetch.bind(window);
      window.nativeArchiveTransport=[];
      let saved=0;const createURL=URL.createObjectURL.bind(URL);
      URL.createObjectURL=blob=>{saved++;return createURL(blob);};
      window.fetch=async (input,options) => {
        const url=typeof input==='string'?input:input.url;
        if(!url.includes('/__native_overview/archive/'))return fetchArchive(input,options);
        const observation={url,cache:options?.cache,hasAbortSignal:options?.signal instanceof AbortSignal,
          aborted:options?.signal?.aborted,savedWhileFetching:false};
        window.nativeArchiveTransport.push(observation);
        options?.signal?.addEventListener('abort',()=>{observation.aborted=true;},{once:true});
        const response=await fetchArchive(input,options);
        observation.savedWhileFetching=saved!==0;observation.fetched=true;
        return response;
      };
    });
    await page.reload();await ready(page);
    const panel=page.locator('.native-overview__archive');await panel.getByRole('checkbox').check();
    const request=page.waitForRequest(request=>new URL(request.url()).pathname.includes('/__native_overview/archive/'));
    await panel.getByRole('button',{name:'Download project archive',exact:true}).click();
    const sent=await request;
    await page.waitForFunction(()=>window.nativeArchiveTransport[0]?.fetched===true);
    const transport=await page.evaluate(()=>window.nativeArchiveTransport[0]);
    assert.ok(transport.hasAbortSignal,'Actual archive fetch receives its mounted operation AbortSignal.');
    assert.equal(transport.cache,'no-store','Archive bytes must bypass browser cache.');
    assert.equal(transport.aborted,false);
    assert.ok((await sent.allHeaders()).cookie?.includes(`lific_token=${fixture.token}`),
      'Actual native archive request carries the current cookie session.');
    assert.equal(transport.savedWhileFetching,false,'Fetching archive bytes alone has not saved a file.');
  });
});

test('native archive acknowledgement, exact filename/bytes, verified owner and deferred object URL cleanup', async () => {
  await withPage(async ({fixture,page,requests}) => {
    await observeDownloads(page);await page.reload();await ready(page);
    const panel=page.locator('.native-overview__archive');
    const button=panel.getByRole('button',{name:'Download project archive',exact:true});
    assert.ok(await button.isDisabled());
    assert.equal(requests.filter(request=>/\/__native_overview\/archive\//.test(request.url())).length,0);
    await panel.getByRole('checkbox').check();
    const responsePromise=page.waitForResponse(response=>/\/__native_overview\/archive\//.test(response.url()));
    const downloadPromise=page.waitForEvent('download',{timeout:7000});
    await button.click();
    const response=await responsePromise, download=await downloadPromise;
    assert.equal(response.status(),200);assert.equal(download.suggestedFilename(),'ACC.lific.tar.gz');
    const bytes=await downloadedBytes(download);
    assert.deepEqual(bytes,await responseBytes(page,new URL(response.url()).pathname),'Saved archive is exactly the real native HTTP response.');
    assert.deepEqual([...bytes.subarray(0,2)],[0x1f,0x8b]);
    assert.ok(gunzipSync(bytes).includes(Buffer.from('Acceptance issue')),'Archive contains the actual fixture graph.');
    assert.ok(requests.some(request=>/\/__native_overview\/archive_owner\//.test(request.url())),
      'Actual fresh owner verification happens before browser save.');
    await page.waitForFunction(()=>window.nativeDownloadLifecycle.some(event=>event.kind==='revoke'));
    const lifecycle=await page.evaluate(()=>window.nativeDownloadLifecycle);
    const created=lifecycle.find(event=>event.kind==='create'),revoked=lifecycle.find(event=>event.kind==='revoke'&&event.url===created.url);
    assert.ok(revoked.time-created.time>=1000,'Download URL is retained until the original 1000ms deferred cleanup.');
    assert.equal(await page.locator('a[download]').count(),0);
  });
});

test('native archive saving is refused after account replacement while preparing', async () => {
  await withPage(async ({fixture,page}) => {
    const replacement=await user(fixture,'replacement-admin',true),token=await login(fixture,replacement.username);
    const downloads=[];page.on('download',download=>downloads.push(download.suggestedFilename()));
    const panel=page.locator('.native-overview__archive');await panel.getByRole('checkbox').check();
    const request=page.waitForEvent('request',{predicate:request=>/\/__native_overview\/archive\//.test(request.url()),timeout:5000});
    const click=panel.getByRole('button',{name:'Download project archive',exact:true}).click();
    await request;await replaceCookie(page,fixture,token);await click;
    await panel.getByRole('alert').waitFor({state:'visible'});
    assert.match(await panel.getByRole('alert').innerText(),/account changed|HTTP (?:401|403)/i);
    assert.deepEqual(downloads,[],'Old account archive never triggers a download.');
  });
});

test('native ZIP export saves its real bytes with the ZIP filename and deferred cleanup', async () => {
  await withPage(async ({page}) => {
    await observeDownloads(page);await page.reload();await ready(page);
    const responsePromise=page.waitForResponse(response=>/\/__native_overview\/export\//.test(response.url()));
    const downloadPromise=page.waitForEvent('download',{timeout:7000});
    await page.getByRole('button',{name:'Export',exact:true}).click();
    const response=await responsePromise,download=await downloadPromise,bytes=await downloadedBytes(download);
    assert.equal(download.suggestedFilename(),'acc-export.zip');assert.deepEqual(bytes,await responseBytes(page,new URL(response.url()).pathname));
    assert.equal(bytes.subarray(0,2).toString(),'PK');
    await page.waitForFunction(()=>window.nativeDownloadLifecycle.some(event=>event.kind==='revoke'));
    const events=await page.evaluate(()=>window.nativeDownloadLifecycle);
    assert.ok(events.find(event=>event.kind==='revoke').time-events.find(event=>event.kind==='create').time>=1000);
    assert.equal(await page.locator('a[download]').count(),0);
  });
});

test('native archive controls deny viewer and maintainer with general authorization off', async () => {
  await withPage(async ({fixture,page}) => {
    const reader=await user(fixture,'archive-reader'),token=await login(fixture,reader.username);
    database(fixture,db=>db.exec('UPDATE instance_settings SET authz_enforced=0'));
    await replaceCookie(page,fixture,token);
    const downloads=[];page.on('download',download=>downloads.push(download.suggestedFilename()));
    for(const role of ['viewer','maintainer']) {
      database(fixture,db=>{
        db.prepare('DELETE FROM project_members WHERE project_id=? AND user_id=?').run(fixture.project.id,reader.id);
        db.prepare('INSERT INTO project_members(project_id,user_id,role) VALUES(?,?,?)').run(fixture.project.id,reader.id,role);
      });
      await page.goto(fixture.url('/ACC/overview'));await ready(page);await page.locator('.native-overview').waitFor();
      assert.equal(await page.getByRole('button',{name:'Download project archive',exact:true}).count(),0);
    }
    assert.deepEqual(downloads,[]);
  });
});

test('native member add retains joined metadata and role refusal rolls back persisted and rendered state', async () => {
  await withPage(async ({fixture,page}) => {
    const person=await user(fixture,'metadata-member');
    database(fixture,db=>db.prepare('UPDATE users SET display_name=? WHERE id=?').run('Metadata person',person.id));
    await page.reload();await ready(page);
    await choose(page,`native-overview-member-person-${fixture.project.id}`,'Metadata person');
    const addResponse=page.waitForResponse(response=>response.url().endsWith('/__native_overview/manage'));
    await page.locator('.native-overview__member-add').getByRole('button',{name:'Add',exact:true}).click();
    assert.deepEqual((await (await addResponse).json())[0],'saved','Actual member add persists before metadata is rendered.');
    const row=member(page,person.username);await row.waitFor();
    assert.ok((await row.innerText()).includes('Metadata person'));
    const roleResponse=page.waitForResponse(response=>response.url().endsWith('/__native_overview/manage'));
    await choose(page,`native-overview-member-${fixture.project.id}-${person.id}`,'Maintainer');
    assert.ok((await roleResponse).ok());
    await page.waitForFunction(id=>document.querySelector(`#native-overview-member-${id}-trigger`)?.textContent.includes('Maintainer'),`${fixture.project.id}-${person.id}`);
    assert.equal(rows(fixture,'SELECT role FROM project_members WHERE project_id=? AND user_id=?',fixture.project.id,person.id)[0].role,'maintainer');
    assert.ok((await row.innerText()).includes('Metadata person'));assert.ok((await row.innerText()).includes(`@${person.username}`));
    // The real last-lead guard supplies a refusal without a synthetic server response.
    const admin=rows(fixture,'SELECT id FROM users WHERE username=?',fixture.credentials.identity)[0];
    database(fixture,db=>{
      db.prepare('INSERT OR REPLACE INTO project_members(project_id,user_id,role) VALUES(?,?,?)').run(fixture.project.id,admin.id,'lead');
    });
    await page.reload();await ready(page);
    await choose(page,`native-overview-member-${fixture.project.id}-${admin.id}`,'Viewer');
    await page.getByRole('alert').filter({visible:true}).first().waitFor();
    assert.equal(rows(fixture,'SELECT role FROM project_members WHERE project_id=? AND user_id=?',fixture.project.id,admin.id)[0].role,'lead');
    assert.ok((await member(page,fixture.credentials.identity).innerText()).includes('Lead'));
  });
});

test('native Overview recent-auth refusal freezes member grant, retries password, rotates cookie and commits once', async () => {
  await withPage(async ({fixture,page,requests}) => {
    const person=await user(fixture,'reauth-member');
    const hash=createHash('sha256').update(fixture.token).digest('hex');
    database(fixture,db=>db.prepare("UPDATE sessions SET created_at=datetime('now','-1 day') WHERE token=?").run(hash));
    await page.reload();await ready(page);
    await choose(page,`native-overview-member-person-${fixture.project.id}`,person.username);
    await choose(page,`native-overview-member-role-${fixture.project.id}`,'Maintainer');
    await page.locator('.native-overview__member-add').getByRole('button',{name:'Add',exact:true}).click();
    const prompt=page.locator('.native-overview__members .native-overview__grant');
    const password=prompt.getByPlaceholder('your current password',{exact:true});await password.waitFor({state:'visible'});
    assert.deepEqual(rows(fixture,'SELECT role FROM project_members WHERE project_id=? AND user_id=?',fixture.project.id,person.id),[]);
    await password.fill('wrong password');await prompt.getByRole('button',{name:'Confirm and continue',exact:true}).click();
    await prompt.getByRole('alert').waitFor({state:'visible'});assert.equal(await password.inputValue(),'');
    assert.equal((await page.context().cookies()).find(cookie=>cookie.name==='lific_token').value,fixture.token);
    await password.fill(fixture.credentials.password);
    await Promise.all([page.waitForURL(fixture.url('/ACC/overview')),prompt.getByRole('button',{name:'Confirm and continue',exact:true}).click()]);
    await member(page,person.username).waitFor();
    assert.equal(rows(fixture,'SELECT role FROM project_members WHERE project_id=? AND user_id=?',fixture.project.id,person.id)[0].role,'maintainer');
    assert.notEqual((await page.context().cookies()).find(cookie=>cookie.name==='lific_token').value,fixture.token);
    assert.equal(requests.filter(request=>request.url().endsWith('/__native_overview/manage_confirm')).length,2,'One rejected password and one verified retry.');
    assert.equal(rows(fixture,'SELECT COUNT(*) AS count FROM project_members WHERE project_id=? AND user_id=?',fixture.project.id,person.id)[0].count,1);
  });
});

test('native Overview pending recent-auth command cannot commit into a replacement account', async () => {
  await withPage(async ({fixture,page}) => {
    const person=await user(fixture,'isolated-member'),replacement=await user(fixture,'isolated-admin',true);
    const token=await login(fixture,replacement.username);
    database(fixture,db=>db.prepare("UPDATE sessions SET created_at=datetime('now','-1 day') WHERE token=?").run(createHash('sha256').update(fixture.token).digest('hex')));
    await page.reload();await ready(page);await choose(page,`native-overview-member-person-${fixture.project.id}`,person.username);
    await page.locator('.native-overview__member-add').getByRole('button',{name:'Add',exact:true}).click();
    const prompt=page.locator('.native-overview__members .native-overview__grant'),password=prompt.getByPlaceholder('your current password',{exact:true});
    await password.waitFor({state:'visible'});await replaceCookie(page,fixture,token);
    const refused=page.waitForResponse(response=>response.url().endsWith('/__native_overview/manage_confirm'));
    await password.fill(fixture.credentials.password);await prompt.getByRole('button',{name:'Confirm and continue',exact:true}).click();
    assert.deepEqual((await (await refused).json())[0],'account_changed');
    assert.deepEqual(rows(fixture,'SELECT role FROM project_members WHERE project_id=? AND user_id=?',fixture.project.id,person.id),[]);
    await page.getByText(/account changed/i).filter({visible:true}).first().waitFor();
    assert.deepEqual(rows(fixture,'SELECT role FROM project_members WHERE project_id=? AND user_id=?',fixture.project.id,person.id),[]);
  });
});

test('native GitHub preview commits the exact previewed repository and mappings through the real importer', async () => {
  await withPage(async ({fixture,page,requests}) => {
    // This case uses the genuine framework upstream; network failures remain visible.
    const repo='tokio-rs/topcoat';
    const importer=page.locator('[data-native-overview-import]');
    await importer.getByPlaceholder('owner/name',{exact:true}).fill(repo);
    await importer.locator('.native-overview-import__mapping select').nth(0).selectOption('open');
    await importer.locator('.native-overview-import__mapping select').nth(1).selectOption('todo');
    const previewResponse=page.waitForResponse(response=>response.url().endsWith('/__native_overview/import_github'));
    await importer.getByRole('button',{name:'Preview import',exact:true}).click();
    const previewReply=await previewResponse;
    assert.ok(previewReply.ok(),'The actual preview procedure completes successfully.');
    assert.deepEqual((await previewReply.json())[0],{t:'Result',ok:'preview'},
      'The real upstream import returns a successful preview.');
    const commit=importer.getByRole('button',{name:/^Import \d+ issues?$/});await commit.waitFor({state:'visible'});
    assert.equal(rows(fixture,"SELECT COUNT(*) AS count FROM issues WHERE source LIKE 'github:%'")[0].count,0,'Preview writes nothing.');
    assert.ok(await commit.isEnabled(),'Live preview contains importable issues.');
    const previewRequest=requests.filter(request=>request.url().endsWith('/__native_overview/import_github')).at(-1);
    const preview=previewRequest.postDataJSON();
    assert.equal(preview[2],repo);assert.equal(preview[4],'open');assert.equal(preview[5],'todo');assert.equal(preview[7],true);
    // The real confirmation step keeps the previewed configuration out of editable UI.
    assert.ok(await importer.getByPlaceholder('owner/name',{exact:true}).isHidden());
    assert.ok(await importer.getByPlaceholder('ghp_…',{exact:true}).isHidden());
    await commit.click();await importer.getByRole('heading',{name:'Import complete',exact:true}).waitFor();
    const confirmed=requests.filter(request=>request.url().endsWith('/__native_overview/import_github')).at(-1).postDataJSON();
    assert.deepEqual(confirmed.slice(0,7),preview.slice(0,7));assert.equal(confirmed[7],false);
    const imported=rows(fixture,"SELECT source,status FROM issues WHERE source LIKE 'github:%'");
    assert.ok(imported.length>0);assert.ok(imported.every(issue=>issue.source.startsWith(`github:${repo}#`)&&issue.status==='todo'));
  });
});
