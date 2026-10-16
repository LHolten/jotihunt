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

          src = ./.;
          buildAndTestSubdir = "server";

          cargoLock = {
            lockFile = ./Cargo.lock;
            allowBuiltinFetchGit = true;
          };

          buildInputs = [ pkgs.sqlite ];            # libraries to link against
        };
      in {
        # For `nix build` & `nix run`:
        defaultPackage = server;

        nixosModules.default = {...}: {
          systemd.services.jotihunt = {
            wantedBy = ["multi-user.target"];
            serviceConfig = {
              # the responses of the API are kept, to test with them outside the hunt
              ExecStart = "${server}/bin/server --record recordings";
              User = "jotihunt";
              Group = "jotihunt";
              WorkingDirectory = "/var/lib/jotihunt";
              StateDirectory = "jotihunt";
              RuntimeDirectory = "jotihunt";
            };
          };

          users.users.jotihunt = {
            isSystemUser = true;
            group = "jotihunt";
          };
          users.groups.jotihunt = {};

          services.caddy = {
            virtualHosts."jotihunt.lucasholten.com".extraConfig = ''
              reverse_proxy unix//run/jotihunt/socket
            '';
          };
        };

        # For `nix develop`:
        devShell = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [nixd caddy];
        };
      }
    );
}
