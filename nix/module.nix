{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.neddit;
in
{
  options.services.neddit = {
    enable = lib.mkEnableOption "Private Reddit frontend";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      description = "The Neddit package to run.";
    };

    address = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "The address on which Neddit listens.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8080;
      description = "The TCP port on which Neddit listens.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Whether to open the Neddit port in the firewall.";
    };

    mediaKeyFile = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Path to a file containing at least 32 bytes used to sign media proxy URLs.";
    };
  };

  config = lib.mkIf cfg.enable (lib.mkMerge [
    {
      systemd.services.neddit = {
        description = "Neddit";
        wantedBy = [ "multi-user.target" ];
        wants = [ "network-online.target" ];
        after = [ "network-online.target" ];
        serviceConfig = {
          Type = "exec";
          ExecStart = "${cfg.package}/bin/neddit --address ${lib.escapeShellArg cfg.address} --port ${toString cfg.port}${lib.optionalString (cfg.mediaKeyFile != null) " --media-key-file %d/media-key"}";
          Restart = "on-failure";
          RestartSec = 5;

          DynamicUser = true;
          AmbientCapabilities = "";
          CapabilityBoundingSet = "";
          LockPersonality = true;
          MemoryDenyWriteExecute = true;
          NoNewPrivileges = true;
          PrivateDevices = true;
          PrivateTmp = true;
          ProcSubset = "pid";
          ProtectClock = true;
          ProtectControlGroups = true;
          ProtectHome = true;
          ProtectHostname = true;
          ProtectKernelLogs = true;
          ProtectKernelModules = true;
          ProtectKernelTunables = true;
          ProtectProc = "invisible";
          ProtectSystem = "strict";
          RemoveIPC = true;
          RestrictAddressFamilies = [
            "AF_INET"
            "AF_INET6"
          ];
          RestrictNamespaces = true;
          RestrictRealtime = true;
          RestrictSUIDSGID = true;
          SystemCallArchitectures = "native";
          SystemCallFilter = [
            "@system-service"
            "~@privileged"
            "~@resources"
          ];
          UMask = "0077";
        } // lib.optionalAttrs (cfg.mediaKeyFile != null) {
          LoadCredential = "media-key:${cfg.mediaKeyFile}";
        };
      };
    }

    (lib.mkIf cfg.openFirewall {
      networking.firewall.allowedTCPPorts = [ cfg.port ];
    })
  ]);
}
