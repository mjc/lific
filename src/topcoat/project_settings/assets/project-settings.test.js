const { test } = require("node:test");
const assert = require("node:assert/strict");
const { archiveError, ArchiveImportController } = require("./project-settings.js");

const success = (data) => ({ ok: true, data });
function setup(upload = async () => success({})) {
  const storage = new Map();
  const states = [];
  const controller = new ArchiveImportController({
    identity: () => "private:1",
    storage: {
      setItem: (key, value) => storage.set(key, value),
      getItem: (key) => storage.get(key),
      removeItem: (key) => storage.delete(key),
    },
    request: async () => success({ can_import: true, max_upload_bytes: 100 }),
    upload,
    onChange: (state) => states.push(structuredClone(state)),
  });
  controller.state = {
    ...controller.state,
    status: "ready",
    capabilities: { can_import: true, max_upload_bytes: 100 },
  };
  return { controller, states, storage };
}

test("archive selection rejects wrong extensions, empty files, and oversized files", () => {
  assert.ok(archiveError({ name: "a.zip", size: 1 }, 20));
  assert.ok(archiveError({ name: "a.tar.gz", size: 0 }, 20));
  assert.ok(archiveError({ name: "a.tar.gz", size: 21 }, 20));
  assert.equal(archiveError({ name: "a.tar.gz", size: 20 }, 20), "");
});

test("upload reports progress and processing, then blocks retry after uncertain outcome", async () => {
  const { controller, storage } = setup(async (_file, progress, processing) => {
    progress(57);
    assert.equal(controller.state.archive.progress, 57);
    processing();
    assert.equal(controller.state.archive.phase, "processing");
    return { ok: false, status: null, error: "connection lost" };
  });

  await controller.importArchive({ name: "project.tar.gz", size: 10 }, true);

  assert.equal(controller.state.archive.phase, "unknown");
  assert.match(controller.state.archive.error, /Check the project list/);
  assert.equal(storage.get("lific:topcoat:archive-pending:private:1"), "1");
  assert.equal(
    await controller.importArchive({ name: "project.tar.gz", size: 10 }, true),
    false,
  );
  controller.acknowledgeArchive();
  assert.equal(controller.state.archive.phase, "idle");
  assert.equal(storage.has("lific:topcoat:archive-pending:private:1"), false);
});

test("definite import refusal remains retryable and successful result preserves external references", async () => {
  const { controller } = setup(async () => ({
    ok: false,
    status: 422,
    error: "checksum invalid",
  }));
  await controller.importArchive({ name: "project.tar.gz", size: 10 }, true);
  assert.equal(controller.state.archive.phase, "error");

  const result = {
    project: { id: 9, identifier: "NEW", is_public: false },
    report: {
      rows: { issues: 3 },
      blobs: 2,
      external_reference_count: 1,
      external_references: ["OLD-1"],
    },
  };
  controller.env.upload = async () => success(result);
  await controller.importArchive({ name: "project.tar.gz", size: 10 }, true);
  assert.deepEqual(controller.state.archive.result, result);
  assert.equal(controller.state.archive.phase, "success");
});

test("unknown outcome is restored after reload until the project list is checked", async () => {
  const { controller, storage } = setup();
  storage.set("lific:topcoat:archive-pending:private:1", "1");
  await controller.load();
  assert.equal(controller.state.archive.phase, "unknown");
  assert.match(controller.state.archive.error, /Check the project list/);
});

test("account change discards an in-flight upload response", async () => {
  let identity = "private:1";
  let finish;
  const { controller } = setup(() => new Promise((resolve) => (finish = resolve)));
  controller.env.identity = () => identity;
  const pending = controller.importArchive({ name: "project.tar.gz", size: 10 }, true);
  identity = "private:2";
  controller.accountChanged();
  finish(success({ project: { identifier: "STALE" }, report: {} }));
  await pending;
  assert.equal(controller.state.archive.phase, "idle");
  assert.equal(controller.state.archive.result, null);
});
