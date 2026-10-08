use std::io::{self, Write};
use worktree_pool::{
    catalog::{InitializationCheckpoint, initialize_observed},
    paths::Paths,
};
fn main() {
    let selected = std::env::var("CHECKPOINT").unwrap();
    let paths = Paths::load(None, None, true).unwrap();
    if std::env::var("OPERATION").as_deref() == Ok("refresh") {
        use worktree_pool::workflows::{RefreshCheckpoint, refresh_observed};
        let repo = std::env::var_os("REPOSITORY_ID").unwrap();
        refresh_observed(&paths, Some(&repo), |checkpoint| {
            let name = match checkpoint {
                RefreshCheckpoint::IntentRecorded => "intent",
                RefreshCheckpoint::FetchObserved => "fetch",
                RefreshCheckpoint::ResultCommitted => "result",
            };
            pause(&selected, name);
        })
        .unwrap();
        return;
    }
    initialize_observed(&paths, |checkpoint| {
        let name = match checkpoint {
            InitializationCheckpoint::IntentRecorded => "intent",
            InitializationCheckpoint::StoreCommitted => "store",
            InitializationCheckpoint::AuthorityPublished => "published",
        };
        pause(&selected, name);
    })
    .unwrap();
}

fn pause(selected: &str, name: &str) {
    if selected == name {
        println!("checkpoint:{name}");
        io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }
}
