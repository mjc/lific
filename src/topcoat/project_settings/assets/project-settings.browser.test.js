const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

test(
  "archive import sends authenticated multipart data, keeps refusals retryable, and renders the report",
  { skip: !process.env.PLAYWRIGHT_EXECUTABLE_PATH },
  async () => {
    const { chromium } = await import(
      path.resolve(__dirname, "../../../..", "e2e/node_modules/playwright/index.mjs")
    );
    const browser = await chromium.launch({
      headless: true,
      executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH,
    });
    try {
      const page = await browser.newPage();
      const requests = [];
      let refusal = true;
      const html =
        '<section data-topcoat-project-settings="archive" aria-busy="true"><p data-project-settings-status role="status"></p><p data-project-settings-error role="alert"></p><div data-project-settings-content></div></section>';
      await page.route("http://lific.test/**", async (route) => {
        const request = route.request();
        if (request.method() !== "POST")
          return route.fulfill({ contentType: "text/html", body: html });
        requests.push({
          url: request.url(),
          authorization: request.headers().authorization,
          body: request.postDataBuffer().toString(),
        });
        if (refusal)
          return route.fulfill({
            status: 422,
            contentType: "application/json",
            body: JSON.stringify({ error: "Invalid archive checksum" }),
          });
        return route.fulfill({
          status: 201,
          contentType: "application/json",
          body: JSON.stringify({
            project: { id: 9, identifier: "NEW", is_public: false },
            report: {
              rows: { issues: 3 },
              blobs: 2,
              external_reference_count: 1,
              external_references: ["OLD-1"],
            },
          }),
        });
      });
      await page.goto("http://lific.test/app/projects/import");
      await page.addScriptTag({
        content: fs.readFileSync(`${__dirname}/project-settings.js`, "utf8"),
      });
      await page.evaluate(() => {
        localStorage.setItem("lific_token", "acceptance-token");
        window.LificTopcoatRouting = { href: (route) => `/app${route}` };
        const session = {
          state: { publicProject: null, user: { id: 1 } },
          request: async () => ({
            ok: true,
            data: { can_import: true, max_upload_bytes: 1000 },
          }),
        };
        window.archiveApp = LificTopcoatProjectSettings.attach(
          document.querySelector("section"),
          { session },
        );
      });

      const chooseArchive = async () => {
        await page.getByLabel("Project archive (.tar.gz)", { exact: true }).setInputFiles({
          name: "project.tar.gz",
          mimeType: "application/gzip",
          buffer: Buffer.from("archive content"),
        });
        await page
          .getByLabel(
            "I understand this imports linked files, history and deleted content that may contain sensitive information.",
            { exact: true },
          )
          .check();
      };
      await chooseArchive();
      await page.getByRole("button", { name: "Import as private project", exact: true }).click();
      await page.getByText("Invalid archive checksum", { exact: true }).waitFor();
      assert.equal(requests.length, 1);
      assert.equal(requests[0].url, "http://lific.test/app/api/project-archives");
      assert.equal(requests[0].authorization, "Bearer acceptance-token");
      assert.match(requests[0].body, /name="archive"; filename="project\.tar\.gz"/);
      assert.match(requests[0].body, /archive content/);

      refusal = false;
      await chooseArchive();
      await page.getByRole("button", { name: "Import as private project", exact: true }).click();
      await page.getByRole("heading", { name: "NEW imported", exact: true }).waitFor();
      await page.getByText("OLD-1", { exact: true }).waitFor();
      assert.equal(requests.length, 2);
      assert.equal(requests[1].authorization, "Bearer acceptance-token");
      assert.match(requests[1].body, /name="archive"; filename="project\.tar\.gz"/);
      assert.match(requests[1].body, /archive content/);

      await page.getByRole("button", { name: "Import another archive", exact: true }).click();
      await page.getByLabel("Project archive (.tar.gz)", { exact: true }).waitFor();
    } finally {
      await browser.close();
    }
  },
);
