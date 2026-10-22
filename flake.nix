{
  description = "Dev shell for bazel-repo";

  inputs = {
    nixpkgs.url = "git+https://github.com/NixOS/nixpkgs?shallow=1&ref=nixpkgs-unstable&rev=5adc50831c259f87faeb09ddf930d43597197473";
    aspect-cli-src = {
      url = "github:aspect-build/aspect-cli?rev=5e8e59a898096f3f2ff88bdf689f52343a1d6d3a";
      flake = false;
    };
    llm-agents = {
      url = "github:numtide/llm-agents.nix?rev=24cf271f40105749bb82ad318013c372183ef28e";
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
            patches = (old.patches or [ ]) ++ [
              ./3rdparty/aspect_cli/delivery_versioned_flags.patch
              ./3rdparty/aspect_cli/python_typing.patch
            ];
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
            cp ${./.aspect/config.axl} .aspect/repo_config.axl
            cp ${./tools/aspect/delivery_flags.axl} .aspect/check.axl
            echo 'load("./check.axl", "config")' > .aspect/config.axl
            echo "common --remote_cache=grpcs://remote.buildbuddy.io" > .bazelrc
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
              # rules_distroless verifies the worker package snapshot signatures.
              pkgs.gnupg
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
