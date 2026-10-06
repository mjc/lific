import type { Context } from "../expression/context";
import { dehydrate } from "../expression/dehydrate";
import { Future } from "./future";

export class Procedure<A extends unknown[] = unknown[], R = unknown> {
	constructor(
		private readonly cx: Context,
		/** The request URL for procedure calls, with route groups removed. */
		private readonly path: string,
	) {}

	call(...args: A): Future<R> {
		return this.request(args, false);
	}

	/** Uses the same procedure transport with Fetch's document-lifetime allowance. */
	call_keepalive(...args: A): Future<R> {
		return this.request(args, true);
	}

	/** Callable adapter for the Rust expression macro's argument-tuple checking. */
	with_keepalive(): { call: (...args: A) => Future<R> } {
		return { call: (...args: A) => this.call_keepalive(...args) };
	}

	private request(args: A, keepalive: boolean): Future<R> {
		return new Future(async () => {
			const response = await fetch(this.path, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify(args.map(dehydrate)),
				// A typed procedure returns JSON; reject redirects before fetching a document.
				redirect: "manual",
				...(keepalive ? { keepalive: true } : {}),
			});
			if (!response.ok) {
				throw new Error(
					`Procedure call failed: ${response.status} ${response.statusText}`,
				);
			}

			return this.cx.hydrate(await response.json()) as R;
		});
	}

	dehydrate(): { t: "Procedure"; path: string } {
		return { t: "Procedure", path: this.path };
	}
}
