let
  repository = builtins.toString ../..;
  flake = builtins.getFlake ("git+file://" + repository);
  packages = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
  libraries = with packages; [
    stdenv.cc.cc.lib zlib nss nspr at-spi2-atk at-spi2-core atk cairo cups dbus
    libdrm mesa libgbm systemdLibs glib pango libX11 libxcb libXcomposite libXdamage libXext
    libXfixes libxkbcommon libXrandr alsa-lib expat
  ];
in
flake.devShells.x86_64-linux.default.overrideAttrs (old: {
  shellHook = (old.shellHook or "") + ''
    export NIX_LD_LIBRARY_PATH=${packages.lib.makeLibraryPath libraries}
  '';
})
