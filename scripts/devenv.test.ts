import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";

function evaluate(attributes: string[], testing = false) {
  const result = Bun.spawnSync([
    "devenv",
    "--no-reload",
    "--option",
    "devenv.isTesting:bool",
    String(testing),
    "eval",
    ...attributes,
  ]);
  if (result.exitCode !== 0) throw new Error(result.stderr.toString());
  return JSON.parse(result.stdout.toString());
}

test("shell setup cannot select the project checks or rewrite formatting", () => {
  const config = evaluate(["tasks", "languages.javascript.bun.install.enable"]);
  for (const name of [
    "lific:check",
    "devenv:git-hooks:run",
    "devenv:treefmt:run",
  ]) {
    expect(config.tasks[name].before).toEqual([]);
  }
  expect(config["languages.javascript.bun.install.enable"]).toBe(false);
  const install = config.tasks["lific:install:web"];
  expect(install.exec).toBe("bun install --frozen-lockfile");
  expect(install.execIfModified).toEqual([]);
  expect(install.before).toContain("devenv:enterShell");
}, 120_000);

test("tests check formatting before compilation and use the native processes", () => {
  const { tasks, processes } = evaluate(["tasks", "processes"], true);
  expect(tasks["lific:check"].before).toContain("devenv:enterTest");
  expect(tasks["devenv:treefmt:run"].exec).toBe("treefmt --ci");
  expect(tasks["lific:web:check"].after).toContain("devenv:treefmt:run");
  expect(tasks["devenv:git-hooks:run"].after).toContain("lific:web:build");
  expect(processes.backend.exec).toContain("mktemp -d");
  expect(processes.frontend.after).toContain("devenv:processes:backend@ready");
}, 120_000);

test("packages use the selected compiler, release profile, and bounded sources", () => {
  const config = evaluate([
    "languages.rust.toolchainPackage.version",
    "languages.rust.toolchainPackage.outPath",
    "treefmt.config.programs.rustfmt.package.version",
    "outputs.lific.cargoBuildType",
    "outputs.lific.nativeBuildInputs",
    "outputs.lific.src.outPath",
    "outputs.web.src.outPath",
  ]);
  expect(config["treefmt.config.programs.rustfmt.package.version"]).toBe(
    config["languages.rust.toolchainPackage.version"],
  );
  expect(config["outputs.lific.cargoBuildType"]).toBe("release-dist");
  expect(config["outputs.lific.nativeBuildInputs"]).toContain(
    config["languages.rust.toolchainPackage.outPath"],
  );
  for (const key of ["outputs.lific.src.outPath", "outputs.web.src.outPath"]) {
    for (const path of [
      "target",
      ".devenv",
      "lific.db",
      "web/node_modules",
      "web/dist",
    ]) {
      expect(existsSync(`${config[key]}/${path}`)).toBe(false);
    }
  }
  const web = config["outputs.web.src.outPath"];
  expect(readFileSync(`${web}/Cargo.toml`, "utf8")).toBe(
    readFileSync("Cargo.toml", "utf8"),
  );
  expect(readFileSync(`${web}/web/bun.lock`, "utf8")).toBe(
    readFileSync("web/bun.lock", "utf8"),
  );
}, 120_000);
