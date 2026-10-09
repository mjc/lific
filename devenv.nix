{
  pkgs,
  config,
  lib,
  inputs,
  ...
}:
let
  repoRoot = if config.git.root != null then config.git.root else builtins.toString ./.;
  manifest = builtins.fromTOML (builtins.readFile ./Cargo.toml);
  lificVersion = manifest.package.version;
  topcoatDependency = manifest.dependencies.topcoat;
  topcoatFormatTargets = "src/server.rs src/topcoat/native src/topcoat/runtime";
  # An explicit config prevents local development and `devenv test` from
  # inheriting an operator's production URL, credentials, or backup location.
  devConfig = pkgs.writeText "lific-dev.toml" ''
    [server]
    host = "127.0.0.1"
    port = ${toString config.processes.backend.ports.http.value}
    public_url = "http://127.0.0.1:${toString config.processes.backend.ports.http.value}"

    [auth]
    required = true
    allow_signup = false

    [backup]
    enabled = false
  '';
  msvcPkgs = import inputs.nixpkgs {
    system = pkgs.stdenv.hostPlatform.system;
    config = {
      allowUnfreePredicate =
        pkg:
        builtins.elem (lib.getName pkg) [
          "win-sdk"
          "xwin-fetch-msvc"
        ];
      microsoftVisualStudioLicenseAccepted = true;
    };
  };
  msvcSdk = msvcPkgs.pkgsCross.x86_64-windows.windows.sdk;
  msvcClang = pkgs.llvmPackages.clang-unwrapped;
  msvcLlvm = pkgs.llvmPackages.llvm;
  msvcLld = pkgs.llvmPackages.lld;
  msvcCompiler = pkgs.writeShellScript "lific-msvc-clang-cl" ''
    exec ${msvcClang}/bin/clang-cl \
      --target=x86_64-pc-windows-msvc \
      /vctoolsdir ${msvcSdk}/crt \
      /winsdkdir ${msvcSdk}/sdk \
      "$@"
  '';
  msvcLinker = pkgs.writeShellScript "lific-msvc-linker" ''
    exec ${msvcLld}/bin/lld-link \
      /libpath:${msvcSdk}/crt/lib/x64 \
      /libpath:${msvcSdk}/sdk/lib/um/x64 \
      /libpath:${msvcSdk}/sdk/lib/ucrt/x64 \
      "$@"
  '';
  rustPlatform = pkgs.makeRustPlatform {
    cargo = config.languages.rust.toolchainPackage;
    rustc = config.languages.rust.toolchainPackage;
  };
  source =
    paths:
    lib.fileset.toSource {
      root = ./.;
      fileset = lib.fileset.unions paths;
    };
  lificPackage = rustPlatform.buildRustPackage {
    pname = "lific";
    version = lificVersion;
    src = source [
      ./Cargo.toml
      ./Cargo.lock
      ./build.rs
      ./src
      ./migrations
      ./LICENSE
      ./README.md
    ];
    cargoLock = {
      lockFile = ./Cargo.lock;
      outputHashes."topcoat-0.10.0" = "sha256-8PQwbTOtVQyw/q17off20+DeR12kp63vmo5e7YEs1RE=";
    };
    buildType = "dist";
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
  lockedBunInstall = workspace: ''
    lock="${config.devenv.state}/locks/lific-${workspace}-bun-install"
    mkdir -p "$(dirname "$lock")"
    deadline=$((SECONDS + 300))
    while ! mkdir "$lock" 2>/dev/null; do
      owner=""
      if [[ -f "$lock/pid" ]]; then
        owner="$(<"$lock/pid")"
      fi
      if [[ -n "$owner" ]] && ! kill -0 "$owner" 2>/dev/null; then
        rm -f "$lock/pid"
        rmdir "$lock" 2>/dev/null || true
        continue
      fi
      if (( SECONDS >= deadline )); then
        echo "timed out waiting for the ${workspace} Bun install lock" >&2
        exit 1
      fi
      sleep 1
    done
    printf '%s\n' "$$" > "$lock/pid"
    cleanup() {
      rm -f "$lock/pid"
      rmdir "$lock" 2>/dev/null || true
    }
    trap cleanup EXIT INT TERM
    bun install --frozen-lockfile
  '';
  browserProfile = {
    languages.javascript.directory = "${repoRoot}/e2e";
    packages = [ playwrightBrowsers pkgs.ffmpeg ];
    env.PLAYWRIGHT_BROWSERS_PATH = "${playwrightBrowsers}";
    env.PLAYWRIGHT_EXECUTABLE_PATH = "${playwrightChromium}";
    tasks = {
      "lific:install:e2e" = {
        cwd = "${repoRoot}/e2e";
        exec = lockedBunInstall "e2e";
        before = [ "devenv:enterShell" ];
      };
      "lific:topcoat:e2e" = {
        after = [ "lific:e2e" ];
      };
      "lific:topcoat:main-e2e" = {
        cwd = repoRoot;
        exec = "node src/topcoat/tests/main/run.js browser";
        after = [ "lific:install:e2e" "lific:debug-build" ];
      };
      "lific:e2e" = {
        cwd = repoRoot;
        exec = ''
          set -e
          cargo test --locked controls_runtime_executes_control_handlers_from_the_shared_layout -- --include-ignored
          cargo test --locked native_home_ -- --include-ignored
          cargo test --locked native::project_overview:: -- --include-ignored
          node --test src/topcoat/acceptance/*.browser.test.js
          node --test src/topcoat/visual_parity/*.browser.test.cjs
        '';
        after = [ "lific:install:e2e" "lific:debug-build" ];
      };
    };
  };
in
{
  languages.rust = {
    enable = true;
    channel = "stable";
    version = "1.99.0";
  };

  languages.javascript = {
    enable = true;
    directory = repoRoot;
    bun = {
      enable = true;
      # The native installer cannot enforce --frozen-lockfile. Use one task
      # per workspace for both shell entry and direct task invocations.
      install.enable = false;
    };
  };

  # devenv's JavaScript module supports one project directory per environment.
  # Each profile adds an explicit frozen install prerequisite so direct task
  # invocations are reproducible without relying on shell entry.
  profiles = {
    # Topcoat is the standard frontend. These names retain existing task entry points.
    topcoat.module = { };
    topcoat-e2e.module = browserProfile;
    docs.module = {
      # Documentation needs Bun, not the Rust toolchain or source hooks.
      languages.rust.enable = lib.mkForce false;
      treefmt.enable = lib.mkForce false;
      git-hooks.hooks.clippy.enable = lib.mkForce false;
      git-hooks.hooks.treefmt.enable = lib.mkForce false;
      languages.javascript.directory = "${repoRoot}/site";
    };
    e2e.module = browserProfile;
    promo.module = {
      languages.javascript.directory = "${repoRoot}/promo";
      packages = pkgs.lib.optionals pkgs.stdenv.isLinux chromiumRuntimePackages;
      env.LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.isLinux (
        pkgs.lib.makeLibraryPath chromiumRuntimePackages
      );
      tasks = {
        "lific:install:promo" = {
          cwd = "${repoRoot}/promo";
          exec = lockedBunInstall "promo";
          before = [ "devenv:enterShell" ];
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
      languages.zig = {
        enable = true;
        lsp.enable = false;
      };
      packages = [ pkgs.cargo-zigbuild ];
      tasks = {
        "lific:release:x86_64-unknown-linux-gnu" = {
          cwd = repoRoot;
          exec = "cargo zigbuild --locked --profile dist --target x86_64-unknown-linux-gnu";
        };
        "lific:release:aarch64-unknown-linux-gnu" = {
          cwd = repoRoot;
          exec = "cargo zigbuild --locked --profile dist --target aarch64-unknown-linux-gnu";
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
          exec = ''
            set -e
            cargo build --locked --profile dist --target x86_64-apple-darwin
            bash scripts/fix-macos-release-linkage.sh target/x86_64-apple-darwin/dist/lific
          '';
        };
        "lific:release:aarch64-apple-darwin" = {
          cwd = repoRoot;
          exec = ''
            set -e
            cargo build --locked --profile dist --target aarch64-apple-darwin
            bash scripts/fix-macos-release-linkage.sh target/aarch64-apple-darwin/dist/lific
          '';
        };
      };
    };
    release-windows-msvc.module = {
      languages.rust.targets = [ "x86_64-pc-windows-msvc" ];
      unsetEnvVars = [ "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER" ];
      packages = [
        msvcClang
        msvcLlvm
        msvcLld
        msvcSdk
      ];
      env = {
        CC_x86_64_pc_windows_msvc = msvcCompiler;
        CXX_x86_64_pc_windows_msvc = msvcCompiler;
        AR_x86_64_pc_windows_msvc = "${msvcLlvm}/bin/llvm-lib";
        CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = msvcLinker;
      };
      tasks."lific:release:x86_64-pc-windows-msvc" = {
        cwd = repoRoot;
        exec = "cargo build --locked --profile dist --target x86_64-pc-windows-msvc";
      };
    };
  };

  treefmt = {
    enable = true;
    config.programs = {
      actionlint.enable = true;
      # Enable the source formatters in the optional formatting baseline, not
      # as part of the Devenv migration.
      # nixfmt.enable = true;
      # prettier.enable = true;
      rustfmt.enable = true;
      rustfmt.package = config.languages.rust.toolchainPackage;
      # shfmt.enable = true;
    };
    config.settings.excludes = [
      "site/.next/*"
      "promo/out/*"
      "src/topcoat/vendor/*"
      "target/*"
    ];
  };

  outputs = {
    lific = lificPackage;
  };

  packages = with pkgs; [
    nodejs
    curl
    file
    git
  ];

  env.CARGO_TERM_COLOR = "always";
  env.RUST_BACKTRACE = "1";
  unsetEnvVars = [ "RUSTC_WRAPPER" ];
  tasks = {
    # Devenv traverses dependents as well as prerequisites on shell entry.
    # Only attach the test graph when actually running `devenv test`.
    "devenv:git-hooks:run" = {
      before = lib.mkForce (lib.optionals config.devenv.isTesting [ "devenv:enterTest" ]);
      after = lib.optionals config.devenv.isTesting [ "lific:topcoat:test" ];
    };
    "devenv:treefmt:run" = {
      # Formatting is explicit in development and checked before CI builds.
      # Shell entry must not silently repair a future formatting failure.
      before = lib.mkForce (lib.optionals config.devenv.isTesting [ "devenv:enterTest" ]);
      exec = lib.mkForce "treefmt --ci";
    };
    "lific:install:site" = {
      cwd = "${repoRoot}/site";
      exec = lockedBunInstall "site";
      before = lib.optionals (config.languages.javascript.directory == "${repoRoot}/site") [
        "devenv:enterShell"
      ];
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
    "lific:rust-test" = {
      cwd = repoRoot;
      exec = "cargo test --all-targets --locked";
      after = lib.optionals config.devenv.isTesting [ "devenv:treefmt:run" ];
    };
    "lific:topcoat:build" = {
      cwd = repoRoot;
      exec = "cargo build --locked";
    };
    "lific:install:styles" = {
      cwd = "${repoRoot}/scripts/tailwind";
      exec = "bun install --frozen-lockfile";
    };
    "lific:topcoat:styles" = {
      cwd = "${repoRoot}/scripts/tailwind";
      exec = "bun run build";
      after = [ "lific:install:styles" ];
    };
    "lific:topcoat:styles-check" = {
      cwd = "${repoRoot}/scripts/tailwind";
      exec = ''
        set -eu
        generated_styles=$(mktemp)
        trap 'rm -f "$generated_styles"' EXIT
        bunx --no-install @tailwindcss/cli -i input.css -o "$generated_styles" --minify
        cmp ../../src/topcoat/assets/tailwind.css "$generated_styles"
      '';
      after = [ "lific:install:styles" ];
    };
    "lific:topcoat:runtime-test" = {
      cwd = repoRoot;
      exec = "cargo test --locked --manifest-path src/topcoat/vendor/topcoat-runtime/Cargo.toml --features router --target-dir target";
      after = [ "lific:rust-test" ];
    };
    "lific:topcoat:test" = {
      cwd = repoRoot;
      exec = ''
        set -e
        node --test src/topcoat/assets/controls.test.mjs
        node --test src/topcoat/native/transport.test.cjs
        node --test src/topcoat/native/session_change_transport.test.cjs
        cargo test --locked native_activity_rate_
      '';
    };
    "lific:topcoat:main-test" = {
      cwd = repoRoot;
      exec = "node src/topcoat/tests/main/run.js unit";
    };
    "lific:topcoat:install-cli" = {
      cwd = repoRoot;
      exec = "cargo install --locked --git ${topcoatDependency.git} --rev ${topcoatDependency.rev} topcoat-cli";
    };
    "lific:topcoat:fmt" = {
      cwd = repoRoot;
      exec = "${config.devenv.state}/cargo-install/bin/topcoat fmt --rustfmt ${topcoatFormatTargets}";
      after = [ "lific:topcoat:install-cli" ];
    };
    "lific:topcoat:fmt-check" = {
      cwd = repoRoot;
      exec = "${config.devenv.state}/cargo-install/bin/topcoat fmt --check --rustfmt ${topcoatFormatTargets}";
      after = [ "lific:topcoat:install-cli" ];
    };
    "lific:community-proxy:check" = {
      cwd = repoRoot;
      exec = "bun test ./deploy/community-redirect/worker.test.mjs";
    };
    "lific:release-test" = {
      cwd = repoRoot;
      exec = "bash scripts/verify-release-binary.test.sh";
    };
    "lific:devenv-test" = {
      cwd = repoRoot;
      exec = "bun test scripts/devenv.test.ts";
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
    };
    "lific:check" = {
      before = lib.optionals config.devenv.isTesting [ "devenv:enterTest" ];
      after = [
        "lific:rust-test"
        "lific:topcoat:runtime-test"
        "lific:topcoat:test"
        "lific:topcoat:styles-check"
        "lific:release-test"
        "lific:community-proxy:check"
        "lific:devenv-test"
      ];
    };
    "lific:debug-build" = {
      cwd = repoRoot;
      exec = "cargo build --locked";
    };
  };

  processes = {
    backend = {
      exec = ''
        set -e
        # Startup tightens config permissions. Keep the immutable template in
        # the store and give the backend its own owner-readable runtime copy.
        dev_config="$(mktemp "$DEVENV_RUNTIME/lific-dev.XXXXXX.toml")"
        install -m 600 ${devConfig} "$dev_config"
        ${lib.optionalString config.devenv.isTesting ''
          # Devenv owns the test process lifetime; the database lives in its
          # isolated runtime directory, never in the developer's state.
          export LIFIC_DEV_DB="$(mktemp -d "$DEVENV_RUNTIME/lific-test.XXXXXX")/lific.db"
        ''}
        exec cargo run --locked -- \
          --config "$dev_config" \
          --db "$LIFIC_DEV_DB" \
          start --init-if-missing --host 127.0.0.1 \
          --port "$LIFIC_DEV_PORT"
      '';
      cwd = repoRoot;
      after = [ "lific:debug-build" ];
      ports.http.allocate = 3456;
      env = {
        LIFIC_INIT_ADMIN_NAME = "Devenv";
        LIFIC_INIT_ADMIN_PASSWORD = "devenv-local-password";
        LIFIC_DEV_PORT = builtins.toString config.processes.backend.ports.http.value;
        LIFIC_DEV_DB = "${config.devenv.state}/lific.db";
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
          ./build.rs
        ];
        extensions = [
          "rs"
          "toml"
          "lock"
          "sql"
          "js"
          "css"
          "png"
          "webmanifest"
        ];
        ignore = [ "target" ];
      };
    };
  };

  enterTest = ''
    wait_for_processes 60
    curl --fail --silent --show-error --connect-timeout 1 --max-time 5 \
      http://127.0.0.1:${toString config.processes.backend.ports.http.value}/api/health
  '';

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
