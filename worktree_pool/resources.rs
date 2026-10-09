//! Readonly logical-byte observations. No build-system execution or symlink traversal.
use std::{
    collections::HashSet,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use crate::{
    domain::CatalogProjection,
    management::{Repository, Worktree},
    paths::EncodedPath,
};

const METRIC: &str = "logical_regular_file_bytes";

/// Reports only observed worktree files; repository common storage is reported once by repository inspection.
#[must_use]
pub fn worktree_usage(w: &Worktree) -> Value {
    let worktree = w.path.to_path().map_or_else(
        |_| unavailable(&w.path, "invalid_path"),
        |p| measure(&p, &mut HashSet::new(), true),
    );
    json!({"metric":METRIC,"worktree":worktree,"known_external_build_state":external(),"shared":{"bytes":null,"status":"reported_by_repository_inspection","reason_code":"shared_git_storage_not_worktree_exclusive"},"unknown":unknown(),"exclusive_bytes":false,"capacity_semantics":"registered_count_not_byte_limit"})
}

/// A historical registration cannot attribute files from a subsequently reused path.
#[must_use]
pub fn retired_usage(w: &Worktree) -> Value {
    json!({"metric":METRIC,"worktree":unavailable(&w.path, "retired_path_unattributed"),"known_external_build_state":external(),"shared":{"bytes":null,"status":"reported_by_repository_inspection"},"unknown":unknown(),"exclusive_bytes":false,"capacity_semantics":"registered_count_not_byte_limit"})
}

/// Aggregates unique regular-file identities across the common directory and enrolled worktrees.
#[must_use]
pub fn repository_usage(state: &CatalogProjection, repository: &Repository) -> Value {
    let mut seen = HashSet::new();
    let shared = repository.common_directory.to_path().map_or_else(
        |_| unavailable(&repository.common_directory, "invalid_path"),
        |p| measure(&p, &mut seen, false),
    );
    let mut total = Some(0_u64);
    let worktrees: Vec<_> = state
        .worktrees
        .iter()
        .filter(|w| w.repository_id == repository.repository_id)
        .map(|w| {
            let observed = w.path.to_path().map_or_else(
                |_| unavailable(&w.path, "invalid_path"),
                |p| measure(&p, &mut seen, true),
            );
            total = total
                .zip(observed["bytes"].as_u64())
                .and_then(|(a, b)| a.checked_add(b));
            json!({"worktree_id":w.worktree_id,"usage":observed})
        })
        .collect();
    json!({"metric":METRIC,"worktree_bytes":total,"worktrees":worktrees,"shared":shared,"known_external_build_state":external(),"unknown":unknown(),"aggregation":"unique_device_inode_across_report_roots_common_directory_first","exclusive_bytes":false,"capacity_semantics":"registered_count_not_byte_limit"})
}
fn external() -> Value {
    json!({"bytes":null,"status":"unmeasured","reason_code":"no_authoritative_build_state_roots","explanation":"build state outside worktrees cannot be attributed from authoritative pool metadata; arbitrary convenience symlinks are not ownership evidence"})
}
fn unknown() -> Value {
    json!([{"bytes":null,"reason_code":"external_build_state_unmeasured","scope":"undiscovered output bases, caches, remote storage, servers and unattributed shared storage"},{"bytes":null,"reason_code":"physical_usage_unmeasured","scope":"physical blocks, filesystem compression, reclaimable bytes and process memory"}])
}
fn unavailable(path: &EncodedPath, status: &str) -> Value {
    json!({"path":path,"bytes":null,"status":status})
}
fn measure(root: &Path, seen: &mut HashSet<(u64, u64)>, exclude_git: bool) -> Value {
    if fs::canonicalize(root).is_ok_and(|canonical| canonical != root) {
        return unavailable(&EncodedPath::from_path(root), "unsafe_root");
    }
    let metadata = match fs::symlink_metadata(root) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => m,
        Ok(_) => return unavailable(&EncodedPath::from_path(root), "unsafe_root"),
        Err(e) => {
            return unavailable(
                &EncodedPath::from_path(root),
                if e.kind() == std::io::ErrorKind::NotFound {
                    "missing"
                } else {
                    "unavailable"
                },
            );
        }
    };
    let device = metadata.dev();
    let mut bytes = 0_u64;
    let mut unknown_paths = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    let mut complete = true;
    while let Some(path) = pending.pop() {
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            complete = false;
            unknown_paths.push(
                json!({"path":EncodedPath::from_path(&path),"reason_code":"metadata_unavailable"}),
            );
            continue;
        };
        if metadata.file_type().is_symlink() || metadata.dev() != device {
            unknown_paths.push(json!({"path":EncodedPath::from_path(&path),"reason_code":"symlink_or_mount_not_followed"}));
        } else if metadata.is_file() {
            if seen.insert((metadata.dev(), metadata.ino())) {
                if let Some(next) = bytes.checked_add(metadata.len()) {
                    bytes = next;
                } else {
                    complete = false;
                }
            }
        } else if metadata.is_dir() {
            if let Ok(entries) = fs::read_dir(&path) {
                let mut children: Vec<PathBuf> = Vec::new();
                for entry in entries {
                    match entry {
                        Ok(entry)
                            if !(exclude_git && path == root && entry.file_name() == ".git") =>
                        {
                            children.push(entry.path());
                        }
                        Ok(_) => {}
                        Err(_) => {
                            complete = false;
                        }
                    }
                }
                children.sort();
                pending.extend(children.into_iter().rev());
            } else {
                complete = false;
                unknown_paths.push(json!({"path":EncodedPath::from_path(&path),"reason_code":"directory_unavailable"}));
            }
        } else {
            unknown_paths.push(json!({"path":EncodedPath::from_path(&path),"reason_code":"special_file_unmeasured"}));
        }
    }
    json!({"path":EncodedPath::from_path(root),"bytes":if complete {Some(bytes)} else {None},"measured_bytes":bytes,"status":if complete {"measured"} else {"partial"},"unmeasured_paths":unknown_paths,"git_metadata_excluded":exclude_git,"symlinks_followed":false})
}
