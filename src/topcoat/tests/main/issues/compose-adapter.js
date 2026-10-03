const {product} = require('./harness');
const attachments = product('attachments/assets/attachments.js', 'LificTopcoatAttachments');
const editor = product('issue_detail/editor/assets/editor.js', 'lificIssueEditor', value => value, {module:{exports:{}}});
const create = product('issue_create/assets/issue-create.js', 'LificTopcoatIssueCreate', value => value, {lificIssueEditor:editor});
async function insertSnippetAt(text, start, end, snippet) {
  // Run the same upload completion/insertion path used by the issue composer.
  const app = create.controller({attachments:{
    upload:() => ({result:Promise.resolve({ok:true,data:{}}),abort(){}}),
    markdown:() => snippet,
  }});
  app.state.description = text;
  await app.upload([{name:'test.png'}], {start,end});
  return {text:app.state.description,caret:app.state.caret};
}
module.exports = {insertSnippetAt, markdownFor:attachments.markdown};
