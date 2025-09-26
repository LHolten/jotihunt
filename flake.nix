{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = {
    self,
    flake-utils,
    nixpkgs,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {inherit system;};

        server = pkgs.rustPlatform.buildRustPackage {
          pname = "jotihunt";
          version = "0.1.0";

          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          src = ./.;
        };
      in {
        # For `nix build` & `nix run`:
        defaultPackage = server;

        nixosModules.default = {...}: {
          systemd.services.jotihunt = {
            wantedBy = ["multi-user.target"];
            serviceConfig = {
              ExecStart = "${server}/bin/server";
              User = "jotihunt";
              Group = "jotihunt";
              WorkingDirectory = "/var/lib/jotihunt";
              StateDirectory = "jotihunt";
              RuntimeDirectory = "jotihunt";
              RuntimeDirectoryMode = "0777";
            };
          };

          users.users.jotihunt = {
            isSystemUser = true;
            group = "jotihunt";
          };
          users.groups.jotihunt = {};

          services.nginx = {
            virtualHosts."jotihunt.lucasholten.com" = {
              enableACME = true;
              forceSSL = true;

              locations."/" = {
                proxyPass = "http://unix:/run/jotihunt/socket";
                proxyWebsockets = true;
                recommendedProxySettings = true;

                extraConfig = ''
                  proxy_connect_timeout 7d;
                  proxy_send_timeout 7d;
                  proxy_read_timeout 7d;
                '';
              };
            };
          };
        };

        # For `nix develop`:
        devShell = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [nixd];
        };
      }
    );
}
