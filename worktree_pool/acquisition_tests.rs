//! Worked decision inputs exercise the public pure ordering seam without Git/store substitutes.
use std::path::Path;

use googletest::{assert_that, matchers::eq};

use crate::{
    acquisition::candidate_order,
    management::{RepositoryId, Worktree, WorktreeId},
    paths::EncodedPath,
};

#[googletest::test]
fn matching_commit_then_latest_release_then_worktree_id_define_selection_order() {
    let repository_id = "00000000-0000-4000-8000-000000000001"
        .parse::<RepositoryId>()
        .unwrap();
    let mut candidates = Vec::new();
    for (number, release, head) in [
        (3, None, "other"),
        (2, Some(9), "selected"),
        (1, Some(10), "selected"),
        (4, Some(99), "other"),
        (5, Some(10), "selected"),
    ] {
        let worktree = Worktree {
            worktree_id: format!("00000000-0000-4000-8000-{number:012}")
                .parse::<WorktreeId>()
                .unwrap(),
            repository_id,
            path: EncodedPath::from_path(Path::new("/candidate")),
            git_directory: EncodedPath::from_path(Path::new("/git-directory")),
            last_release_position: release,
        };
        candidates.push((number, worktree, head));
    }
    candidates.sort_by_key(|(_, worktree, head)| candidate_order(worktree, head, "selected"));
    let actual: Vec<_> = candidates.iter().map(|(number, _, _)| *number).collect();
    assert_that!(actual.as_slice(), eq([1, 5, 2, 4, 3].as_slice()));
}
