use serde_json::json;
use std::{env, fs, path::PathBuf};

fn main() {
    let args: Vec<String> = env::args().collect();
    let root = PathBuf::from(env::var("FAKE_BAZEL_BIN").unwrap());
    if args[1] == "info" {
        println!("{}", root.display());
        return;
    }
    let mode = env::var("FAKE_MODE").unwrap_or_else(|_| "package-failure".into());
    if args[2] == "//worktree_pool/install:acceptance" {
        let value = |flag| PathBuf::from(&args[args.iter().position(|s| s == flag).unwrap() + 1]);
        let archive = value("--archive");
        let report = value("--report");
        if archive
            != root.join("worktree_pool/publish.runs/new-consumer/target/package/consumer.crate")
            || report != root.join("worktree_pool/publish.runs/new-consumer/report.json")
        {
            eprintln!("wrong consumer handoff");
            std::process::exit(29);
        }
        let output = value("--output");
        fs::create_dir(&output).unwrap();
        fs::write(
            output.join("artifact.json"),
            json!({"generic_run_id":"new-consumer"}).to_string(),
        )
        .unwrap();
        if mode == "installed-failure" {
            fs::write(
                output.join("installed-cli.log"),
                "public CLI assertion failed\n",
            )
            .unwrap();
            std::process::exit(23);
        }
        fs::write(output.join("installed-cli.log"), "124 passed\n").unwrap();
        fs::write(output.join("runtime.json"), json!({"helper_scope":"Bazel test_support children; public CLI commands use Cargo-installed artifact"}).to_string()).unwrap();
        fs::write(
            output.join("installation-configuration.json"),
            "private config",
        )
        .unwrap();
        return;
    }
    let (name, path) = match args[2].as_str() {
        "//tools/cargo_publish/fixtures:publish" => {
            ("library", "tools/cargo_publish/fixtures/publish.runs")
        }
        "//tools/cargo_publish/fixtures:binary_publish" => {
            ("binary", "tools/cargo_publish/fixtures/binary_publish.runs")
        }
        "//tools/cargo_publish/fixtures:combined_publish" => (
            "combined",
            "tools/cargo_publish/fixtures/combined_publish.runs",
        ),
        "//worktree_pool:publish" => ("consumer", "worktree_pool/publish.runs"),
        _ => std::process::exit(30),
    };
    let failure = mode == "package-failure" || mode == "collection-failure";
    let run = root.join(path).join(if failure {
        "new-failed-run".into()
    } else {
        format!("new-{name}")
    });
    if mode == "early-failure" {
        fs::create_dir_all(&run).unwrap();
        fs::write(
            run.join("report.json"),
            json!({"status":"incomplete"}).to_string(),
        )
        .unwrap();
        fs::write(run.join("1-toolchain.log"), "toolchain setup failed\n").unwrap();
        std::process::exit(19);
    }
    fs::create_dir_all(run.join("target/package")).unwrap();
    if mode == "redirected-package" && name == "library" {
        let outside = root.join("outside-private");
        fs::create_dir(&outside).unwrap();
        fs::remove_dir(run.join("target/package")).unwrap();
        std::os::unix::fs::symlink(&outside, run.join("target/package")).unwrap();
    }
    let archive = run.join(format!("target/package/{name}.crate"));
    fs::write(&archive, b"controlled external archive").unwrap();
    fs::write(run.join("report.json"), json!({"status":if failure {"incomplete"} else {"success"},"run_id":run.file_name().unwrap().to_str().unwrap()}).to_string()).unwrap();
    fs::write(
        run.join("1-cargo_package.log"),
        if failure {
            "missing crate in offline cache\n"
        } else {
            "package verified\n"
        },
    )
    .unwrap();
    fs::create_dir(run.join("cargo-home")).unwrap();
    fs::write(
        run.join("cargo-home/credentials.toml"),
        "private credential",
    )
    .unwrap();
    if failure {
        if mode == "collection-failure" {
            fs::write(
                PathBuf::from(env::var("FAKE_EVIDENCE").unwrap()).join("library/runs"),
                "blocked collector destination",
            )
            .unwrap();
        }
        eprintln!("--remote_header=x-buildbuddy-api-key=dummy-header-secret");
        std::process::exit(17);
    }
    println!(
        "{}",
        json!({"status":"success","run_id":run.file_name().unwrap().to_str().unwrap(),"archive":archive,"report":run.join("report.json")})
    );
}
