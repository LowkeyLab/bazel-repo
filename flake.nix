{
  description = "Dev shell for bazel-repo";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.bazelisk
              (pkgs.writeShellScriptBin "bazel" ''exec bazelisk "$@"'')
              pkgs.gcc
              pkgs.pre-commit

              # agent-browser: AI browser automation CLI (https://github.com/vercel-labs/agent-browser)
              pkgs.nodejs # npm shim launcher for agent-browser
            ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.chromium # hermetic browser — use with: agent-browser --executable-path $(which chromium)
            ];

            # Linux: set NIX_LD so bazelisk can run downloaded Bazel binaries
            # On NixOS, this additionally requires `programs.nix-ld.enable = true`
            env = pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
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
              # Point agent-browser at Nix-provided Chromium
              CHROME_PATH = "${pkgs.chromium}/bin/chromium";
            };
          };
        }
      );
    };
}
