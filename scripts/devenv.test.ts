import { expect, test } from "bun:test";
import {
  existsSync,
  readFileSync,
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  chmodSync,
  statSync,
  rmSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

function evaluate(attributes: string[], testing = false, profile?: string) {
  const profileArgs = profile ? ["--profile", profile] : [];
  const result = Bun.spawnSync(
    [
      "devenv",
      "--no-reload",
      ...profileArgs,
      "--option",
      "devenv.isTesting:bool",
      String(testing),
      "eval",
      ...attributes,
    ],
    { timeout: 300_000, killSignal: "SIGKILL" },
  );
  if (result.exitCode !== 0) {
    throw new Error(
      `devenv eval exited ${result.exitCode}: ${result.stderr.toString()}`,
    );
  }
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
  expect(Object.keys(config.tasks).some((name) => name.startsWith("lific:web:") || name === "lific:install:web")).toBe(false);
  expect(config.tasks["lific:debug-build"].after).toEqual([]);
  expect(config.tasks["lific:rust-test"].exec).toBe("cargo test --all-targets --locked");
  expect(config.tasks["lific:check"].after).toContain("lific:topcoat:test");
  expect(config.tasks["lific:install:site"].before).toEqual([]);
  expect(config.tasks["lific:docs:check"].after).toContain("lific:docs:build");
}, 360_000);

test("e2e profile runs the production frontend without a development bundler", () => {
  const { tasks, processes } = evaluate(["tasks", "processes"], false, "e2e");
  const install = tasks["lific:install:e2e"];
  expect(install.exec).toContain("locks/lific-e2e-bun-install");
  expect(install.exec).toContain("bun install --frozen-lockfile");
  expect(install.before).toContain("devenv:enterShell");
  expect(tasks["lific:e2e"].after).toEqual([
    "lific:install:e2e",
    "lific:debug-build",
  ]);
  expect(tasks["lific:e2e"].exec).toContain("src/topcoat/public/assets/public.browser.test.js");
  expect(tasks["lific:e2e"].exec).not.toContain("topcoat-spike");
  expect(Object.keys(processes)).toEqual(["backend"]);
}, 360_000);

test("docs profile omits Rust and source formatting tools", () => {
  const config = evaluate(
    [
      "languages.rust.enable",
      "treefmt.enable",
      "git-hooks.hooks.clippy.enable",
      "git-hooks.hooks.treefmt.enable",
      "tasks",
    ],
    false,
    "docs",
  );
  for (const key of [
    "languages.rust.enable",
    "treefmt.enable",
    "git-hooks.hooks.clippy.enable",
    "git-hooks.hooks.treefmt.enable",
  ]) {
    expect(config[key]).toBe(false);
  }
  expect(config.tasks["lific:install:site"].before).toContain("devenv:enterShell");
}, 360_000);

test("test graph checks formatting before compilation and isolates the backend", () => {
  const { tasks, processes } = evaluate(["tasks", "processes"], true);
  expect(tasks["lific:check"].before).toContain("devenv:enterTest");
  expect(tasks["devenv:treefmt:run"].exec).toBe("treefmt --ci");
  expect(tasks["devenv:git-hooks:run"].after).toContain("lific:topcoat:test");
  expect(tasks["lific:rust-test"].after).toContain("devenv:treefmt:run");
  expect(processes.backend.exec).toContain("mktemp -d");
  expect(Object.keys(processes)).toEqual(["backend"]);
  const watchedPaths = processes.backend.watch.paths;
  expect(watchedPaths.join()).toMatch(/\/build\.rs/);
  expect(processes.backend.watch.extensions).toEqual(expect.arrayContaining(["js", "css", "png"]));
}, 360_000);

test("backend starts with a private runtime config instead of an immutable store config", () => {
  const { processes } = evaluate(["processes"], true);
  const runtime = mkdtempSync(join(tmpdir(), "lific-backend-config-"));
  try {
    const bin = join(runtime, "bin");
    mkdirSync(bin);
    const capture = join(runtime, "arguments");
    const cargo = join(bin, "cargo");
    writeFileSync(cargo, '#!/bin/sh\nprintf "%s\\n" "$@" > "$LIFIC_TEST_ARGUMENTS"\n');
    chmodSync(cargo, 0o700);
    const result = Bun.spawnSync(["bash", "-e", "-c", processes.backend.exec], {
      env: {
        ...process.env,
        PATH: `${bin}:${process.env.PATH}`,
        DEVENV_RUNTIME: runtime,
        LIFIC_TEST_ARGUMENTS: capture,
        LIFIC_DEV_PORT: "3456",
      },
    });
    expect(result.exitCode, result.stderr.toString()).toBe(0);
    const args = readFileSync(capture, "utf8").trim().split("\n");
    const configPath = args[args.indexOf("--config") + 1];
    expect(configPath.startsWith(`${runtime}/`)).toBe(true);
    expect(statSync(configPath).mode & 0o777).toBe(0o600);
    const config = readFileSync(configPath, "utf8");
    expect(config).toContain('host = "127.0.0.1"');
    expect(config).toContain("required = true");
    expect(config).toContain("enabled = false");
    expect(args[args.indexOf("--db") + 1].startsWith(`${runtime}/`)).toBe(true);
  } finally {
    rmSync(runtime, { recursive: true, force: true });
  }
}, 360_000);

test("the MSVC release profile owns its cross-linker environment", () => {
  const config = evaluate(
    ["languages.rust.targets", "env", "tasks"],
    false,
    "release-windows-msvc",
  );
  expect(config["languages.rust.targets"]).toEqual(["x86_64-pc-windows-msvc"]);
  expect(config.env.CC_x86_64_pc_windows_msvc).toContain("clang-cl");
  expect(config.env.AR_x86_64_pc_windows_msvc).toContain("llvm-lib");
  expect(config.env.CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER).toContain(
    "lific-msvc-linker",
  );
  expect(config.tasks["lific:release:x86_64-pc-windows-msvc"].after).toEqual([]);
  expect(config.tasks["lific:release:x86_64-pc-windows-msvc"].exec).toBe(
    "cargo build --locked --profile dist --target x86_64-pc-windows-msvc",
  );
}, 360_000);

test("packages use the selected compiler, release profile, and bounded sources", () => {
  const config = evaluate([
    "languages.rust.toolchainPackage.version",
    "languages.rust.toolchainPackage.outPath",
    "treefmt.config.programs.rustfmt.package.version",
    "outputs.lific.cargoBuildType",
    "outputs.lific.nativeBuildInputs",
    "outputs.lific.src.outPath",
  ]);
  expect(config["treefmt.config.programs.rustfmt.package.version"]).toBe(
    config["languages.rust.toolchainPackage.version"],
  );
  expect(config["outputs.lific.cargoBuildType"]).toBe("dist");
  expect(config["outputs.lific.nativeBuildInputs"]).toContain(
    config["languages.rust.toolchainPackage.outPath"],
  );
  for (const key of ["outputs.lific.src.outPath"]) {
    for (const path of [
      "target",
      ".devenv",
      "lific.db",
      "web",
    ]) {
      expect(existsSync(`${config[key]}/${path}`)).toBe(false);
    }
  }
  const source = config["outputs.lific.src.outPath"];
  expect(readFileSync(`${source}/Cargo.toml`, "utf8")).toBe(
    readFileSync("Cargo.toml", "utf8"),
  );
  expect(existsSync(`${source}/src/topcoat/assets/controls.css`)).toBe(true);
  expect(existsSync(`${source}/migrations`)).toBe(true);
}, 360_000);
