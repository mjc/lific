const {product} = require('./harness');
const attachments = product('attachments/assets/attachments.js', 'LificTopcoatAttachments');
const create = product('issue_create/assets/issue-create.js', 'LificTopcoatIssueCreate');
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
