// Port of main 9683d38a web/tests/insertSnippet.test.ts; original assertions retained.
const { expect, test } = require("./harness");
const { insertSnippetAt, markdownFor } = require("./compose-adapter");
const SNIPPET = "![shot.png](/api/attachments/7)";
function upload(over = {}) {
  return {
    id: 7,
    url: "/api/attachments/7",
    filename: "shot.png",
    mime: "image/png",
    size: 1234,
    ...over
  };
}
test("images embed, everything else links", async () => {
  expect(markdownFor(upload())).toBe("![shot.png](/api/attachments/7)");
  expect(markdownFor(upload({ filename: "trace.log", mime: "text/plain" }))).toBe("[trace.log](/api/attachments/7)");
});
test("inserting into an empty composer adds no leading break", async () => {
  const { text, caret } = await insertSnippetAt("", 0, 0, SNIPPET);
  expect(text).toBe(`${SNIPPET}
`);
  expect(caret).toBe(SNIPPET.length + 1);
});
test("inserting mid-line breaks onto its own block", async () => {
  const current = "see this";
  const { text, caret } = await insertSnippetAt(current, 8, 8, SNIPPET);
  expect(text).toBe(`see this
${SNIPPET}
`);
  expect(caret).toBe(text.length);
});
test("inserting at the start of a fresh line keeps it inline", async () => {
  const current = `intro
`;
  const { text, caret } = await insertSnippetAt(current, 6, 6, SNIPPET);
  expect(text).toBe(`intro
${SNIPPET}
`);
  expect(caret).toBe(text.length);
});
test("inserting in the middle keeps the tail and lands the caret before it", async () => {
  const current = `before
after`;
  const { text, caret } = await insertSnippetAt(current, 7, 7, SNIPPET);
  expect(text).toBe(`before
${SNIPPET}
after`);
  expect(caret).toBe(`before
${SNIPPET}
`.length);
  expect(text.slice(caret)).toBe("after");
});
test("a selection is replaced, not pushed aside", async () => {
  const current = "keep DROP keep";
  const { text, caret } = await insertSnippetAt(current, 5, 9, SNIPPET);
  expect(text).toBe(`keep 
${SNIPPET}
 keep`);
  expect(text.slice(caret)).toBe(" keep");
});
test("a backwards selection is normalised", async () => {
  const forward = await insertSnippetAt("keep DROP keep", 5, 9, SNIPPET);
  const backward = await insertSnippetAt("keep DROP keep", 9, 5, SNIPPET);
  expect(backward).toEqual(forward);
});
test("out-of-range offsets clamp to the text instead of throwing", async () => {
  const current = "short";
  const { text, caret } = await insertSnippetAt(current, 999, 999, SNIPPET);
  expect(text).toBe(`short
${SNIPPET}
`);
  expect(caret).toBe(text.length);
  const negative = await insertSnippetAt(current, -4, -4, SNIPPET);
  expect(negative.text).toBe(`${SNIPPET}
short`);
  expect(negative.caret).toBe(SNIPPET.length + 1);
});
test("consecutive inserts stack one per line", async () => {
  const first = await insertSnippetAt("", 0, 0, "![a](/api/attachments/1)");
  const second = await insertSnippetAt(first.text, first.caret, first.caret, "![b](/api/attachments/2)");
  expect(second.text).toBe(`![a](/api/attachments/1)
![b](/api/attachments/2)
`);
  expect(second.caret).toBe(second.text.length);
});
