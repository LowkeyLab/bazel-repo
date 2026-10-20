use std::path::PathBuf;
use std::sync::Mutex;

use askama::Template;
use googletest::prelude::{Result, assert_that, contains_substring, gtest};
use sha2::{Digest, Sha256};

static EVENTS: Mutex<Vec<(&'static str, usize, Option<u8>)>> = Mutex::new(Vec::new());

pub fn hit(path: &'static str, id: usize) -> &'static str {
    EVENTS.lock().expect("event lock").push((path, id, None));
    ""
}

pub fn branch(path: &'static str, id: usize, value: &bool) -> bool {
    hit(path, id);
    EVENTS
        .lock()
        .expect("event lock")
        .push((path, id, Some(u8::from(!*value))));
    *value
}

struct Name<'a> {
    id: u32,
    discord_id: &'a str,
    server_id: &'a str,
    name: &'a str,
}
#[derive(Template)]
#[template(path = "names/names_table.html")]
struct Table<'a> {
    names: &'a [Name<'a>],
}
#[derive(Template)]
#[template(path = "unused.html")]
struct Unused<'a> {
    enabled: bool,
    label: &'a str,
}

#[gtest]
fn maintained_loop_and_include() -> Result<()> {
    let output = PathBuf::from(std::env::var("TEST_UNDECLARED_OUTPUTS_DIR")?);
    let names = [Name {
        id: 42,
        discord_id: "discord",
        server_id: "server",
        name: "<&>",
    }];
    let _unused = std::mem::size_of::<Unused<'static>>();
    let empty = Table { names: &[] }.render()?;
    let populated = Table { names: &names }.render()?;
    assert_that!(populated, contains_substring("&#60;&#38;&#62;"));
    assert_that!(populated, contains_substring("name-row-42"));
    std::fs::write(output.join("render-0.html"), empty)?;
    std::fs::write(output.join("render-1.html"), populated)?;
    let events = serde_json::to_vec_pretty(&*EVENTS.lock().expect("event lock"))?;
    std::fs::write(output.join("events.json"), &events)?;
    let hash = Sha256::digest(&events)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    std::fs::write(
        output.join("completion.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"functional":true,"events_sha256":hash}))?,
    )?;
    std::fs::write(
        output.join("functional.txt"),
        "Both maintained subset assertions passed.\n",
    )?;
    Ok(())
}
