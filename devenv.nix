{lib, pkgs, config, ...}: let
  repoRoot = config.git.root;
  darwinTargets = lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
    "x86_64-apple-darwin"
    "aarch64-apple-darwin"
  ];
  playwrightBrowsers = pkgs.playwright-driver.browsers.override {
    withChromium = true;
    withChromiumHeadlessShell = true;
    withFirefox = false;
    withWebkit = false;
    withFfmpeg = false;
  };
  playwrightChromium = pkgs.writeShellScript "lific-playwright-chromium" ''
    set -euo pipefail
    for executable in \
      "$PLAYWRIGHT_BROWSERS_PATH"/chromium-*/chrome-linux*/chrome \
      "$PLAYWRIGHT_BROWSERS_PATH"/chromium-*/chrome-mac*/Google\ Chrome\ for\ Testing.app/Contents/MacOS/Google\ Chrome\ for\ Testing; do
      if [ -x "$executable" ]; then
        exec "$executable" "$@"
      fi
    done
    echo "devenv Chromium executable not found" >&2
    exit 1
  '';
in {
  # The current Darwin SDK bundles CoreFoundation, Security, and the other
  # system frameworks. Legacy individual framework aliases were removed.
  apple.sdk = lib.mkIf pkgs.stdenv.hostPlatform.isDarwin pkgs.apple-sdk;

  languages.rust = {
    enable = true;
    channel = "stable";
    components = ["rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src"];
    targets = ["aarch64-unknown-linux-gnu"] ++ darwinTargets;
  };

  packages = with pkgs; [
    bun
    cargo-zigbuild
    curl
    git
    zig
  ];

  env.CARGO_TERM_COLOR = "always";
  env.RUST_BACKTRACE = "1";
  # Keep release artifacts independent of devenv's optional compiler cache and
  # its development linker flags. cargo-zigbuild supplies the release linker.
  env.RUSTC_WRAPPER = "";

  scripts = {
    lific-install = {
      exec = ''
        set -euo pipefail
        root="''${DEVENV_ROOT:?enter the devenv shell first}"
        for directory in web e2e site promo; do
          (cd "$root/$directory" && bun install --frozen-lockfile)
        done
      '';
      description = "Install all Bun workspace dependencies from lockfiles";
    };

    lific-rust-check = {
      exec = ''
        set -euo pipefail
        cd "''${DEVENV_ROOT:?enter the devenv shell first}"
        cargo fmt --all -- --check
        cargo clippy --all-targets --locked -- -D warnings
        cargo test --all-targets --locked
      '';
      description = "Run Rust formatting, lint, and test checks";
    };

    lific-web-check = {
      exec = ''
        set -euo pipefail
        (cd "''${DEVENV_ROOT:?enter the devenv shell first}/web" && bun run check && bun test)
      '';
      description = "Run Svelte typechecks and frontend unit tests";
    };

    lific-web-build = {
      exec = ''
        set -euo pipefail
        (cd "''${DEVENV_ROOT:?enter the devenv shell first}/web" && bun run build)
      '';
      description = "Build the frontend embedded by the Rust binary";
    };

    lific-docs-check = {
      exec = ''
        set -euo pipefail
        root="''${DEVENV_ROOT:?enter the devenv shell first}"
        (cd "$root/site" && bun run build)
        (cd "$root" && bun scripts/check-docs.mjs)
      '';
      description = "Build the docs site and check navigation, links, and MCP count";
    };

    lific-e2e = {
      exec = ''
        set -euo pipefail
        root="''${DEVENV_ROOT:?enter the devenv shell first}"
        test -d "''${PLAYWRIGHT_BROWSERS_PATH:-/nonexistent}" || {
          echo "enter the e2e profile before running browser tests" >&2
          exit 1
        }
        test -x "$root/target/debug/lific" || {
          echo "build target/debug/lific before running E2E tests" >&2
          exit 1
        }
        test -f "$root/web/dist/index.html" || {
          echo "run lific-web-build before running E2E tests" >&2
          exit 1
        }
        (cd "$root/e2e" && bun run smoke)
        (cd "$root/e2e" && bun run archives)
        (cd "$root/e2e" && bun run public)
        (cd "$root/e2e" && bun run sidebar && bun run mobile-nav && bun run context-menu)
      '';
      description = "Run browser smoke, archive, public, sidebar, mobile, and context-menu suites";
    };

    lific-verify-release = {
      exec = ''
        root="''${DEVENV_ROOT:?enter the devenv shell first}"
        bash "$root/scripts/verify-release-binary.sh" "$@"
      '';
      description = "Smoke-test a native release binary and its embedded web UI";
    };

    lific-check = {
      exec = ''
        devenv tasks run lific:check
      '';
      description = "Run the complete non-browser CI check";
    };

    lific-build-release = {
      exec = ''
        set -euo pipefail
        root="''${DEVENV_ROOT:?enter the devenv shell first}"
        (cd "$root/web" && bun run build)
        target="''${1:-$(rustc -vV | sed -n 's/^host: //p')}"
        case "$target" in
          x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)
            (cd "$root" && env \
              -u CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER \
              -u CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER \
              cargo zigbuild --locked --profile release-dist --target "$target")
            ;;
          x86_64-apple-darwin|aarch64-apple-darwin)
            (cd "$root" && cargo build --locked --profile release-dist --target "$target")
            ;;
          x86_64-pc-windows-msvc)
            echo "Windows MSVC release builds are tracked separately; use the Windows CI job" >&2
            exit 2
            ;;
          *)
            echo "unsupported release target: $target" >&2
            exit 2
            ;;
        esac
      '';
      description = "Build a locked release-dist binary with the frontend embedded";
    };

    lific-start = {
      exec = ''
        set -euo pipefail
        root="''${DEVENV_ROOT:?enter the devenv shell first}"
        state="''${DEVENV_STATE:-$root/.devenv/state}"
        instance="$state/lific"
        config="$instance/lific.toml"
        db="$instance/lific.db"
        mkdir -p "$instance"
        if [ ! -f "$config" ] || [ ! -f "$db" ]; then
          cargo run --manifest-path "$root/Cargo.toml" --locked -- \
            --config "$config" --db "$db" init --no-service \
            --name "''${LIFIC_DEV_ADMIN_NAME:-Devenv}" \
            --auth-mode passwords \
            --password "''${LIFIC_DEV_ADMIN_PASSWORD:-devenv-local-password}"
        fi
        cd "$root"
        cargo run --locked -- --config "$config" --db "$db" start \
          --host 127.0.0.1 --port "''${LIFIC_DEV_PORT:-3456}" "$@"
      '';
      description = "Start the local Lific server";
    };

    lific-web-dev = {
      exec = ''
        set -euo pipefail
        root="''${DEVENV_ROOT:?enter the devenv shell first}"
        if [ ! -d "$root/web/node_modules" ]; then
          (cd "$root/web" && bun install --frozen-lockfile)
        fi
        (cd "$root/web" && bun run dev)
      '';
      description = "Start the Vite development server";
    };
  };

  tasks = {
    "lific:install" = {exec = "lific-install";};
    "lific:rust-check" = {
      exec = "lific-rust-check";
      after = ["lific:install"];
    };
    "lific:web-check" = {
      exec = "lific-web-check";
      after = ["lific:install"];
    };
    "lific:web-build" = {
      exec = "lific-web-build";
      after = ["lific:web-check"];
    };
    "lific:docs-check" = {
      exec = "lific-docs-check";
      after = ["lific:install"];
    };
    "lific:check" = {
      exec = ":";
      after = ["lific:rust-check" "lific:web-build" "lific:docs-check"];
    };
    "lific:debug-build" = {
      exec = ''
        cd "''${DEVENV_ROOT:?enter the devenv shell first}"
        cargo build --locked
      '';
      after = ["lific:web-build"];
    };
    "lific:e2e" = {
      exec = "lific-e2e";
      after = ["lific:debug-build"];
    };
    "lific:release" = {
      exec = "lific-build-release";
      after = ["lific:web-build"];
    };
  };

  processes = {
    backend = {
      exec = "lific-start";
      cwd = repoRoot;
      ports.http.allocate = 3456;
      env.LIFIC_DEV_PORT = builtins.toString config.processes.backend.ports.http.value;
      ready.http.get = {
        port = config.processes.backend.ports.http.value;
        path = "/api/health";
      };
    };
    frontend = {
      exec = "lific-web-dev";
      cwd = repoRoot;
      env.VITE_API_TARGET = "http://127.0.0.1:${builtins.toString config.processes.backend.ports.http.value}";
      after = ["devenv:processes:backend@ready"];
    };
  };

  profiles.e2e.module = {
    packages = [playwrightBrowsers];
    env.PLAYWRIGHT_BROWSERS_PATH = "${playwrightBrowsers}";
    env.PLAYWRIGHT_EXECUTABLE_PATH = "${playwrightChromium}";
  };

  git-hooks.hooks = {
    cargo-fmt = {
      enable = true;
      entry = "cargo fmt --all -- --check";
      language = "system";
      pass_filenames = false;
      types = ["rust"];
    };
    cargo-clippy = {
      enable = true;
      entry = "cargo clippy --all-targets --locked -- -D warnings";
      language = "system";
      pass_filenames = false;
      types = ["rust"];
    };
  };

  enterTest = ''
    set -euo pipefail
    command -v cargo >/dev/null
    command -v rustfmt >/dev/null
    command -v cargo-clippy >/dev/null
    command -v bun >/dev/null
    if [ "$(uname -s)" = Darwin ]; then
      command -v xcrun >/dev/null
      xcrun --find clang >/dev/null
    fi
  '';
}
