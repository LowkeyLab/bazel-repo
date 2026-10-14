{
  description = "Dev shell for bazel-repo";

  inputs = {
    nixpkgs.url = "git+https://github.com/NixOS/nixpkgs?shallow=1&ref=nixpkgs-unstable&rev=8edc0c72e3a38faf5434e40d1f19431125fe4b30";
    lowkeylab-nix = {
      url = "github:LowkeyLab/nix?rev=fbc633b8aac49d0d1f67a7204531410091f1614b";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    llm-agents = {
      url = "github:numtide/llm-agents.nix?rev=d058453d8128aaeaba2c02275bac259913a0dd3e";
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
