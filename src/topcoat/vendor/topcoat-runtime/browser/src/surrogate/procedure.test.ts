import { afterEach, expect, it, vi } from "vitest";

import { Context } from "../expression/context";
import { SignalRegistry } from "../signal-registry";
import { Bool } from "./bool";
import { Procedure } from "./procedure";
import { String as RuntimeString } from "./string";

afterEach(() => vi.unstubAllGlobals());

it("sends argument arrays for empty, unit, and multiple arguments", async () => {
	const fetch = vi.fn(async () => new Response("true"));
	vi.stubGlobal("fetch", fetch);
	const procedure = new Procedure(
		new Context(new SignalRegistry()),
		"/api/example",
	);

	for (const [args, body] of [
		[[], "[]"],
		[[undefined], "[null]"],
		[[new Bool(true), new RuntimeString("hello")], '[true,"hello"]'],
	] as const) {
		await procedure.call(...args);
		expect(fetch).toHaveBeenLastCalledWith(
			"/api/example",
			expect.objectContaining({ method: "POST", body }),
		);
	}
});


it("keepalive preserves procedure argument framing, hydration, and lazy single execution", async () => {
	const fetch = vi.fn(async () => new Response("true"));
	vi.stubGlobal("fetch", fetch);
	const procedure = new Procedure(new Context(new SignalRegistry()), "/app/native/example");
	for (const [args, body] of [
		[[], "[]"],
		[[undefined], "[null]"],
		[[new Bool(true), new RuntimeString("hello")], '[true,"hello"]'],
	] as const) {
		const count = fetch.mock.calls.length;
		const future = procedure.call_keepalive(...args);
		expect(fetch).toHaveBeenCalledTimes(count);
		const value = await future;
		expect(value).toBeInstanceOf(Bool);
		expect((value as Bool).v).toBe(true);
		await future;
		expect(fetch).toHaveBeenCalledTimes(count + 1);
		expect(fetch).toHaveBeenLastCalledWith("/app/native/example", {
			method: "POST", headers: { "Content-Type": "application/json" }, body, keepalive: true,
		});
	}
	await procedure.call(new RuntimeString("ordinary"));
	expect(fetch).toHaveBeenLastCalledWith("/app/native/example", {
		method: "POST", headers: { "Content-Type": "application/json" }, body: '["ordinary"]',
	});
});

it("keepalive rejects unsuccessful responses once without decoding or retrying", async () => {
	const response = new Response("not JSON", { status: 403, statusText: "Forbidden" });
	const json = vi.spyOn(response, "json");
	const fetch = vi.fn(async () => response);
	vi.stubGlobal("fetch", fetch);
	const future = new Procedure(new Context(new SignalRegistry()), "/native/denied").call_keepalive();
	await expect(Promise.resolve(future)).rejects.toThrow("Procedure call failed: 403 Forbidden");
	await expect(Promise.resolve(future)).rejects.toThrow("Procedure call failed: 403 Forbidden");
	expect(fetch).toHaveBeenCalledTimes(1);
	expect(json).not.toHaveBeenCalled();
});


it("callable keepalive adapter retains zero, unit, and multiple argument arrays", async () => {
	const fetch = vi.fn(async () => new Response('"result"'));
	vi.stubGlobal("fetch", fetch);
	const procedure = new Procedure(new Context(new SignalRegistry()), "/native/adapter");
	for (const [args, body] of [
		[[], "[]"], [[undefined], "[null]"],
		[[new Bool(true), new RuntimeString("hello")], '[true,"hello"]'],
	] as const) {
		const result = await procedure.with_keepalive().call(...args);
		expect((result as RuntimeString).v).toBe("result");
		expect(fetch).toHaveBeenLastCalledWith("/native/adapter", {
			method: "POST", headers: { "Content-Type": "application/json" }, body, keepalive: true,
		});
	}
	expect(fetch).toHaveBeenCalledTimes(3);
});
