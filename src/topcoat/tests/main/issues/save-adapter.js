const {product, expose, bounded} = require('./harness');
function createSaveQueue({send, onStateChange}) {
  const identity = product('identity/assets/identity.js', 'LificTopcoatIdentity', source => {
    source = expose(source, 'return {state, loadInstance,', 'const queueObservation = {get busy(){return !!settingsSave;},get pending(){return Object.keys(settingsQueue).length ? settingsQueue : null;}};\n');
    source = source.replace('return {state, loadInstance,', 'return {queueObservation,state,loadInstance,');
    source = source.replace('settingsSave = (async () => {', "env.observeQueue?.('sending'); settingsSave = (async () => {");
    return source.replace('finally {settingsSave=null;}', "finally {settingsSave=null; env.observeQueue?.('idle');}");
  });
  const app = identity.controller({observeQueue:onStateChange, session:{
    async request(route, options) {
      if (route !== '/instance/settings') throw new Error(`Unexpected save route ${route}`);
      const patch = JSON.parse(options.body);
      const ok = await send(patch);
      return ok ? {ok:true,data:{...app.state.settings,...patch}} : {ok:false,status:500,error:'Save refused'};
    },
  }});
  app.state.settings = {};
  return {push(patch){const result=bounded(app.saveSettings(patch).then(result=>result.ok), 'settings save'); result.catch(()=>{}); return result;},
    get busy(){return app.queueObservation.busy;}, get pending(){return app.queueObservation.pending;}};
}
module.exports = {createSaveQueue};
