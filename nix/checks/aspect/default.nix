{ pkgs, perSystem, ... }:

pkgs.runCommand "aspect-smoke-check"
  {
    nativeBuildInputs = [ perSystem.self.aspect ];
  }
  ''
    test -x "${perSystem.self.aspect}/bin/aspect"
    command -v aspect > "$TMPDIR/aspect-path.txt"
    grep '/bin/aspect$' "$TMPDIR/aspect-path.txt"
    touch "$out"
  ''
