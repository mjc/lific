// Ported from web/tests/attachmentDeepLink.test.ts on master 9683d38a.
const { describe, expect, test }=require('./assertions.js');
const {
  formatLineAnchor,
  fullLineLink,
  hashWithLineTarget,
  lineTargetFromHash,
  parseLineAnchor,
  routeWithLineTarget,
  routeWithoutLineTarget
}=require('./deep-link-adapter.js');
const { viewerKindFor }=require('./subjects.js').subject('attachments/viewers/kind');
describe("line anchors", () => {
  test("formats single lines and ranges", async () => {
    expect((await formatLineAnchor(12, 340))).toBe("att12-L340");
    expect((await formatLineAnchor(12, 340, 360))).toBe("att12-L340-360");
    expect((await formatLineAnchor(12, 340, 340))).toBe("att12-L340");
  });
  test("normalizes a backwards range", async () => {
    expect((await formatLineAnchor(3, 90, 12))).toBe("att3-L12-90");
  });
  test("parses with or without the leading hash", async () => {
    expect((await parseLineAnchor("att12-L340"))).toEqual({
      attachmentId: 12,
      start: 340,
      end: 340
    });
    expect((await parseLineAnchor("#att12-L340-360"))).toEqual({
      attachmentId: 12,
      start: 340,
      end: 360
    });
  });
  test("rejects anything that is not an attachment line anchor", async () => {
    for (const bad of [
      "#comment-42",
      "#/LIF/issues/LIF-1",
      "att12",
      "attX-L1",
      "att0-L1",
      "att12-L0",
      "att12-L1-",
      ""
    ]) {
      expect((await parseLineAnchor(bad))).toBeNull();
    }
  });
});
describe("reading a target out of the location hash", () => {
  test("reads the path-style fragment", async () => {
    expect((await lineTargetFromHash("#att7-L5-9"))).toEqual({
      attachmentId: 7,
      start: 5,
      end: 9
    });
  });
  test("reads the query carried on a hash route", async () => {
    expect((await lineTargetFromHash("#/LIF/issues/LIF-1?att=att7-L5"))).toEqual({
      attachmentId: 7,
      start: 5,
      end: 5
    });
    expect((await lineTargetFromHash("#/LIF/pages/3?view=all&att=att8-L2-4&comment=1"))).toEqual({ attachmentId: 8, start: 2, end: 4 });
  });
  test("returns null for routes and anchors that carry no target", async () => {
    expect((await lineTargetFromHash("#/LIF/issues/LIF-1"))).toBeNull();
    expect((await lineTargetFromHash("#/LIF/issues/LIF-1?comment=42"))).toBeNull();
    expect((await lineTargetFromHash(""))).toBeNull();
  });
});
describe("writing the target back into the URL", () => {
  test("adds and removes the query without disturbing other params", async () => {
    expect((await routeWithLineTarget("/LIF/issues/LIF-1?view=all", "att2-L9"))).toBe("/LIF/issues/LIF-1?view=all&att=att2-L9");
    expect((await routeWithoutLineTarget("/LIF/issues/LIF-1?view=all&att=att2-L9"))).toBe("/LIF/issues/LIF-1?view=all");
    expect((await routeWithoutLineTarget("/LIF/issues/LIF-1?att=att2-L9"))).toBe("/LIF/issues/LIF-1");
  });
  test("keeps the hash route intact when selecting a line", async () => {
    expect((await hashWithLineTarget("#/LIF/issues/LIF-1", "att2-L9"))).toBe("#/LIF/issues/LIF-1?att=att2-L9");
    expect((await hashWithLineTarget("#/LIF/issues/LIF-1?att=att2-L9", null))).toBe("#/LIF/issues/LIF-1");
  });
  test("uses the bare fragment when the route is not in the hash", async () => {
    expect((await hashWithLineTarget("", "att2-L9"))).toBe("#att2-L9");
    expect((await hashWithLineTarget("#att2-L9", null))).toBe("");
  });
});
describe("copyable full links", () => {
  test("turns a hash route into a path-style URL", async () => {
    expect((await fullLineLink("att2-L9", {
      origin: "https://lific.example",
      pathname: "/",
      search: "",
      hash: "#/LIF/issues/LIF-1?att=att2-L9"
    }))).toBe("https://lific.example/LIF/issues/LIF-1#att2-L9");
  });
  test("preserves a sub-path deployment prefix", async () => {
    expect((await fullLineLink("att2-L9", {
      origin: "https://host",
      pathname: "/tracker/",
      search: "",
      hash: "#/LIF/pages/3"
    }))).toBe("https://host/tracker/LIF/pages/3#att2-L9");
  });
  test("keeps an existing path-style location as-is", async () => {
    expect((await fullLineLink("att2-L9", {
      origin: "https://host",
      pathname: "/LIF/issues/LIF-1",
      search: "?view=all",
      hash: "#att2-L1"
    }))).toBe("https://host/LIF/issues/LIF-1?view=all#att2-L9");
  });
});
describe("viewer dispatch", () => {
  test("reads extensions off a filename", async () => {
    expect(viewerKindFor({filename:'build.log'})).toBe('text');
    expect(viewerKindFor({filename:'archive.tar.gz'})).toBe('file');
    expect(viewerKindFor({filename:'Makefile'})).toBe('text');
    expect(viewerKindFor({filename:'.gitignore'})).toBe('text');
  });
  test("mime wins for media and images", async () => {
    expect(viewerKindFor({ filename: "clip.webm", mime: "video/webm" })).toBe("video");
    expect(viewerKindFor({ filename: "clip.webm", mime: "audio/webm" })).toBe("audio");
    expect(viewerKindFor({ filename: "shot.png", mime: "image/png" })).toBe("image");
    expect(viewerKindFor({ filename: "voice.mp3", mime: "audio/mpeg" })).toBe("audio");
  });
  test("extension decides what kind of text a text/plain file is", async () => {
    expect(viewerKindFor({ filename: "fix.patch", mime: "text/plain" })).toBe("diff");
    expect(viewerKindFor({ filename: "fix.diff", mime: "text/plain" })).toBe("diff");
    expect(viewerKindFor({ filename: "rows.csv", mime: "text/plain" })).toBe("csv");
    expect(viewerKindFor({ filename: "rows.tsv", mime: "text/plain" })).toBe("csv");
    expect(viewerKindFor({ filename: "payload.json", mime: "text/plain" })).toBe("json");
    expect(viewerKindFor({ filename: "notes.txt", mime: "text/plain" })).toBe("text");
  });
  test("works with no mime at all, as markdown links do", async () => {
    expect(viewerKindFor({ filename: "shot.png" })).toBe("image");
    expect(viewerKindFor({ filename: "logo.svg" })).toBe("image");
    expect(viewerKindFor({ filename: "clip.mp4" })).toBe("video");
    expect(viewerKindFor({ filename: "build.log" })).toBe("text");
    expect(viewerKindFor({ filename: "main.rs" })).toBe("text");
    expect(viewerKindFor({ filename: "bundle.zip" })).toBe("zip");
    expect(viewerKindFor({ filename: "lific.sqlite3" })).toBe("sqlite");
    expect(viewerKindFor({ filename: "Dockerfile" })).toBe("text");
  });
  test("unknown types fall back to the download chip", async () => {
    expect(viewerKindFor({ filename: "report.pdf", mime: "application/pdf" })).toBe("file");
    expect(viewerKindFor({ filename: "binary.bin" })).toBe("file");
    expect(viewerKindFor({ filename: "" })).toBe("file");
  });
});
