{
  pkgs,
  config,
  lib,
  ...
}:
let
  repoRoot = if config.git.root != null then config.git.root else builtins.toString ./.;
  lificVersion = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
  lificSource = lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      lib.cleanSourceFilter path type
      && !(lib.hasPrefix "${toString ./web/dist}" (toString path))
      && !(lib.hasInfix "/node_modules/" (toString path));
  };
  webSource = lib.cleanSourceWith {
    src = ./web;
    filter =
      path: type:
      lib.cleanSourceFilter path type
      && !(lib.hasPrefix "${toString ./web/dist}" (toString path))
      && !(lib.hasInfix "/node_modules/" (toString path));
  };
  webBundle = pkgs.buildNpmPackage {
    pname = "lific-web";
    version = lificVersion;
    # The browser bundle must be reproducible from source, never copied from
    # a developer's checkout.
    src = webSource;
    npmDepsHash = "sha256-eunc7N/rWE4fPxquVGD6sR0GTwBJhIxO56zqnt+P5L4=";
    npmBuildScript = "build";
    installPhase = ''
      runHook preInstall
      mkdir -p "$out"
      cp -R dist/. "$out/"
      runHook postInstall
    '';
  };
  lificPackage = pkgs.rustPlatform.buildRustPackage {
    pname = "lific";
    version = lificVersion;
    src = lificSource;
    cargoLock.lockFile = ./Cargo.lock;
    # This is the derivation's private source copy. The checkout is never
    # modified: every release package embeds the UI built by webBundle.
    postPatch = ''
      mkdir -p web
      rm -rf web/dist
      cp -R ${webBundle} web/dist
    '';
    doCheck = false;
  };
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
    version = "1.88.0";
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
  # Each profile adds an explicit frozen install prerequisite so direct task
  # invocations are reproducible without relying on shell entry.
  profiles = {
    docs.module = {
      languages.javascript.directory = "${repoRoot}/site";
      tasks = {
        "lific:install:site" = {
          cwd = "${repoRoot}/site";
          exec = "bun install --frozen-lockfile";
        };
        "lific:docs:build" = {
          cwd = "${repoRoot}/site";
          exec = "bun run build";
          after = [ "lific:install:site" ];
        };
        "lific:docs:check" = {
          cwd = repoRoot;
          exec = "bun scripts/check-docs.mjs";
          after = [ "lific:docs:build" ];
        };
      };
    };
    e2e.module = {
      languages.javascript.directory = "${repoRoot}/e2e";
      packages = [ playwrightBrowsers ];
      env.PLAYWRIGHT_BROWSERS_PATH = "${playwrightBrowsers}";
      env.PLAYWRIGHT_EXECUTABLE_PATH = "${playwrightChromium}";
      tasks = {
        "lific:install:e2e" = {
          cwd = "${repoRoot}/e2e";
          exec = "bun install --frozen-lockfile";
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
            "lific:install:e2e"
          ];
        };
      };
    };
    promo.module = {
      languages.javascript.directory = "${repoRoot}/promo";
      packages = pkgs.lib.optionals pkgs.stdenv.isLinux chromiumRuntimePackages;
      env.LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.isLinux (
        pkgs.lib.makeLibraryPath chromiumRuntimePackages
      );
      tasks = {
        "lific:install:promo" = {
          cwd = "${repoRoot}/promo";
          exec = "bun install --frozen-lockfile";
        };
        "lific:promo:check" = {
          cwd = "${repoRoot}/promo";
          exec = "bun run lint";
          after = [ "lific:install:promo" ];
        };
        "lific:promo:render" = {
          cwd = "${repoRoot}/promo";
          exec = "bunx remotion render BoardLoop ${repoRoot}/site/public/board-loop.mp4";
          after = [ "lific:promo:check" ];
        };
      };
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
    release-windows.module = {
      languages.rust.targets = [ "x86_64-pc-windows-gnu" ];
      unsetEnvVars = [ "CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER" ];
      packages = with pkgs; [
        cargo-zigbuild
        zig
      ];
      tasks."lific:release:x86_64-pc-windows-gnu" = {
        cwd = repoRoot;
        exec = "cargo zigbuild --locked --profile release-dist --target x86_64-pc-windows-gnu";
        after = [ "lific:web:build" ];
      };
    };
  };

  treefmt = {
    enable = true;
    config.programs = {
      actionlint.enable = true;
      nixfmt.enable = true;
      prettier.enable = true;
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

  outputs = {
    lific = lificPackage;
    web = webBundle;
    treefmtCheck = config.treefmt.config.build.check (pkgs.lib.cleanSource ./.);
  };

  packages = with pkgs; [
    curl
    file
    git
  ];

  env.CARGO_TERM_COLOR = "always";
  env.RUST_BACKTRACE = "1";
  unsetEnvVars = [ "RUSTC_WRAPPER" ];
  tasks = {
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
      after = [ "lific:install:web" ];
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
    "lific:release-test" = {
      cwd = repoRoot;
      exec = "bash scripts/verify-release-binary.test.sh";
    };
    "lific:publish" = {
      cwd = repoRoot;
      exec = ''
        set -o pipefail
        if out="$(cargo publish --locked --allow-dirty --no-verify 2>&1)"; then
          echo "$out"
        else
          echo "$out"
          if printf '%s\n' "$out" | grep -qiE 'crate version .* is already uploaded'; then
            echo "::notice::Version already on crates.io; treating as success."
          else
            echo "::error::cargo publish failed."
            exit 1
          fi
        fi
      '';
      after = [ "lific:web:build" ];
    };
    "lific:check" = {
      before = [ "devenv:enterTest" ];
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
  };

  processes = lib.mkIf (!config.devenv.isTesting) {
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
          ./migrations
          ./Cargo.toml
          ./Cargo.lock
        ];
        extensions = [
          "rs"
          "toml"
          "lock"
          "sql"
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
