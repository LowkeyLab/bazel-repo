//! Public native-source archive policy; these fixtures do not certify an installed CLI.
use cargo_publish_core::digest_file;
use flate2::{Compression, write::GzEncoder};
use googletest::{assert_that, gtest, matchers::eq};
use installer_native::extract_pinned_source;
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
};
fn fixture(root: &Path) -> PathBuf {
    let path = root.join("native.tar.gz");
    let encoder = GzEncoder::new(fs::File::create(&path).unwrap(), Compression::default());
    let mut archive = tar::Builder::new(encoder);
    let mut file = tar::Header::new_gnu();
    file.set_size(12);
    file.set_mode(0o755);
    file.set_cksum();
    archive
        .append_data(&mut file, "native/data", Cursor::new(b"native data\n"))
        .unwrap();
    for (name, target, kind) in [
        ("native/symbolic", "data", tar::EntryType::Symlink),
        ("native/hard", "native/data", tar::EntryType::Link),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o777);
        header.set_entry_type(kind);
        header.set_link_name(target).unwrap();
        header.set_cksum();
        archive
            .append_data(&mut header, name, Cursor::new([]))
            .unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap();
    path
}
#[gtest]
fn checksum_pinned_native_source_preserves_internal_symbolic_and_hard_links() {
    let temporary = tempfile::tempdir().unwrap();
    let archive = fixture(temporary.path());
    let extracted = extract_pinned_source(
        &archive,
        &digest_file(&archive).unwrap(),
        &temporary.path().join("extracted"),
    )
    .unwrap();
    assert_that!(extracted, eq(&temporary.path().join("extracted/native")));
    for name in ["data", "symbolic", "hard"] {
        assert_that!(
            fs::read(extracted.join(name)).unwrap(),
            eq(b"native data\n")
        );
    }
    assert_that!(
        fs::symlink_metadata(extracted.join("symbolic"))
            .unwrap()
            .is_symlink(),
        eq(true)
    );
}

fn raw_archive(
    root: &Path,
    name: &str,
    entries: &[(&str, tar::EntryType, &str, &[u8])],
) -> PathBuf {
    let path = root.join(name);
    let encoder = GzEncoder::new(fs::File::create(&path).unwrap(), Compression::default());
    let mut archive = tar::Builder::new(encoder);
    for (name, kind, target, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o755);
        header.set_size(data.len() as u64);
        header.set_entry_type(*kind);
        header.as_mut_bytes()[..100].fill(0);
        header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
        if !target.is_empty() {
            header.set_link_name(target).unwrap();
        }
        header.set_cksum();
        archive.append(&header, Cursor::new(*data)).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap();
    path
}
#[gtest]
fn checksum_failure_occurs_before_destination_or_decoder_effects() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("bad.tar.xz");
    fs::write(&source, b"not an XZ source").unwrap();
    let destination = temporary.path().join("extracted");
    let result = extract_pinned_source(
        &source,
        &cargo_publish_core::Sha256::new("0".repeat(64)).unwrap(),
        &destination,
    );
    assert_that!(result.is_err(), eq(true));
    assert_that!(
        result
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch"),
        eq(true)
    );
    assert_that!(destination.exists(), eq(false));
}
#[gtest]
fn every_unsafe_native_member_is_rejected_before_any_extraction_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let file = tar::EntryType::Regular;
    let symbolic = tar::EntryType::Symlink;
    let hard = tar::EntryType::Link;
    let cases: Vec<Vec<(&str, tar::EntryType, &str, &[u8])>> = vec![
        vec![("native/../escaped", file, "", b"bad")],
        vec![("/outside", file, "", b"bad")],
        vec![("native/bad", symbolic, "../../outside", b"")],
        vec![("native/bad", hard, "other/data", b"")],
        vec![("native/fifo", tar::EntryType::Fifo, "", b"")],
        vec![("native/data", file, "", b"duplicate")],
        vec![("native/data/child", file, "", b"conflict")],
        vec![
            ("native/alias", symbolic, "data", b""),
            ("native/alias/child", file, "", b"escape"),
        ],
        vec![
            ("native/first", symbolic, "second", b""),
            ("native/second", symbolic, "../../outside", b""),
        ],
        vec![
            ("native/first", symbolic, "second", b""),
            ("native/second", symbolic, "first", b""),
        ],
        vec![("other/data", file, "", b"other root")],
        vec![("native/bad", symbolic, "missing", b"")],
        vec![("native/bad", hard, "native", b"")],
    ];
    for (index, mut entries) in cases.into_iter().enumerate() {
        entries.insert(0, ("native/data", file, "", b"native data\n"));
        let archive = raw_archive(root, &format!("malicious-{index}.tar.gz"), &entries);
        let destination = root.join(format!("extracted-{index}"));
        let result = extract_pinned_source(&archive, &digest_file(&archive).unwrap(), &destination);
        assert_that!(result.is_err(), eq(true));
        assert_that!(destination.exists(), eq(false));
    }
    assert_that!(root.join("escaped").exists(), eq(false));
}
#[gtest]
fn existing_destination_is_preserved() {
    let temporary = tempfile::tempdir().unwrap();
    let archive = fixture(temporary.path());
    let destination = temporary.path().join("existing");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("retained"), b"keep").unwrap();
    assert_that!(
        extract_pinned_source(&archive, &digest_file(&archive).unwrap(), &destination).is_err(),
        eq(true)
    );
    assert_that!(fs::read(destination.join("retained")).unwrap(), eq(b"keep"));
    assert_that!(destination.join("native").exists(), eq(false));
}
fn runfile(path: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("TEST_SRCDIR").unwrap())
        .join(std::env::var_os("TEST_WORKSPACE").unwrap())
        .join(path)
}
#[gtest]
fn actual_pinned_git_236_xz_archive_checksums_decodes_and_preflights() {
    let temporary = tempfile::tempdir().unwrap();
    let extracted = extract_pinned_source(
        &runfile(env!("GIT_SOURCE")),
        &cargo_publish_core::Sha256::new(installer_native::GIT_SHA256).unwrap(),
        &temporary.path().join("git"),
    )
    .unwrap();
    assert_that!(
        extracted.file_name().unwrap().to_str(),
        eq(Some("git-2.36.0"))
    );
    assert_that!(
        fs::read_to_string(extracted.join("GIT-VERSION-GEN"))
            .unwrap()
            .contains("DEF_VER=v2.36.0"),
        eq(true)
    );
    assert_that!(extracted.join("git.c").is_file(), eq(true));
}
#[gtest]
fn actual_pinned_zlib_gzip_archive_checksums_decodes_and_preflights() {
    let temporary = tempfile::tempdir().unwrap();
    let extracted = extract_pinned_source(
        &runfile(env!("ZLIB_SOURCE")),
        &cargo_publish_core::Sha256::new(installer_native::ZLIB_SHA256).unwrap(),
        &temporary.path().join("zlib"),
    )
    .unwrap();
    assert_that!(
        extracted.file_name().unwrap().to_str(),
        eq(Some("zlib-1.3.1"))
    );
    assert_that!(extracted.join("configure").is_file(), eq(true));
}
