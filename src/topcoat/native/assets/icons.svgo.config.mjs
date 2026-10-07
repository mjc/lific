import fs from 'node:fs';

function markup(node) {
  const attributes = Object.entries(node.attributes)
    .map(([name, value]) => ` ${name}="${value.replaceAll('&', '&amp;').replaceAll('"', '&quot;')}"`)
    .join('');
  return `<${node.name}${attributes}/>`;
}

export default {
  multipass: true,
  plugins: [
    {
      name: 'preset-default',
      params: {
        overrides: {
          convertShapeToPath: false,
          convertPathData: false,
          mergePaths: false,
          collapseGroups: false,
        },
      },
    },
    {
      name: 'exportInlineIconBodies',
      fn: () => ({
        root: {
          exit(root) {
            const bodies = {};
            for (const icon of root.children[0].children) {
              const name = icon.attributes['data-icon'];
              if (name) bodies[name] = icon.children.map(markup).join('');
            }
            fs.writeFileSync(process.env.NATIVE_ICONS_OUTPUT, JSON.stringify(bodies));
          },
        },
      }),
    },
  ],
};
