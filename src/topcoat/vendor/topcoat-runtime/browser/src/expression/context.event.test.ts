import { expect, it } from "vitest";
import { Window } from "happy-dom";
import { SignalRegistry } from "../signal-registry";
import { Context } from "./context";
import { Event as RuntimeEvent } from "../surrogate";

it("adapts native keyboard events without wire hydration", () => {
	const window = new Window();
	const native = new window.KeyboardEvent("keydown", {key: "Tab", code: "Tab", shiftKey: true, ctrlKey: true, bubbles: true, cancelable: true});
	const cx = new Context(new SignalRegistry());
	const event = cx.event(native as unknown as globalThis.Event);
	expect(event).toBeInstanceOf(RuntimeEvent);
	expect(event.key.v).toBe("Tab");
	expect(event.code.v).toBe("Tab");
	expect(event.shift_key.v).toBe(true);
	expect(event.ctrl_key.v).toBe(true);
	expect(event.default_prevented.v).toBe(false);
	event.prevent_default();
	expect(native.defaultPrevented).toBe(true);
	expect(event.default_prevented.v).toBe(true);
	window.happyDOM.abort();
});

it("keeps native event target and propagation connected to the DOM", () => {
	const window = new Window();
	const input = window.document.createElement("input");input.id = "actual-input";input.value = "draft";
	window.document.body.append(input);
	const cx = new Context(new SignalRegistry());
	let bubbled = false;
	window.document.body.addEventListener("keydown", () => {bubbled = true;});
	input.addEventListener("keydown", native => {
		const event = cx.event(native as unknown as globalThis.Event);
		expect(event.target.id.v).toBe("actual-input");
		expect(event.target.value.v).toBe("draft");
		event.stop_propagation();
	});
	input.dispatchEvent(new window.KeyboardEvent("keydown", {key: "Tab", bubbles: true}));
	expect(bubbled).toBe(false);
	window.happyDOM.abort();
});
