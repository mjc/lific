const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {declaration} = require('../shell/source.js');
const source = fs.readFileSync(path.resolve(__dirname, '../../../attachments/assets/attachments.js'), 'utf8');
const product = {globalThis: {}};
vm.runInNewContext(source, product);
const resizeCrop = product.globalThis.LificTopcoatAttachments.resizeCrop;
const rectLine = source.split('\n').find(line => line.trim().startsWith('const rect = shape =>'));
const pointLine = source.split('\n').find(line => line.trim().startsWith('const point=event=>'));

function normalizeRect(from, to) {
  return vm.runInNewContext(`${rectLine}\nrect(shape);`, {shape: {from, to}});
}

function clippedPoint(point, bounds) {
  const canvas = {width: bounds.w, height: bounds.h,
    getBoundingClientRect: () => ({left: 0, top: 0, width: bounds.w, height: bounds.h})};
  return vm.runInNewContext(`${pointLine}\npoint(event);`, {canvas, event: {clientX: point.x, clientY: point.y}});
}

function clampRect(rect, bounds) {
  return normalizeRect(clippedPoint({x: rect.x, y: rect.y}, bounds),
    clippedPoint({x: rect.x + rect.w, y: rect.y + rect.h}, bounds));
}

function redact(region, bounds) {
  const clipped = clampRect(region, bounds);
  const operations = [];
  const context = new Proxy({}, {get: (_, key) => (...args) => operations.push({kind: key, args}), set: () => true});
  const environment = {canvas: {width: bounds.w, height: bounds.h}, image: {}, stroke: 3,
    shapes: [{kind: 'redact', color: '#000', from: {x: clipped.x, y: clipped.y},
      to: {x: clipped.x + clipped.w, y: clipped.y + clipped.h}}], active: null, crop: null,
    rect: shape => normalizeRect(shape.from, shape.to), context};
  vm.runInNewContext(`${declaration(source, 'draw')}\ndraw(context);`, environment);
  const fill = operations.find(operation => operation.kind === 'fillRect');
  const [x, y, w, h] = fill.args;
  return {operations, region: {x, y, w, h}};
}

function pixelateSteps(region, bounds) {
  const result = redact(region, bounds);
  if (!result.region.w || !result.region.h) return null; // A canvas fill of zero area paints no pixels.
  const sampled = result.operations.find(operation => operation.kind === 'drawImage' && operation.args.length > 3);
  // Report actual raster operations, including the lack of a sampling buffer.
  return {source: result.region, dest: result.region,
    small: sampled ? {w: sampled.args.at(-2), h: sampled.args.at(-1)} : null,
    block: sampled ? result.region.w / sampled.args.at(-2) : null};
}

const pixelBlockSize = region => pixelateSteps(region, {w: region.w, h: region.h})?.block;
const strokeExpression = source.match(/,stroke=(Math\.max\(3,[^;]+);/)[1];
const strokeWidthFor = size => vm.runInNewContext(strokeExpression, {canvas: {width: size.w, height: size.h}});
const mimeExpression = source.match(/const mime=(\['image\/jpeg','image\/jpg'\][^;]+);const blob=/)[1];
const outputMime = type => vm.runInNewContext(mimeExpression, {file: {type}});
const filenameExpression = source.match(/const name=(file\.name\.replace[^;]+);finish/)[1];
const outputFilename = (name, mime) => vm.runInNewContext(filenameExpression, {file: {name}, mime});

const undoLimit = Number(source.match(/if\(history\.length===(\d+)\)history\.shift/)[1]);
function createUndoStack() {
  const environment = {history: [], shapes: [], crop: null, ctx: {}, draw() {},
    finished: false, win: {clearTimeout() {}, removeEventListener() {}}, timer: null,
    signal: {removeEventListener() {}}, aborted() {}, key() {}, previous: null,
    prompt: {remove() {}}, dialog: {remove() {}}, resolve() {}};
  const undoLine = source.split('\n').find(line => line.trim().startsWith('const undo=()=>'));
  vm.runInNewContext(`${declaration(source, 'snapshot')}\n${undoLine}\n${declaration(source, 'finish')}\n` +
    'globalThis.observed={snapshot,undo,finish};', environment);
  const observed = environment.observed;
  return {push(value) {environment.shapes = [{value}]; observed.snapshot();},
    undo() {const available = environment.history.length > 0 && !environment.finished; observed.undo(); return available ? environment.shapes[0]?.value : undefined;},
    size: () => environment.finished ? 0 : environment.history.length,
    canUndo: () => !environment.finished && environment.history.length > 0,
    clear() {observed.finish(null);}};
}

module.exports = {normalizeRect, clampRect, pixelBlockSize, pixelateSteps, resizeCrop,
  strokeWidthFor, outputMime, outputFilename, createUndoStack, undoLimit};
