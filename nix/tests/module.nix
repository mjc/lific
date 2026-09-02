{
  nixpkgs,
  pkgs,
  self,
  system,
}: let
  inherit (nixpkgs) lib;
  externalConfig = "/run/secrets/lific.toml";
  credentialConfig = "/run/credentials/lific.service/lific.toml";

  evaluate = service:
    lib.nixosSystem {
      inherit system;
      modules = [
        self.nixosModules.default
        {
          system.stateVersion = "26.05";
          services.lific =
            {
              enable = true;
            }
            // service;
        }
      ];
    };

  failedAssertions = module:
    builtins.filter (
      item: !item.assertion && lib.hasPrefix "services.lific." item.message
    )
    module.config.assertions;

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
  relativeWritePathModule = evaluate {
    extraReadWritePaths = ["backups"];
  };
  invalidPort = builtins.tryEval (
    (evaluate {
      port = 80;
    }).config.systemd.services.lific.serviceConfig.ExecStart
  );
  generatedModule = evaluate {};
  externalModule = evaluate {
    configFile = externalConfig;
    extraReadWritePaths = ["/srv/lific-backups"];
  };

  generatedService = generatedModule.config.systemd.services.lific.serviceConfig;
  externalService = externalModule.config.systemd.services.lific.serviceConfig;
  externalWritePaths = externalService.ReadWritePaths;
in
  assert lib.any (item: lib.hasInfix "mutually exclusive" item.message) (
    failedAssertions invalidModule
  );
  assert lib.any (item: lib.hasInfix "mcp_path_token" item.message) (failedAssertions invalidModule);
  assert lib.any (item: lib.hasInfix "configFile must be an absolute path" item.message) (
    failedAssertions relativeConfigModule
  );
  assert lib.any (item: lib.hasInfix "extraReadWritePaths entries must be absolute" item.message) (
    failedAssertions relativeWritePathModule
  );
  assert !invalidPort.success;
  assert failedAssertions generatedModule == [];
  assert failedAssertions externalModule == [];
  assert externalService.LoadCredential == "lific.toml:${externalConfig}";
  assert lib.hasInfix credentialConfig externalService.ExecStart;
  assert lib.hasInfix "--db" externalService.ExecStart;
  assert lib.hasInfix "/var/lib/lific/lific.db" externalService.ExecStart;
  assert generatedService.StateDirectory == "lific";
  assert generatedService.WorkingDirectory == "/var/lib/lific";
  assert generatedService.ProtectSystem == "strict";
  assert generatedService.ReadWritePaths == [];
  assert externalWritePaths == ["/srv/lific-backups"];
    pkgs.testers.runNixOSTest {
      name = "lific";

      nodes.machine = {pkgs, ...}: {
        imports = [self.nixosModules.default];
        system.stateVersion = "26.05";

        environment.etc."lific-test.toml" = {
          mode = "0400";
          user = "root";
          group = "root";
          text = ''
            [auth]
            required = false

            [backup]
            dir = "/srv/lific-backups"

          '';
        };

        services.lific = {
          enable = true;
          configFile = "/etc/lific-test.toml";
          extraReadWritePaths = ["/srv/lific-backups"];
        };

        systemd.tmpfiles.rules = ["d /srv/lific-backups 0750 lific lific -"];
        environment.systemPackages = [pkgs.curl];
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
        machine.succeed("runuser -u lific -- touch /srv/lific-backups/probe")
        machine.fail("runuser -u lific -- touch /var/cache/lific-escape-probe")
        machine.wait_until_succeeds("test -s /srv/lific-backups/lific_*.tar.gz")
      '';
    }
