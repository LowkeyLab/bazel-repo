{
  description = "Dev shell for bazel-repo";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    lowkeylab-nix = {
      url = "github:LowkeyLab/nix";
      inputs.nixpkgs.follows = "nixpkgs";
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
          bazelFhs = pkgs.buildFHSEnv {
            name = "bazel-fhs";
            targetPkgs = pkgs: [
              pkgs.bashInteractive
              pkgs.bazel_9
              pkgs.zlib
            ];
            runScript = "${pkgs.bazel_9}/bin/bazel";
          };
          format = pkgs.writeShellScriptBin "format" ''exec ${bazelFhs}/bin/bazel-fhs run //tools/format -- "$@"'';
          coverage = pkgs.writeShellScriptBin "coverage" ''exec ${bazelFhs}/bin/bazel-fhs run //tools/coverage -- "$@"'';
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.bashInteractive
              pkgs.bazel_9
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
              pkgs.jdk25_headless
              pkgs.gcc
              pkgs.pre-commit
              agent-browser
              pkgs.nodejs_24
              pkgs.cargo
              pkgs.lcov
            ] ++ [
              aspect
            ];

            env = {
              JAVA_HOME = "${pkgs.jdk25_headless}";
            };
          };
        }
      );
    };
}
