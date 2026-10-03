const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

test('headless identity screens preserve auth policy, credential errors, and admin visibility',
  {skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH}, async t => {
    const {chromium}=await import(path.resolve(__dirname,'../../../..','e2e/node_modules/playwright/index.mjs'));
    const browser=await chromium.launch({headless:true,executablePath:process.env.PLAYWRIGHT_EXECUTABLE_PATH});
    const page=await browser.newPage(); page.setDefaultTimeout(5000);
    const errors=[]; page.on('pageerror',error=>errors.push(error.stack||error.message));
    const script=fs.readFileSync(`${__dirname}/identity.js`,'utf8');
    async function mount(mode,user=null,instance={allow_signup:true,has_users:true,instance_name:'Lific',login_message:''},basePath='') {
      await page.route('http://identity.test/**',route=>route.fulfill({contentType:'text/html',body:`<!doctype html><html><body>
        <section class="tc-identity" data-topcoat-identity="${mode}" aria-busy="true"><p data-identity-status role="status">Loading</p><div data-identity-content></div></section></body></html>`}));
      await page.goto(`http://identity.test${basePath}/`);
      await page.evaluate(({user,instance,basePath})=>{
        window.LificTopcoatRouting={href:route=>`${basePath}${route}`};
        window.calls=[];window.navigated=[];
        window.lificSession={state:{user,loading:false},request:async(path,options={})=>{
          calls.push({path,options});
          if(path==='/instance')return {ok:true,data:instance};
          if(path==='/auth/me')return user?{ok:true,data:user}:{ok:false,status:401,error:'not signed in'};
          if(path==='/auth/keys'||path==='/auth/bots'||path==='/users')return {ok:true,data:[]};
          if(path==='/instance/settings')return {ok:true,data:{instance_name:'Lific',allow_signup:true,signup_email_domains:[],session_lifetime_days:30,login_message:'',web_auto_login:false,authz_enforced:true}};
          if(path==='/auth/login')return {ok:false,status:400,error:'Invalid username or password'};
          return {ok:false,status:403,error:'Unexpected request'};
        },refreshAccount:async()=>user?{ok:true,data:user}:{ok:false,status:401,error:'not signed in'},
          saveSession(){},clearSession(){},logout:async()=>({ok:true})};
      },{user,instance,basePath});
      await page.addScriptTag({content:script});
      await page.waitForFunction(()=>document.querySelector('[data-identity-content]')?.children.length>0);
    }
    try {
      await t.test('login exposes recoverable credential failures and password visibility control',async()=>{
        await mount('login');
        await page.getByLabel('Username or email').fill('alex');
        await page.locator('input[name="password"]').fill('bad-password');
        await page.getByRole('button',{name:'Log in'}).click();
        await page.getByRole('alert').waitFor();
        assert.match(await page.getByRole('alert').textContent(),/Invalid username or password/);
        await page.getByRole('button',{name:'Show password'}).click();
        assert.equal(await page.locator('input[name="password"]').getAttribute('type'),'text');
      });
      await t.test('first account setup remains available when public signup is closed',async()=>{
        await mount('signup',null,{allow_signup:false,has_users:false,instance_name:'New Lific',login_message:''});
        assert.equal(await page.getByRole('heading',{name:'Set up your instance'}).count(),1);
        assert.equal(await page.getByLabel('Username').count(),1);
      });
      await t.test('ordinary accounts get no instance administration requests or controls',async()=>{
        await mount('instance',{id:7,username:'reader',is_admin:false});
        assert.equal(await page.getByRole('alert').textContent(),'Administrator access is required.');
        assert.equal(await page.evaluate(()=>calls.some(call=>call.path==='/users'||call.path==='/instance/settings')),false);
        assert.equal(await page.getByRole('button',{name:'Create account'}).count(),0);
      });
      await t.test('tool creation after reauthentication shows the returned secret and client setup',async()=>{
        await mount('settings',{id:1,username:'admin',display_name:'Admin',email:'admin@example.com',is_admin:false});
        await page.evaluate(()=>{
          const previous=lificSession.request;let first=true;
          lificSession.request=async(path,options={})=>{
            if(path==='/auth/bots'&&options.method==='POST'&&first){first=false;return {ok:false,status:403,code:'recent_auth_required',error:'Recent authentication required'};}
            if(path==='/auth/bots'&&options.method==='POST')return {ok:true,data:{key:'one-time-secret',tool:'codex'}};
            if(path==='/auth/me/refresh')return {ok:true,data:{token:'refreshed'}};
            return previous(path,options);
          };
        });
        await page.getByLabel('Client template').selectOption('codex');
        await page.getByRole('button',{name:'Connect tool'}).click();
        await page.getByRole('heading',{name:'Confirm your identity'}).waitFor();
        await page.getByLabel('Password',{exact:true}).fill('correct horse');
        await page.getByRole('button',{name:'Confirm and continue'}).click();
        await page.getByText('one-time-secret',{exact:true}).waitFor();
        assert.match(await page.locator('[data-client-config]').textContent(),/bearer_token_env_var = "LIFIC_API_KEY"/);
        assert.match(await page.locator('[data-secret]').textContent(),/export LIFIC_API_KEY=/);
        assert.equal(await page.locator('[data-secret]').isVisible(),true);
      });
      await t.test('Cursor, Claude Code, and Pi connections expose client setup and copyable commands',async()=>{
        for(const tool of ['cursor','claude-code','pi']) {
          await mount('settings',{id:1,username:'admin',display_name:'Admin',email:'admin@example.com',is_admin:false});
          await page.evaluate(()=>{
            const previous=lificSession.request;window.connectedTool=null;window.copied=[];
            Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async text=>copied.push(text)}});
            lificSession.request=async(path,options={})=>{
              if(path==='/auth/bots'&&options.method==='POST'){
                connectedTool=JSON.parse(options.body).tool;
                return {ok:true,data:{key:'client-secret'}};
              }
              return previous(path,options);
            };
          });
          await page.getByLabel('Client template').selectOption(tool);
          await page.getByRole('button',{name:'Connect tool'}).click();
          await page.getByText('client-secret',{exact:true}).waitFor();
          assert.equal(await page.evaluate(()=>connectedTool),tool);
          const config=JSON.parse(await page.locator('[data-client-config]').textContent());
          assert.equal(config.lific.url,'http://identity.test/mcp');
          if(tool==='pi'){
            assert.equal(config.lific.bearerTokenEnv,'LIFIC_API_KEY');
            assert.equal(config.lific.lifecycle,'keep-alive');
            assert.equal(config.lific.headers,undefined);
            assert.match(await page.locator('[data-secret]').textContent(),/restart Pi/);
            await page.getByRole('button',{name:'Copy environment command'}).click();
            assert.match(await page.evaluate(()=>copied.at(-1)),/LIFIC_API_KEY.*client-secret/);
          } else {
            assert.equal(config.lific.headers.Authorization,'Bearer client-secret');
            assert.equal(config.lific.type,tool==='claude-code'?'http':undefined);
          }
          if(tool!=='cursor'){
            const command=await page.locator('[data-setup-command]').textContent();
            assert.match(command,tool==='pi'?/pi install npm:pi-mcp-adapter/:/claude mcp add --transport http --scope user/);
            await page.getByRole('button',{name:'Copy setup command'}).click();
            assert.equal(await page.evaluate(()=>copied.at(-1)),command);
          }
          await page.getByRole('button',{name:'Copy client configuration'}).click();
          assert.deepEqual(JSON.parse(await page.evaluate(()=>copied.at(-1))),config);
        }
      });
      await t.test('prototype-named custom connections keep one-time secrets available',async()=>{
        for(const tool of ['constructor','toString','__proto__']) {
          await mount('settings',{id:1,username:'member',display_name:'Member',is_admin:false});
          await page.evaluate(()=>{
            const previous=lificSession.request;
            lificSession.request=async(path,options={})=>path==='/auth/bots'&&options.method==='POST'
              ?{ok:true,data:{key:'custom-one-time-secret'}}:previous(path,options);
          });
          await page.getByLabel('Client template').selectOption('custom');
          await page.getByLabel('Custom tool ID').fill(tool);
          await page.getByRole('button',{name:'Connect tool'}).click();
          await page.getByText('custom-one-time-secret',{exact:true}).waitFor();
          assert.match(await page.locator('[data-secret]').textContent(),/Custom MCP client/);
          assert.deepEqual(JSON.parse(await page.locator('[data-client-config]').textContent()),{
            url:'http://identity.test/mcp',headers:{Authorization:'Bearer custom-one-time-secret'},
          });
        }
      });
      await t.test('secret copy controls fall back when clipboard access is unavailable or rejected and report failure safely',async()=>{
        await mount('settings',{id:1,username:'member',display_name:'Member',is_admin:false});
        await page.evaluate(()=>{
          const root=document.querySelector('[data-identity-content]'),app=document.querySelector('[data-topcoat-identity]')._app;
          app.state.secret={kind:'Connected tool key',value:'private-copy-secret',toolId:'pi',...LificTopcoatIdentity.toolSetup('pi',location.origin,'private-copy-secret')};
          LificTopcoatIdentity.showSecret(root,app);
          window.fallbackCopies=[];
          Object.defineProperty(navigator,'clipboard',{configurable:true,value:undefined});
          document.execCommand=command=>{
            if(command==='copy')fallbackCopies.push(document.activeElement.value);
            return command==='copy';
          };
        });
        await page.getByRole('button',{name:'Copy key',exact:true}).click();
        await page.waitForFunction(()=>fallbackCopies.length===1);
        assert.equal(await page.evaluate(()=>fallbackCopies[0]),'private-copy-secret');
        assert.equal(await page.locator('[data-copy-secret]').textContent(),'Copied');
        assert.equal(await page.locator('textarea').count(),0);
        await page.evaluate(()=>{
          Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async()=>{throw new Error('denied private-copy-secret');}}});
        });
        await page.getByRole('button',{name:'Copy client configuration'}).click();
        await page.getByRole('button',{name:'Copy setup command'}).click();
        await page.getByRole('button',{name:'Copy environment command'}).click();
        assert.equal(await page.evaluate(()=>fallbackCopies.length),4);
        assert.equal(JSON.parse(await page.evaluate(()=>fallbackCopies[1])).lific.bearerTokenEnv,'LIFIC_API_KEY');
        assert.equal(await page.evaluate(()=>fallbackCopies[2]),'pi install npm:pi-mcp-adapter');
        assert.match(await page.evaluate(()=>fallbackCopies[3]),/LIFIC_API_KEY.*private-copy-secret/);
        await page.evaluate(()=>{document.execCommand=()=>{throw new Error('blocked private-copy-secret');};});
        await page.locator('[data-copy-secret]').click();
        await page.locator('[data-copy-result][role=alert]').waitFor();
        assert.equal(await page.locator('[data-copy-result]').textContent(),"Couldn't copy to clipboard. Select the text and copy it manually.");
        assert.equal(await page.locator('[data-copy-secret]').textContent(),'Copy key');
        assert.equal(await page.getByText('private-copy-secret',{exact:true}).count(),1);
        assert.equal(await page.locator('textarea').count(),0);
      });
      await t.test('passwordless settings reauthentication shows no password prompt and omits the refresh body',async()=>{
        await mount('instance',{id:1,username:'admin',display_name:'Admin',email:'admin@example.com',is_admin:true},
          {allow_signup:true,has_users:true,instance_name:'Lific',login_message:'',web_auto_login:true});
        await page.evaluate(()=>{
          const previous=lificSession.request;let first=true;window.refreshBody='not-called';
          lificSession.request=async(path,options={})=>{
            if(path==='/instance/settings'&&options.method==='PATCH'&&first){first=false;return {ok:false,status:403,code:'recent_auth_required',error:'Recent authentication required'};}
            if(path==='/instance/settings'&&options.method==='PATCH')return {ok:true,data:{instance_name:'Lific',allow_signup:false,signup_email_domains:[],session_lifetime_days:30,login_message:'',web_auto_login:false,authz_enforced:true}};
            if(path==='/auth/me/refresh'){refreshBody=options.body;return {ok:true,data:{token:'passwordless-session'}};}
            return previous(path,options);
          };
        });
        await page.getByLabel('Allow sign up').uncheck();
        await page.getByRole('heading',{name:'Confirm your identity'}).waitFor();
        assert.equal(await page.locator('[data-reauth] input[type=password]').count(),0);
        assert.match(await page.locator('.tc-identity__reauth').textContent(),/passwordless session/);
        await page.getByRole('button',{name:'Confirm and continue'}).click();
        await page.waitForFunction(()=>refreshBody===undefined);
        assert.equal(await page.getByRole('alert').count(),0);
      });
      await t.test('account creation resets the captured form and refreshes the roster',async()=>{
        await mount('instance',{id:1,username:'admin',display_name:'Admin',email:'admin@example.com',is_admin:true});
        await page.evaluate(()=>{
          const previous=lificSession.request;window.createdUsers=[];
          lificSession.request=async(path,options={})=>{
            if(path==='/users'&&options.method==='POST'){
              const input=JSON.parse(options.body),user={id:9,username:input.username,display_name:input.username,is_admin:false,is_active:true};
              createdUsers.push(user);return {ok:true,data:user};
            }
            if(path==='/users'&&options.method!=='POST')return {ok:true,data:createdUsers};
            return previous(path,options);
          };
        });
        const username=page.getByLabel('Username');
        await username.fill('new-member');
        await page.getByLabel('Initial password').fill('long-password');
        await page.getByRole('button',{name:'Create account'}).click();
        await page.getByText('@new-member · Member · Active').waitFor();
        assert.equal(await page.getByLabel('Username').inputValue(),'');
        assert.equal(await page.getByLabel('Initial password').inputValue(),'');
      });
      await t.test('profile changes save trimmed values and preserve the form after a refused update',async()=>{
        await mount('settings',{id:1,username:'member',display_name:'Member',email:'old@example.com',is_admin:false});
        await page.evaluate(()=>{
          const previous=lificSession.request;window.profileWrites=[];window.rejectProfile=true;
          lificSession.request=async(path,options={})=>{
            if(path==='/auth/me'&&options.method==='PATCH'){
              const patch=JSON.parse(options.body);profileWrites.push(patch);
              return rejectProfile?{ok:false,status:400,error:'Email is already in use'}:{ok:true,data:{id:1,username:'member',...patch}};
            }
            return previous(path,options);
          };
        });
        await page.getByLabel('Display name',{exact:true}).fill('  Changed Member  ');
        await page.getByLabel('Email',{exact:true}).fill('new@example.com');
        await page.getByRole('button',{name:'Save profile'}).click();
        await page.getByText('Email is already in use',{exact:true}).waitFor();
        assert.equal(await page.getByLabel('Display name',{exact:true}).inputValue(),'  Changed Member  ');
        await page.evaluate(()=>window.rejectProfile=false);
        await page.getByRole('button',{name:'Save profile'}).click();
        await page.getByText('Profile saved.',{exact:true}).waitFor();
        assert.deepEqual(await page.evaluate(()=>profileWrites.at(-1)),{display_name:'Changed Member',email:'new@example.com'});
        assert.equal(await page.evaluate(()=>document.querySelector('[data-topcoat-identity]')._app.state.user.display_name),'Changed Member');
      });
      await t.test('connected tool lifecycle confirms disconnect and delete and reconnects the original custom identity',async()=>{
        await mount('settings',{id:1,username:'member',display_name:'Member',is_admin:false});
        await page.evaluate(()=>{
          const previous=lificSession.request;window.lifecycle=[];window.confirmation=true;window.confirmMessages=[];
          window.confirm=message=>{confirmMessages.push(message);return confirmation;};
          window.bots=[{id:8,username:'obsolete-owner-name',tool_id:'custom-laptop',display_name:'Laptop',connected:true}];
          lificSession.request=async(path,options={})=>{
            if(path==='/auth/bots'&&options.method!=='POST')return {ok:true,data:bots};
            if(path==='/auth/bots/8/disconnect'){lifecycle.push(['disconnect']);bots[0].connected=false;return {ok:true,data:{disconnected:true}};}
            if(path==='/auth/bots'&&options.method==='POST'){
              lifecycle.push(['reconnect',JSON.parse(options.body)]);bots[0].connected=true;return {ok:true,data:{key:'replacement-tool-secret'}};
            }
            if(path==='/auth/bots/8'&&options.method==='DELETE'){lifecycle.push(['delete']);bots=[];return {ok:true,data:{deleted:true}};}
            return previous(path,options);
          };
          const app=document.querySelector('[data-topcoat-identity]')._app;app.state.bots=bots;LificTopcoatIdentity.renderSettings(document.querySelector('[data-identity-content]'),app);
        });
        await page.evaluate(()=>window.confirmation=false);
        await page.getByRole('button',{name:'Disconnect',exact:true}).click();
        assert.deepEqual(await page.evaluate(()=>lifecycle),[]);
        await page.evaluate(()=>window.confirmation=true);
        await page.getByRole('button',{name:'Disconnect',exact:true}).click();
        await page.getByText('Laptop · Disconnected',{exact:false}).waitFor();
        await page.getByRole('button',{name:'Reconnect',exact:true}).click();
        await page.getByText('replacement-tool-secret',{exact:true}).waitFor();
        assert.equal(await page.getByRole('button',{name:'Disconnect',exact:true}).count(),1);
        assert.deepEqual(await page.evaluate(()=>lifecycle[1]),['reconnect',{tool:'custom-laptop',display_name:'Laptop'}]);
        await page.getByRole('button',{name:'Delete',exact:true}).click();
        await page.waitForFunction(()=>!document.querySelector('[data-bot-action]'));
        assert.deepEqual(await page.evaluate(()=>lifecycle.map(action=>action[0])),['disconnect','reconnect','delete']);
        assert.match(await page.evaluate(()=>confirmMessages.at(-1)),/Delete/);
      });
      await t.test('canceling recent authentication restores stored settings and discards the coalesced patch',async()=>{
        await mount('instance',{id:1,username:'admin',is_admin:true});
        await page.evaluate(()=>{
          const previous=lificSession.request;window.settingsWrites=[];
          lificSession.request=async(path,options={})=>{
            if(path==='/instance/settings'&&options.method==='PATCH'){settingsWrites.push(JSON.parse(options.body));return {ok:false,status:403,code:'recent_auth_required',error:'Recent authentication required'};}
            return previous(path,options);
          };
        });
        await page.getByLabel('Allow sign up').uncheck();
        await page.getByRole('heading',{name:'Confirm your identity'}).waitFor();
        assert.equal(await page.getByLabel('Allow sign up').isChecked(),false);
        await page.getByLabel('Instance name').fill('Pending name');await page.getByLabel('Instance name').blur();
        await page.getByRole('button',{name:'Cancel',exact:true}).click();
        await page.waitForFunction(()=>document.querySelector('[data-setting=allow_signup]').checked);
        assert.equal(await page.getByLabel('Instance name').inputValue(),'Lific');
        assert.equal(await page.getByRole('heading',{name:'Confirm your identity'}).count(),0);
        assert.deepEqual(await page.evaluate(()=>document.querySelector('[data-topcoat-identity]')._app.state.pendingPatch),{});
        assert.equal(await page.evaluate(()=>settingsWrites.length),1);
      });
      await t.test('admin lifecycle requires confirmation and recent authentication before promotion then demotion and activation changes',async()=>{
        await mount('instance',{id:1,username:'admin',is_admin:true});
        await page.evaluate(()=>{
          const previous=lificSession.request;window.lifecycle=[];window.confirmation=false;window.requireRecent=true;
          window.confirm=()=>confirmation;window.managedUser={id:7,username:'reader',display_name:'Reader',is_admin:false,is_active:true};
          lificSession.request=async(path,options={})=>{
            if(path==='/users'&&options.method!=='POST')return {ok:true,data:[managedUser]};
            if(path==='/auth/me/refresh')return {ok:true,data:{token:'recent-session'}};
            if(path.startsWith('/users/7/')){
              const action=path.split('/').at(-1);lifecycle.push(action);
              if(requireRecent){requireRecent=false;return {ok:false,status:403,code:'recent_auth_required',error:'Recent authentication required'};}
              if(action==='promote'||action==='demote')managedUser.is_admin=action==='promote';
              else managedUser.is_active=action==='reactivate';
              return {ok:true,data:managedUser};
            }
            return previous(path,options);
          };
          const app=document.querySelector('[data-topcoat-identity]')._app;app.state.users=[managedUser];LificTopcoatIdentity.renderInstance(document.querySelector('[data-identity-content]'),app);
        });
        await page.getByRole('button',{name:'Make admin',exact:true}).click();
        assert.deepEqual(await page.evaluate(()=>lifecycle),[]);
        await page.evaluate(()=>window.confirmation=true);
        await page.getByRole('button',{name:'Make admin',exact:true}).click();
        await page.getByRole('heading',{name:'Confirm your identity'}).waitFor();
        await page.getByRole('button',{name:'Cancel',exact:true}).click();
        await page.getByRole('button',{name:'Make admin',exact:true}).waitFor();
        assert.deepEqual(await page.evaluate(()=>lifecycle),['promote']);
        await page.evaluate(()=>window.requireRecent=true);
        await page.getByRole('button',{name:'Make admin',exact:true}).click();
        await page.getByLabel('Password',{exact:true}).fill('current password');
        await page.getByRole('button',{name:'Confirm and continue'}).click();
        await page.getByRole('button',{name:'Remove admin',exact:true}).waitFor();
        await page.getByRole('button',{name:'Remove admin',exact:true}).click();
        await page.getByRole('button',{name:'Make admin',exact:true}).waitFor();
        await page.getByRole('button',{name:'Deactivate',exact:true}).click();
        await page.getByRole('button',{name:'Reactivate',exact:true}).waitFor();
        await page.getByRole('button',{name:'Reactivate',exact:true}).click();
        await page.getByRole('button',{name:'Deactivate',exact:true}).waitFor();
        assert.deepEqual(await page.evaluate(()=>lifecycle),['promote','promote','promote','demote','deactivate','reactivate']);
      });
      await t.test('prefixed identity links and tool client endpoints stay under the deployment mount',async()=>{
        await mount('login',null,{allow_signup:true,has_users:true,instance_name:'Lific',login_message:''},'/app');
        assert.equal(await page.getByRole('link',{name:'Create an account',exact:true}).getAttribute('href'),'/app/signup');
        await mount('settings',{id:1,username:'admin',is_admin:true},{allow_signup:true,has_users:true},'/app');
        assert.equal(await page.getByRole('link',{name:'Instance settings',exact:true}).getAttribute('href'),'/app/settings/instance');
        await page.evaluate(()=>{const previous=lificSession.request;lificSession.request=async(path,options={})=>path==='/auth/bots'&&options.method==='POST'?{ok:true,data:{key:'prefixed-secret'}}:previous(path,options);});
        await page.getByLabel('Client template').selectOption('cursor');await page.getByRole('button',{name:'Connect tool'}).click();
        await page.getByText('prefixed-secret',{exact:true}).waitFor();
        assert.equal(JSON.parse(await page.locator('[data-client-config]').textContent()).lific.url,'http://identity.test/app/mcp');
      });
      await t.test('password rotation refreshes revoked credentials and removes displayed secrets',async()=>{
        await mount('settings',{id:1,username:'admin',display_name:'Admin',email:'admin@example.com',is_admin:true});
        await page.evaluate(async()=>{
          const previous=lificSession.request;window.rotated=false;
          lificSession.request=async(path,options={})=>{
            if(path==='/auth/me/password'){rotated=true;return {ok:true,data:{token:'replacement'}};}
            if(path==='/auth/keys')return {ok:true,data:[{id:3,name:'build-key',created_at:'today',revoked:rotated}]};
            if(path==='/auth/bots')return {ok:true,data:[{id:8,username:'codex-bot',display_name:'Codex',connected:!rotated}]};
            return previous(path,options);
          };
          await lificSession.request('/auth/keys');
          const root=document.querySelector('[data-identity-content]');
          root._app=document.querySelector('[data-topcoat-identity]')._app;
          root._app.state.keys=[{id:3,name:'build-key',created_at:'today',revoked:false}];
          root._app.state.bots=[{id:8,username:'codex-bot',display_name:'Codex',connected:true}];
          root._app.state.secret={kind:'Connected tool key',value:'old-one-time-secret',name:'Codex',toolId:'codex',path:'~/.codex/config.toml',instructions:'setup',config:'config'};
          LificTopcoatIdentity.renderSettings(root,root._app);LificTopcoatIdentity.showSecret(root,root._app);
        });
        await page.getByLabel('Current password').fill('old-password');
        await page.getByLabel('New password').fill('replacement-password');
        await page.getByRole('button',{name:'Change password'}).click();
        await page.getByText('Password changed. Other sessions and connected tools were revoked.').waitFor();
        assert.equal(await page.getByText('old-one-time-secret',{exact:true}).count(),0);
        assert.equal(await page.getByText('build-key · today · Revoked').count(),1);
        assert.equal(await page.getByText('Codex · Disconnected').count(),1);
      });
      assert.deepEqual(errors,[]);
    } finally {await browser.close();}
  });
