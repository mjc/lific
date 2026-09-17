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
    targets = [
      "x86_64-unknown-linux-gnu"
      "aarch64-unknown-linux-gnu"
    ] ++ darwinTargets;
  };

  languages.javascript = {
    enable = true;
    bun.enable = true;
  };

  packages = with pkgs; [
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
  };

  tasks = {
    "lific:state" = {
      cwd = repoRoot;
      exec = ''
        mkdir -p "$DEVENV_STATE/lific"
        test -f "$DEVENV_STATE/lific/lific.toml" || : > "$DEVENV_STATE/lific/lific.toml"
      '';
      status = "test -f \"$DEVENV_STATE/lific/lific.toml\"";
    };

    "lific:install:web" = {
      cwd = "${repoRoot}/web";
      exec = "bun install --frozen-lockfile";
      execIfModified = ["package.json" "bun.lock"];
    };
    "lific:install:e2e" = {
      cwd = "${repoRoot}/e2e";
      exec = "bun install --frozen-lockfile";
      execIfModified = ["package.json" "bun.lock"];
    };
    "lific:install:site" = {
      cwd = "${repoRoot}/site";
      exec = "bun install --frozen-lockfile";
      execIfModified = ["package.json" "bun.lock"];
    };
    "lific:install:promo" = {
      cwd = "${repoRoot}/promo";
      exec = "bun install --frozen-lockfile";
      execIfModified = ["package.json" "bun.lock"];
    };
    "lific:install" = {
      after = [
        "lific:install:web"
        "lific:install:e2e"
        "lific:install:site"
        "lific:install:promo"
      ];
    };

    "lific:rust-check" = {
      cwd = repoRoot;
      exec = ''
        cargo fmt --all -- --check
        cargo clippy --all-targets --locked -- -D warnings
        cargo test --all-targets --locked
      '';
    };
    "lific:web:check" = {
      cwd = "${repoRoot}/web";
      exec = "bun run check && bun test";
      after = ["lific:install:web"];
    };
    "lific:web:build" = {
      cwd = "${repoRoot}/web";
      exec = "bun run build";
      after = ["lific:web:check"];
    };
    "lific:docs:build" = {
      cwd = "${repoRoot}/site";
      exec = "bun run build";
      after = ["lific:install:site"];
    };
    "lific:docs:check" = {
      cwd = repoRoot;
      exec = "bun scripts/check-docs.mjs";
      after = ["lific:docs:build"];
    };
    "lific:release-test" = {
      cwd = repoRoot;
      exec = "bash scripts/verify-release-binary.test.sh";
    };
    "lific:check" = {
      after = ["lific:rust-check" "lific:web:build" "lific:docs:check" "lific:release-test"];
    };
    "lific:debug-build" = {
      cwd = repoRoot;
      exec = "cargo build --locked";
      after = ["lific:web:build"];
    };
    "lific:e2e" = {
      cwd = "${repoRoot}/e2e";
      exec = ''
        bun run smoke
        bun run archives
        bun run public
        bun run sidebar
        bun run mobile-nav
        bun run context-menu
      '';
      after = ["lific:debug-build" "lific:install:e2e"];
    };
    "lific:release" = {
      cwd = repoRoot;
      exec = "lific-build-release";
      after = ["lific:web:build"];
    };
  };

  processes = {
    backend = {
      exec = ''
        exec env \
          LIFIC_INIT_ADMIN_NAME="''${LIFIC_DEV_ADMIN_NAME:-Devenv}" \
          LIFIC_INIT_ADMIN_PASSWORD="''${LIFIC_DEV_ADMIN_PASSWORD:-devenv-local-password}" \
          cargo run --locked -- \
            --config "$DEVENV_STATE/lific/lific.toml" \
            --db "$DEVENV_STATE/lific/lific.db" \
            start --init-if-missing --host 127.0.0.1 \
            --port "''${LIFIC_DEV_PORT:-3456}"
      '';
      cwd = repoRoot;
      ports.http.allocate = 3456;
      env.LIFIC_DEV_PORT = builtins.toString config.processes.backend.ports.http.value;
      ready.http.get = {
        port = config.processes.backend.ports.http.value;
        path = "/api/health";
      };
      ready.period = 1;
      ready.timeout = 600;
      after = ["lific:state"];
      watch = {
        paths = [./src ./Cargo.toml ./Cargo.lock];
        extensions = ["rs" "toml" "lock"];
        ignore = ["target"];
      };
    };
    frontend = {
      exec = "bun run dev";
      cwd = "${repoRoot}/web";
      env.VITE_API_TARGET = "http://127.0.0.1:${builtins.toString config.processes.backend.ports.http.value}";
      after = ["lific:install:web" "devenv:processes:backend@ready"];
      ready.http.get = {
        port = 5173;
        path = "/";
      };
      ready.timeout = 30;
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
