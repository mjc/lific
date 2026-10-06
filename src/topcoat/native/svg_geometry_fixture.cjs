// Observe painted SVG presentation and the actual served geometry. This helper
// belongs to browser comparisons; it does not change application state or markup.
async function inspectSvg(svg) {
  const rect = svg.getBoundingClientRect(), style = getComputedStyle(svg);
  let nodes = [...svg.children];
  if (nodes.length === 1 && nodes[0].localName === 'use') {
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
    width:rect.width,height:rect.height,color:style.color,
    viewBox:svg.getAttribute('viewBox'),stroke:style.stroke,strokeWidth:style.strokeWidth,
    shape:nodes.map(node=>({tag:node.localName,attributes:Object.fromEntries([...node.attributes]
      .filter(attribute=>!['class','style'].includes(attribute.name))
      .map(attribute=>[attribute.name,attribute.value]))})),
  };
}

module.exports = {inspectSvg};
