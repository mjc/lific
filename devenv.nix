{lib, pkgs, ...}: let
  hostTarget = builtins.getAttr pkgs.system {
    "x86_64-linux" = "x86_64-unknown-linux-gnu";
    "aarch64-linux" = "aarch64-unknown-linux-gnu";
    "x86_64-darwin" = "x86_64-apple-darwin";
    "aarch64-darwin" = "aarch64-apple-darwin";
  };
  darwinTargets = lib.optionals pkgs.stdenv.isDarwin [
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
  ];
  rustToolchain = pkgs.rust-bin.stable.latest.default.override {
    extensions = ["rust-src" "rust-analyzer" "clippy" "rustfmt"];
    targets = lib.unique ([hostTarget "aarch64-unknown-linux-gnu"] ++ darwinTargets);
  };
  darwinFrameworks = lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
    CoreFoundation
    Security
    SystemConfiguration
  ]);
in {
  packages = (with pkgs; [
    bun
    curl
    git
    pre-commit
    rustToolchain
  ]) ++ darwinFrameworks;

  env.CARGO_TERM_COLOR = "always";
  env.RUST_BACKTRACE = "1";
  env.RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
  env.RUSTC_WRAPPER = "";

  enterShell = ''
    export PATH="${rustToolchain}/bin:$PATH"
  '';

  scripts = {
    lific-install = {
      exec = ''
        set -euo pipefail
        for directory in web e2e site promo; do
          (cd "$directory" && bun install --frozen-lockfile)
        done
      '';
      description = "Install all Bun workspace dependencies from lockfiles";
    };

    lific-rust-check = {
      exec = ''
        set -euo pipefail
        cargo fmt --all -- --check
        cargo clippy --all-targets --locked -- -D warnings
        cargo test --all-targets --locked
      '';
      description = "Run Rust formatting, lint, and test checks";
    };

    lific-web-check = {
      exec = ''
        set -euo pipefail
        (cd web && bun run check && bun test)
      '';
      description = "Run Svelte typechecks and frontend unit tests";
    };

    lific-web-build = {
      exec = ''
        set -euo pipefail
        (cd web && bun run build)
      '';
      description = "Build the frontend embedded by the Rust binary";
    };

    lific-docs-check = {
      exec = ''
        set -euo pipefail
        (cd site && bun run build)
        bun scripts/check-docs.mjs
      '';
      description = "Build the docs site and check navigation, links, and MCP count";
    };

    lific-e2e = {
      exec = ''
        set -euo pipefail
        test -x target/debug/lific || {
          echo "build target/debug/lific before running E2E tests" >&2
          exit 1
        }
        test -f web/dist/index.html || {
          echo "run lific-web-build before running E2E tests" >&2
          exit 1
        }
        (cd e2e && bunx playwright install chromium)
        (cd e2e && bun run smoke)
        (cd e2e && bun run archives)
        (cd e2e && bun run public)
        (cd e2e && bun run sidebar && bun run mobile-nav && bun run context-menu)
      '';
      description = "Run browser smoke, archive, public, sidebar, mobile, and context-menu suites";
    };

    lific-verify-release = {
      exec = ''
        bash scripts/verify-release-binary.sh "$@"
      '';
      description = "Smoke-test a native release binary and its embedded web UI";
    };

    lific-check = {
      exec = ''
        set -euo pipefail
        lific-install
        lific-rust-check
        lific-web-check
        lific-web-build
        lific-docs-check
      '';
      description = "Run the complete non-browser CI check";
    };

    lific-build-release = {
      exec = ''
        set -euo pipefail
        (cd web && bun run build)
        if [ "$#" -eq 0 ]; then
          cargo build --locked --profile release-dist
        elif [ "$1" = "aarch64-unknown-linux-gnu" ] && command -v cargo-zigbuild >/dev/null; then
          cargo zigbuild --locked --profile release-dist --target "$1"
        else
          cargo build --locked --profile release-dist --target "$1"
        fi
      '';
      description = "Build a locked release-dist binary with the frontend embedded";
    };

    lific-start = {
      exec = ''
        cargo run -- start --host 127.0.0.1 --port 3456 "$@"
      '';
      description = "Start the local Lific server";
    };

    lific-web-dev = {
      exec = ''
        (cd web && bun run dev)
      '';
      description = "Start the Vite development server";
    };
  };

  profiles.cross.module = {pkgs, ...}: {
    packages = with pkgs; [cargo-zigbuild zig];
  };

  enterTest = ''
    set -euo pipefail
    command -v cargo >/dev/null
    command -v rustfmt >/dev/null
    command -v cargo-clippy >/dev/null
    command -v bun >/dev/null
    command -v pre-commit >/dev/null
    if [ "$(uname -s)" = Darwin ]; then
      command -v xcrun >/dev/null
      xcrun --find clang >/dev/null
    fi
  '';
}
