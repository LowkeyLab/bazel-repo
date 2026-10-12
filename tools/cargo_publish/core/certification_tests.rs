use std::io::Read;

use googletest::{
    assert_that, gtest,
    matchers::{anything, elements_are, eq, err, is_false, ok},
};
use serde_json::json;

use crate::{
    Check, CheckStatus, ExpectedPackage, ReportStatus, ValidationReport, digest_bytes,
    inspect_archive, validate_report, validate_verified_archive, write_json,
};

fn library_report() -> ValidationReport {
    serde_json::from_str(include_str!("testdata/library-report.json")).unwrap()
}
fn combined_report() -> ValidationReport {
    serde_json::from_str(include_str!("testdata/combined-report.json")).unwrap()
}

#[gtest]
fn missing_required_checks_never_certify_success() {
    let baseline = library_report();
    assert_that!(validate_report(&baseline, None), ok(anything()));
    for phase in Check::ALL {
        let mut missing = baseline.clone();
        missing.checks.remove(&phase);
        assert_that!(
            validate_report(&missing, None),
            err(anything()),
            "missing {phase:?}"
        );
        let mut pending = baseline.clone();
        pending.checks.get_mut(&phase).unwrap().status = CheckStatus::Pending;
        assert_that!(
            validate_report(&pending, None),
            err(anything()),
            "pending {phase:?}"
        );
        let mut incomplete = baseline.clone();
        incomplete.checks.get_mut(&phase).unwrap().completed = false;
        assert_that!(
            validate_report(&incomplete, None),
            err(anything()),
            "incomplete {phase:?}"
        );
        let mut empty = baseline.clone();
        empty.checks.get_mut(&phase).unwrap().evidence.clear();
        assert_that!(
            validate_report(&empty, None),
            err(anything()),
            "empty {phase:?}"
        );
        let mut inconsistent = baseline.clone();
        let record = inconsistent.checks.get_mut(&phase).unwrap();
        let first = record.evidence.keys().next().unwrap().clone();
        record
            .evidence
            .insert(first, json!("inconsistent evidence"));
        assert_that!(
            validate_report(&inconsistent, None),
            err(anything()),
            "inconsistent {phase:?}"
        );
    }
}

#[gtest]
fn successful_report_requires_real_selected_target_command_coverage() {
    let baseline = combined_report();
    assert_that!(validate_report(&baseline, None), ok(anything()));
    for fault in 0..12 {
        let mut report = baseline.clone();
        let phase = match fault {
            0..=5 => Check::Toolchain,
            6 => Check::LockedMetadata,
            7 | 10 | 11 => Check::LockedBuilds,
            _ => Check::CargoPackage,
        };
        let index = report
            .commands
            .iter()
            .position(|command| command.phase == phase)
            .unwrap();
        match fault {
            0 => report.commands[index].id = report.commands[index + 1].id.clone(),
            1 => report.commands[index].completed = false,
            2 => report.commands[index].returncode = Some(17),
            3 => report.commands[index].phase = Check::InputFidelity,
            4 => report.commands[index].argv.clear(),
            5 => report.commands[index].log.clear(),
            6 | 7 => report.commands[index]
                .argv
                .retain(|argument| argument != "--locked"),
            8 => report.commands[index].argv.push("--locked".into()),
            9 => report.commands[index].argv.push("--no-verify".into()),
            10 => report.commands[index]
                .argv
                .retain(|argument| argument != "build"),
            11 => {
                let bin = report
                    .commands
                    .iter_mut()
                    .find(|command| command.argv.iter().any(|argument| argument == "--bin"))
                    .unwrap();
                let selector = bin
                    .argv
                    .iter()
                    .position(|argument| argument == "--bin")
                    .unwrap();
                bin.argv[selector + 1] = "not-selected".into();
            }
            _ => unreachable!(),
        }
        assert_that!(
            validate_report(&report, None),
            err(anything()),
            "command fault {fault}"
        );
    }
    let mut report = baseline.clone();
    report.selection.as_mut().unwrap().binaries.pop().unwrap();
    let targets: Vec<String> = report
        .selection
        .as_ref()
        .unwrap()
        .selected_targets()
        .unwrap()
        .iter()
        .map(ToString::to_string)
        .collect();
    for phase in [Check::GeneratedManifest, Check::LockedBuilds] {
        report
            .checks
            .get_mut(&phase)
            .unwrap()
            .evidence
            .insert("targets".into(), json!(targets));
    }
    assert_that!(
        validate_report(&report, None),
        err(anything()),
        "commands still build a removed target"
    );
}

#[gtest]
fn report_rejects_unpinned_toolchains_wrong_consumers_and_malformed_contracts() {
    let baseline = library_report();
    for (name, version) in [
        ("another-package", "0.1.0"),
        ("publishing-fixture", "99.0.0"),
    ] {
        let expected = ExpectedPackage {
            name: name.into(),
            version: version.into(),
        };
        assert_that!(validate_report(&baseline, Some(&expected)), err(anything()));
    }
    for fault in 0..7 {
        let mut report = baseline.clone();
        match fault {
            0 => report.toolchain.as_mut().unwrap().rustc = "release: 1.95.0\n".into(),
            1 => report.toolchain.as_mut().unwrap().cargo = "cargo 1.95.0 (old)\n".into(),
            2 => report.toolchain.as_mut().unwrap().host = "aarch64-unknown-linux-gnu".into(),
            3 => report.configuration.as_mut().unwrap().target = "x86_64-unknown-linux-musl".into(),
            4 => report.status = ReportStatus::Failure,
            5 => report.completed_at = None,
            6 => report.schema_version = 2,
            _ => unreachable!(),
        }
        // Keep repeated evidence consistent to exercise the pinned policy.
        report
            .checks
            .get_mut(&Check::Toolchain)
            .unwrap()
            .evidence
            .insert("versions".into(), json!(report.toolchain));
        report
            .checks
            .get_mut(&Check::Configuration)
            .unwrap()
            .evidence = serde_json::from_value(json!(report.configuration)).unwrap();
        assert_that!(
            validate_report(&report, None),
            err(anything()),
            "report fault {fault}"
        );
    }
    for (pointer, value) in [
        ("/archive/sha256", json!("not-a-digest")),
        ("/archive/inventory/0/path", json!("../escape")),
        ("/commands/0/returncode", json!("0")),
        ("/checks/toolchain/evidence", json!([])),
    ] {
        let mut malformed = serde_json::to_value(&baseline).unwrap();
        *malformed.pointer_mut(pointer).unwrap() = value;
        assert_that!(
            serde_json::from_value::<ValidationReport>(malformed),
            err(anything()),
            "malformed {pointer}"
        );
    }
    assert_that!(
        serde_json::from_str::<ValidationReport>("[]"),
        err(anything())
    );
    let incomplete = json!({"schema_version":1,"report_kind":"cargo_publish_validation","run_id":"failed-run","status":"failure","completed":false,"started_at":"2026-10-11T19:00:00+00:00","completed_at":null,"checks":{"input_fidelity":{"status":"pending","completed":false}},"commands":[],"diagnostics":{"phase":"input_fidelity","message":"cannot read declared input"}});
    let report: ValidationReport = serde_json::from_value(incomplete.clone()).unwrap();
    assert_that!(serde_json::to_value(&report).unwrap(), eq(&incomplete));
    assert_that!(validate_report(&report, None), err(anything()));
}

#[gtest]
fn actual_library_binary_and_combined_archives_interoperate_with_typed_handoffs() {
    for (name, source, bytes) in [
        (
            "library",
            include_str!("testdata/library-report.json"),
            include_bytes!("testdata/library.crate").as_slice(),
        ),
        (
            "binary",
            include_str!("testdata/binary-report.json"),
            include_bytes!("testdata/binary.crate").as_slice(),
        ),
        (
            "combined",
            include_str!("testdata/combined-report.json"),
            include_bytes!("testdata/combined.crate").as_slice(),
        ),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let report: ValidationReport = serde_json::from_str(source).unwrap();
        assert_that!(
            serde_json::to_value(&report).unwrap(),
            eq(&serde_json::from_str::<serde_json::Value>(source).unwrap())
        );
        let report_path = scratch.path().join("report.json");
        write_json(&report_path, &report).unwrap();
        let archive = scratch.path().join("relocated.crate");
        std::fs::write(&archive, bytes).unwrap();
        let inspected = inspect_archive(
            &archive,
            report.package.as_ref().unwrap(),
            report.inputs.as_ref().unwrap(),
            scratch.path().join("inspection"),
        )
        .unwrap();
        assert_that!(
            inspected.inventory,
            eq(&report.archive.as_ref().unwrap().inventory),
            "{name}"
        );
        let verified = validate_verified_archive(
            &report_path,
            &archive,
            scratch.path().join("verified"),
            None,
        )
        .unwrap();
        assert_that!(
            crate::digest_file(verified.extracted_root.join("Cargo.lock")).unwrap(),
            eq(&report.lockfile.as_ref().unwrap().sha256),
            "{name}"
        );
    }
}

#[derive(Clone)]
struct Member {
    name: String,
    kind: tar::EntryType,
    contents: Vec<u8>,
}
fn library_members() -> Vec<Member> {
    let decoder = flate2::read::GzDecoder::new(include_bytes!("testdata/library.crate").as_slice());
    let mut archive = tar::Archive::new(decoder);
    archive
        .entries()
        .unwrap()
        .map(|entry| {
            let mut entry = entry.unwrap();
            let name = String::from_utf8(entry.path_bytes().to_vec()).unwrap();
            let kind = entry.header().entry_type();
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents).unwrap();
            Member {
                name,
                kind,
                contents,
            }
        })
        .collect()
}
fn gzip_archive(members: &[Member]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for member in members {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(member.kind);
        header.set_size(member.contents.len() as u64);
        header.set_mode(0o644);
        // Raw names bypass builder validation to exercise malicious real tar data.
        header.as_mut_bytes()[..member.name.len()].copy_from_slice(member.name.as_bytes());
        if member.kind.is_symlink() || member.kind.is_hard_link() {
            header.set_link_name("../../outside").unwrap();
        }
        header.set_cksum();
        archive.append(&header, member.contents.as_slice()).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}

#[gtest]
fn unsafe_or_unfaithful_archives_fail_before_extraction_writes() {
    let report = library_report();
    let baseline = library_members();
    let root = "publishing-fixture-0.1.0";
    let mut bad_members = Vec::new();
    for suffix in [
        "../escape",
        "./Cargo.toml",
        "/Cargo.toml",
        "src\\escape",
        "UNDECLARED.md",
    ] {
        let mut members = baseline.clone();
        members.push(Member {
            name: format!("{root}/{suffix}"),
            kind: tar::EntryType::Regular,
            contents: b"unexpected".to_vec(),
        });
        bad_members.push(members);
    }
    for (name, kind) in [
        ("/outside", tar::EntryType::Regular),
        ("other-0.1.0/file", tar::EntryType::Regular),
        ("publishing-fixture-0.1.0/src/link", tar::EntryType::Symlink),
        ("publishing-fixture-0.1.0/src/hard", tar::EntryType::Link),
        ("publishing-fixture-0.1.0/src/fifo", tar::EntryType::Fifo),
        ("publishing-fixture-0.1.0/src/device", tar::EntryType::Char),
        (
            "publishing-fixture-0.1.0/undeclared/",
            tar::EntryType::Directory,
        ),
    ] {
        let mut members = baseline.clone();
        members.push(Member {
            name: name.into(),
            kind,
            contents: Vec::new(),
        });
        bad_members.push(members);
    }
    let mut duplicate = baseline.clone();
    duplicate.push(baseline[0].clone());
    bad_members.push(duplicate);
    for missing in ["Cargo.lock", "Cargo.toml", "Cargo.toml.orig", "src/lib.rs"] {
        bad_members.push(
            baseline
                .iter()
                .filter(|member| member.name != format!("{root}/{missing}"))
                .cloned()
                .collect(),
        );
    }
    let mut changed = baseline.clone();
    changed
        .iter_mut()
        .find(|member| member.name.ends_with("/src/lib.rs"))
        .unwrap()
        .contents = b"altered source".to_vec();
    bad_members.push(changed);
    for (index, members) in bad_members.iter().enumerate() {
        let scratch = tempfile::tempdir().unwrap();
        let archive = scratch.path().join("malicious.crate");
        std::fs::write(&archive, gzip_archive(members)).unwrap();
        let destination = scratch.path().join("destination");
        assert_that!(
            inspect_archive(
                &archive,
                report.package.as_ref().unwrap(),
                report.inputs.as_ref().unwrap(),
                &destination
            ),
            err(anything()),
            "archive fault {index}"
        );
        assert_that!(
            destination.exists(),
            is_false(),
            "archive fault {index} wrote extraction output"
        );
        assert_that!(scratch.path().join("escape").exists(), is_false());
    }
    for corrupt in [
        b"not a gzip archive".to_vec(),
        include_bytes!("testdata/library.crate")[..100].to_vec(),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let archive = scratch.path().join("corrupt.crate");
        std::fs::write(&archive, corrupt).unwrap();
        let destination = scratch.path().join("destination");
        assert_that!(
            inspect_archive(
                &archive,
                report.package.as_ref().unwrap(),
                report.inputs.as_ref().unwrap(),
                &destination
            ),
            err(anything())
        );
        assert_that!(destination.exists(), is_false());
    }
}

#[gtest]
fn declared_paths_and_archive_directories_cannot_conflict_or_overwrite_destinations() {
    let report = library_report();
    let scratch = tempfile::tempdir().unwrap();
    let archive = scratch.path().join("archive.crate");
    std::fs::write(&archive, include_bytes!("testdata/library.crate")).unwrap();
    for path in ["Cargo.toml", "src/lib.rs", "src"] {
        let mut inputs = report.inputs.clone().unwrap();
        let mut input = inputs[0].clone();
        input.path = crate::PackagePath::new(path).unwrap();
        inputs.push(input);
        let destination = scratch.path().join("rejected");
        assert_that!(
            inspect_archive(
                &archive,
                report.package.as_ref().unwrap(),
                &inputs,
                &destination
            ),
            err(anything())
        );
        assert_that!(destination.exists(), is_false());
    }
    let mut wrong_size = report.inputs.clone().unwrap();
    wrong_size[0].size += 1;
    let destination = scratch.path().join("wrong-size");
    assert_that!(
        inspect_archive(
            &archive,
            report.package.as_ref().unwrap(),
            &wrong_size,
            &destination
        ),
        err(anything())
    );
    assert_that!(destination.exists(), is_false());
    let destination = scratch.path().join("existing");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("preserve"), "original").unwrap();
    assert_that!(
        inspect_archive(
            &archive,
            report.package.as_ref().unwrap(),
            report.inputs.as_ref().unwrap(),
            &destination
        ),
        err(anything())
    );
    assert_that!(
        std::fs::read_to_string(destination.join("preserve")).unwrap(),
        eq("original")
    );
    let mut members = library_members();
    for directory in [
        "publishing-fixture-0.1.0/",
        "publishing-fixture-0.1.0/src/",
        "publishing-fixture-0.1.0/assets/",
    ] {
        members.push(Member {
            name: directory.into(),
            kind: tar::EntryType::Directory,
            contents: Vec::new(),
        });
    }
    std::fs::write(&archive, gzip_archive(&members)).unwrap();
    let inspected = inspect_archive(
        &archive,
        report.package.as_ref().unwrap(),
        report.inputs.as_ref().unwrap(),
        scratch.path().join("directories"),
    )
    .unwrap();
    let directories: Vec<String> = inspected
        .inventory
        .iter()
        .filter_map(|entry| match entry {
            crate::ArchiveEntry::Directory { path } => Some(path.to_string()),
            _ => None,
        })
        .collect();
    assert_that!(directories, elements_are![eq("assets"), eq("src")]);
}

#[gtest]
fn verified_handoff_rejects_byte_inventory_lockfile_and_manifest_tampering() {
    let baseline = library_report();
    for fault in 0..7 {
        let scratch = tempfile::tempdir().unwrap();
        let mut report = baseline.clone();
        let archive = scratch.path().join("archive.crate");
        std::fs::write(&archive, include_bytes!("testdata/library.crate")).unwrap();
        let wrong = digest_bytes(b"wrong");
        match fault {
            0 => {
                std::fs::write(&archive, b"tampered bytes").unwrap();
            }
            1 => report.archive.as_mut().unwrap().size += 1,
            2 => {
                if let crate::ArchiveEntry::File { sha256, .. } =
                    &mut report.archive.as_mut().unwrap().inventory[0]
                {
                    *sha256 = wrong.clone();
                }
                report
                    .checks
                    .get_mut(&Check::ArchivePolicy)
                    .unwrap()
                    .evidence
                    .insert(
                        "inventory".into(),
                        json!(report.archive.as_ref().unwrap().inventory),
                    );
            }
            3 => {
                report.lockfile.as_mut().unwrap().sha256 = wrong.clone();
                report
                    .checks
                    .get_mut(&Check::LockedMetadata)
                    .unwrap()
                    .evidence
                    .insert("lockfile_sha256".into(), json!(wrong));
            }
            4 => {
                report
                    .manifests
                    .as_mut()
                    .unwrap()
                    .original
                    .as_mut()
                    .unwrap()
                    .sha256 = wrong;
                report
                    .checks
                    .get_mut(&Check::OriginalManifest)
                    .unwrap()
                    .evidence =
                    serde_json::from_value(json!(report.manifests.as_ref().unwrap().original))
                        .unwrap();
            }
            5 => {
                report
                    .manifests
                    .as_mut()
                    .unwrap()
                    .normalized
                    .as_mut()
                    .unwrap()
                    .sha256 = wrong;
                report
                    .checks
                    .get_mut(&Check::NormalizedManifest)
                    .unwrap()
                    .evidence
                    .insert(
                        "manifest".into(),
                        json!(report.manifests.as_ref().unwrap().normalized),
                    );
            }
            6 => {
                report.manifests.as_mut().unwrap().generated.sha256 = wrong.clone();
                report
                    .checks
                    .get_mut(&Check::InputFidelity)
                    .unwrap()
                    .evidence
                    .insert("manifest_sha256".into(), json!(wrong));
            }
            _ => unreachable!(),
        }
        // Repeated report evidence is internally consistent. Only actual bytes
        // can reject these dishonest inventory/lockfile/manifest fingerprints.
        assert_that!(validate_report(&report, None), ok(anything()));
        let report_path = scratch.path().join("report.json");
        write_json(&report_path, &report).unwrap();
        assert_that!(
            validate_verified_archive(
                &report_path,
                &archive,
                scratch.path().join("rejected"),
                None
            ),
            err(anything()),
            "handoff fault {fault}"
        );
    }
}

#[gtest]
fn failed_spawn_reports_retain_command_and_phase_errors_without_certification() {
    // This literal follows the existing Python producer's OSError report shape;
    // genuine successful Cargo reports are separate interoperability fixtures.
    let failed = json!({
        "schema_version":1,"report_kind":"cargo_publish_validation","run_id":"failed-spawn",
        "status":"failure","completed":false,"started_at":"2026-10-11T19:00:00+00:00",
        "completed_at":"2026-10-11T19:00:01+00:00",
        "checks":{"toolchain":{"status":"failed","completed":false,"error":"cannot execute pinned rustc"}},
        "commands":[{"id":"1","phase":"toolchain","argv":["/missing/rustc","-Vv"],
          "cwd":"/validation/run","log":"/validation/run/1-toolchain.log",
          "started_at":"2026-10-11T19:00:00+00:00","completed_at":"2026-10-11T19:00:01+00:00",
          "completed":true,"returncode":null,"error":"cannot execute pinned rustc"}],
        "diagnostics":{"phase":"toolchain","message":"cannot execute pinned rustc"}
    });
    let report: ValidationReport = serde_json::from_value(failed.clone()).unwrap();
    assert_that!(serde_json::to_value(&report).unwrap(), eq(&failed));
    assert_that!(validate_report(&report, None), err(anything()));
}

#[gtest]
fn unsuccessful_archive_fidelity_never_certifies_success() {
    let baseline = library_report();
    assert_that!(validate_report(&baseline, None), ok(anything()));
    for status in [CheckStatus::Pending, CheckStatus::Failed] {
        let mut report = baseline.clone();
        report.archive.as_mut().unwrap().fidelity = status;
        assert_that!(validate_report(&report, None), err(anything()));
    }
}

#[gtest]
fn offline_reports_require_offline_cargo_commands() {
    let mut baseline = combined_report();
    assert_that!(validate_report(&baseline, None), ok(anything()));
    baseline.configuration.as_mut().unwrap().network_mode = crate::NetworkMode::Offline;
    baseline
        .checks
        .get_mut(&Check::Configuration)
        .unwrap()
        .evidence = serde_json::from_value(json!(baseline.configuration)).unwrap();
    for command in &mut baseline.commands {
        if command.phase != Check::Toolchain {
            command.argv.push("--offline".into());
        }
    }
    assert_that!(validate_report(&baseline, None), ok(anything()));
    for (index, command) in baseline.commands.iter().enumerate() {
        if command.phase == Check::Toolchain {
            continue;
        }
        let mut report = baseline.clone();
        report.commands[index]
            .argv
            .retain(|argument| argument != "--offline");
        assert_that!(validate_report(&report, None), err(anything()));
    }
}
