use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use askama_parser::{Node, Parsed, Syntax};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn digest(value: impl AsRef<[u8]>) -> String {
    Sha256::digest(value)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

type Edit = (usize, usize, String);

fn marker(path: &str, id: usize) -> String {
    format!("{{{{ crate::hit({path:?}, {id}) }}}}")
}

fn visit(
    nodes: &[Box<Node<'_>>],
    source: &str,
    path: &str,
    inventory: &mut Vec<Value>,
    edits: &mut Vec<Edit>,
) -> Result<()> {
    for node in nodes {
        let kind = match &**node {
            Node::Lit(_) | Node::Comment(_) => continue,
            Node::Expr(..) => "expression",
            Node::If(_) => "if",
            Node::Loop(_) => "loop",
            Node::Include(_) => "include",
            other => bail!("Unsupported bounded adapter node: {other:?}"),
        };
        let range = node.span().byte_range().context("Missing AST span")?;
        let line = source[..range.start]
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            + 1;
        let column = source[..range.start]
            .rsplit('\n')
            .next()
            .unwrap_or("")
            .len();
        inventory.push(json!({"id":range.start,"kind":kind,"line":line,"column":column,"end":range.end,"arms":if kind == "if" {2} else {0}}));
        match &**node {
            Node::Expr(..) | Node::Loop(_) | Node::Include(_) => {
                let delimiter = if kind == "expression" { "{{" } else { "{%" };
                let opening = source[..range.start]
                    .rfind(delimiter)
                    .context("Missing opening delimiter before AST node")?;
                edits.push((opening, opening, marker(path, range.start)));
            }
            _ => {}
        }
        match &**node {
            Node::If(value) => {
                if value.branches.len() != 2 {
                    bail!("The bounded fixture requires one if/else pair");
                }
                let condition = value.branches[0]
                    .cond
                    .as_ref()
                    .context("Missing if condition")?;
                if condition.target.is_some() || value.branches[1].cond.is_some() {
                    bail!("Only a simple Boolean condition is supported");
                }
                let expr = condition
                    .expr
                    .span()
                    .byte_range()
                    .context("Missing condition expression span")?;
                edits.push((
                    expr.start,
                    expr.end,
                    format!(
                        "crate::branch({path:?}, {}, ({}))",
                        range.start,
                        &source[expr.clone()]
                    ),
                ));
                for branch in &value.branches {
                    visit(&branch.nodes, source, path, inventory, edits)?;
                }
            }
            Node::Loop(value) => {
                if value.cond.is_some() || !value.else_nodes.is_empty() {
                    bail!("Filtered and else loops are outside the bounded adapter");
                }
                visit(&value.body, source, path, inventory, edits)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().context("Missing output directory")?);
    std::fs::create_dir_all(output.join("templates"))?;
    let mut files = Vec::new();
    for input in args {
        let source = std::fs::read_to_string(&input)?;
        let path = format!(
            "tools/research_askama/templates/{}",
            Path::new(&input)
                .file_name()
                .context("Missing filename")?
                .to_string_lossy()
        );
        let parsed = Parsed::new(Arc::from(source.as_str()), None, &Syntax::default())?;
        let mut inventory = Vec::new();
        let mut edits = Vec::new();
        visit(parsed.nodes(), &source, &path, &mut inventory, &mut edits)?;
        let mut transformed = source.clone();
        edits.sort_by(|a, b| b.0.cmp(&a.0));
        for (start, end, text) in edits {
            transformed.replace_range(start..end, &text);
        }
        std::fs::write(
            output
                .join("templates")
                .join(Path::new(&input).file_name().context("Missing filename")?),
            transformed,
        )?;
        files.push(json!({"path":path,"sha256":digest(source.as_bytes()),"nodes":inventory}));
    }
    std::fs::write(
        output.join("baseline.json"),
        serde_json::to_vec_pretty(&json!({"schema":1,"files":files}))?,
    )?;
    Ok(())
}
