const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const filesPath=path.join(__dirname,'../../../files/assets/files.js');
const filesContext={module:{exports:{}}};
vm.runInNewContext(fs.readFileSync(filesPath,'utf8').replace('Object.freeze({PAGE_SIZE,','Object.freeze({compareCells,looksLikeDiff,PAGE_SIZE,'),filesContext);
const files=filesContext.module.exports;
const {exportSelected}=require('../../../issue_list/assets/issue-list.js');
const attachmentsPath = path.join(__dirname, '../../../attachments/assets/attachments.js');
const source = fs.readFileSync(attachmentsPath, 'utf8');
const context = {globalThis: {}, FormData, Headers, URLSearchParams, TextDecoder, Blob, AbortController};
// Expose existing internal functions without supplying replacement implementations.
const additional = ['resizeMime','DEFAULT_UPLOAD_CAP'];
vm.runInNewContext(source.replace('globalThis.LificTopcoatAttachments = {', `globalThis.LificTopcoatAttachments = {${additional.join(',')},`), context);
const attachments = context.globalThis.LificTopcoatAttachments;
const maps = {
  'attachments/production': attachments,
  'attachments/viewers/csv': files,
  'attachments/viewers/diff': files,
  'attachments/viewers/kind': {viewerKindFor: attachments.viewerKind},
  'issues/export': {selectedIssueExport: identifiers=>exportSelected(identifiers.map(identifier=>({identifier})),{identity:()=>globalThis.localStorage?.getItem('lific_token'),fetch:(...args)=>globalThis.fetch(...args),headers:()=>{const token=globalThis.localStorage?.getItem('lific_token');return token?{Authorization:`Bearer ${token}`}:{}}})},
  'attachments/downscale': {DEFAULT_UPLOAD_CAP_BYTES: attachments.DEFAULT_UPLOAD_CAP, DOWNSCALE_EDGE_PX: 2560,
    scaledDimensions: (width,height,targetEdge)=>{assert.equal(targetEdge,2560);const offer=attachments.decideDownscale({width,height,bytes:1,mime:'image/jpeg'});return offer?{width:offer.width,height:offer.height}:{width,height};},
    estimateDownscaledBytes: (source,target)=>{const offer=attachments.decideDownscale(source);assert.equal(target.width,offer.width);assert.equal(target.height,offer.height);return offer.estimatedBytes;},
    decideDownscale: attachments.decideDownscale, outputMimeFor: attachments.resizeMime, parseUploadCap: attachments.parseUploadCap},
  'files/files': {...require('./files-adapter.js'), formatSweepCountdown: files.formatCountdown, canDeleteAttachment: files.canDelete, entityHref: files.entityHref},
};
module.exports = {subject: name => maps[name]};
