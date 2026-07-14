import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Fully static: the page has no server-side anything, and production
  // hosting is Cloudflare Workers static assets (see wrangler.jsonc).
  output: "export",
};

export default nextConfig;
