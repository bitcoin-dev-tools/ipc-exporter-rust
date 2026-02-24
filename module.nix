{
  config,
  lib,
  ...
}:
let
  cfg = config.services.bitcoind-ipc-exporter;
in
{
  options.services.bitcoind-ipc-exporter = {
    enable = lib.mkEnableOption "Bitcoin Core IPC metrics exporter";

    package = lib.mkOption {
      type = lib.types.package;
      description = "The ipc-exporter-rust package to use.";
    };

    socketPath = lib.mkOption {
      type = lib.types.str;
      description = "Path to the bitcoind IPC Unix socket.";
    };

    metricsAddr = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1:9332";
      description = "Host:port for the Prometheus metrics endpoint.";
    };

    debug = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Enable verbose debug logging to stderr.";
    };

    user = lib.mkOption {
      type = lib.types.str;
      default = "root";
      description = "User to run the exporter as. Should have read access to the IPC socket.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "root";
      description = "Group to run the exporter as.";
    };

    after = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Systemd services to start after.";
    };

    bindsTo = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      description = "Systemd services to bind lifecycle to.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.bitcoind-ipc-exporter = {
      description = "Bitcoin Core IPC Metrics Exporter";
      inherit (cfg) after bindsTo;
      wantedBy = [ "multi-user.target" ];

      startLimitIntervalSec = 300;
      startLimitBurst = 20;

      serviceConfig = {
        User = cfg.user;
        Group = cfg.group;
        Restart = "on-failure";
        RestartSec = 10;
        ExecStart = builtins.concatStringsSep " " (
          [
            "${cfg.package}/bin/ipc-exporter-rust"
            "--metrics-addr"
            cfg.metricsAddr
          ]
          ++ lib.optionals cfg.debug [ "--debug" ]
          ++ [ cfg.socketPath ]
        );
      };
    };
  };
}
