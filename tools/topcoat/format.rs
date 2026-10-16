//! Compose the pinned rustfmt with the selected release's macro grammar.
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, ensure};
use topcoat_core_grammar::pretty::{Registry, pretty_print_str};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let rustfmt = args.next().context("missing declared rustfmt tool")?;
    let args: Vec<_> = args.collect();
    let check = args.iter().any(|a| a == "--check");
    let files: Vec<_> = args
        .iter()
        .filter(|a| {
            Path::new(a)
                .extension()
                .is_some_and(|extension| extension == "rs")
        })
        .collect();
    let flags: Vec<_> = args
        .iter()
        .filter(|a| {
            Path::new(a)
                .extension()
                .is_none_or(|extension| extension != "rs")
                && *a != "--check"
        })
        .collect();
    ensure!(!files.is_empty(), "expected Rust source files");
    let mut registry = Registry::new();
    registry.register_macro::<topcoat_view_grammar::view::View>("view");
    registry.register_macro::<topcoat_view_grammar::attributes::Attributes>("attributes");
    registry.register_macro::<topcoat_view_grammar::class::Class>("class");
    registry.register_macro::<topcoat_view_grammar::live::Live>("live");
    registry.register_macro::<topcoat_view_grammar::live::Emit>("emit");
    registry.register_macro::<topcoat_font_grammar::font_face::FontFace>("font_face");
    registry.register_macro::<topcoat_font_grammar::font::Font>("font");
    let mut changed = Vec::new();
    for file in files {
        let original = fs::read_to_string(file)?;
        let mut child = Command::new(&rustfmt)
            .args(&flags)
            .args(["--edition", "2024", "--emit", "stdout"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .context("rustfmt stdin")?
            .write_all(original.as_bytes())?;
        let output = child.wait_with_output()?;
        ensure!(
            output.status.success(),
            "rustfmt failed for {file}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rust = String::from_utf8(output.stdout)?;
        let formatted = pretty_print_str(&registry, &rust)
            .map_err(|errors| anyhow::anyhow!("macro formatting failed for {file}: {errors:?}"))?;
        if formatted != original {
            if check {
                changed.push(file.clone());
            } else {
                fs::write(file, formatted)?;
            }
        }
    }
    ensure!(
        changed.is_empty(),
        "formatting required: {}",
        changed.join(", ")
    );
    Ok(())
}
