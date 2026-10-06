// Ported from web/tests/diff.test.ts on master 9683d38a.
const { describe, expect, test }=require('./assertions.js');
const {
  looksLikeDiff,
  parseUnifiedDiff,
  summarizeDiff
}=require('./subjects.js').subject('attachments/viewers/diff');
const GIT_DIFF = `diff --git a/src/main.rs b/src/main.rs
index 1234567..89abcde 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,7 +10,8 @@ fn main() {
     let args = Args::parse();
-    println!("hello");
+    println!("hello, world");
+    println!("extra");
     run(args);
diff --git a/README.md b/README.md
--- a/README.md
+++ b/README.md
@@ -1,3 +1,2 @@
 # Lific
-old line
 tail
`;
describe("unified diff parsing", () => {
  test("splits files and counts each side", () => {
    const parsed = parseUnifiedDiff(GIT_DIFF);
    expect(parsed.files).toHaveLength(2);
    expect(parsed.files[0].display).toBe("src/main.rs");
    expect(parsed.files[0].additions).toBe(2);
    expect(parsed.files[0].deletions).toBe(1);
    expect(parsed.files[1].display).toBe("README.md");
    expect(parsed.files[1].additions).toBe(0);
    expect(parsed.files[1].deletions).toBe(1);
    expect(parsed.additions).toBe(2);
    expect(parsed.deletions).toBe(2);
  });
  test("numbers lines from the hunk header, per side", () => {
    const parsed = parseUnifiedDiff(GIT_DIFF);
    const body = parsed.files[0].lines.filter((l) => l.kind !== "meta" && l.kind !== "hunk");
    expect(body.map((l) => [l.kind, l.oldNo, l.newNo])).toEqual([
      ["context", 10, 10],
      ["del", 11, null],
      ["add", null, 11],
      ["add", null, 12],
      ["context", 12, 13]
    ]);
  });
  test("keeps the +/- marker out of the line text", () => {
    const parsed = parseUnifiedDiff(GIT_DIFF);
    const added = parsed.files[0].lines.find((l) => l.kind === "add");
    expect(added?.text).toBe('    println!("hello, world");');
  });
  test("summarizes with correct pluralization", () => {
    expect(summarizeDiff(parseUnifiedDiff(GIT_DIFF))).toBe("2 files changed, +2 -2");
    const single = parseUnifiedDiff(`--- a/x
+++ b/x
@@ -1 +1 @@
-a
+b
`);
    expect(summarizeDiff(single)).toBe("1 file changed, +1 -1");
  });
  test("handles a bare diff -u with no git header", () => {
    const parsed = parseUnifiedDiff(`--- one.txt	2026-01-01
+++ two.txt	2026-01-02
@@ -1,2 +1,2 @@
 keep
-drop
+take
`);
    expect(parsed.files).toHaveLength(1);
    expect(parsed.files[0].display).toBe("one.txt -> two.txt");
    expect(parsed.files[0].additions).toBe(1);
  });
  test("names new and deleted files from the surviving side", () => {
    const added = parseUnifiedDiff(`diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+hi
`);
    expect(added.files[0].display).toBe("new.txt");
    const removed = parseUnifiedDiff(`diff --git a/gone.txt b/gone.txt
deleted file mode 100644
--- a/gone.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
`);
    expect(removed.files[0].display).toBe("gone.txt");
  });
  test("flags binary files and gives them no hunk lines", () => {
    const parsed = parseUnifiedDiff(`diff --git a/logo.png b/logo.png
index 000..111 100644
Binary files a/logo.png and b/logo.png differ
`);
    expect(parsed.files[0].binary).toBe(true);
    expect(parsed.files[0].additions).toBe(0);
  });
  test("discards a format-patch preamble", () => {
    const parsed = parseUnifiedDiff(`From abc Mon Sep 17 00:00:00 2001
Subject: [PATCH] fix

A commit message.

diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-x
+y
`);
    expect(parsed.files).toHaveLength(1);
    expect(parsed.files[0].display).toBe("a.txt");
  });
  test("an empty or prose-only document parses to nothing", () => {
    expect(parseUnifiedDiff("").files).toHaveLength(0);
    expect(parseUnifiedDiff(`just some notes
`).files).toHaveLength(0);
  });
  test("sniffs a diff by its hunk header", () => {
    expect(looksLikeDiff(GIT_DIFF)).toBe(true);
    expect(looksLikeDiff("no hunks here @@ nope")).toBe(false);
  });
});
