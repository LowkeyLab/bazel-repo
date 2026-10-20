use std::path::PathBuf;
use std::sync::Mutex;

use askama::Template;
use googletest::prelude::{Result, assert_that, contains_substring, gtest};

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

#[derive(Template)]
#[template(path = "main.html")]
struct Page<'a> {
    label: &'a str,
    enabled: bool,
    items: &'a [u32],
}

#[derive(Template)]
#[template(path = "unused.html")]
struct Unused<'a> {
    label: &'a str,
    enabled: bool,
}

#[gtest]
fn whitespace_and_loop_matrix() -> Result<()> {
    let output = PathBuf::from(std::env::var("TEST_UNDECLARED_OUTPUTS_DIR")?);
    let _unused = std::mem::size_of::<Unused<'static>>();
    for (index, (enabled, items)) in [
        (true, &[][..]),
        (true, &[3, 5][..]),
        (false, &[][..]),
        (false, &[3, 5][..]),
    ]
    .into_iter()
    .enumerate()
    {
        let text = Page {
            label: "<&>",
            enabled,
            items,
        }
        .render()?;
        assert_that!(text, contains_substring("&#60;&#38;&#62;"));
        if !items.is_empty() {
            assert_that!(text, contains_substring("<x>3</x><x>5</x>"));
        }
        std::fs::write(output.join(format!("render-{index}.html")), text)?;
    }
    std::fs::write(
        output.join("functional.txt"),
        "Four render assertions passed.\n",
    )?;
    std::fs::write(
        output.join("events.json"),
        serde_json::to_vec_pretty(&*EVENTS.lock().expect("event lock"))?,
    )?;
    Ok(())
}
