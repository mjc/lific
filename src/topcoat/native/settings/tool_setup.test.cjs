'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const url = input.mounted_url;
const key = 'lific_sk-live-test-key';
const markerUrl = '__LIFIC_MCP_URL__';
const markerKey = '__LIFIC_API_KEY__';
const templates = Object.fromEntries(input.templates.map(template => [template.id, template]));
const generic = input.generic;
const materialize = template => template.config_template
  .replaceAll(markerUrl, url)
  .replaceAll(markerKey, key);
const notesFor = template => template.notes.map(step => step.command
  ? {command: step.command.replaceAll(markerUrl, url).replaceAll(markerKey, key)}
  : step);
const bearer = {Authorization: `Bearer ${key}`};
const jsonConfig = template => JSON.parse(materialize(template));

assert.deepEqual(Object.keys(templates).sort(), [
  'claude', 'claude-code', 'codex', 'cursor', 'opencode', 'pi', 'vscode', 'zed',
]);

const expectedPaths = {
  opencode: {
    linux: '~/.config/opencode/opencode.json',
    mac: '~/.config/opencode/opencode.json',
    windows: '%USERPROFILE%\\.config\\opencode\\opencode.json',
  },
  cursor: {
    linux: '~/.cursor/mcp.json (global) · .cursor/mcp.json (project)',
    mac: '~/.cursor/mcp.json (global) · .cursor/mcp.json (project)',
    windows: '%USERPROFILE%\\.cursor\\mcp.json (global) · .cursor\\mcp.json (project)',
  },
  'claude-code': {
    linux: '~/.claude.json (user scope)',
    mac: '~/.claude.json (user scope)',
    windows: '%USERPROFILE%\\.claude.json (user scope)',
  },
  claude: {
    linux: null,
    mac: '~/Library/Application Support/Claude/claude_desktop_config.json',
    windows: '%APPDATA%\\Claude\\claude_desktop_config.json',
  },
  codex: {
    linux: '~/.codex/config.toml',
    mac: '~/.codex/config.toml',
    windows: '%USERPROFILE%\\.codex\\config.toml',
  },
  pi: {
    linux: '~/.pi/agent/mcp.json',
    mac: '~/.pi/agent/mcp.json',
    windows: '%USERPROFILE%\\.pi\\agent\\mcp.json',
  },
  vscode: {
    linux: '~/.config/Code/User/mcp.json (user) · .vscode/mcp.json (workspace)',
    mac: '~/.config/Code/User/mcp.json (user) · .vscode/mcp.json (workspace)',
    windows: '%APPDATA%\\Code\\User\\mcp.json (user) · .vscode\\mcp.json (workspace)',
  },
  zed: {
    linux: '~/.config/zed/settings.json',
    mac: '~/.config/zed/settings.json',
    windows: '%APPDATA%\\Zed\\settings.json',
  },
};
for (const [id, paths] of Object.entries(expectedPaths)) {
  assert.deepEqual(templates[id].paths, paths, `${id} config paths`);
}

assert.deepEqual(jsonConfig(templates.opencode), {
  lific: {type: 'remote', url, headers: bearer},
});
assert.deepEqual(jsonConfig(templates.cursor), {
  lific: {url, headers: bearer},
});
assert.deepEqual(jsonConfig(templates['claude-code']), {
  lific: {type: 'http', url, headers: bearer},
});
assert.deepEqual(jsonConfig(templates.claude), {
  lific: {
    command: 'npx',
    args: ['-y', 'mcp-remote', url],
    env: {AUTHORIZATION: `Bearer ${key}`},
  },
});
assert.equal(materialize(templates.codex),
  '[mcp_servers.lific]\ntransport.type = "http"\n' +
  `transport.url = "${url}"\n` +
  'transport.bearer_token_env_var = "LIFIC_API_KEY"');
assert.deepEqual(jsonConfig(templates.pi), {
  lific: {url, auth: 'bearer', bearerTokenEnv: 'LIFIC_API_KEY', lifecycle: 'keep-alive'},
});
assert.deepEqual(jsonConfig(templates.vscode), {
  servers: {lific: {type: 'http', url, headers: bearer}},
});
assert.deepEqual(jsonConfig(templates.zed), {
  context_servers: {lific: {url, headers: bearer}},
});

assert.deepEqual(notesFor(templates.opencode), [
  {text: 'Add this to the "mcp" section of your config.'},
]);
assert.deepEqual(notesFor(templates.cursor), [
  {text: 'Add this to the "mcpServers" section, then reload Cursor.'},
]);
assert.deepEqual(notesFor(templates['claude-code']), [
  {text: 'Easiest: run this command (it writes the config for you):'},
  {command: 'claude mcp add --transport http --scope user lific ' +
    `${url} --header "Authorization: Bearer <key>"`},
  {text: 'Or add the block below to the "mcpServers" section manually.'},
]);
assert.deepEqual(notesFor(templates.claude), [
  {text: 'Requires mcp-remote (installed automatically by npx).'},
  {text: 'Add the block below to the "mcpServers" section, then fully restart Claude Desktop.'},
]);
assert.deepEqual(notesFor(templates.pi), [
  {text: 'First install the adapter, then restart Pi:'},
  {command: 'pi install npm:pi-mcp-adapter'},
  {text: 'Add the block below to the "mcpServers" section. The key is read from the LIFIC_API_KEY env var (set it in step 3).'},
]);
assert.deepEqual(notesFor(templates.vscode), [
  {text: 'Add this to the "servers" section. Or run the command palette action:'},
  {command: 'MCP: Open User Configuration'},
  {text: 'VS Code 1.101+ with GitHub Copilot is required.'},
]);
assert.deepEqual(notesFor(templates.zed), [
  {text: 'Add this to the "context_servers" section of your Zed settings (Command Palette: zed: open settings).'},
]);
assert.deepEqual(notesFor(templates.codex), [
  {text: 'Add the block below under [mcp_servers] in config.toml.'},
  {text: 'The key is read from the LIFIC_API_KEY env var (set it in step 3).'},
]);

for (const id of ['codex', 'pi']) {
  assert.equal(templates[id].env_var, 'LIFIC_API_KEY', `${id} uses Main's environment variable`);
  assert.deepEqual(templates[id].exports, {
    linux: `export LIFIC_API_KEY=\"${markerKey}\"`,
    mac: `export LIFIC_API_KEY=\"${markerKey}\"`,
    windows: `setx LIFIC_API_KEY \"${markerKey}\"`,
  });
}
assert.equal(templates.opencode.env_var, null);
assert.deepEqual(input.persistence_hints, {
  linux: 'Runs in the current shell. To persist it, add the line to ~/.bashrc or ~/.profile.',
  mac: 'Runs in the current shell. To persist it, add the line to ~/.zshrc (the default macOS shell).',
  windows: 'setx applies to new terminals, not the current one. Reopen your terminal (or Pi) after running it.',
});
for (const id of ['opencode', 'cursor', 'claude-code', 'codex', 'pi', 'vscode', 'zed']) {
  assert.deepEqual(templates[id].groups, [
    {key: 'linux', label: 'Linux / macOS', oses: ['linux', 'mac']},
    {key: 'windows', label: 'Windows', oses: ['windows']},
  ], `${id} groups identical paths in Main's order`);
}
assert.deepEqual(templates.claude.groups, [
  {key: 'mac', label: 'macOS', oses: ['mac']},
  {key: 'windows', label: 'Windows', oses: ['windows']},
]);
assert.deepEqual(generic.groups, []);
assert.equal(generic.id, 'custom');
assert.deepEqual(generic.paths, {linux: null, mac: null, windows: null});
assert.deepEqual(generic.exports, {linux: null, mac: null, windows: null});
assert.deepEqual(JSON.parse(generic.config_template
  .replaceAll(markerUrl, url).replaceAll(markerKey, key)), {
  url,
  headers: bearer,
});
assert.deepEqual(notesFor(generic), [{
  text: "Use this URL and Authorization header in your client's HTTP MCP settings. " +
    'The exact config format depends on your client.',
}]);
assert.equal(generic.env_var, null);
process.stdout.write(JSON.stringify({setup_parity: true}));
