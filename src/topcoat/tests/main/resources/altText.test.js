// Main selected images by insertion offset. Topcoat selects by attachment ID;
// these cases assert the same document edits through its production function.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {subject} = require('./subjects.js');
const {replaceImageAlt} = subject('attachments/production');
// A known reference is selected in each fixture; offsets are no longer a public API.
const body='intro\n![shot.png](/api/attachments/9)\noutro';
for(const [title,raw,expected] of [
 ['folds newlines and collapses runs of whitespace','  the crash\n  dialog  ','the crash dialog'],
 ['strips brackets that would terminate the alt early','panel [left] side','panel left side'],
]) test(title,()=>assert.equal(replaceImageAlt('![filename](/api/attachments/1)',1,raw),`![${expected}](/api/attachments/1)`));
test('reduces a whitespace-only input to nothing',()=>assert.equal(replaceImageAlt('![](/api/attachments/1)',1,'  \n\t '),'![](/api/attachments/1)'));
test('locates the alt of the reference at the insertion point',()=>assert.equal(replaceImageAlt(body,9,'Selected'), 'intro\n![Selected](/api/attachments/9)\noutro'));
test('ignores a bracket pair that is not an image reference',()=>assert.equal(replaceImageAlt('![not a link] then ![real.png](/api/attachments/1)',1,'Selected'),'![not a link] then ![Selected](/api/attachments/1)'));
test('falls back to searching from the start when the offset is past it',()=>assert.equal(replaceImageAlt(body,9,'Selected'),'intro\n![Selected](/api/attachments/9)\noutro'));
test('returns null when there is no image reference',()=>assert.equal(replaceImageAlt('just [a link](/x) here',1,'Selected'),'just [a link](/x) here'));
test('replaces the filename placeholder with the description',()=>assert.equal(replaceImageAlt('Here it is:\n![shot.png](/api/attachments/9)\n',9,'The crash dialog'),'Here it is:\n![The crash dialog](/api/attachments/9)\n'));
test('rewrites the reference at the offset, not an earlier one',()=>assert.equal(replaceImageAlt('![first.png](/api/attachments/1)\n![second.png](/api/attachments/2)',2,'Second shot'),'![first.png](/api/attachments/1)\n![Second shot](/api/attachments/2)'));
test('fills an empty alt slot',()=>assert.equal(replaceImageAlt('![](/api/attachments/3)',3,'Board view'),'![Board view](/api/attachments/3)'));
test('treats a blank description as a skip',()=>assert.equal(replaceImageAlt('![shot.png](/api/attachments/9)',9,'   '),'![shot.png](/api/attachments/9)'));
test('leaves the document alone when the reference is gone',()=>assert.equal(replaceImageAlt('nothing to see',1,'Anything'),'nothing to see'));
test('sanitizes on the way in so the reference cannot be broken',()=>assert.equal(replaceImageAlt('![a.png](/api/attachments/1)',1,'two\nlines [and] brackets'),'![two lines and brackets](/api/attachments/1)'));
test('tolerates an out-of-range offset',()=>assert.equal(replaceImageAlt('![a.png](/api/attachments/1)',1,'Fine'),'![Fine](/api/attachments/1)'));
