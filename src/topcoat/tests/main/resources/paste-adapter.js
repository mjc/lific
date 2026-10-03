const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {declaration} = require('../shell/source.js');
const source = fs.readFileSync(path.resolve(__dirname, '../../../attachments/assets/attachments.js'), 'utf8');
const context = {globalThis: {}};
vm.runInNewContext(source, context);
const attachments = context.globalThis.LificTopcoatAttachments;
const lineExpression = source.match(/const isBigPaste = text => [^;]+/)[0].match(/text\.split\([^)]*\)\.length/)[0];
const countLines = text => vm.runInNewContext(lineExpression, {text});
const labelExpression = source.match(/node\.textContent=(`[^`]+lines pasted\.\s*`)/)[1];
const describePaste = value => vm.runInNewContext(labelExpression, {value}).replace(/ pasted\.\s*$/, '');

function pasteFileFrom(value, at) {
  let file;
  const environment = {
    pasteOffer: {value}, clearPaste() {}, win: {File},
    Date: class extends Date {constructor(...args) {super(...(args.length ? args : [at.getTime()]));}},
    enqueue(files) {file = files[0]; return Promise.resolve();},
  };
  vm.runInNewContext(`${declaration(source, 'pasteAttachment')}\npasteAttachment();`, environment);
  return file;
}

module.exports = {BIG_PASTE_CHAR_LIMIT: 6000, BIG_PASTE_LINE_LIMIT: 60,
  isBigPaste: attachments.isBigPaste, countLines, describePaste, pasteFileFrom,
  pasteFilename: at => pasteFileFrom('', at).name};
