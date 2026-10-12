//! Controlled protocol fixture; this executable provides no real Cargo certification.
use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
};

use flate2::{Compression, write::GzEncoder};
use serde_json::{Value, json};
use tar::{Builder, Header};

fn main() {
    let executable = env::current_exe().unwrap();
    let root = executable.parent().unwrap();
    let scenario: Value =
        serde_json::from_slice(&fs::read(root.join("scenario.json")).unwrap()).unwrap();
    let args: Vec<String> = env::args().skip(1).collect();
    let mode = scenario["mode"].as_str().unwrap_or("");
    if args == ["-Vv"] || args == ["-V"] {
        probe_toolchain(&executable, &scenario, &args, mode);
        return;
    }
    for key in ["RUSTFLAGS", "RUSTC_WRAPPER", "CC", "CFLAGS", "LD_PRELOAD"] {
        assert!(
            env::var_os(key).is_none(),
            "ambient {key} escaped isolation"
        );
    }
    assert_eq!(env::var("CARGO_BUILD_JOBS").unwrap(), "2");
    if let Some(expected) = scenario
        .get("expected_proxy_environment")
        .and_then(Value::as_object)
    {
        for (key, value) in expected {
            assert_eq!(env::var(key).unwrap(), value.as_str().unwrap());
        }
    }
    let manifest = PathBuf::from(
        &args[args
            .iter()
            .position(|arg| arg == "--manifest-path")
            .unwrap()
            + 1],
    );
    let command = &args[0];
    if mode == command {
        eprintln!("controlled {command} failure");
        std::process::exit(23);
    }
    if command == "package" {
        package_archive(&scenario, &manifest, &args, mode);
    } else if command == "metadata" {
        assert!(args.iter().any(|arg| arg == "--locked"));
        if mode == "malformed_metadata" {
            println!("not-json");
            return;
        }
        let mut metadata = scenario["metadata"].clone();
        for target in metadata["packages"][0]["targets"].as_array_mut().unwrap() {
            target["src_path"] = json!(
                manifest
                    .parent()
                    .unwrap()
                    .join(target["src_path"].as_str().unwrap())
            );
        }
        println!("{metadata}");
    } else if command == "build" {
        assert!(args.iter().any(|arg| arg == "--locked"));
        assert!(args.iter().any(|arg| arg == "--lib" || arg == "--bin"));
        assert!(manifest.to_string_lossy().contains("/extracted/"));
        assert_ne!(env::current_dir().unwrap(), manifest.parent().unwrap());
        if mode == "second_binary" && args.windows(2).any(|pair| pair == ["--bin", "second"]) {
            eprintln!("second binary compilation failed");
            std::process::exit(24);
        }
        if mode == "lock" {
            fs::write(manifest.parent().unwrap().join("Cargo.lock"), "changed").unwrap();
        }
    } else {
        panic!("unexpected command");
    }
}

fn append<W: io::Write>(archive: &mut Builder<W>, name: &str, bytes: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    archive.append_data(&mut header, name, bytes).unwrap();
}

fn add_files<W: io::Write>(
    archive: &mut Builder<W>,
    directory: &Path,
    root: &Path,
    prefix: &str,
    mode: &str,
    normalized: Option<&str>,
) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            add_files(archive, &path, root, prefix, mode, normalized);
        } else {
            let relative = path.strip_prefix(root).unwrap();
            let mut bytes = if relative == Path::new("Cargo.toml")
                && let Some(manifest) = normalized
            {
                manifest.as_bytes().to_vec()
            } else {
                fs::read(&path).unwrap()
            };
            if relative == Path::new("Cargo.toml") && mode == "normalized" {
                bytes.extend_from_slice(b"\n[features]\nforbidden=[]\n");
            }
            append(archive, &format!("{prefix}/{}", relative.display()), &bytes);
        }
    }
}

fn probe_toolchain(executable: &Path, scenario: &Value, args: &[String], mode: &str) {
    let root = executable.parent().unwrap();
    fs::write(
        root.join(format!(
            "probe-{}.json",
            executable.file_name().unwrap().to_string_lossy()
        )),
        serde_json::to_vec(&env::vars().collect::<BTreeMap<_, _>>()).unwrap(),
    )
    .unwrap();
    if mode == "signal" && args == ["-Vv"] {
        // Raise SIGTERM in the child itself so the verifier receives a signaled ExitStatus.
        unsafe extern "C" {
            fn raise(signal: i32) -> i32;
        }
        unsafe {
            raise(15);
        }
    }
    if args == ["-Vv"] {
        println!(
            "rustc 1.96.0 (controlled fixture)\nrelease: {}\nhost: {}",
            if mode == "version" {
                "1.95.0"
            } else {
                scenario["release"].as_str().unwrap()
            },
            if mode == "host" {
                "aarch64-unknown-linux-gnu"
            } else {
                "x86_64-unknown-linux-gnu"
            }
        );
    } else {
        println!("cargo 1.96.0 (controlled fixture)");
    }
}

fn package_archive(scenario: &Value, manifest: &Path, args: &[String], mode: &str) {
    assert!(
        !args
            .iter()
            .any(|arg| arg == "--locked" || arg == "--no-verify")
    );
    assert!(args.iter().any(|arg| arg == "--allow-dirty"));
    let package = &scenario["contract"]["package"];
    let prefix = format!(
        "{}-{}",
        package["name"].as_str().unwrap(),
        package["version"].as_str().unwrap()
    );
    let output = PathBuf::from(env::var_os("CARGO_TARGET_DIR").unwrap()).join("package");
    fs::create_dir_all(&output).unwrap();
    let encoder = GzEncoder::new(
        fs::File::create(output.join(format!("{prefix}.crate"))).unwrap(),
        Compression::default(),
    );
    let mut archive = Builder::new(encoder);
    add_files(
        &mut archive,
        manifest.parent().unwrap(),
        manifest.parent().unwrap(),
        &prefix,
        mode,
        scenario.get("normalized").and_then(Value::as_str),
    );
    let original = if mode == "original" {
        b"changed original".to_vec()
    } else {
        fs::read(manifest).unwrap()
    };
    append(
        &mut archive,
        &format!("{prefix}/Cargo.toml.orig"),
        &original,
    );
    let version = if mode == "lock_version" {
        "1.2.4"
    } else {
        "1.2.3"
    };
    let source = if mode == "lock_source" {
        "registry+https://example.invalid/index"
    } else {
        "registry+https://github.com/rust-lang/crates.io-index"
    };
    let checksum = if mode == "lock_checksum" { "b" } else { "a" }.repeat(64);
    let checksum_field = if mode == "lock_missing_checksum" {
        String::new()
    } else {
        format!("checksum = '{checksum}'\n")
    };
    let entry = format!(
        "\n[[package]]\nname = 'upstream'\nversion = '{version}'\nsource = '{source}'\n{checksum_field}"
    );
    let mut lockfile = "version = 4\n".to_string();
    if mode != "lock_missing_dependency" {
        lockfile.push_str(&entry);
    }
    if mode == "lock_duplicate_dependency" {
        lockfile.push_str(&entry);
    }
    // Cargo can retain additional packages outside the selected direct dependency contract.
    lockfile.push_str("\n[[package]]\nname = 'transitive-only'\nversion = '9.9.9'\nsource = 'registry+https://github.com/rust-lang/crates.io-index'\nchecksum = 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc'\n");
    append(
        &mut archive,
        &format!("{prefix}/Cargo.lock"),
        lockfile.as_bytes(),
    );
    if mode == "archive" {
        append(&mut archive, &format!("{prefix}/unexpected"), b"unexpected");
    }
    archive.into_inner().unwrap().finish().unwrap();
}
