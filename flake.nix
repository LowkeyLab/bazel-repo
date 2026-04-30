{
  description = "Dev shell for bazel-repo";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    aspect-cli-src = {
      url = "github:aspect-build/aspect-cli/v2026.4.2";
      flake = false;
    };
    lowkeylab-nix = {
      url = "github:LowkeyLab/nix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.aspect-cli-src.follows = "aspect-cli-src";
    };
    llm-agents = {
      url = "github:numtide/llm-agents.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      lowkeylab-nix,
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
          aspect = lowkeylab-nix.packages.${system}.aspect;
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
          inherit (mkPackagesForSystem system) aspect;
        in
        {
          inherit aspect;
          default = aspect;
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
          format = pkgs.writeShellScriptBin "format" ''exec bazel run //tools/format -- "$@"'';
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
              format
              coverage
              pkgs.buildifier
              pkgs.starpls
              pkgs.prettier
              pkgs.go
              pkgs.pnpm
              pkgs.jdk21_headless
              pkgs.gcc
              pkgs.pre-commit
              agent-browser
              pkgs.nodejs_24
              pkgs.cargo
              pkgs.lcov
            ] ++ [
              aspect
            ];

            # Linux: set NIX_LD so bazelisk can run downloaded Bazel binaries
            # On NixOS, this additionally requires `programs.nix-ld.enable = true`
            env = {
              JAVA_HOME = "${pkgs.jdk21_headless}";
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
