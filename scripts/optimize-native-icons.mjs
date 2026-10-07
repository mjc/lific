import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const sourcePath = join(root, 'src/topcoat/native/assets/project-icons.json');
const outputPath = join(root, 'src/topcoat/native/assets/project-icons.inline.json');
const icons = JSON.parse(readFileSync(sourcePath, 'utf8'));
const temp = mkdtempSync(join(tmpdir(), 'lific-native-icons-'));
const catalogPath = join(temp, 'catalog.svg');
const optimizedPath = join(temp, 'optimized.svg');
const bodyPath = join(temp, 'bodies.json');

try {
  const groups = Object.entries(icons).map(([name, nodes]) => {
    const children = nodes.map(([tag, attributes]) => {
      const attrs = Object.entries(attributes)
        .map(([key, value]) => ` ${key}="${value.replaceAll('&', '&amp;').replaceAll('"', '&quot;')}"`)
        .join('');
      return `<${tag}${attrs}/>`;
    }).join('');
    return `<svg data-icon="${name}">${children}</svg>`;
  }).join('');
  writeFileSync(catalogPath,
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${groups}</svg>`);

  const result = spawnSync('nix', [
    'run', 'nixpkgs#svgo', '--', '--input', catalogPath, '--output', optimizedPath,
    '--config', join(root, 'src/topcoat/native/assets/icons.svgo.config.mjs'),
    '--precision', '6', '--quiet',
  ], { cwd: root, encoding: 'utf8', env: { ...process.env, NATIVE_ICONS_OUTPUT: bodyPath } });
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || `SVGO exited ${result.status}`);
  }
  const bodies = JSON.parse(readFileSync(bodyPath, 'utf8'));
  if (Object.keys(bodies).length !== Object.keys(icons).length) {
    throw new Error(`SVGO returned ${Object.keys(bodies).length} icons; expected ${Object.keys(icons).length}`);
  }
  writeFileSync(outputPath, `${JSON.stringify(bodies)}\n`);
} finally {
  rmSync(temp, { recursive: true, force: true });
}
