let
  repository = builtins.toString ../../..;
  flake = builtins.getFlake ("git+file://" + repository);
  packages = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
in
flake.devShells.x86_64-linux.default.overrideAttrs (old: {
  nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ packages.cmake packages.pkg-config packages.python3 ];
  buildInputs = (old.buildInputs or [ ]) ++ [ packages.elfutils packages.curl packages.openssl packages.zlib ];
})
