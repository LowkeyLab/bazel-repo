mod child;
#[path = "relocated"]
mod alternate { #[path = "deep.rs"] mod chosen; }
