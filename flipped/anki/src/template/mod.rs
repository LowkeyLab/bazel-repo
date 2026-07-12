use std::collections::BTreeMap;

use flipped::{Flashcard, FlippedError};

use self::render::RenderMode;

mod ast;
mod parser;
mod render;

/// Named fields from an Anki note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnkiNoteFields {
    fields: BTreeMap<String, String>,
}

impl AnkiNoteFields {
    #[must_use]
    pub fn new(fields: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) -> Self {
        Self {
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        }
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }

    #[must_use]
    fn is_present(&self, name: &str) -> bool {
        self.get(name).is_some_and(|value| !value.trim().is_empty())
    }
}

/// Front/back templates for a single Anki card template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiCardTemplate {
    pub name: String,
    pub front: String,
    pub back: String,
}

impl AnkiCardTemplate {
    #[must_use]
    pub fn new(name: impl Into<String>, front: impl Into<String>, back: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            front: front.into(),
            back: back.into(),
        }
    }
}

/// Options for rendering an Anki template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderOptions {
    /// The one-based cloze number to hide when rendering `{{cloze:Field}}` on
    /// the front. Anki's first cloze deletion is `c1`, not `c0`.
    pub cloze_number: Option<u32>,
}

/// Rendered front/back content that can be converted into a flipped flashcard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedCard {
    pub front: String,
    pub back: String,
}

impl RenderedCard {
    pub fn into_flashcard(self) -> Result<Flashcard, FlippedError> {
        Flashcard::new(self.front, self.back)
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum TemplateRenderError {
    #[error("unclosed template tag: {tag}")]
    UnclosedTag { tag: String },

    #[error("unclosed template section: {name}")]
    UnclosedSection { name: String },

    #[error("mismatched template section: expected {expected}, found {found}")]
    MismatchedSection { expected: String, found: String },

    #[error("closing template section without open: {name}")]
    ClosingSectionWithoutOpen { name: String },

    #[error("template tag cannot be empty")]
    EmptyTag,
}

pub fn render_template(
    template: &AnkiCardTemplate,
    fields: &AnkiNoteFields,
) -> Result<RenderedCard, TemplateRenderError> {
    render_template_with_options(template, fields, RenderOptions::default())
}

pub fn render_template_with_options(
    template: &AnkiCardTemplate,
    fields: &AnkiNoteFields,
    options: RenderOptions,
) -> Result<RenderedCard, TemplateRenderError> {
    let front_ast = parser::parse(&template.front)?;
    let front = render::render(&front_ast, fields, None, RenderMode::Front(options));
    let back_ast = parser::parse(&template.back)?;
    let back = render::render(&back_ast, fields, Some(front.as_str()), RenderMode::Back);

    Ok(RenderedCard { front, back })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> AnkiNoteFields {
        AnkiNoteFields::new(pairs.iter().copied())
    }

    #[test]
    fn renders_named_fields() {
        let template = AnkiCardTemplate::new("Basic", "{{Word}}", "{{Meaning}}");
        let rendered =
            render_template(&template, &fields(&[("Word", "perro"), ("Meaning", "dog")]))
                .expect("template renders");

        assert_eq!(rendered.front, "perro");
        assert_eq!(rendered.back, "dog");
    }

    #[test]
    fn renders_front_side_on_back() {
        let template = AnkiCardTemplate::new(
            "Basic",
            "<b>{{Word}}</b>",
            "{{FrontSide}}<hr id=answer>{{Meaning}}",
        );
        let rendered =
            render_template(&template, &fields(&[("Word", "perro"), ("Meaning", "dog")]))
                .expect("template renders");

        assert_eq!(rendered.front, "<b>perro</b>");
        assert_eq!(rendered.back, "<b>perro</b><hr id=answer>dog");
    }

    #[test]
    fn renders_conditional_sections() {
        let template = AnkiCardTemplate::new(
            "Basic",
            "{{#Example}}Example: {{Example}}{{/Example}}{{^Hint}}No hint{{/Hint}}",
            "{{Meaning}}",
        );
        let rendered = render_template(
            &template,
            &fields(&[("Example", "El perro corre."), ("Meaning", "dog")]),
        )
        .expect("template renders");

        assert_eq!(rendered.front, "Example: El perro corre.No hint");
        assert_eq!(rendered.back, "dog");
    }

    #[test]
    fn hides_matching_cloze_on_front_and_reveals_all_on_back() {
        let template = AnkiCardTemplate::new("Cloze", "{{cloze:Text}}", "{{cloze:Text}}");
        let rendered = render_template_with_options(
            &template,
            &fields(&[("Text", "{{c1::Paris}} is the capital of {{c2::France}}.")]),
            RenderOptions {
                cloze_number: Some(2),
            },
        )
        .expect("template renders");

        assert_eq!(rendered.front, "Paris is the capital of [...].");
        assert_eq!(rendered.back, "Paris is the capital of France.");
    }

    #[test]
    fn renders_cloze_hints_as_blanks() {
        let template = AnkiCardTemplate::new("Cloze", "{{cloze:Text}}", "{{cloze:Text}}");
        let rendered = render_template_with_options(
            &template,
            &fields(&[("Text", "Capital: {{c1::Paris::city}}")]),
            RenderOptions {
                cloze_number: Some(1),
            },
        )
        .expect("template renders");

        assert_eq!(rendered.front, "Capital: [city]");
        assert_eq!(rendered.back, "Capital: Paris");
    }

    #[test]
    fn rendered_cards_convert_to_flipped_flashcards() {
        let flashcard = RenderedCard {
            front: "hola".to_owned(),
            back: "hello".to_owned(),
        }
        .into_flashcard()
        .expect("valid flipped card");

        assert_eq!(flashcard.front().as_str(), "hola");
        assert_eq!(flashcard.back().as_str(), "hello");
    }

    #[test]
    fn rejects_unclosed_sections() {
        let template = AnkiCardTemplate::new("Broken", "{{#Word}}{{Word}}", "{{Word}}");
        let err = render_template(&template, &fields(&[("Word", "hola")]))
            .expect_err("unclosed section should fail");

        assert_eq!(
            err,
            TemplateRenderError::UnclosedSection {
                name: "Word".to_owned()
            }
        );
    }

    #[test]
    fn preserves_missing_whitespace_and_presence_behavior() {
        let template = AnkiCardTemplate::new(
            "Compatibility",
            "{{Missing}}|{{Spaced}}|{{#Blank}}yes{{/Blank}}{{^Blank}}no{{/Blank}}",
            "back",
        );
        let rendered = render_template(
            &template,
            &fields(&[("Spaced", "  value  "), ("Blank", "   ")]),
        )
        .expect("template renders");
        assert_eq!(rendered.front, "|  value  |no");
    }

    #[test]
    fn renders_nested_sections() {
        let template = AnkiCardTemplate::new(
            "Nested",
            "{{#Outer}}A{{^Inner}}B{{/Inner}}{{#Inner}}C{{/Inner}}{{/Outer}}",
            "back",
        );
        assert_eq!(
            render_template(&template, &fields(&[("Outer", "yes")]))
                .expect("template renders")
                .front,
            "AB"
        );
    }

    #[test]
    fn injects_the_fully_rendered_front() {
        let template = AnkiCardTemplate::new("Basic", "<b>{{Word}}</b>", "{{FrontSide}}");
        assert_eq!(
            render_template(&template, &fields(&[("Word", "rendered")]))
                .expect("template renders")
                .back,
            "<b>rendered</b>"
        );
    }

    #[test]
    fn leaves_runtime_template_looking_values_literal() {
        let template = AnkiCardTemplate::new("Basic", "{{Value}}", "back");
        let value = "{{Other}}{{#Flag}}x{{/Flag}}{{FrontSide}}";
        assert_eq!(
            render_template(&template, &fields(&[("Value", value)]))
                .expect("template renders")
                .front,
            value
        );
    }

    #[test]
    fn reports_front_errors_before_parsing_the_back() {
        let template = AnkiCardTemplate::new("Broken", "{{#Front}}x{{/Wrong}}", "{{#Back}}");
        assert_eq!(
            render_template(&template, &AnkiNoteFields::default()),
            Err(TemplateRenderError::MismatchedSection {
                expected: "Front".to_owned(),
                found: "Wrong".to_owned(),
            })
        );
    }
}
