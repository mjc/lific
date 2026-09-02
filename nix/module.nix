{self}: {
  config,
  lib,
  pkgs,
  utils,
  ...
}: let
  cfg = config.services.lific;
  format = pkgs.formats.toml {};
  dataDir = "/var/lib/lific";
  databasePath = "${dataDir}/lific.db";
  credentialConfigPath = "/run/credentials/lific.service/lific.toml";
  generatedConfig = format.generate "lific.toml" (
    lib.recursiveUpdate cfg.settings {
      server = {
        inherit (cfg) host port;
      };
      database.path = databasePath;
    }
  );
  configSource =
    if cfg.configFile == null
    then generatedConfig
    else cfg.configFile;
in {
  options.services.lific = {
    enable = lib.mkEnableOption "Lific issue tracker and MCP server";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "lific.packages.${pkgs.stdenv.hostPlatform.system}.default";
      description = "Lific package to run.";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "Address on which Lific listens.";
    };

    port = lib.mkOption {
      type = lib.types.ints.between 1024 65535;
      default = 3456;
      description = "Unprivileged TCP port on which Lific listens.";
    };

    settings = lib.mkOption {
      inherit (format) type;
      default = {};
      example = {
        server.public_url = "https://lific.example.com";
        auth.allow_signup = false;
      };
      description = ''
        Configuration written to lific.toml. The listen address, port, and
        database path are managed by this module. Do not put secrets here
        because generated configuration is stored in the world-readable Nix
        store; use configFile for a secret-bearing runtime file.
      '';
    };

    configFile = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "/run/secrets/lific.toml";
      description = ''
        Absolute path to an externally managed lific.toml. systemd loads it as
        a read-only service credential, so the source may remain root-only.
        The service always passes host, port, and database path as CLI flags,
        so those values in the external TOML are overridden. Set this for
        secret-bearing configuration. It is mutually exclusive with settings.
        Restart lific.service after rotating the source because systemd loads
        credentials only when starting the service.
      '';
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "lific";
      description = "User account under which Lific runs.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "lific";
      description = "Group account under which Lific runs.";
    };

    extraReadWritePaths = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [];
      example = ["/srv/lific-backups"];
      description = ''
        Additional absolute runtime paths that Lific may write. Use this for
        an external absolute backup.dir; the default database, attachments,
        and backups remain under StateDirectory.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.configFile == null || cfg.settings == {};
        message = "services.lific.configFile is mutually exclusive with services.lific.settings";
      }
      {
        assertion = cfg.configFile == null || lib.hasPrefix "/" cfg.configFile;
        message = "services.lific.configFile must be an absolute path";
      }
      {
        assertion = lib.attrByPath ["server" "mcp_path_token"] null cfg.settings == null;
        message = "services.lific.settings.server.mcp_path_token would be exposed in the Nix store; use services.lific.configFile";
      }
      {
        assertion = builtins.all (path: lib.hasPrefix "/" path) cfg.extraReadWritePaths;
        message = "services.lific.extraReadWritePaths entries must be absolute paths";
      }
    ];

    users.groups = lib.mkIf (cfg.group == "lific") {
      lific = {};
    };
    users.users = lib.mkIf (cfg.user == "lific") {
      lific = {
        isSystemUser = true;
        inherit (cfg) group;
        home = dataDir;
      };
    };

    systemd.services.lific = {
      description = "Lific issue tracker and MCP server";
      wantedBy = ["multi-user.target"];
      after = ["network.target"];
      restartTriggers = lib.optional (cfg.configFile == null) generatedConfig;

      serviceConfig = {
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = dataDir;
        StateDirectory = "lific";
        StateDirectoryMode = "0750";
        UMask = "0077";
        LoadCredential = "lific.toml:${configSource}";
        ExecStart = utils.escapeSystemdExecArgs [
          (lib.getExe cfg.package)
          "--config"
          credentialConfigPath
          "--db"
          databasePath
          "start"
          "--host"
          cfg.host
          "--port"
          (toString cfg.port)
        ];
        Restart = "on-failure";
        RestartSec = 2;

        AmbientCapabilities = "";
        CapabilityBoundingSet = "";
        LockPersonality = true;
        NoNewPrivileges = true;
        PrivateDevices = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectSystem = "strict";
        ReadWritePaths = cfg.extraReadWritePaths;
        RestrictAddressFamilies = [
          "AF_UNIX"
          "AF_INET"
          "AF_INET6"
        ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
      };
    };
  };
}
