use super::{
    TemplateRenderError,
    ast::{SectionKind, TemplateAst, TemplateNode},
};

pub(super) fn parse(template: &str) -> Result<TemplateAst, TemplateRenderError> {
    parse_nodes(template, None).map(|(nodes, _)| TemplateAst(nodes))
}

fn parse_nodes<'a>(
    template: &'a str,
    closing_name: Option<&str>,
) -> Result<(Vec<TemplateNode>, &'a str), TemplateRenderError> {
    let mut nodes = Vec::new();
    let mut rest = template;

    while let Some(open_index) = rest.find("{{") {
        if open_index > 0 {
            nodes.push(TemplateNode::Text(rest[..open_index].to_owned()));
        }
        let after_open = &rest[open_index + 2..];
        let Some(close_index) = after_open.find("}}") else {
            return Err(TemplateRenderError::UnclosedTag {
                tag: after_open.to_owned(),
            });
        };

        let tag = after_open[..close_index].trim();
        if tag.is_empty() {
            return Err(TemplateRenderError::EmptyTag);
        }
        rest = &after_open[close_index + 2..];

        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim();
            return match closing_name {
                Some(expected) if expected == name => Ok((nodes, rest)),
                Some(expected) => Err(TemplateRenderError::MismatchedSection {
                    expected: expected.to_owned(),
                    found: name.to_owned(),
                }),
                None => Err(TemplateRenderError::ClosingSectionWithoutOpen {
                    name: name.to_owned(),
                }),
            };
        }

        let section = tag
            .strip_prefix('#')
            .map(|name| (name, SectionKind::Positive))
            .or_else(|| {
                tag.strip_prefix('^')
                    .map(|name| (name, SectionKind::Inverted))
            });
        if let Some((name, kind)) = section {
            let name = name.trim();
            let (children, remaining) = parse_nodes(rest, Some(name))?;
            nodes.push(TemplateNode::Section {
                name: name.to_owned(),
                kind,
                children,
            });
            rest = remaining;
            continue;
        }

        nodes.push(if tag == "FrontSide" {
            TemplateNode::FrontSide
        } else if let Some(field_name) = tag.strip_prefix("cloze:") {
            TemplateNode::ClozeField(field_name.trim().to_owned())
        } else {
            TemplateNode::Field(tag.to_owned())
        });
    }

    if !rest.is_empty() {
        nodes.push(TemplateNode::Text(rest.to_owned()));
    }

    match closing_name {
        Some(name) => Err(TemplateRenderError::UnclosedSection {
            name: name.to_owned(),
        }),
        None => Ok((nodes, "")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> TemplateNode {
        TemplateNode::Text(value.to_owned())
    }

    fn field(name: &str) -> TemplateNode {
        TemplateNode::Field(name.to_owned())
    }

    #[test]
    fn parses_literal_text() {
        assert_eq!(parse("literal"), Ok(TemplateAst(vec![text("literal")])));
    }

    #[test]
    fn parses_field_between_text() {
        assert_eq!(
            parse("a{{ Word }}b"),
            Ok(TemplateAst(vec![text("a"), field("Word"), text("b")]))
        );
    }

    #[test]
    fn parses_special_references() {
        assert_eq!(
            parse("{{FrontSide}}"),
            Ok(TemplateAst(vec![TemplateNode::FrontSide]))
        );
        assert_eq!(
            parse("{{cloze: Text }}"),
            Ok(TemplateAst(vec![TemplateNode::ClozeField(
                "Text".to_owned()
            )]))
        );
    }

    #[test]
    fn adjacent_tags_have_no_empty_text_nodes() {
        assert_eq!(
            parse("{{First}}{{Second}}"),
            Ok(TemplateAst(vec![field("First"), field("Second")]))
        );
    }

    #[test]
    fn parses_positive_and_inverted_sections() {
        assert_eq!(
            parse("{{#Word}}yes{{/Word}}"),
            Ok(TemplateAst(vec![TemplateNode::Section {
                name: "Word".to_owned(),
                kind: SectionKind::Positive,
                children: vec![text("yes")],
            }]))
        );
        assert_eq!(
            parse("{{^Word}}no{{/Word}}"),
            Ok(TemplateAst(vec![TemplateNode::Section {
                name: "Word".to_owned(),
                kind: SectionKind::Inverted,
                children: vec![text("no")],
            }]))
        );
    }

    #[test]
    fn parses_nested_sections() {
        assert_eq!(
            parse("{{#Outer}}A{{^Inner}}B{{/Inner}}C{{/Outer}}"),
            Ok(TemplateAst(vec![TemplateNode::Section {
                name: "Outer".to_owned(),
                kind: SectionKind::Positive,
                children: vec![
                    text("A"),
                    TemplateNode::Section {
                        name: "Inner".to_owned(),
                        kind: SectionKind::Inverted,
                        children: vec![text("B")],
                    },
                    text("C"),
                ],
            }]))
        );
    }

    #[test]
    fn preserves_whitespace_only_section_names() {
        assert_eq!(
            parse("{{#   }}x{{/   }}"),
            Ok(TemplateAst(vec![TemplateNode::Section {
                name: String::new(),
                kind: SectionKind::Positive,
                children: vec![text("x")],
            }]))
        );
    }

    #[test]
    fn reports_structural_errors() {
        assert_eq!(
            parse("before {{Field"),
            Err(TemplateRenderError::UnclosedTag {
                tag: "Field".to_owned()
            })
        );
        assert_eq!(
            parse("{{#Word}}x"),
            Err(TemplateRenderError::UnclosedSection {
                name: "Word".to_owned()
            })
        );
        assert_eq!(
            parse("{{#Word}}x{{/Other}}"),
            Err(TemplateRenderError::MismatchedSection {
                expected: "Word".to_owned(),
                found: "Other".to_owned(),
            })
        );
        assert_eq!(
            parse("{{/Word}}"),
            Err(TemplateRenderError::ClosingSectionWithoutOpen {
                name: "Word".to_owned()
            })
        );
        assert_eq!(parse("{{   }}"), Err(TemplateRenderError::EmptyTag));
    }

    #[test]
    fn reports_the_innermost_malformed_nested_section() {
        assert_eq!(
            parse("{{#Outer}}{{#Inner}}x{{/Outer}}{{/Inner}}"),
            Err(TemplateRenderError::MismatchedSection {
                expected: "Inner".to_owned(),
                found: "Outer".to_owned(),
            })
        );
    }
}
