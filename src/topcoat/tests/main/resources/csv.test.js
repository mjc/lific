// Ported from web/tests/csv.test.ts on master 9683d38a.
const { describe, expect, test }=require('./assertions.js');
const {
  compareCells,
  detectDelimiter,
  parseDelimited,
  sortRows
}=require('./subjects.js').subject('attachments/viewers/csv');
describe("delimited parsing", () => {
  test("reads a header row and body rows", () => {
    const table = parseDelimited(`name,age
ada,36
alan,41
`);
    expect(table.headers).toEqual(["name", "age"]);
    expect(table.rows).toEqual([
      ["ada", "36"],
      ["alan", "41"]
    ]);
    expect(table.totalRows).toBe(2);
    expect(table.truncated).toBe(false);
    expect(table.columnCount).toBe(2);
  });
  test("honors quotes, escaped quotes, and embedded separators", () => {
    const table = parseDelimited(`a,b
"x,y","he said ""hi"""
`);
    expect(table.rows[0]).toEqual(["x,y", 'he said "hi"']);
  });
  test("keeps newlines inside a quoted field", () => {
    const table = parseDelimited(`a,b
"line one
line two",tail
`);
    expect(table.rows).toHaveLength(1);
    expect(table.rows[0][0]).toBe(`line one
line two`);
  });
  test("tolerates CRLF and a UTF-8 BOM", () => {
    const table = parseDelimited(`\uFEFFa,b\r
1,2\r
`);
    expect(table.headers).toEqual(["a", "b"]);
    expect(table.rows).toEqual([["1", "2"]]);
  });
  test("caps rows but still reports the true total", () => {
    const body = Array.from({ length: 500 }, (_, i) => `r${i},${i}`).join(`
`);
    const table = parseDelimited(`a,b
${body}
`, { maxRows: 200 });
    expect(table.rows).toHaveLength(200);
    expect(table.totalRows).toBe(500);
    expect(table.truncated).toBe(true);
  });
  test("reports the widest row for a ragged file", () => {
    const table = parseDelimited(`a,b
1,2,3
`);
    expect(table.columnCount).toBe(3);
  });
  test("an empty document yields no headers and no rows", () => {
    const table = parseDelimited("");
    expect(table.headers).toEqual([]);
    expect(table.rows).toEqual([]);
    expect(table.totalRows).toBe(0);
  });
  test("splits on tabs when told to", () => {
    const table = parseDelimited(`a	b
1	2
`, { delimiter: "\t" });
    expect(table.headers).toEqual(["a", "b"]);
    expect(table.rows).toEqual([["1", "2"]]);
  });
});
describe("delimiter detection", () => {
  test("prefers the extension", () => {
    expect(detectDelimiter("data.tsv", "a,b,c")).toBe("\t");
    expect(detectDelimiter("data.csv", "a\tb\tc")).toBe(",");
  });
  test("falls back to the most common candidate in the first line", () => {
    expect(detectDelimiter("export.txt", `a;b;c
1;2;3`)).toBe(";");
    expect(detectDelimiter("export.txt", "a\tb\tc")).toBe("\t");
    expect(detectDelimiter("export.txt", "single-column")).toBe(",");
  });
});
describe("sorting", () => {
  test("compares numbers numerically and text case-insensitively", () => {
    expect(compareCells("9", "10")).toBeLessThan(0);
    expect(compareCells("Beta", "alpha")).toBeGreaterThan(0);
    expect(compareCells("same", "same")).toBe(0);
  });
  test("sinks blanks in both directions", () => {
    const rows = [["b"], [""], ["a"]];
    expect(sortRows(rows, 0, "asc").map((r) => r[0])).toEqual(["a", "b", ""]);
    expect(sortRows(rows, 0, "desc").map((r) => r[0])).toEqual(["b", "a", ""]);
  });
  test("does not mutate the input", () => {
    const rows = [["b"], ["a"]];
    sortRows(rows, 0, "asc");
    expect(rows.map((r) => r[0])).toEqual(["b", "a"]);
  });
  test("treats a missing cell as blank", () => {
    const rows = [["b", "1"], ["a"]];
    expect(sortRows(rows, 1, "asc").map((r) => r[0])).toEqual(["b", "a"]);
  });
});
