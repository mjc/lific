const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
test(
  "headless project administration edits the shared identity and rolls back failed group assignment",
  { skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH },
  async () => {
    const { chromium } = await import(
      path.resolve(
        __dirname,
        "../../../..",
        "e2e/node_modules/playwright/index.mjs",
      )
    );
    const browser = await chromium.launch({
      headless: true,
      executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH,
    });
    try {
      const page = await browser.newPage();
      page.setDefaultTimeout(5000);
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      const html =
        '<html><head><title>Project settings</title></head><body><header><div data-project-identity><h1>Old name</h1></div></header><section data-topcoat-project-settings="settings" data-project-identifier="LIF"><p data-project-settings-status role="status"></p><p data-project-settings-error role="alert"></p><p data-project-settings-warning role="status"></p><div data-project-settings-content></div></section></body></html>';
      await page.route("http://lific.test/**", (route) =>
        route.fulfill({ contentType: "text/html", body: html }),
      );
      await page.goto("http://lific.test/app/LIF/overview");
      await page.addScriptTag({
        content: fs.readFileSync(`${__dirname}/project-settings.js`, "utf8"),
      });
      await page.evaluate(() => {
        window.LificTopcoatRouting = {href: route => `/app${route}`};
        window.calls = [];
        window.role = "lead";
        window.project = {
          id: 4,
          identifier: "LIF",
          name: "Lific",
          description: "An issue tracker",
          emoji: null,
          lead_user_id: 1,
          is_public: false,
        };
        const session = {
          state: { publicProject: null, user: { id: 1 } },
          request: async (path, options = {}) => {
            window.calls.push({ path, ...options });
            if (path === "/projects/4" && options.method === "PUT") {
              window.project = {
                ...window.project,
                ...JSON.parse(options.body),
              };
              return { ok: true, data: window.project };
            }
            if (path === "/project-groups/assign")
              return { ok: false, error: "Group was removed", status: 404 };
            return {
              ok: true,
              data:
                path === "/projects"
                  ? [window.project]
                  : path === "/project-groups"
                    ? [
                        {
                          id: 1,
                          name: "Work",
                          sort_order: 0,
                          project_ids: [4],
                        },
                        {
                          id: 2,
                          name: "Personal",
                          sort_order: 1,
                          project_ids: [],
                        },
                      ]
                    : path === "/projects/4/my-role"
                      ? { role: window.role, enforced: true, is_admin: false }
                      : path === "/project-archives"
                        ? { can_import: true, max_upload_bytes: 100 }
                        : path === "/users"
                          ? [{ id: 1, username: "me" }]
                          : [],
            };
          },
        };
        window.session = session;
        window.app = LificTopcoatProjectSettings.attach(
          document.querySelector("[data-topcoat-project-settings]"),
          { session },
        );
      });
      const download = await page.evaluate(async () => {
        const calls = []; window.fetch = async (url, options) => {calls.push({url, headers: options.headers});return new Response('archive');};
        localStorage.setItem('lific_token', 'fixture-token');
        const result = await app.controller.env.download('/projects/4/archive', 'archive.tar.gz');
        return {ok: result.ok, calls};
      });
      assert.deepEqual(download, {ok: true, calls: [{url: '/app/api/projects/4/archive', headers: {Authorization: 'Bearer fixture-token'}}]});
      await page.getByRole("button", { name: "Lific", exact: true }).click();
      await page
        .locator("[data-project-identity]")
        .getByLabel("Name")
        .fill("New project");
      await page
        .locator("[data-project-identity]")
        .getByRole("button", { name: "Save", exact: true })
        .click();
      await page.waitForFunction(
        () =>
          document.querySelector("[data-project-identity] h1")?.textContent ===
          "New project",
      );
      assert.equal(await page.locator("h1").count(), 1);
      assert.deepEqual(errors, []);
      assert.ok(
        await page.locator("form[data-form=assignment]").count(),
        await page.locator("body").innerHTML(),
      );
      const assignment = page.locator("form[data-form=assignment]");
      await assignment.getByLabel("Group", { exact: true }).selectOption("2");
      await assignment.getByRole("button", { name: "Save group" }).click();
      await page.waitForFunction(
        () =>
          document.querySelector("[data-project-settings-error]")
            .textContent === "Group was removed",
      );
      assert.equal(
        await assignment.getByLabel("Group", { exact: true }).inputValue(),
        "1",
      );
      await page.evaluate(() => {
        window.role = "viewer";
        window.project.lead_user_id = null;
        dispatchEvent(
          new CustomEvent("lific:realtime", {
            detail: { type: "project.updated", project_id: 4 },
          }),
        );
      });
      await page.waitForFunction(
        () => window.app.controller.state.role?.role === "viewer",
      );
      assert.equal(
        await page
          .getByRole("button", { name: "Download project archive" })
          .count(),
        0,
      );
      assert.equal(
        await page.locator("[data-project-identity] button").count(),
        0,
      );
      await page.evaluate(() => {
        window.session.state.user = { id: 2 };
        window.app.controller.accountChanged();
      });
      assert.equal(
        await page.locator("[data-project-settings-content]").textContent(),
        "",
      );
      assert.deepEqual(errors, []);
    } finally {
      await browser.close();
    }
  },
);
test(
  "headless archive import keeps uncertain outcome blocked until the project list is checked",
  { skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH },
  async () => {
    const { chromium } = await import(
      path.resolve(
        __dirname,
        "../../../..",
        "e2e/node_modules/playwright/index.mjs",
      )
    );
    const browser = await chromium.launch({
      headless: true,
      executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH,
    });
    try {
      const page = await browser.newPage();
      page.setDefaultTimeout(5000);
      await page.route("http://lific.test/**", (route) =>
        route.fulfill({
          contentType: "text/html",
          body: '<html><head><title>Archive import</title></head><body><section data-topcoat-project-settings="archive"><p data-project-settings-status role="status"></p><p data-project-settings-error role="alert"></p><p data-project-settings-warning role="status"></p><div data-project-settings-content></div></section></body></html>',
        }),
      );
      await page.goto("http://lific.test/app/projects/import");
      await page.addScriptTag({
        content: fs.readFileSync(`${__dirname}/project-settings.js`, "utf8"),
      });
      await page.evaluate(() => {
        window.LificTopcoatRouting = {href: route => `/app${route}`};
        const session = {
          state: { publicProject: null, user: { id: 1 } },
          request: async (path) => ({
            ok: true,
            data:
              path === "/project-archives"
                ? { can_import: true, max_upload_bytes: 100 }
                : [],
          }),
        };
        window.app = LificTopcoatProjectSettings.attach(
          document.querySelector("[data-topcoat-project-settings]"),
          { session },
        );
        window.app.controller.env.upload = async (
          _file,
          progress,
          processing,
        ) => {
          progress(70);
          processing();
          return { ok: false, status: null, error: "Connection lost" };
        };
      });
      await page
        .getByLabel("Project archive (.tar.gz)", { exact: true })
        .setInputFiles({
          name: "project.tar.gz",
          mimeType: "application/gzip",
          buffer: Buffer.from("archive"),
        });
      await page.getByLabel(/I understand/).check();
      await page
        .getByRole("button", { name: "Import as private project" })
        .click();
      await page
        .getByRole("button", { name: "I've checked the project list" })
        .waitFor();
      assert.equal(
        await page
          .getByRole("button", { name: "Import as private project" })
          .count(),
        0,
      );
      await page
        .getByRole("button", { name: "I've checked the project list" })
        .click();
      await page
        .getByRole("button", { name: "Import as private project" })
        .waitFor();
    } finally {
      await browser.close();
    }
  },
);

test('project publication, repository bindings, archive transfer and deletion complete through the mounted browser UI',
  {skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH, timeout: 60000}, async t => {
    const {chromium} = await import(path.resolve(__dirname, '../../../..', 'e2e/node_modules/playwright/index.mjs'));
    const browser = await chromium.launch({headless: true, executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH});
    const page = await browser.newPage();page.setDefaultTimeout(5000);
    const requests = [], errors = [];
    page.on('pageerror', error => errors.push(error.message));
    let project = {id: 4, identifier: 'LIF', name: 'Lific', description: '', lead_user_id: 1, is_public: false};
    let role = {role:'lead',enforced:true,is_admin:false};
    let bindings = [], refusePublication = true, refuseBinding = true, refuseDeletion = true, refuseArchive = true, refuseImport = true;
    const record = {binding: {id: 7, project_id: 4, created_by: 1}, identities: [{id: 8, binding_id: 7, kind: 'remote', value: 'v1:github.com/acme/app'}]};
    await page.route('http://accept.test/**', async route => {
      const request = route.request(), url = new URL(request.url()), api = url.pathname.slice('/app/api'.length);
      if (!url.pathname.startsWith('/app/api')) {
        return route.fulfill({contentType:'text/html',body:`<!doctype html><body><section data-topcoat-project-settings="${url.pathname.endsWith('/import')?'archive':'settings'}" data-project-identifier="${url.pathname.endsWith('/import')?'':'LIF'}"><p data-project-settings-status></p><p data-project-settings-error></p><p data-project-settings-warning></p><div data-project-settings-content></div></section></body>`});
      }
      requests.push({path:url.pathname,method:request.method(),body:request.postData(),authorization:request.headers().authorization});
      const send = (body, status=200) => route.fulfill({status,contentType:'application/json',body:JSON.stringify(body)});
      if(api==='/projects')return send([project]);
      if(api==='/projects/4/my-role')return send(role);
      if(api==='/project-archives'&&request.method()==='GET')return send({can_import:true,max_upload_bytes:1048576});
      if(api==='/auth/me')return send({id:1,username:'lead'});
      if(api==='/projects/4/bindings')return send(bindings);
      if(api==='/projects/4'&&request.method()==='PUT') {
        const patch=JSON.parse(request.postData());
        if('is_public' in patch&&refusePublication)return send({error:'Publication refused'},403);
        project={...project,...patch};return send(project);
      }
      if(api==='/repos/bind') {
        if(refuseBinding)return send({error:'Alias is already bound'},409);
        const alias=JSON.parse(request.postData()).aliases[0];
        bindings=[{...record,identities:[{...record.identities[0],...alias}]}];return send(bindings[0]);
      }
      if(api==='/repos/bindings/7'&&request.method()==='DELETE'){bindings=[];return send({deleted:true});}
      if(api==='/project-archives/LIF') {
        if(refuseArchive)return send({error:'Archive access refused'},403);
        return route.fulfill({contentType:'application/gzip',headers:{'content-disposition':'attachment; filename="LIF.lific.tar.gz"'},body:'archive bytes'});
      }
      if(api==='/project-archives'&&request.method()==='POST') {
        if(refuseImport)return send({error:'Invalid archive checksum'},400);
        return send({project:{id:9,identifier:'COPY',name:'Imported',is_public:false,lead_user_id:1},report:{rows:{issues:2,pages:1},blobs:1,external_reference_count:1,external_references:['OTHER-7']}},201);
      }
      if(api==='/projects/4'&&request.method()==='DELETE') {
        if(refuseDeletion)return send({error:'Deletion refused'},403);
        return send({deleted:true});
      }
      return send([]);
    });
    async function mount(route) {
      await page.goto(`http://accept.test/app/${route}`);
      await page.evaluate(() => {
        localStorage.setItem('lific_token','acceptance-token');
        window.LificTopcoatRouting={href:route=>`/app${route}`};
        window.lificSession={state:{publicProject:null,user:{id:1}},async request(path,options={}) {
          const response=await fetch(`/app/api${path}`,{...options,headers:{'content-type':'application/json',Authorization:'Bearer acceptance-token',...options.headers}});
          const data=await response.json();return response.ok?{ok:true,data}:{ok:false,status:response.status,error:data.error};
        }};
      });
      await page.addScriptTag({content:fs.readFileSync(`${__dirname}/project-settings.js`,'utf8')});
      await page.waitForFunction(()=>window.lificProjectSettings?.controller.state.status==='ready');
    }
    const mutations = method => requests.filter(request=>request.method===method);
    try {
      await mount('LIF/settings');
      await t.test('legacy viewers and nonmembers can read bindings without mutation controls or requests',async()=>{
        project={...project,lead_user_id:2};bindings=[record];
        const before=requests.filter(request=>request.method!=='GET').length;
        for (const membership of ['viewer','maintainer',null]) {
          role={role:membership,enforced:false,is_admin:false};
          await mount('LIF/settings');
          await page.getByText('remote: v1:github.com/acme/app',{exact:true}).waitFor();
          assert.equal(await page.getByRole('button',{name:'Bind repository',exact:true}).count(),0);
          assert.equal(await page.getByRole('button',{name:'Remove binding',exact:true}).count(),0);
          assert.deepEqual(await page.evaluate(async()=>[
            await window.lificProjectSettings.controller.binding('add',{kind:'remote',value:'v1:github.com/acme/app'}),
            await window.lificProjectSettings.controller.binding('remove',{id:7}),
          ]),[false,false]);
        }
        assert.equal(requests.filter(request=>request.method!=='GET').length,before);
        project={...project,lead_user_id:1};role={role:'lead',enforced:true,is_admin:false};bindings=[];
        await mount('LIF/settings');
      });
      await t.test('publication requires acknowledgement, preserves refusal, publishes and unpublishes with exact payloads',async()=>{
        const before=mutations('PUT').length;
        await page.getByRole('button',{name:'Publish project',exact:true}).click();
        assert.equal(mutations('PUT').length,before);
        await page.getByLabel('I understand this makes existing project content public.').check();
        await page.getByRole('button',{name:'Publish project',exact:true}).click();
        await page.getByText('Publication refused',{exact:true}).waitFor();assert.equal(project.is_public,false);
        refusePublication=false;
        await page.getByLabel('I understand this makes existing project content public.').check();
        await page.getByRole('button',{name:'Publish project',exact:true}).click();
        await page.getByRole('button',{name:'Turn off public access',exact:true}).waitFor();assert.equal(project.is_public,true);
        await page.getByRole('button',{name:'Turn off public access',exact:true}).click();
        await page.getByRole('button',{name:'Publish project',exact:true}).waitFor();assert.equal(project.is_public,false);
        assert.deepEqual(mutations('PUT').slice(before).map(request=>JSON.parse(request.body)),[{is_public:true},{is_public:true},{is_public:false}]);
      });
      await t.test('repository bind conflict, successful alias payload, and cancelled or completed removal',async()=>{
        assert.equal(await page.getByRole('option',{name:'Root commit',exact:true}).getAttribute('value'),'root');
        await page.getByText("Run lific bind --json in the checkout and copy an alias's exact kind and value, including the v1: prefix.",{exact:true}).waitFor();
        await page.getByText('Remote example: v1:github.com/acme/app. Root commit example: v1:0123456789abcdef0123456789abcdef01234567.',{exact:true}).waitFor();
        await page.getByLabel('Repository alias',{exact:true}).fill('v1:github.com/acme/app');
        await page.getByRole('button',{name:'Bind repository',exact:true}).click();
        await page.getByText('Alias is already bound',{exact:true}).waitFor();assert.equal(bindings.length,0);
        refuseBinding=false;
        await page.getByRole('button',{name:'Bind repository',exact:true}).click();
        await page.getByText('remote: v1:github.com/acme/app',{exact:true}).waitFor();assert.equal(bindings.length,1);
        assert.deepEqual(JSON.parse(mutations('POST').at(-1).body),{project:'LIF',aliases:[{kind:'remote',value:'v1:github.com/acme/app'}]});
        const before=mutations('DELETE').length;
        page.once('dialog',dialog=>dialog.dismiss());await page.getByRole('button',{name:'Remove binding',exact:true}).click();
        assert.equal(mutations('DELETE').length,before);assert.equal(bindings.length,1);
        page.once('dialog',dialog=>dialog.accept());await page.getByRole('button',{name:'Remove binding',exact:true}).click();
        await page.getByText('No repositories are bound to this project.',{exact:true}).waitFor();assert.equal(bindings.length,0);
        assert.equal(mutations('DELETE').at(-1).path,'/app/api/repos/bindings/7');
        const rootValue='v1:0123456789abcdef0123456789abcdef01234567';
        await page.getByLabel('Repository alias type',{exact:true}).selectOption('root');
        await page.getByLabel('Repository alias',{exact:true}).fill(rootValue);
        await page.getByRole('button',{name:'Bind repository',exact:true}).click();
        await page.getByText(`root: ${rootValue}`,{exact:true}).waitFor();
        assert.deepEqual(JSON.parse(mutations('POST').at(-1).body),{project:'LIF',aliases:[{kind:'root',value:rootValue}]});
      });
      await t.test('archive export requires acknowledgement, refuses access, then downloads authenticated bytes',async()=>{
        const before=requests.filter(request=>request.path==='/app/api/project-archives/LIF').length;
        await page.getByRole('button',{name:'Download project archive',exact:true}).click();
        assert.equal(requests.filter(request=>request.path==='/app/api/project-archives/LIF').length,before);
        await page.getByLabel('I understand this archive includes history and deleted content.').check();
        await page.getByRole('button',{name:'Download project archive',exact:true}).click();
        await page.getByText('Archive access refused',{exact:true}).waitFor();
        refuseArchive=false;
        await page.getByLabel('I understand this archive includes history and deleted content.').check();
        const downloaded=page.waitForEvent('download');
        await page.getByRole('button',{name:'Download project archive',exact:true}).click();
        const download=await downloaded, stream=await download.createReadStream(),chunks=[];
        for await(const chunk of stream)chunks.push(chunk);
        assert.equal(download.suggestedFilename(),'LIF.lific.tar.gz');assert.equal(Buffer.concat(chunks).toString(),'archive bytes');
        assert.equal(requests.filter(request=>request.path==='/app/api/project-archives/LIF').at(-1).authorization,'Bearer acceptance-token');
        assert.ok(requests.some(request=>request.path==='/app/api/auth/me'));
      });
      await t.test('archive multipart import refuses invalid content then completes with private project and report links',async()=>{
        await mount('projects/import');
        await page.getByLabel('Project archive (.tar.gz)',{exact:true}).setInputFiles({name:'project.tar.gz',mimeType:'application/gzip',buffer:Buffer.from('archive content')});
        const before=mutations('POST').length;
        await page.getByRole('button',{name:'Import as private project',exact:true}).click();assert.equal(mutations('POST').length,before);
        await page.getByLabel(/I understand this imports/).check();
        await page.getByRole('button',{name:'Import as private project',exact:true}).click();
        await page.getByText('Invalid archive checksum',{exact:true}).waitFor();
        refuseImport=false;
        await page.getByLabel('Project archive (.tar.gz)',{exact:true}).setInputFiles({name:'project.tar.gz',mimeType:'application/gzip',buffer:Buffer.from('archive content')});
        await page.getByLabel(/I understand this imports/).check();
        await page.getByRole('button',{name:'Import as private project',exact:true}).click();
        await page.getByRole('heading',{name:'COPY imported',exact:true}).waitFor();
        assert.equal(await page.getByRole('link',{name:'Open imported project',exact:true}).getAttribute('href'),'/app/COPY/overview');
        await page.getByText('OTHER-7',{exact:true}).waitFor();await page.getByText('1 files imported.',{exact:true}).waitFor();
        const request=mutations('POST').at(-1);assert.equal(request.path,'/app/api/project-archives');assert.equal(request.authorization,'Bearer acceptance-token');
        assert.match(request.body,/name="archive"; filename="project.tar.gz"/);assert.match(request.body,/archive content/);
        await page.getByRole('button',{name:'Import another archive',exact:true}).click();
        await page.getByRole('button',{name:'Import as private project',exact:true}).waitFor();
      });
      await t.test('project deletion rejects wrong confirmation and server refusal before completed mounted navigation',async()=>{
        await mount('LIF/settings');
        const before=mutations('DELETE').length;
        await page.getByLabel('Type LIF to delete this project',{exact:true}).fill('WRONG');
        await page.getByRole('button',{name:'Delete project',exact:true}).click();
        await page.getByText('Type the project identifier to confirm deletion.',{exact:true}).waitFor();assert.equal(mutations('DELETE').length,before);
        await page.getByLabel('Type LIF to delete this project',{exact:true}).fill('LIF');
        await page.getByRole('button',{name:'Delete project',exact:true}).click();
        await page.getByText('Deletion refused',{exact:true}).waitFor();assert.equal(page.url(),'http://accept.test/app/LIF/settings');
        refuseDeletion=false;
        await page.getByLabel('Type LIF to delete this project',{exact:true}).fill('LIF');
        await page.getByRole('button',{name:'Delete project',exact:true}).click();
        await page.waitForURL('http://accept.test/app/');
        assert.equal(mutations('DELETE').at(-1).path,'/app/api/projects/4');assert.equal(mutations('DELETE').at(-1).body,null);
      });
      assert.deepEqual(errors,[]);
    } finally {await browser.close();}
  });
