{
  bun2nix,
  craneLib,
  lib,
  src,
  stdenvNoCC,
  supportedSystems,
}: let
  manifest = builtins.fromTOML (builtins.readFile (src + "/Cargo.toml"));
  inherit (manifest.package) version;

  webSrc = lib.fileset.toSource {
    root = src;
    fileset = lib.fileset.unions [
      (src + "/Cargo.toml")
      (src + "/web")
    ];
  };

  cargoSrc = lib.fileset.toSource {
    root = src;
    fileset = lib.fileset.unions [
      (src + "/Cargo.toml")
      (src + "/Cargo.lock")
      (src + "/build.rs")
      (src + "/migrations")
      (src + "/src")
    ];
  };

  frontend = stdenvNoCC.mkDerivation {
    pname = "lific-web";
    inherit version;
    src = webSrc;

    nativeBuildInputs = [bun2nix.hook];
    bunRoot = "web";
    bunDeps = bun2nix.fetchBunDeps {
      bunNix = src + "/web/bun.nix";
    };
    dontUseBunBuild = true;
    dontUseBunCheck = true;

    buildPhase = ''
      runHook preBuild
      cd web
      bun run build
      runHook postBuild
    '';

    installPhase = ''
      runHook preInstall
      mkdir -p $out
      cp -R dist/. $out/
      runHook postInstall
    '';
  };

  commonArgs = {
    pname = manifest.package.name;
    inherit version;
    src = cargoSrc;
    strictDeps = true;
    cargoExtraArgs = "--locked";
    postPatch = ''
      mkdir -p web/dist
    '';
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in
  craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      postPatch =
        commonArgs.postPatch
        + ''
          cp -R ${frontend}/. web/dist/
        '';

      # The canonical Cargo CI runs the test suite. The Nix package verifies
      # its integrated frontend + release build and leaves project testing to
      # that existing workflow.
      doCheck = false;
      doInstallCheck = true;
      installCheckPhase = ''
        $out/bin/lific --version
      '';

      meta = {
        inherit (manifest.package) description;
        homepage = manifest.package.repository;
        license = lib.licenses.asl20;
        mainProgram = manifest.package.name;
        platforms = supportedSystems;
      };
    }
  )
