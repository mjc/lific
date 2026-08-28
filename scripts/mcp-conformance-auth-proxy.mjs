import http from "node:http";
import https from "node:https";

const target = new URL(requiredEnv("MCP_CONFORMANCE_TARGET_URL"));
const bearerToken = requiredEnv("MCP_CONFORMANCE_BEARER_TOKEN");
const transport = target.protocol === "http:" ? http : target.protocol === "https:" ? https : null;
if (!transport) throw new Error(`unsupported target protocol: ${target.protocol}`);

const hopByHopHeaders = new Set([
  "connection",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
]);

function requiredEnv(name) {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function forwardedHeaders(headers) {
  const forwarded = { ...headers };
  for (const name of hopByHopHeaders) delete forwarded[name];
  return forwarded;
}

const server = http.createServer((request, response) => {
  const destination = new URL(target);
  destination.search = new URL(request.url ?? "/", "http://proxy.invalid").search;

  const headers = forwardedHeaders(request.headers);
  headers.authorization = `Bearer ${bearerToken}`;
  const upstream = transport.request(
    destination,
    { method: request.method, headers },
    (upstreamResponse) => {
      response.writeHead(
        upstreamResponse.statusCode ?? 502,
        forwardedHeaders(upstreamResponse.headers),
      );
      upstreamResponse.pipe(response);
    },
  );
  upstream.on("error", (error) => {
    if (!response.headersSent) response.writeHead(502, { "content-type": "text/plain" });
    response.end(`MCP conformance auth proxy failed: ${error.message}`);
  });
  request.pipe(upstream);
});

server.listen(0, "127.0.0.1", () => {
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("proxy did not bind a TCP port");
  process.stdout.write(`http://127.0.0.1:${address.port}${target.pathname}${target.search}\n`);
});

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => server.close(() => process.exit(0)));
}
