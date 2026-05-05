self: { config, lib, pkgs, ... }:
with lib;
let
  opt = config.services.turn-proxy.server;
in
{
  options.services.turn-proxy.server = {
    enable = mkEnableOption "DTLS Turn Proxy Server";
    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.system}.server;
      description = "Package with turn proxy";
    };
    configPath = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "Path to config.toml if it using";
    };
    config = {
      listeningOn = mkOption {
        type = types.str;
        default = "0.0.0.0:56000";
        description = "Address to listening";
      };
      proxyInto = mkOption {
        type = types.str;
        description = "Address of UDP-based application";
      };
      maxConnections = mkOption {
        type = types.int;
        description = "Max connections for server";
      };
    };
  };

  config = mkIf opt.enable {
    systemd.services.turn-proxy-server = {
      description = "DTLS TURN Proxy Server";
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];
      serviceConfig = {
        ExecStart = concatStringsSep " " [
          "${opt.package}/bin/turn-proxy-server"
          (if opt.configPath != null then "--config=${opt.configPath}" else "--no-config")
          "--max-connections=${opt.config.maxConnections}"
          "--listening-on=${opt.config.listeningOn}"
          "--proxy-into=${opt.config.proxyInto}"
        ];
        Restart = "always";
        LimitNOFILE = 65535;
      };
    };
  };
}
