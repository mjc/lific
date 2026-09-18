{
  pkgs,
  config,
  ...
}:
let
  repoRoot = if config.git.root != null then config.git.root else builtins.toString ./.;
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
  chromiumRuntimePackages = with pkgs; [
    alsa-lib
    at-spi2-atk
    atk
    cairo
    cups
    dbus
    expat
    fontconfig
    freetype
    glib
    gtk3
    libdrm
    libxkbcommon
    libxshmfence
    mesa
    nspr
    nss
    pango
    wayland
    xorg.libX11
    xorg.libXcomposite
    xorg.libXdamage
    xorg.libXext
    xorg.libXfixes
    xorg.libXi
    xorg.libXrandr
    xorg.libXrender
    xorg.libXcursor
    xorg.libXtst
    xorg.libxcb
  ];
in
{
  languages.rust = {
    enable = true;
    channel = "stable";
  };

  languages.javascript = {
    enable = true;
    directory = "${repoRoot}/web";
    bun = {
      enable = true;
      install.enable = true;
    };
  };

  # devenv's JavaScript module supports one project directory per environment.
  # Profiles keep the other lockfiles on the same native Bun install lifecycle.
  profiles = {
    docs.module = {
      languages.javascript.directory = "${repoRoot}/site";
    };
    e2e.module = {
      languages.javascript.directory = "${repoRoot}/e2e";
      tasks."lific:web:check".after = [ "lific:install:web" ];
      packages = [ playwrightBrowsers ];
      env.PLAYWRIGHT_BROWSERS_PATH = "${playwrightBrowsers}";
      env.PLAYWRIGHT_EXECUTABLE_PATH = "${playwrightChromium}";
    };
    promo.module = {
      languages.javascript.directory = "${repoRoot}/promo";
      packages = pkgs.lib.optionals pkgs.stdenv.isLinux chromiumRuntimePackages;
      env.LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.isLinux (
        pkgs.lib.makeLibraryPath chromiumRuntimePackages
      );
    };
    release-linux.module = {
      languages.rust.targets = [
        "x86_64-unknown-linux-gnu"
        "aarch64-unknown-linux-gnu"
      ];
      unsetEnvVars = [
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER"
        "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"
      ];
      packages = with pkgs; [
        cargo-zigbuild
        zig
      ];
      tasks = {
        "lific:release:x86_64-unknown-linux-gnu" = {
          cwd = repoRoot;
          exec = "cargo zigbuild --locked --profile release-dist --target x86_64-unknown-linux-gnu";
          after = [ "lific:web:build" ];
        };
        "lific:release:aarch64-unknown-linux-gnu" = {
          cwd = repoRoot;
          exec = "cargo zigbuild --locked --profile release-dist --target aarch64-unknown-linux-gnu";
          after = [ "lific:web:build" ];
        };
      };
    };
    release-darwin.module = {
      languages.rust.targets = [
        "x86_64-apple-darwin"
        "aarch64-apple-darwin"
      ];
      tasks = {
        "lific:release:x86_64-apple-darwin" = {
          cwd = repoRoot;
          exec = "cargo build --locked --profile release-dist --target x86_64-apple-darwin";
          after = [ "lific:web:build" ];
        };
        "lific:release:aarch64-apple-darwin" = {
          cwd = repoRoot;
          exec = "cargo build --locked --profile release-dist --target aarch64-apple-darwin";
          after = [ "lific:web:build" ];
        };
      };
    };
  };

  treefmt = {
    enable = true;
    config.programs = {
      actionlint.enable = true;
      nixfmt.enable = true;
      rustfmt.enable = true;
      shfmt.enable = true;
    };
    config.settings.excludes = [
      "web/dist/*"
      "site/.next/*"
      "promo/out/*"
      "target/*"
    ];
  };

  outputs.treefmtCheck = config.treefmt.config.build.check (pkgs.lib.cleanSource ./.);

  packages = with pkgs; [
    curl
    git
  ];

  env.CARGO_TERM_COLOR = "always";
  env.RUST_BACKTRACE = "1";
  unsetEnvVars = [ "RUSTC_WRAPPER" ];
  enterTest = "devenv tasks run lific:check --mode before";
  tasks = {
    # The e2e profile must build the web project from a different JavaScript
    # directory before it can launch browser tests.
    "lific:install:web" = {
      cwd = "${repoRoot}/web";
      exec = "bun install --frozen-lockfile";
    };

    "lific:rust-test" = {
      cwd = repoRoot;
      exec = "cargo test --all-targets --locked";
      after = [ "lific:web:build" ];
    };
    "lific:web:check" = {
      cwd = "${repoRoot}/web";
      exec = "bun run check && bun test";
    };
    "lific:web:build" = {
      cwd = "${repoRoot}/web";
      exec = ''
        bun run build
        # Vite clears dist before writing the bundle; keep the tracked checkout
        # marker so the generated tree remains safe for native git hooks.
        touch dist/.gitkeep
      '';
      after = [ "lific:web:check" ];
    };
    "lific:docs:build" = {
      cwd = "${repoRoot}/site";
      exec = "bun run build";
    };
    "lific:docs:check" = {
      cwd = repoRoot;
      exec = "bun scripts/check-docs.mjs";
      after = [ "lific:docs:build" ];
    };
    "lific:promo:check" = {
      cwd = "${repoRoot}/promo";
      exec = "bun run lint";
    };
    "lific:promo:render" = {
      cwd = "${repoRoot}/promo";
      exec = "bunx remotion render BoardLoop ${repoRoot}/site/public/board-loop.mp4";
      after = [ "lific:promo:check" ];
    };
    "lific:release-test" = {
      cwd = repoRoot;
      exec = "bash scripts/verify-release-binary.test.sh";
    };
    "lific:check" = {
      after = [
        "lific:rust-test"
        "lific:release-test"
      ];
    };
    "lific:debug-build" = {
      cwd = repoRoot;
      exec = "cargo build --locked";
      after = [ "lific:web:build" ];
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
      after = [
        "lific:debug-build"
        "lific:install:web"
      ];
    };
  };

  processes = {
    backend = {
      exec = ''
        exec cargo run --locked -- \
          --db "$DEVENV_STATE/lific.db" \
          start --init-if-missing --host 127.0.0.1 \
          --port "$LIFIC_DEV_PORT"
      '';
      cwd = repoRoot;
      ports.http.allocate = 3456;
      env = {
        LIFIC_INIT_ADMIN_NAME = "Devenv";
        LIFIC_INIT_ADMIN_PASSWORD = "devenv-local-password";
        LIFIC_DEV_PORT = builtins.toString config.processes.backend.ports.http.value;
      };
      ready.http.get = {
        port = config.processes.backend.ports.http.value;
        path = "/api/health";
      };
      ready.period = 1;
      ready.timeout = 600;
      watch = {
        paths = [
          ./src
          ./Cargo.toml
          ./Cargo.lock
        ];
        extensions = [
          "rs"
          "toml"
          "lock"
        ];
        ignore = [ "target" ];
      };
    };
    frontend = {
      exec = "bun run dev";
      cwd = "${repoRoot}/web";
      ports.http.allocate = 5173;
      env.VITE_PORT = builtins.toString config.processes.frontend.ports.http.value;
      env.VITE_API_TARGET = "http://127.0.0.1:${builtins.toString config.processes.backend.ports.http.value}";
      after = [ "devenv:processes:backend@ready" ];
      ready.http.get = {
        port = config.processes.frontend.ports.http.value;
        path = "/";
      };
      ready.timeout = 30;
    };
  };

  git-hooks.hooks = {
    treefmt.enable = true;
    clippy = {
      enable = true;
      settings = {
        denyWarnings = true;
        offline = false;
        extraArgs = "--all-targets --locked";
      };
    };
  };
}
