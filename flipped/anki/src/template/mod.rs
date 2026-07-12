use std::collections::BTreeMap;

use flipped::{Flashcard, FlippedError};

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
    cloze_number: Option<u32>,
) -> Result<RenderedCard, TemplateRenderError> {
    let front_ast = parser::parse(&template.front)?;
    let front = render::render_front(&front_ast, fields, cloze_number);
    let back_ast = parser::parse(&template.back)?;
    let back = render::render_back(&back_ast, fields, &front);

    Ok(RenderedCard { front, back })
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> AnkiNoteFields {
        AnkiNoteFields::new(pairs.iter().copied())
    }

    #[test]
    fn renders_named_fields() -> Result<()> {
        let template = AnkiCardTemplate::new("Basic", "{{Word}}", "{{Meaning}}");
        let rendered = match render_template(
            &template,
            &fields(&[("Word", "perro"), ("Meaning", "dog")]),
            None,
        ) {
            Ok(rendered) => rendered,
            Err(error) => return fail!("template renders; unexpected error: {:?}", error),
        };

        verify_that!(rendered.front, eq("perro"))?;
        verify_that!(rendered.back, eq("dog"))?;
        Ok(())
    }

    #[test]
    fn renders_front_side_on_back() -> Result<()> {
        let template = AnkiCardTemplate::new(
            "Basic",
            "<b>{{Word}}</b>",
            "{{FrontSide}}<hr id=answer>{{Meaning}}",
        );
        let rendered = match render_template(
            &template,
            &fields(&[("Word", "perro"), ("Meaning", "dog")]),
            None,
        ) {
            Ok(rendered) => rendered,
            Err(error) => return fail!("template renders; unexpected error: {:?}", error),
        };

        verify_that!(rendered.front, eq("<b>perro</b>"))?;
        verify_that!(rendered.back, eq("<b>perro</b><hr id=answer>dog"))?;
        Ok(())
    }

    #[test]
    fn renders_conditional_sections() -> Result<()> {
        let template = AnkiCardTemplate::new(
            "Basic",
            "{{#Example}}Example: {{Example}}{{/Example}}{{^Hint}}No hint{{/Hint}}",
            "{{Meaning}}",
        );
        let rendered = match render_template(
            &template,
            &fields(&[("Example", "El perro corre."), ("Meaning", "dog")]),
            None,
        ) {
            Ok(rendered) => rendered,
            Err(error) => return fail!("template renders; unexpected error: {:?}", error),
        };

        verify_that!(rendered.front, eq("Example: El perro corre.No hint"))?;
        verify_that!(rendered.back, eq("dog"))?;
        Ok(())
    }

    #[test]
    fn hides_matching_cloze_on_front_and_reveals_all_on_back() -> Result<()> {
        let template = AnkiCardTemplate::new("Cloze", "{{cloze:Text}}", "{{cloze:Text}}");
        let rendered = match render_template(
            &template,
            &fields(&[("Text", "{{c1::Paris}} is the capital of {{c2::France}}.")]),
            Some(2),
        ) {
            Ok(rendered) => rendered,
            Err(error) => return fail!("template renders; unexpected error: {:?}", error),
        };

        verify_that!(rendered.front, eq("Paris is the capital of [...]."))?;
        verify_that!(rendered.back, eq("Paris is the capital of France."))?;
        Ok(())
    }

    #[test]
    fn renders_cloze_hints_as_blanks() -> Result<()> {
        let template = AnkiCardTemplate::new("Cloze", "{{cloze:Text}}", "{{cloze:Text}}");
        let rendered = match render_template(
            &template,
            &fields(&[("Text", "Capital: {{c1::Paris::city}}")]),
            Some(1),
        ) {
            Ok(rendered) => rendered,
            Err(error) => return fail!("template renders; unexpected error: {:?}", error),
        };

        verify_that!(rendered.front, eq("Capital: [city]"))?;
        verify_that!(rendered.back, eq("Capital: Paris"))?;
        Ok(())
    }

    #[test]
    fn rendered_cards_convert_to_flipped_flashcards() -> Result<()> {
        let flashcard = match (RenderedCard {
            front: "hola".to_owned(),
            back: "hello".to_owned(),
        })
        .into_flashcard()
        {
            Ok(flashcard) => flashcard,
            Err(error) => return fail!("valid flipped card; unexpected error: {:?}", error),
        };

        verify_that!(flashcard.front().as_str(), eq("hola"))?;
        verify_that!(flashcard.back().as_str(), eq("hello"))?;
        Ok(())
    }

    #[test]
    fn rejects_unclosed_sections() -> Result<()> {
        let template = AnkiCardTemplate::new("Broken", "{{#Word}}{{Word}}", "{{Word}}");
        let err = match render_template(&template, &fields(&[("Word", "hola")]), None) {
            Err(error) => error,
            Ok(value) => {
                return fail!(
                    "unclosed section should fail; unexpected value: {:?}",
                    value
                );
            }
        };

        verify_that!(
            err,
            eq(&TemplateRenderError::UnclosedSection {
                name: "Word".to_owned()
            })
        )?;
        Ok(())
    }

    #[test]
    fn preserves_missing_whitespace_and_presence_behavior() -> Result<()> {
        let template = AnkiCardTemplate::new(
            "Compatibility",
            "{{Missing}}|{{Spaced}}|{{#Blank}}yes{{/Blank}}{{^Blank}}no{{/Blank}}",
            "back",
        );
        let rendered = match render_template(
            &template,
            &fields(&[("Spaced", "  value  "), ("Blank", "   ")]),
            None,
        ) {
            Ok(rendered) => rendered,
            Err(error) => return fail!("template renders; unexpected error: {:?}", error),
        };
        verify_that!(rendered.front, eq("|  value  |no"))?;
        Ok(())
    }

    #[test]
    fn renders_nested_sections() -> Result<()> {
        let template = AnkiCardTemplate::new(
            "Nested",
            "{{#Outer}}A{{^Inner}}B{{/Inner}}{{#Inner}}C{{/Inner}}{{/Outer}}",
            "back",
        );
        let rendered = match render_template(&template, &fields(&[("Outer", "yes")]), None) {
            Ok(rendered) => rendered,
            Err(error) => return fail!("template renders; unexpected error: {:?}", error),
        };
        verify_that!(rendered.front, eq("AB"))?;
        Ok(())
    }

    #[test]
    fn injects_the_fully_rendered_front() -> Result<()> {
        let template = AnkiCardTemplate::new("Basic", "<b>{{Word}}</b>", "{{FrontSide}}");
        let rendered = match render_template(&template, &fields(&[("Word", "rendered")]), None) {
            Ok(rendered) => rendered,
            Err(error) => return fail!("template renders; unexpected error: {:?}", error),
        };
        verify_that!(rendered.back, eq("<b>rendered</b>"))?;
        Ok(())
    }

    #[test]
    fn leaves_runtime_template_looking_values_literal() -> Result<()> {
        let template = AnkiCardTemplate::new("Basic", "{{Value}}", "back");
        let value = "{{Other}}{{#Flag}}x{{/Flag}}{{FrontSide}}";
        let rendered = match render_template(&template, &fields(&[("Value", value)]), None) {
            Ok(rendered) => rendered,
            Err(error) => return fail!("template renders; unexpected error: {:?}", error),
        };
        verify_that!(rendered.front, eq(value))?;
        Ok(())
    }

    #[test]
    fn reports_front_errors_before_parsing_the_back() -> Result<()> {
        let template = AnkiCardTemplate::new("Broken", "{{#Front}}x{{/Wrong}}", "{{#Back}}");
        verify_that!(
            render_template(&template, &AnkiNoteFields::default(), None),
            err(eq(&TemplateRenderError::MismatchedSection {
                expected: "Front".to_owned(),
                found: "Wrong".to_owned(),
            }))
        )?;
        Ok(())
    }
}
