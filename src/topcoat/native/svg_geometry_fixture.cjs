// Observe painted SVG presentation and the actual CSS or served geometry. This helper
// belongs to browser comparisons; it does not change application state or markup.
async function inspectSvg(svg, pseudoElement) {
  const rect = svg.getBoundingClientRect(), style = getComputedStyle(svg, pseudoElement);
  let nodes = [...svg.children];
  let viewBox = svg.getAttribute('viewBox');
  if (pseudoElement || svg.matches('svg.native-icon[data-icon]')) {
    const mask = style.maskImage.match(/^url\((["']?)(data:image\/svg\+xml,[^"'()]*)\1\)$/);
    if (!mask) throw new Error(`The painted native icon has no inline SVG mask: ${style.maskImage}`);
    const markup = decodeURIComponent(mask[2].split(',').slice(1).join(','));
    const asset = new DOMParser().parseFromString(markup, 'image/svg+xml');
    if (asset.querySelector('parsererror')) throw new Error('The painted icon SVG mask is malformed.');
    const root = asset.documentElement, geometry = asset.getElementById('icon');
    if (root.localName !== 'svg' || !geometry) throw new Error('The painted icon mask has no SVG geometry.');
    if (root.getAttribute('fill') !== 'none' || root.getAttribute('stroke') !== 'black'
      || root.getAttribute('stroke-width') !== '2') throw new Error('The painted icon mask changed its stroke presentation.');
    if (style.maskSize !== 'contain' || style.maskRepeat !== 'no-repeat' || style.maskPosition !== '50% 50%') {
      throw new Error('The painted icon mask changed its sizing or repetition.');
    }
    if (pseudoElement) {
      if (style.backgroundColor !== style.color) throw new Error('The icon pseudo-element does not paint its current color.');
      viewBox = root.getAttribute('viewBox');
    } else {
      if (style.maskMode !== 'alpha') throw new Error('The native icon does not use the SVG alpha channel.');
      if (root.getAttribute('viewBox') !== viewBox) throw new Error('The painted icon mask changed its viewBox.');
      if (nodes.length !== 1 || nodes[0].localName !== 'rect') throw new Error('The native icon has no solid paint rectangle.');
      if (nodes[0].getAttribute('width') !== '24' || nodes[0].getAttribute('height') !== '24') {
        throw new Error('The native icon paint rectangle does not cover its viewBox.');
      }
      const paint = getComputedStyle(nodes[0]);
      if (paint.fill !== style.color || paint.stroke !== 'none') throw new Error('The native icon does not paint its current color.');
    }
    nodes = [...geometry.children];
  } else if (nodes.length === 1 && nodes[0].localName === 'use') {
    const href = nodes[0].getAttribute('href');
    if (!href) throw new Error('The painted SVG use has no asset reference.');
    const url = new URL(href, document.baseURI);
    if (url.origin !== location.origin) throw new Error('The icon asset must use the production origin.');
    const id = decodeURIComponent(url.hash.slice(1));
    url.hash = '';
    const response = await fetch(url.href, {cache:'force-cache'});
    if (!response.ok) throw new Error(`Icon geometry request failed: ${response.status} ${url.href}`);
    if (!response.headers.get('content-type')?.includes('image/svg+xml')) {
      throw new Error('The icon geometry response is not SVG.');
    }
    const asset = new DOMParser().parseFromString(await response.text(), 'image/svg+xml');
    if (asset.querySelector('parsererror')) throw new Error('The served icon SVG is malformed.');
    const geometry = asset.getElementById(id);
    if (!geometry) throw new Error(`The served icon has no referenced geometry: ${id}`);
    nodes = [...geometry.children];
  }
  return {
    width:pseudoElement ? parseFloat(style.width) : rect.width,
    height:pseudoElement ? parseFloat(style.height) : rect.height,color:style.color,
    viewBox,stroke:style.stroke,strokeWidth:style.strokeWidth,
    shape:nodes.map(node=>({tag:node.localName,attributes:Object.fromEntries([...node.attributes]
      .filter(attribute=>!['class','style'].includes(attribute.name))
      .map(attribute=>[attribute.name,attribute.value]))})),
  };
}

module.exports = {inspectSvg};
