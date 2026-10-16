{
  description = "Dev shell for bazel-repo";

  inputs = {
    nixpkgs.url = "git+https://github.com/NixOS/nixpkgs?shallow=1&ref=nixpkgs-unstable&rev=319e4cebef10cbb636c6ea392137b52ad105336a";
    aspect-cli-src = {
      url = "github:aspect-build/aspect-cli?rev=618f5c40cb0b3e015649acb6fc030aed4ac685f1";
      flake = false;
    };
    llm-agents = {
      url = "github:numtide/llm-agents.nix?rev=af01f46dd7520c12ddb35a139171af9b879ee24d";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      aspect-cli-src,
      llm-agents,
      ...
    }:
    let
      supportedSystems = [
        "x86_64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      mkPackagesForSystem =
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          # Upstream 618f5c4 passes version-gated flag tuples to its redactor.
          aspect = (import ./nix/packages/aspect {
            inherit pkgs;
            inputs = { inherit aspect-cli-src; };
          }).overrideAttrs (old: {
            patches = (old.patches or [ ]) ++ [ ./3rdparty/aspect_cli/delivery_versioned_flags.patch ];
          });
          agent-browser = llm-agents.packages.${system}.agent-browser;
        in
        {
          inherit pkgs aspect agent-browser;
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          inherit (mkPackagesForSystem system) pkgs aspect;
        in
        {
          inherit aspect;
          inherit (pkgs) lcov;
          default = aspect;
        }
      );
      checks = forAllSystems (
        system:
        let
          inherit (mkPackagesForSystem system) pkgs aspect;
        in
        {
          aspect = import ./nix/checks/aspect {
            inherit pkgs;
            perSystem.self = { inherit aspect; };
          };
          aspect-delivery-flags = pkgs.runCommand "aspect-delivery-flags" { nativeBuildInputs = [ aspect ]; } ''
            export ASPECT_CLI_CACHE="$TMPDIR/aspect-cache"
            mkdir .aspect
            cp ${./tools/aspect/delivery_flags.axl} .aspect/check.axl
            aspect delivery-flags
            touch "$out"
          '';
        }
      );
      devShells = forAllSystems (
        system:
        let
          inherit (mkPackagesForSystem system) pkgs aspect agent-browser;
          bazel =
            if pkgs.stdenv.isLinux then
              pkgs.buildFHSEnv {
                name = "bazel";
                targetPkgs = pkgs: [
                  pkgs.bashInteractive
                  pkgs.bazelisk
                  pkgs.zlib
                ];
                runScript = "bazelisk";
              }
            else
              pkgs.writeShellScriptBin "bazel" ''exec bazelisk "$@"'';
          coverage = pkgs.writeShellScriptBin "coverage" ''exec bazel run //tools/coverage -- "$@"'';
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.bashInteractive
              pkgs.bazelisk
              bazel
              pkgs.bazel-watcher
              pkgs.buildozer
              pkgs.buf
              coverage
              pkgs.buildifier
              pkgs.starpls
              pkgs.prettier
              pkgs.go
              pkgs.pnpm
              pkgs.jdk25_headless
              pkgs.gcc
              pkgs.pre-commit
              agent-browser
              pkgs.nodejs_24
              pkgs.cargo
              pkgs.lcov
              pkgs.railway
            ] ++ [
              aspect
            ];

            # Linux: set NIX_LD so bazelisk can run downloaded Bazel binaries
            # On NixOS, this additionally requires `programs.nix-ld.enable = true`
            env = {
              JAVA_HOME = "${pkgs.jdk25_headless}";
              CHROMIUM_EXECUTABLE = "${pkgs.chromium}/bin/chromium";
            } // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
              NIX_LD = pkgs.stdenv.cc.bintools.dynamicLinker;
              NIX_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
                pkgs.stdenv.cc.cc.lib
                pkgs.zlib
              ];
            };
          };
        }
      );
    };
}
