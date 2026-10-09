use std::io::{self, Write};
use worktree_pool::{
    catalog::{InitializationCheckpoint, initialize_observed},
    paths::Paths,
};
fn main() {
    let selected = std::env::var("CHECKPOINT").unwrap();
    let paths = Paths::load(None, None, true).unwrap();
    match std::env::var("OPERATION").as_deref() {
        Ok("relocate") => return relocate_catalog(&paths, &selected),
        Ok("rebuild") => return rebuild_catalog(&paths, &selected),
        Ok("retire") => return retire_registration(&paths, &selected),
        _ => {}
    }
    if std::env::var("OPERATION").as_deref() == Ok("recover-repository") {
        use worktree_pool::repository_recovery::{RepositoryRecoveryCheckpoint, resume_observed};
        let operation_id = std::env::var("OPERATION_ID").unwrap();
        resume_observed(&paths, None, &operation_id, |checkpoint| {
            let name = match checkpoint {
                RepositoryRecoveryCheckpoint::IntentCommitted => "intent",
                RepositoryRecoveryCheckpoint::ResultCommitted => "result",
            };
            pause(&selected, name);
        })
        .unwrap();
        return;
    }
    if std::env::var("OPERATION").as_deref() == Ok("recover-catalog") {
        recover_catalog(&paths, &selected);
        return;
    }
    if std::env::var("OPERATION").as_deref() == Ok("recover") {
        use worktree_pool::recovery_workflow::{RecoveryCheckpoint, abandon_assignment_observed};
        let handle = std::env::var("ASSIGNMENT_HANDLE").unwrap();
        abandon_assignment_observed(&paths, None, &handle, |checkpoint| {
            let name = match checkpoint {
                RecoveryCheckpoint::IntentCommitted => "intent",
                RecoveryCheckpoint::PreservationObserved => "preservation-effect",
                RecoveryCheckpoint::PreservationCommitted => "preserved",
                RecoveryCheckpoint::ResultCommitted => "result",
            };
            pause(&selected, name);
        })
        .unwrap();
        return;
    }
    if std::env::var("OPERATION").as_deref() == Ok("release") {
        use worktree_pool::release_workflow::{ReleaseCheckpoint, release_observed};
        let handle = std::env::var("ASSIGNMENT_HANDLE").unwrap().parse().unwrap();
        release_observed(&paths, None, handle, |checkpoint| {
            let name = match checkpoint {
                ReleaseCheckpoint::IntentCommitted => "intent",
                ReleaseCheckpoint::PreservationObserved => "preservation-effect",
                ReleaseCheckpoint::PreservationCommitted => "preserved",
                ReleaseCheckpoint::ResultCommitted => "result",
            };
            pause(&selected, name);
        })
        .unwrap();
        return;
    }
    if std::env::var("OPERATION").as_deref() == Ok("acquire") {
        use worktree_pool::acquisition_workflow::{AcquisitionCheckpoint, acquire_observed};
        let repo = std::env::var_os("REPOSITORY_ID").unwrap();
        acquire_observed(&paths, Some(&repo), None, |checkpoint| {
            let name = match checkpoint {
                AcquisitionCheckpoint::RefreshCommitted => "refreshed",
                AcquisitionCheckpoint::CreationIntended => "creation-intent",
                AcquisitionCheckpoint::CreationPathObserved => "creation-path-effect",
                AcquisitionCheckpoint::CreationPathCommitted => "creation-path",
                AcquisitionCheckpoint::CreationObserved => "creation-effect",
                AcquisitionCheckpoint::ReservationCommitted => "reservation",
                AcquisitionCheckpoint::PreservationIntended => "preservation-intent",
                AcquisitionCheckpoint::PreservationObserved => "preservation-effect",
                AcquisitionCheckpoint::PreservationCommitted => "preserved",
                AcquisitionCheckpoint::CheckoutIntended => "checkout-intent",
                AcquisitionCheckpoint::CheckoutObserved => "checkout-effect",
                AcquisitionCheckpoint::ResultCommitted => "result",
            };
            pause(&selected, name);
        })
        .unwrap();
        return;
    }
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
        if std::env::var("CONTINUABLE").as_deref() == Ok("1") {
            let mut line = String::new();
            io::stdin().read_line(&mut line).unwrap();
        } else {
            loop {
                std::thread::park();
            }
        }
    }
}

fn recover_catalog(paths: &Paths, selected: &str) {
    use worktree_pool::catalog::{AuthorityRecoveryCheckpoint, reconcile_authority_observed};
    reconcile_authority_observed(paths, |checkpoint| {
        let name = match checkpoint {
            AuthorityRecoveryCheckpoint::IntentRecorded => "intent",
            AuthorityRecoveryCheckpoint::StoreCommitted => "store",
            AuthorityRecoveryCheckpoint::AuthorityPublished => "published",
        };
        pause(selected, name);
    })
    .unwrap();
}

fn rebuild_catalog(paths: &Paths, selected: &str) {
    worktree_pool::catalog::rebuild_observed(paths, |checkpoint| {
        let name = match checkpoint {
            worktree_pool::rebuild::RebuildCheckpoint::Validated => "validated",
            worktree_pool::rebuild::RebuildCheckpoint::BeforeCommit => "before-commit",
            worktree_pool::rebuild::RebuildCheckpoint::Committed => "committed",
        };
        pause(selected, name);
    })
    .unwrap();
}

fn retire_registration(paths: &Paths, selected: &str) {
    use worktree_pool::retirement_workflow::{RetirementCheckpoint, retire_observed};
    let ids = std::env::var("OPERATION_ID").unwrap();
    let (worktree, proof) = ids.split_once(':').unwrap();
    retire_observed(
        paths,
        None,
        worktree.as_ref(),
        proof.parse().unwrap(),
        true,
        |checkpoint| {
            pause(
                selected,
                match checkpoint {
                    RetirementCheckpoint::IntentCommitted => "intent",
                    RetirementCheckpoint::ResultCommitted => "result",
                },
            );
        },
    )
    .unwrap();
}

fn relocate_catalog(paths: &Paths, selected: &str) {
    use worktree_pool::relocation::{RelocationCheckpoint, relocate_observed};
    let destination = std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join("relocated");
    relocate_observed(paths, &destination, |checkpoint| {
        let name = match checkpoint {
            RelocationCheckpoint::IntentRecorded => "intent",
            RelocationCheckpoint::DirectoryPrepared => "directory",
            RelocationCheckpoint::DestinationCreated => "created",
            RelocationCheckpoint::CopyStarted => "copy-started",
            RelocationCheckpoint::CopySynced => "copied",
            RelocationCheckpoint::DestinationPrepared => "prepared",
            RelocationCheckpoint::LocatorSwitched => "switched",
            RelocationCheckpoint::Completed => "completed",
        };
        pause(selected, name);
    })
    .unwrap();
}
