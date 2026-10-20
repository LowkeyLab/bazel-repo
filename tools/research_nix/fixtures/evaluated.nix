let
  forced =
    if true then
      1 + 2
    else
      throw "untaken";
  lazy =
    throw "must stay lazy";
  builder = ''
    printf '%s\n' builder-ran > "$out"
    if false; then
      printf '%s\n' untaken >> "$out"
    fi
  '';
in
{
  result = forced + forced;
  script = builder;
}
