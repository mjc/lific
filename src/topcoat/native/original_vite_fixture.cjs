// Pinned reference Vite startup and shutdown; application source is unchanged.
async function prepareOriginalVite(vite) {
  const clients = new Set();
  vite.httpServer.on('connection', socket => {
    clients.add(socket);
    socket.once('close', () => clients.delete(socket));
  });
  vite.lificClientSockets = clients;
  const client = vite.environments.client;
  const entry = await client.transformRequest('/src/main.ts');
  if (!entry) throw new Error('Pinned original main entry did not transform.');
  await client.waitForRequestsIdle();
  await client.depsOptimizer?.scanProcessing;
  // Warmup logs dependent transform errors instead of throwing. Re-read the
  // actual discovered modules through transformRequest so setup propagates them.
  await Promise.all(Array.from(client.moduleGraph.urlToModuleMap.keys(), url =>
    client.transformRequest(url)));
  const metadata = client.depsOptimizer?.metadata;
  if (metadata) await Promise.all(Object.values({...metadata.optimized, ...metadata.discovered})
    .map(dependency => dependency.processing).filter(Boolean));
}

async function closeOriginalVite(vite) {
  const closing = vite.close();
  for (const socket of vite.lificClientSockets ?? []) socket.destroy();
  vite.httpServer?.closeAllConnections();
  await closing;
}

module.exports = {prepareOriginalVite, closeOriginalVite};
