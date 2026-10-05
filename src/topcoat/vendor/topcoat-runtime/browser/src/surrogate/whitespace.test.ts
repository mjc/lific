import { expect, it } from "vitest";
import { String as RuntimeString } from "./string";
const cases = [
	["", ""], [" plain ", "plain"], ["\uFEFF value \uFEFF", "value"],
	["\u0085value\u0085", "\u0085value\u0085"], ["\uFEFF\u0085x\u0085\uFEFF", "\u0085x\u0085"],
	["\u2028😀\u2029", "😀"], ["\tvalue\t", "value"], ["\nvalue\n", "value"],
	["\u000Bvalue\u000B", "value"], ["\u000Cvalue\u000C", "value"], ["\rvalue\r", "value"],
	["\u00A0value\u00A0", "value"], ["\u1680value\u1680", "value"],
	["\u2000\u2001\u2002\u2003\u2004\u2005\u2006\u2007\u2008\u2009\u200Avalue\u200A", "value"],
	["\u202Fvalue\u202F", "value"], ["\u205Fvalue\u205F", "value"], ["\u3000value\u3000", "value"],
	[" x\uFEFFy ", "x\uFEFFy"],
];
it.each(cases)("ECMAScript trim preserves its exact whitespace set: %j", (source, expected) => {
	expect(new RuntimeString(source).trim_ecmascript().dehydrate()).toBe(expected);
});
it("does not alter ordinary Rust Unicode White_Space trim", () => {
	expect(new RuntimeString("\uFEFFvalue\uFEFF").trim().to_owned().dehydrate()).toBe("\uFEFFvalue\uFEFF");
	expect(new RuntimeString("\u0085value\u0085").trim().to_owned().dehydrate()).toBe("value");
});
