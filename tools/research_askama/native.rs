use std::ffi::CString;
use std::path::PathBuf;

use askama::Template;
use googletest::prelude::{Result, assert_that, contains_substring, gtest};

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

unsafe extern "C" {
    fn __llvm_profile_set_filename(name: *const std::ffi::c_char);
}

fn render_page() -> askama::Result<String> {
    let page = Page {
        enabled: true,
        label: "fixture",
        items: &[3, 5],
    };
    page.render()
}

#[gtest]
fn render_original_templates() -> Result<()> {
    let rendered = render_page()?;
    assert_that!(rendered, contains_substring("yes fixture"));
    assert_that!(rendered, contains_substring("included fixture"));
    assert_that!(rendered, contains_substring("<span>3</span>"));
    assert_that!(rendered, contains_substring("<span>5</span>"));
    let _unused_identity = std::mem::size_of::<Unused<'static>>();
    let output = PathBuf::from(std::env::var("TEST_UNDECLARED_OUTPUTS_DIR")?);
    std::fs::write(
        output.join("functional.txt"),
        "Rust render assertions passed.\n",
    )?;
    std::fs::write(output.join("native-rendered.html"), &rendered)?;
    let profile = CString::new(output.join("native.profraw").to_string_lossy().as_bytes())?;
    // Retain this filename until the profiler writes at process exit.
    unsafe {
        __llvm_profile_set_filename(profile.into_raw());
    }
    Ok(())
}
