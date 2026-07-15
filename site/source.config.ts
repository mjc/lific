import { defineConfig, defineDocs } from "fumadocs-mdx/config";
import lastModified from "fumadocs-mdx/plugins/last-modified";

export const docs = defineDocs({
  dir: "content/docs",
});

export default defineConfig({
  // Git-derived last-modified dates, surfaced in the sitemap.
  plugins: [lastModified()],
});
