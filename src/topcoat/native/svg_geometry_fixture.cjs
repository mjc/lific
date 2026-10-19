// Inspect the SVG geometry emitted by the native icon helpers.
function inspectSvg(svg) {
  const rect = svg.getBoundingClientRect(), style = getComputedStyle(svg);
  return {
    width: rect.width,
    height: rect.height,
    color: style.color,
    viewBox: svg.getAttribute('viewBox'),
    stroke: style.stroke,
    strokeWidth: style.strokeWidth,
    shape: [...svg.children].map(node => ({
      tag: node.localName,
      attributes: Object.fromEntries([...node.attributes]
        .filter(attribute => !['class', 'style'].includes(attribute.name))
        .map(attribute => [attribute.name, attribute.value])),
    })),
  };
}

module.exports = {inspectSvg};
