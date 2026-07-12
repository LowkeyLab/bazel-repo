use super::{
    AnkiNoteFields,
    ast::{SectionKind, TemplateAst, TemplateNode},
};

const DEFAULT_CLOZE_BLANK: &str = "[...]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderContext<'a> {
    Front { cloze_number: Option<u32> },
    Back { front_side: &'a str },
}

pub(super) fn render_front(
    ast: &TemplateAst,
    fields: &AnkiNoteFields,
    cloze_number: Option<u32>,
) -> String {
    render(ast, fields, RenderContext::Front { cloze_number })
}

pub(super) fn render_back(ast: &TemplateAst, fields: &AnkiNoteFields, front_side: &str) -> String {
    render(ast, fields, RenderContext::Back { front_side })
}

fn render(ast: &TemplateAst, fields: &AnkiNoteFields, context: RenderContext<'_>) -> String {
    let mut output = String::new();
    render_nodes(&ast.0, fields, context, &mut output);
    output
}

fn render_nodes(
    nodes: &[TemplateNode],
    fields: &AnkiNoteFields,
    context: RenderContext<'_>,
    output: &mut String,
) {
    for node in nodes {
        match node {
            TemplateNode::Text(text) => output.push_str(text),
            TemplateNode::Field(name) => output.push_str(fields.get(name).unwrap_or_default()),
            TemplateNode::FrontSide => {
                if let RenderContext::Back { front_side } = context {
                    output.push_str(front_side);
                }
            }
            TemplateNode::ClozeField(name) => {
                let text = fields.get(name).unwrap_or_default();
                match context {
                    RenderContext::Front { cloze_number } => {
                        output.push_str(&render_cloze_front(text, cloze_number));
                    }
                    RenderContext::Back { .. } => output.push_str(&render_cloze_back(text)),
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
                    render_nodes(children, fields, context, output);
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

    fn render_default_front(ast: &TemplateAst, fields: &AnkiNoteFields) -> String {
        render_front(ast, fields, None)
    }

    #[test]
    fn renders_present_missing_and_untrimmed_fields() {
        let ast = TemplateAst(vec![
            TemplateNode::Field("Present".to_owned()),
            TemplateNode::Field("Missing".to_owned()),
            TemplateNode::Field("Spaced".to_owned()),
        ]);
        assert_eq!(
            render_default_front(
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
            assert_eq!(render_default_front(&ast, &values), "no");
        }
        assert_eq!(
            render_default_front(&ast, &fields(&[("Value", "x")])),
            "yes"
        );
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
        assert_eq!(render_default_front(&ast, &fields(&[("Outer", "x")])), "AB");
        assert_eq!(render_default_front(&ast, &AnkiNoteFields::default()), "");
    }

    #[test]
    fn supports_front_and_back_rendering_with_front_side() {
        let ast = TemplateAst(vec![TemplateNode::FrontSide]);
        assert_eq!(render_default_front(&ast, &AnkiNoteFields::default()), "");
        assert_eq!(
            render_back(&ast, &AnkiNoteFields::default(), "<b>rendered front</b>"),
            "<b>rendered front</b>"
        );
    }

    #[test]
    fn renders_cloze_ordinals_and_hints() {
        let ast = TemplateAst(vec![TemplateNode::ClozeField("Text".to_owned())]);
        let values = fields(&[("Text", "{{c1::Paris}} is in {{c2::France}}")]);
        assert_eq!(render_default_front(&ast, &values), "[...] is in [...]");
        assert_eq!(render_front(&ast, &values, Some(2)), "Paris is in [...]");
        assert_eq!(render_back(&ast, &values, ""), "Paris is in France");

        let hinted = fields(&[("Text", "Capital: {{c1::Paris::city}}")]);
        assert_eq!(render_front(&ast, &hinted, Some(1)), "Capital: [city]");
        assert_eq!(render_back(&ast, &hinted, ""), "Capital: Paris");
    }

    #[test]
    fn malformed_cloze_markers_pass_through() {
        let ast = TemplateAst(vec![TemplateNode::ClozeField("Text".to_owned())]);
        let values = fields(&[("Text", "before {{cX::answer}} after")]);
        assert_eq!(
            render_default_front(&ast, &values),
            "before {{cX::answer}} after"
        );
        assert_eq!(
            render_back(&ast, &values, ""),
            "before {{cX::answer}} after"
        );
    }

    #[test]
    fn ordinary_field_values_are_never_parsed_as_templates() {
        let ast = TemplateAst(vec![TemplateNode::Field("Value".to_owned())]);
        let value = "{{Other}}{{#Flag}}x{{/Flag}}{{FrontSide}}";
        assert_eq!(
            render_default_front(&ast, &fields(&[("Value", value)])),
            value
        );
    }
}
