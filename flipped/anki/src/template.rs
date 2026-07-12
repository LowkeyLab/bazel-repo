use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use flipped::{Flashcard, FlippedError};

const DEFAULT_CLOZE_BLANK: &str = "[...]";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateRenderError {
    UnclosedTag { tag: String },
    UnclosedSection { name: String },
    MismatchedSection { expected: String, found: String },
    ClosingSectionWithoutOpen { name: String },
    EmptyTag,
}

impl Display for TemplateRenderError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnclosedTag { tag } => write!(f, "unclosed template tag: {tag}"),
            Self::UnclosedSection { name } => write!(f, "unclosed template section: {name}"),
            Self::MismatchedSection { expected, found } => {
                write!(
                    f,
                    "mismatched template section: expected {expected}, found {found}"
                )
            }
            Self::ClosingSectionWithoutOpen { name } => {
                write!(f, "closing template section without open: {name}")
            }
            Self::EmptyTag => f.write_str("template tag cannot be empty"),
        }
    }
}

impl Error for TemplateRenderError {}

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
    let front = render_part(&template.front, fields, None, RenderMode::Front(options))?;
    let back = render_part(
        &template.back,
        fields,
        Some(front.as_str()),
        RenderMode::Back,
    )?;

    Ok(RenderedCard { front, back })
}

#[derive(Debug, Clone, Copy)]
enum RenderMode {
    Front(RenderOptions),
    Back,
}

fn render_part(
    template: &str,
    fields: &AnkiNoteFields,
    front_side: Option<&str>,
    mode: RenderMode,
) -> Result<String, TemplateRenderError> {
    render_section(template, fields, front_side, mode, None).map(|(rendered, _)| rendered)
}

fn render_section<'a>(
    template: &'a str,
    fields: &AnkiNoteFields,
    front_side: Option<&str>,
    mode: RenderMode,
    closing_name: Option<&str>,
) -> Result<(String, &'a str), TemplateRenderError> {
    let mut output = String::new();
    let mut rest = template;

    while let Some(open_index) = rest.find("{{") {
        output.push_str(&rest[..open_index]);
        let after_open = &rest[open_index + 2..];
        let Some(close_index) = after_open.find("}}") else {
            return Err(TemplateRenderError::UnclosedTag {
                tag: after_open.to_owned(),
            });
        };

        let raw_tag = &after_open[..close_index];
        let tag = raw_tag.trim();
        if tag.is_empty() {
            return Err(TemplateRenderError::EmptyTag);
        }
        rest = &after_open[close_index + 2..];

        if let Some(name) = tag.strip_prefix('/') {
            let name = name.trim();
            return match closing_name {
                Some(expected) if expected == name => Ok((output, rest)),
                Some(expected) => Err(TemplateRenderError::MismatchedSection {
                    expected: expected.to_owned(),
                    found: name.to_owned(),
                }),
                None => Err(TemplateRenderError::ClosingSectionWithoutOpen {
                    name: name.to_owned(),
                }),
            };
        }

        if let Some(name) = tag.strip_prefix('#') {
            let name = name.trim();
            let (body, remaining) = render_conditional_body(rest, fields, front_side, mode, name)?;
            if fields.is_present(name) {
                output.push_str(&body);
            }
            rest = remaining;
            continue;
        }

        if let Some(name) = tag.strip_prefix('^') {
            let name = name.trim();
            let (body, remaining) = render_conditional_body(rest, fields, front_side, mode, name)?;
            if !fields.is_present(name) {
                output.push_str(&body);
            }
            rest = remaining;
            continue;
        }

        output.push_str(&render_replacement(tag, fields, front_side, mode));
    }

    output.push_str(rest);

    if let Some(name) = closing_name {
        Err(TemplateRenderError::UnclosedSection {
            name: name.to_owned(),
        })
    } else {
        Ok((output, ""))
    }
}

fn render_conditional_body<'a>(
    rest: &'a str,
    fields: &AnkiNoteFields,
    front_side: Option<&str>,
    mode: RenderMode,
    name: &str,
) -> Result<(String, &'a str), TemplateRenderError> {
    render_section(rest, fields, front_side, mode, Some(name))
}

fn render_replacement(
    tag: &str,
    fields: &AnkiNoteFields,
    front_side: Option<&str>,
    mode: RenderMode,
) -> String {
    if tag == "FrontSide" {
        return front_side.unwrap_or_default().to_owned();
    }

    if let Some(field_name) = tag.strip_prefix("cloze:") {
        let text = fields.get(field_name.trim()).unwrap_or_default();
        return match mode {
            RenderMode::Front(options) => render_cloze_front(text, options.cloze_number),
            RenderMode::Back => render_cloze_back(text),
        };
    }

    fields.get(tag).unwrap_or_default().to_owned()
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
}
