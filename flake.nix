{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, flake-utils, rust-overlay, crane, nixpkgs }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain (p: p.rust-bin.stable.latest.default );
        
        server = craneLib.buildPackage {
          pname = "jotihunt";
          version = "0.1.0";
          src = craneLib.cleanCargoSource ./.;
          strictDeps = true;

          nativeBuildInputs = with pkgs; [ pkg-config rustPlatform.bindgenHook ];
          buildInputs = with pkgs; [ openssl ];
          cargoExtraArgs = "-p server";
        };
      in {
        # For `nix build` & `nix run`:
        defaultPackage = server;

        nixosModules.default = { ... }: {
          systemd.services.jotihunt = {
            wantedBy = [ "multi-user.target" ];
            serviceConfig = {
              ExecStart = "${server}/bin/server";
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
