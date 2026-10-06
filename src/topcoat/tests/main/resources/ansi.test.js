// Ported from web/tests/ansi.test.ts on master 9683d38a.
const { describe, expect, test }=require('./assertions.js');
const {
  ansiLineToSpans,
  ansiStyleToCss,
  ansiToSpans,
  hasAnsi,
  stripAnsi
}=require('./viewer-adapter.js');
const ESC = "\x1B";
describe("ansi to spans", () => {
  test("plain text becomes one unstyled span", async () => {
    const { spans } = (await ansiLineToSpans("cargo test"));
    expect(spans).toEqual([{ text: "cargo test", style: {} }]);
  });
  test("colors the run between an SGR code and its reset", async () => {
    const { spans } = (await ansiLineToSpans(`ok ${ESC}[32mpassed${ESC}[0m done`));
    expect(spans.map((s) => s.text)).toEqual(["ok ", "passed", " done"]);
    expect(spans[1].style.fg).toBe("var(--ansi-green)");
    expect(spans[2].style.fg).toBeUndefined();
  });
  test("combines attributes and clears them individually", async () => {
    const { spans } = (await ansiLineToSpans(`${ESC}[1;4;31mloud${ESC}[24mquieter`));
    expect(spans[0].style).toMatchObject({
      bold: true,
      underline: true,
      fg: "var(--ansi-red)"
    });
    expect(spans[1].style).toMatchObject({ bold: true, underline: false });
  });
  test("reads bright, 256-color and truecolor forms", async () => {
    expect((await ansiLineToSpans(`${ESC}[91mx`)).spans[0].style.fg).toBe("var(--ansi-bright-red)");
    expect((await ansiLineToSpans(`${ESC}[38;5;33mx`)).spans[0].style.fg).toBe("#0087ff");
    expect((await ansiLineToSpans(`${ESC}[38;5;250mx`)).spans[0].style.fg).toBe("#bcbcbc");
    expect((await ansiLineToSpans(`${ESC}[38;2;18;52;86mx`)).spans[0].style.fg).toBe("#123456");
    expect((await ansiLineToSpans(`${ESC}[48;5;1mx`)).spans[0].style.bg).toBe("var(--ansi-red)");
  });
  test("an empty parameter list is a reset", async () => {
    const { spans } = (await ansiLineToSpans(`${ESC}[31mred${ESC}[mplain`));
    expect(spans[1].style.fg).toBeUndefined();
  });
  test("strips sequences it does not support instead of printing them", async () => {
    const { spans } = (await ansiLineToSpans(`${ESC}[2K${ESC}[1Gprogress${ESC}]0;window title${ESC}\\ done`));
    expect(spans.map((s) => s.text).join("")).toBe("progress done");
  });
  test("carries style across a line boundary", async () => {
    const lines = (await ansiToSpans(`${ESC}[33mfirst
second${ESC}[0m
third`));
    expect(lines).toHaveLength(3);
    expect(lines[0][0].style.fg).toBe("var(--ansi-yellow)");
    expect(lines[1][0].style.fg).toBe("var(--ansi-yellow)");
    expect(lines[2][0].style.fg).toBeUndefined();
  });
  test("a line of pure escapes still yields one empty span", async () => {
    const { spans } = (await ansiLineToSpans(`${ESC}[0m`));
    expect(spans).toEqual([{ text: "", style: {} }]);
  });
  test("merges adjacent runs that share a style", async () => {
    const { spans } = (await ansiLineToSpans(`a${ESC}[32mb${ESC}[32mc`));
    expect(spans).toHaveLength(2);
    expect(spans[1].text).toBe("bc");
  });
});
describe("ansi helpers", () => {
  test("detects and removes escapes", async () => {
    expect((await hasAnsi("plain"))).toBe(false);
    expect((await hasAnsi(`${ESC}[31mred`))).toBe(true);
    expect((await stripAnsi(`${ESC}[1;31merror${ESC}[0m: boom`))).toBe("error: boom");
  });
  test("maps style to css, swapping colors when inverted", async () => {
    expect((await ansiStyleToCss({ fg: "#ff0000", bold: true }))).toBe("color:#ff0000;font-weight:600");
    expect((await ansiStyleToCss({ fg: "#ff0000", bg: "#000000", inverse: true }))).toBe("color:#000000;background:#ff0000");
    expect((await ansiStyleToCss({ underline: true, strike: true }))).toBe("text-decoration:underline line-through");
    expect((await ansiStyleToCss({}))).toBe("");
  });
});
