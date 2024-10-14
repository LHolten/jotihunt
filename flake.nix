{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    nixpkgs-mozilla = {
      url = "github:mozilla/nixpkgs-mozilla";
      flake = false;
    };
  };

  outputs = { self, flake-utils, naersk, nixpkgs, nixpkgs-mozilla }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) {
          inherit system;

          overlays = [
            (import nixpkgs-mozilla)
          ];
        };

        toolchain = (pkgs.rustChannelOf {
          date = "2024-07-25"; # 1.80.0
          channel = "stable";
          sha256 = "sha256-6eN/GKzjVSjEhGO9FhWObkRFaE1Jf+uqMSdQnb8lcB4=";
        }).rust;

        naersk' = pkgs.callPackage naersk {
          cargo = toolchain;
          rustc = toolchain;
        };

        server = naersk'.buildPackage {
          nativeBuildInputs = with pkgs; [ pkg-config rustPlatform.bindgenHook ];
          buildInputs = with pkgs; [ openssl ];
          src = ./server;
        };
      in {
        # For `nix build` & `nix run`:
        defaultPackage = server;

        nixosModules.default = { ... }: {
          systemd.services.jotihunt = {
            wantedBy = [ "multi-user.target" ];
            serviceConfig = {
              ExecStart = "${server}/bin/jotihunt-server";
              User = "jotihunt";
              Group = "jotihunt";
              WorkingDirectory = "/var/lib/jotihunt";
              StateDirectory = "jotihunt";
            };
          };
          
          users.users.jotihunt = {
            isSystemUser = true;
            group = "jotihunt";
          };
          users.groups.jotihunt = {};

          services.nginx = {
            recommendedProxySettings = true;

            virtualHosts."jotihunt.lucasholten.com" = {
              enableACME = true;
              forceSSL = true;

              locations."/" = {
                proxyPass = "http://127.0.0.1:4848";
              };
            };
          };
        };

        # For `nix develop`:
        devShell = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [ rustc cargo ];
        };
      }
    );
}
