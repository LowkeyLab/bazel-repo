use std::io::{self, Write};
use worktree_pool::{
    catalog::{InitializationCheckpoint, initialize_observed},
    paths::Paths,
};
fn main() {
    let selected = std::env::var("CHECKPOINT").unwrap();
    let paths = Paths::load(None, None, true).unwrap();
    initialize_observed(&paths, |checkpoint| {
        let name = match checkpoint {
            InitializationCheckpoint::IntentRecorded => "intent",
            InitializationCheckpoint::StoreCommitted => "store",
            InitializationCheckpoint::AuthorityPublished => "published",
        };
        if selected == name {
            println!("checkpoint:{name}");
            io::stdout().flush().unwrap();
            loop {
                std::thread::park();
            }
        }
    })
    .unwrap();
}
