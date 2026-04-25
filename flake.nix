{
  description = "Dev shell for bazel-repo";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      binaryReleases = {
        aspect = {
          version = "2026.4.2";
          # Note: aspect-cli does not publish aarch64-linux binaries.
          # The tool will be unavailable on that platform.
          binaries = {
            "x86_64-linux" = {
              url = "https://github.com/aspect-build/aspect-cli/releases/download/v2026.4.2/aspect-cli-x86_64-unknown-linux-musl";
              sha256 = "58057a7bfb94838749cbb3fedc015baeefa1887caf00e1ed4dd5eb8ef00c6cef";
            };
            "aarch64-darwin" = {
              url = "https://github.com/aspect-build/aspect-cli/releases/download/v2026.4.2/aspect-cli-aarch64-apple-darwin";
              sha256 = "5cae50dcd8a2548ec433833a80d8e0ef3d41965c3673db700f8bbc52e5f15600";
            };
          };
        };
      };
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          mkFetchedBinary =
            { name, version, url, sha256 }:
            pkgs.stdenvNoCC.mkDerivation {
              pname = name;
              inherit version;
              src = pkgs.fetchurl {
                inherit url sha256;
              };
              dontUnpack = true;
              installPhase = ''
                install -Dm755 "$src" "$out/bin/${name}"
              '';
            };
          aspectBinary = binaryReleases.aspect.binaries.${system} or null;
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
          aspect =
            if aspectBinary == null then
              null
            else
              mkFetchedBinary {
                name = "aspect";
                inherit (binaryReleases.aspect) version;
                inherit (aspectBinary) url sha256;
              };
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
              pkgs.nodejs_24
              pkgs.cargo
              pkgs.lcov

              # agent-browser: AI browser automation CLI (https://github.com/vercel-labs/agent-browser)
              pkgs.nodejs # npm shim launcher for agent-browser
            ] ++ pkgs.lib.optionals (aspect != null) [
              aspect
            ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.chromium # hermetic browser — use with: agent-browser --executable-path $(which chromium)
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

                # Chrome/Chromium runtime deps for agent-browser
                pkgs.libxcb
                pkgs.libx11
                pkgs.libxext
                pkgs.libxrandr
                pkgs.libxcomposite
                pkgs.libxcursor
                pkgs.libxdamage
                pkgs.libxfixes
                pkgs.libxi
                pkgs.libxrender
                pkgs.libxshmfence
                pkgs.libxkbcommon
                pkgs.gtk3
                pkgs.pango
                pkgs.at-spi2-atk
                pkgs.at-spi2-core
                pkgs.cairo
                pkgs.gdk-pixbuf
                pkgs.mesa
                pkgs.libdrm
                pkgs.alsa-lib
                pkgs.dbus
                pkgs.cups
                pkgs.freetype
                pkgs.fontconfig
                pkgs.nss
                pkgs.nspr
              ];
              # Point agent-browser at Nix-provided Chromium (Linux-only;
              # on Darwin, agent-browser auto-detects Chrome from /Applications).
              # CHROME_BIN is the same path under a different name — karma-chrome-launcher
              # reads CHROME_BIN to locate the browser for `bazel coverage` on Karma tests.
              CHROME_PATH = pkgs.lib.getExe pkgs.chromium;
              CHROME_BIN = pkgs.lib.getExe pkgs.chromium;
            };
          };
        }
      );
    };
}
