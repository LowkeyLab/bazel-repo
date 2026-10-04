mod flat;
mod directory;
mod inline { mod external; }
#[path = "renamed.rs"]
mod custom;
#[path = "nested/lib.rs"]
mod nested_library_name;
