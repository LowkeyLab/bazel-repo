use std::path::PathBuf;
use std::sync::Mutex;

use askama::Template;
use googletest::prelude::{Result, assert_that, contains_substring, gtest};
use sha2::{Digest, Sha256};

fn digest(value: impl AsRef<[u8]>) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

static EVENTS: Mutex<Vec<(&'static str, usize, Option<u8>)>> = Mutex::new(Vec::new());

pub fn hit(path: &'static str, id: usize) -> &'static str {
    EVENTS
        .lock()
        .expect("Event lock failed")
        .push((path, id, None));
    ""
}

pub fn branch(path: &'static str, id: usize, value: &bool) -> bool {
    let value = *value;
    hit(path, id);
    EVENTS
        .lock()
        .expect("Event lock failed")
        .push((path, id, Some(u8::from(!value))));
    value
}

#[derive(Template)]
#[template(path = "main.html")]
struct Page<'a> {
    enabled: bool,
    label: &'a str,
    items: &'a [u32],
}

#[derive(Template)]
#[template(path = "unused.html")]
struct Unused<'a> {
    enabled: bool,
    label: &'a str,
}

fn render_page() -> askama::Result<String> {
    Page {
        enabled: true,
        label: "fixture",
        items: &[3, 5],
    }
    .render()
}

#[gtest]
fn render_instrumented_templates() -> Result<()> {
    let rendered = render_page()?;
    assert_that!(rendered, contains_substring("yes fixture"));
    assert_that!(rendered, contains_substring("included fixture"));
    assert_that!(rendered, contains_substring("<span>3</span>"));
    assert_that!(rendered, contains_substring("<span>5</span>"));
    let _unused_identity = std::mem::size_of::<Unused<'static>>();
    let output = PathBuf::from(std::env::var("TEST_UNDECLARED_OUTPUTS_DIR")?);
    std::fs::write(
        output.join("template-functional.txt"),
        "Rust render assertions passed.\n",
    )?;
    std::fs::write(output.join("adapter-rendered.html"), &rendered)?;
    let baseline = std::fs::read(std::env::var("ASKAMA_BASELINE")?)?;
    let baseline_hash = digest(&baseline);
    let events = EVENTS.lock().expect("Event lock failed");
    let profile = serde_json::to_vec_pretty(
        &serde_json::json!({"schema":1,"baseline_sha256":baseline_hash,"events":*events}),
    )?;
    std::fs::write(output.join("template-profile.json"), &profile)?;
    std::fs::write(
        output.join("template-completion.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"complete":true,"profile_sha256":digest(&profile)}),
        )?,
    )?;
    Ok(())
}
