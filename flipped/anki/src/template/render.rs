use super::{
    AnkiNoteFields, RenderOptions,
    ast::{SectionKind, TemplateAst, TemplateNode},
};

const DEFAULT_CLOZE_BLANK: &str = "[...]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RenderMode {
    Front(RenderOptions),
    Back,
}

pub(super) fn render(
    ast: &TemplateAst,
    fields: &AnkiNoteFields,
    front_side: Option<&str>,
    mode: RenderMode,
) -> String {
    let mut output = String::new();
    render_nodes(&ast.0, fields, front_side, mode, &mut output);
    output
}

fn render_nodes(
    nodes: &[TemplateNode],
    fields: &AnkiNoteFields,
    front_side: Option<&str>,
    mode: RenderMode,
    output: &mut String,
) {
    for node in nodes {
        match node {
            TemplateNode::Text(text) => output.push_str(text),
            TemplateNode::Field(name) => output.push_str(fields.get(name).unwrap_or_default()),
            TemplateNode::FrontSide => output.push_str(front_side.unwrap_or_default()),
            TemplateNode::ClozeField(name) => {
                let text = fields.get(name).unwrap_or_default();
                match mode {
                    RenderMode::Front(options) => {
                        output.push_str(&render_cloze_front(text, options.cloze_number));
                    }
                    RenderMode::Back => output.push_str(&render_cloze_back(text)),
                }
            }
            TemplateNode::Section {
                name,
                kind,
                children,
            } => {
                let selected = match kind {
                    SectionKind::Positive => fields.is_present(name),
                    SectionKind::Inverted => !fields.is_present(name),
                };
                if selected {
                    render_nodes(children, fields, front_side, mode, output);
                }
            }
        }
    }
}

fn render_cloze_front(text: &str, cloze_number: Option<u32>) -> String {
    render_cloze(text, cloze_number, true)
}

fn render_cloze_back(text: &str) -> String {
    render_cloze(text, None, false)
}

fn render_cloze(text: &str, target: Option<u32>, hide_target: bool) -> String {
    let mut output = String::new();
    let mut rest = text;

    while let Some(open_index) = rest.find("{{c") {
        output.push_str(&rest[..open_index]);
        let after_open = &rest[open_index + 3..];
        let Some(number_end) = after_open.find("::") else {
            output.push_str("{{c");
            rest = after_open;
            continue;
        };
        let Ok(number) = after_open[..number_end].parse::<u32>() else {
            output.push_str("{{c");
            rest = after_open;
            continue;
        };
        let content_start = number_end + 2;
        let Some(close_index) = after_open[content_start..].find("}}") else {
            output.push_str("{{c");
            rest = after_open;
            continue;
        };

        let raw_content = &after_open[content_start..content_start + close_index];
        let (answer, hint) = split_cloze_content(raw_content);
        let should_hide = hide_target && target.is_none_or(|target| target == number);
        if should_hide {
            match hint {
                Some(hint) => {
                    output.push('[');
                    output.push_str(hint);
                    output.push(']');
                }
                None => output.push_str(DEFAULT_CLOZE_BLANK),
            }
        } else {
            output.push_str(answer);
        }

        rest = &after_open[content_start + close_index + 2..];
    }

    output.push_str(rest);
    output
}

fn split_cloze_content(raw_content: &str) -> (&str, Option<&str>) {
    match raw_content.split_once("::") {
        Some((answer, hint)) => (answer, Some(hint)),
        None => (raw_content, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> AnkiNoteFields {
        AnkiNoteFields::new(pairs.iter().copied())
    }

    fn render_front(ast: &TemplateAst, fields: &AnkiNoteFields) -> String {
        render(
            ast,
            fields,
            None,
            RenderMode::Front(RenderOptions::default()),
        )
    }

    #[test]
    fn renders_present_missing_and_untrimmed_fields() {
        let ast = TemplateAst(vec![
            TemplateNode::Field("Present".to_owned()),
            TemplateNode::Field("Missing".to_owned()),
            TemplateNode::Field("Spaced".to_owned()),
        ]);
        assert_eq!(
            render_front(
                &ast,
                &fields(&[("Present", "value"), ("Spaced", "  value  ")])
            ),
            "value  value  "
        );
    }

    #[test]
    fn selects_sections_using_nonblank_presence() {
        let ast = TemplateAst(vec![
            TemplateNode::Section {
                name: "Value".to_owned(),
                kind: SectionKind::Positive,
                children: vec![TemplateNode::Text("yes".to_owned())],
            },
            TemplateNode::Section {
                name: "Value".to_owned(),
                kind: SectionKind::Inverted,
                children: vec![TemplateNode::Text("no".to_owned())],
            },
        ]);

        for value in [None, Some(""), Some("   ")] {
            let values =
                value.map_or_else(AnkiNoteFields::default, |value| fields(&[("Value", value)]));
            assert_eq!(render_front(&ast, &values), "no");
        }
        assert_eq!(render_front(&ast, &fields(&[("Value", "x")])), "yes");
    }

    #[test]
    fn recursively_renders_only_selected_branches() {
        let ast = TemplateAst(vec![TemplateNode::Section {
            name: "Outer".to_owned(),
            kind: SectionKind::Positive,
            children: vec![
                TemplateNode::Text("A".to_owned()),
                TemplateNode::Section {
                    name: "Inner".to_owned(),
                    kind: SectionKind::Inverted,
                    children: vec![
                        TemplateNode::Field("Missing".to_owned()),
                        TemplateNode::Text("B".to_owned()),
                    ],
                },
                TemplateNode::Section {
                    name: "Inner".to_owned(),
                    kind: SectionKind::Positive,
                    children: vec![TemplateNode::Text("unselected".to_owned())],
                },
            ],
        }]);
        assert_eq!(render_front(&ast, &fields(&[("Outer", "x")])), "AB");
        assert_eq!(render_front(&ast, &AnkiNoteFields::default()), "");
    }

    #[test]
    fn supports_front_back_modes_and_front_side() {
        let ast = TemplateAst(vec![TemplateNode::FrontSide]);
        assert_eq!(render_front(&ast, &AnkiNoteFields::default()), "");
        assert_eq!(
            render(
                &ast,
                &AnkiNoteFields::default(),
                Some("<b>rendered front</b>"),
                RenderMode::Back,
            ),
            "<b>rendered front</b>"
        );
    }

    #[test]
    fn renders_cloze_ordinals_and_hints() {
        let ast = TemplateAst(vec![TemplateNode::ClozeField("Text".to_owned())]);
        let values = fields(&[("Text", "{{c1::Paris}} is in {{c2::France}}")]);
        assert_eq!(render_front(&ast, &values), "[...] is in [...]");
        assert_eq!(
            render(
                &ast,
                &values,
                None,
                RenderMode::Front(RenderOptions {
                    cloze_number: Some(2)
                }),
            ),
            "Paris is in [...]"
        );
        assert_eq!(
            render(&ast, &values, None, RenderMode::Back),
            "Paris is in France"
        );

        let hinted = fields(&[("Text", "Capital: {{c1::Paris::city}}")]);
        assert_eq!(
            render(
                &ast,
                &hinted,
                None,
                RenderMode::Front(RenderOptions {
                    cloze_number: Some(1)
                }),
            ),
            "Capital: [city]"
        );
        assert_eq!(
            render(&ast, &hinted, None, RenderMode::Back),
            "Capital: Paris"
        );
    }

    #[test]
    fn malformed_cloze_markers_pass_through() {
        let ast = TemplateAst(vec![TemplateNode::ClozeField("Text".to_owned())]);
        let values = fields(&[("Text", "before {{cX::answer}} after")]);
        for mode in [
            RenderMode::Front(RenderOptions::default()),
            RenderMode::Back,
        ] {
            assert_eq!(
                render(&ast, &values, None, mode),
                "before {{cX::answer}} after"
            );
        }
    }

    #[test]
    fn ordinary_field_values_are_never_parsed_as_templates() {
        let ast = TemplateAst(vec![TemplateNode::Field("Value".to_owned())]);
        let value = "{{Other}}{{#Flag}}x{{/Flag}}{{FrontSide}}";
        assert_eq!(render_front(&ast, &fields(&[("Value", value)])), value);
    }
}
