{
  nixpkgs,
  pkgs,
  self,
  system,
}:
let
  inherit (nixpkgs) lib;
  backupDir = "/var/cache/lific backups";
  externalConfig = "/run/secrets/lific.toml";
  credentialConfig = "/run/credentials/lific.service/lific.toml";

  evaluate =
    service:
    lib.nixosSystem {
      inherit system;
      modules = [
        self.nixosModules.default
        {
          system.stateVersion = "26.05";
          services.lific = {
            enable = true;
          }
          // service;
        }
      ];
    };

  failedAssertions =
    module:
    builtins.filter (
      item: !item.assertion && lib.hasPrefix "services.lific." item.message
    ) module.config.assertions;

  invalidModule = evaluate {
    configFile = externalConfig;
    settings = {
      log.level = "debug";
      server.mcp_path_token = "not-for-the-nix-store";
    };
  };
  relativeConfigModule = evaluate {
    configFile = "lific.toml";
  };
  generatedModule = evaluate {
    settings.backup.dir = backupDir;
  };
  externalModule = evaluate {
    configFile = externalConfig;
  };

  generatedService = generatedModule.config.systemd.services.lific.serviceConfig;
  externalService = externalModule.config.systemd.services.lific.serviceConfig;
in
assert lib.any (item: lib.hasInfix "mutually exclusive" item.message) (
  failedAssertions invalidModule
);
assert lib.any (item: lib.hasInfix "mcp_path_token" item.message) (failedAssertions invalidModule);
assert lib.any (item: lib.hasInfix "configFile must be an absolute path" item.message) (
  failedAssertions relativeConfigModule
);
assert failedAssertions generatedModule == [ ];
assert failedAssertions externalModule == [ ];
assert externalService.LoadCredential == "lific.toml:${externalConfig}";
assert lib.hasInfix credentialConfig externalService.ExecStart;
assert lib.hasInfix "--db" externalService.ExecStart;
assert lib.hasInfix "/var/lib/lific/lific.db" externalService.ExecStart;
assert generatedService.StateDirectory == "lific";
assert generatedService.WorkingDirectory == "/var/lib/lific";
assert generatedService.ProtectSystem == "full";
assert !(generatedService ? ReadWritePaths);
pkgs.testers.runNixOSTest {
  name = "lific";

  nodes.machine = { pkgs, ... }: {
    imports = [ self.nixosModules.default ];
    system.stateVersion = "26.05";

    environment.etc."lific-test.toml" = {
      mode = "0400";
      user = "root";
      group = "root";
      text = ''
        [auth]
        required = false

        [backup]
        dir = "${backupDir}"
      '';
    };

    services.lific = {
      enable = true;
      configFile = "/etc/lific-test.toml";
    };

    systemd.tmpfiles.settings."10-lific-test".${backupDir}.d = {
      mode = "0750";
      user = "lific";
      group = "lific";
    };

    environment.systemPackages = [ pkgs.curl ];
  };

  testScript = ''
    import re

    machine.wait_for_unit("lific.service")
    machine.wait_for_open_port(3456)
    machine.succeed("curl --fail http://127.0.0.1:3456/api/auth/instance")

    index = machine.succeed("curl --fail http://127.0.0.1:3456/")
    asset = re.search(r'<script[^>]+src="([^"]+\.js)"', index)
    assert asset, index
    machine.succeed(f"curl --fail --output /tmp/lific-app.js http://127.0.0.1:3456{asset.group(1)}")
    machine.succeed("test -s /tmp/lific-app.js")

    machine.succeed("test -s /var/lib/lific/lific.db")
    machine.succeed("test -s /run/credentials/lific.service/lific.toml")
    machine.succeed("test $(stat -c %a /etc/lific-test.toml) = 400")
    machine.succeed("test $(stat -c %U:%G /etc/lific-test.toml) = root:root")
    machine.wait_until_succeeds("test -s '/var/cache/lific backups'/lific_*.tar.gz")
  '';
}
