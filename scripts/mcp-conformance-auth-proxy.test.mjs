import assert from "node:assert/strict";
import http from "node:http";
import { once } from "node:events";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { test } from "node:test";

const proxyScript = new URL("./mcp-conformance-auth-proxy.mjs", import.meta.url);

test("auth proxy injects the bearer token without changing the MCP wire", async (t) => {
  let observed;
  const upstream = http.createServer((request, response) => {
    let body = "";
    request.setEncoding("utf8");
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      observed = {
        method: request.method,
        url: request.url,
        authorization: request.headers.authorization,
        host: request.headers.host,
        protocolVersion: request.headers["mcp-protocol-version"],
        body,
      };
      response.writeHead(201, {
        "content-type": "application/json",
        "x-upstream": "preserved",
      });
      response.end('{"jsonrpc":"2.0","id":7,"result":{}}');
    });
  });
  upstream.listen(0, "127.0.0.1");
  await once(upstream, "listening");
  t.after(() => upstream.close());

  const address = upstream.address();
  assert(address && typeof address === "object");
  const child = spawn(process.execPath, [proxyScript], {
    env: {
      ...process.env,
      MCP_CONFORMANCE_TARGET_URL: `http://127.0.0.1:${address.port}/mcp`,
      MCP_CONFORMANCE_BEARER_TOKEN: "secret-test-token",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  t.after(() => child.kill("SIGTERM"));

  const lines = createInterface({ input: child.stdout });
  const proxyUrl = await Promise.race([
    once(lines, "line").then(([line]) => line),
    once(child, "exit").then(([code]) => {
      throw new Error(`proxy exited before announcing its URL (status ${code})`);
    }),
  ]);
  const response = await new Promise((resolve, reject) => {
    const request = http.request(
      `${proxyUrl}?case=one`,
      {
        method: "POST",
        headers: {
          host: "untrusted.example",
          "content-type": "application/json",
          "mcp-protocol-version": "2026-07-28",
        },
      },
      (incoming) => {
        let body = "";
        incoming.setEncoding("utf8");
        incoming.on("data", (chunk) => {
          body += chunk;
        });
        incoming.on("end", () => {
          resolve({
            status: incoming.statusCode,
            upstreamHeader: incoming.headers["x-upstream"],
            body,
          });
        });
      },
    );
    request.on("error", reject);
    request.end('{"jsonrpc":"2.0","id":7,"method":"tools/list"}');
  });

  assert.deepEqual(observed, {
    method: "POST",
    url: "/mcp?case=one",
    authorization: "Bearer secret-test-token",
    host: "untrusted.example",
    protocolVersion: "2026-07-28",
    body: '{"jsonrpc":"2.0","id":7,"method":"tools/list"}',
  });
  assert.deepEqual(response, {
    status: 201,
    upstreamHeader: "preserved",
    body: '{"jsonrpc":"2.0","id":7,"result":{}}',
  });
});
