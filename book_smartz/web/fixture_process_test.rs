//! Observe a controlled kernel refusal through the actual packaged executable.
mod fixture_process;
use std::{
    fs::{self, OpenOptions},
    os::fd::AsRawFd,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};
use googletest::{assert_that, matchers::eq};
use runfiles::{Runfiles, rlocation};
use serde_json::{Value, json};

#[googletest::test]
fn held_writer_is_identified_without_disclosing_command_payloads() -> Result<()> {
    let package = unpack_package()?;
    let executable = package.path().join("book-smartz");
    let writer = OpenOptions::new().write(true).open(&executable)?;
    let reader = fs::File::open(&executable)?;
    let other_package = tempfile::tempdir()?;
    let other_executable = other_package.path().join("book-smartz");
    fs::write(&other_executable, b"unrelated executable identity")?;
    let unrelated_writer = OpenOptions::new().write(true).open(&other_executable)?;
    let unrelated_fd = u64::try_from(unrelated_writer.as_raw_fd())?;
    let read_write = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&executable)?;
    let read_write_fd = u64::try_from(read_write.as_raw_fd())?;
    let writer_fd = u64::try_from(writer.as_raw_fd())?;
    let reader_fd = u64::try_from(reader.as_raw_fd())?;
    let pid = u64::from(std::process::id());
    let payload = "private-reader-token-and-book-metadata";
    let mut command = Command::new(&executable);
    command
        .arg(payload)
        .env_clear()
        .env("PRIVATE_FIXTURE_VALUE", payload)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let error = fixture_process::spawn(&mut command, "controlled_writer_spawn")
        .err()
        .context("held writer must prevent execution")?;
    assert_that!(
        error
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error),
        eq(Some(26))
    );
    let message = error.to_string();
    let record: Value = serde_json::from_str(
        message
            .strip_prefix("fixture_process_failure ")
            .with_context(|| format!("structured diagnostic missing: {error}"))?,
    )?;
    assert_that!(
        record["stage"].as_str(),
        eq(Some("controlled_writer_spawn"))
    );
    assert_that!(record["operation"].as_str(), eq(Some("spawn")));
    assert_that!(record["os_error"].as_i64(), eq(Some(26)));
    assert_that!(
        record["executable"]["name"].as_str(),
        eq(Some("book-smartz"))
    );
    let writers = record["observation"]["writers"]
        .as_array()
        .context("writer observations")?;
    assert_that!(
        writers
            .iter()
            .any(|w| w["pid"].as_u64() == Some(pid) && w["fd"].as_u64() == Some(writer_fd)),
        eq(true)
    );
    assert_that!(
        writers
            .iter()
            .any(|w| w["pid"].as_u64() == Some(pid) && w["fd"].as_u64() == Some(reader_fd)),
        eq(false)
    );
    assert_that!(
        writers
            .iter()
            .any(|w| w["pid"].as_u64() == Some(pid) && w["fd"].as_u64() == Some(unrelated_fd)),
        eq(false)
    );
    assert_that!(
        writers.iter().any(|w| w["pid"].as_u64() == Some(pid)
            && w["fd"].as_u64() == Some(read_write_fd)
            && w["access"].as_str() == Some("read_write")),
        eq(true)
    );
    let full_error = format!("{error:#}");
    assert_that!(full_error.contains(payload), eq(false));
    assert_that!(
        format!("{error:#}").contains(package.path().to_str().context("temporary path")?),
        eq(false)
    );
    let status_error = fixture_process::status(&mut command, "controlled_writer_status")
        .err()
        .context("status must preserve kernel refusal")?;
    assert_that!(
        status_error
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error),
        eq(Some(26))
    );
    let status_message = status_error.to_string();
    let status_record: Value = serde_json::from_str(
        status_message
            .strip_prefix("fixture_process_failure ")
            .context("status diagnostic")?,
    )?;
    assert_that!(
        status_record["stage"].as_str(),
        eq(Some("controlled_writer_status"))
    );
    assert_that!(status_record["operation"].as_str(), eq(Some("status")));
    if let Some(path) = std::env::var_os("TEST_UNDECLARED_OUTPUTS_DIR") {
        fs::write(
            std::path::Path::new(&path).join("controlled-held-writer.json"),
            serde_json::to_vec_pretty(
                &json!({"scenario":"controlled_held_writer","diagnostic":record}),
            )?,
        )?;
    }
    drop(writer);
    drop(read_write);
    // The invalid command exits normally after the kernel permits execution.
    let status = fixture_process::status(&mut command, "released_writer_status")?;
    assert_that!(status.code().is_some(), eq(true));
    Ok(())
}

#[googletest::test]
fn a_missing_executable_keeps_its_stage_and_original_error() -> Result<()> {
    let package = tempfile::tempdir()?;
    let mut command = Command::new(package.path().join("book-smartz"));
    let error = fixture_process::spawn(&mut command, "missing_executable")
        .err()
        .context("missing executable must fail")?;
    assert_that!(
        error
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error),
        eq(Some(2))
    );
    let message = error.to_string();
    let record: Value = serde_json::from_str(
        message
            .strip_prefix("fixture_process_failure ")
            .context("structured diagnostic")?,
    )?;
    assert_that!(record["stage"].as_str(), eq(Some("missing_executable")));
    assert_that!(
        record["executable"]["name"].as_str(),
        eq(Some("book-smartz"))
    );
    assert_that!(
        record["observation"]["status"].as_str(),
        eq(Some("not_requested"))
    );
    assert_that!(
        format!("{error:#}").contains(package.path().to_str().context("temporary path")?),
        eq(false)
    );
    Ok(())
}

fn unpack_package() -> Result<tempfile::TempDir> {
    let files = Runfiles::create()?;
    let archive =
        rlocation!(files, std::env::var("PACKAGE_ARCHIVE")?).context("package archive")?;
    let package = tempfile::tempdir()?;
    tar::Archive::new(fs::File::open(archive)?).unpack(package.path())?;
    Ok(package)
}
#[googletest::test]
fn excess_writers_produce_a_limited_report_and_preserve_the_error() -> Result<()> {
    let package = unpack_package()?;
    let executable = package.path().join("book-smartz");
    let _writers = (0..18)
        .map(|_| OpenOptions::new().write(true).open(&executable))
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut command = Command::new(&executable);
    command
        .arg("invalid-fixture-command")
        .env_clear()
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let error = fixture_process::spawn(&mut command, "controlled_writer_limit")
        .err()
        .context("held writers must prevent execution")?;
    assert_that!(
        error
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::raw_os_error),
        eq(Some(26))
    );
    let message = error.to_string();
    let record: Value = serde_json::from_str(
        message
            .strip_prefix("fixture_process_failure ")
            .context("structured diagnostic")?,
    )?;
    assert_that!(
        record["observation"]["writers"]
            .as_array()
            .context("writers")?
            .len(),
        eq(16)
    );
    assert_that!(record["observation"]["limited"].as_bool(), eq(Some(true)));
    assert_that!(record["observation"]["complete"].as_bool(), eq(Some(false)));
    Ok(())
}
