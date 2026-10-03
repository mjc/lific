const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {declaration} = require('../shell/source.js');
const source = fs.readFileSync(path.resolve(__dirname, '../../../attachments/assets/attachments.js'), 'utf8');

function lineStarting(prefix) {
  const line = source.split('\n').find(line => line.trim().startsWith(prefix));
  if (!line) throw new Error(`Voice instrumentation changed: ${prefix}`);
  return line;
}

const mimeLine = lineStarting('const mime=[');
const VOICE_MIME_CANDIDATES = vm.runInNewContext(mimeLine.match(/const mime=(\[[^\]]+\])/)[1]);
const pickAudioMime = supported => vm.runInNewContext(`${mimeLine}\nmime;`,
  {win: {MediaRecorder: {isTypeSupported: supported}}});
const extensionExpression = source.match(/extension=(base==='audio\/mp4'[^;]+);const name=/)[1];
const extensionForMime = base => vm.runInNewContext(extensionExpression, {base});

function voiceNoteFilename(at, mime = 'audio/webm') {
  const actions = new Map();
  let uploaded;
  const panel = {replaceChildren() {}, remove() {}, append() {}};
  const environment = {
    phase: 'preview', error: '', panel, host: {append() {}}, previewUrl: 'blob:test',
    recorded: new Blob(['recorded bytes'], {type: mime.split(';')[0]}),
    Date: class extends Date {
      constructor(...args) {super(...(args.length ? args : [at.getTime()]));}
      static now() {return at.getTime();}
    },
    win: {File}, doc: {createElement: () => ({style: {}, setAttribute() {}})},
    button(label, action) {actions.set(label, action);},
    release() {}, update() {}, onChange() {}, cancel() {}, stop() {},
    onFile(files) {uploaded = files[0];},
  };
  vm.runInNewContext(`${declaration(source, 'renderVoice')}\nrenderVoice();`, environment);
  actions.get('Attach')();
  return uploaded.name;
}

function tick(elapsed, samples = null) {
  const display = {textContent: '0:00'}, meter = {value: 0};
  const environment = {
    win: {performance: {now: () => elapsed}, requestAnimationFrame: () => 1},
    started: 0, level: 0, raf: 0, samples,
    analyser: samples ? {getByteTimeDomainData() {}} : null,
    panel: {querySelector: selector => selector === 'meter' ? meter : display},
    stop() {},
  };
  vm.runInNewContext(`${declaration(source, 'tick')}\ntick();`, environment);
  return {elapsed: display.textContent, level: meter.value};
}

function formatElapsed(elapsed) {
  // Preserve the original formatter's pure boundary; stopping the recorder is
  // covered by lifecycle tests, not by these formatting inputs.
  const display = {textContent: ''};
  const expression = declaration(source, 'tick').match(/const seconds=[\s\S]+?(?=if\(analyser)/)[0];
  vm.runInNewContext(expression, {elapsed, panel: {querySelector: () => display}});
  return display.textContent;
}

module.exports = {VOICE_MIME_CANDIDATES, pickAudioMime, extensionForMime, voiceNoteFilename,
  formatElapsed, meterLevel: samples => tick(0, samples).level};
