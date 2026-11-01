//! The production text boundary counts extended grapheme clusters after normalization.
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Text {
    pub name: String,
    pub biography: String,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Errors {
    pub name: Option<&'static str>,
    pub biography: Option<&'static str>,
}
impl Errors {
    pub const fn any(self) -> bool {
        self.name.is_some() || self.biography.is_some()
    }
}
impl Text {
    pub fn validate(name: &str, biography: &str) -> Result<Self, Errors> {
        let name = name.trim().to_owned();
        let biography = if biography.chars().all(char::is_whitespace) {
            String::new()
        } else {
            biography.replace("\r\n", "\n").replace('\r', "\n")
        };
        let mut errors = Errors::default();
        if name.chars().any(|c| {
            matches!(
                c,
                '\n' | '\r' | '\u{000b}' | '\u{000c}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
            )
        }) {
            errors.name = Some("Use one line for your display name.");
        } else if name.graphemes(true).count() > 100 {
            errors.name = Some("Use 100 visible characters or fewer for your display name.");
        }
        if biography.graphemes(true).count() > 500 {
            errors.biography = Some("Use 500 visible characters or fewer for your biography.");
        }
        if errors.any() {
            Err(errors)
        } else {
            Ok(Self { name, biography })
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::{assert_that, matchers::eq};

    use super::Text;
    #[googletest::test]
    fn optional_text_normalizes_whitespace_and_preserves_paragraphs() {
        let empty = Text::validate("\u{0085}\t\n ", "\r\n\u{2003}").unwrap();
        assert_that!(empty, eq(&Text::default()));
        let text =
            Text::validate("  Reader  ", " First paragraph.\r\n\r\nSecond paragraph. ").unwrap();
        assert_that!(text.name, eq("Reader"));
        assert_that!(
            text.biography,
            eq(" First paragraph.\n\nSecond paragraph. ")
        );
        for separator in ["\n", "\r", "\u{0085}", "\u{2028}", "\u{2029}"] {
            let errors = Text::validate(&format!("A{separator}B"), "").unwrap_err();
            assert_that!(errors.name, eq(Some("Use one line for your display name.")));
            assert_that!(errors.biography.is_none(), eq(true));
        }
    }
    #[googletest::test]
    fn limits_count_graphemes_in_accents_joined_emoji_and_flags() {
        for unit in ["a", "é", "e\u{301}", "👩‍👩‍👧‍👦", "🇬🇧", "👍🏽"] {
            let name = unit.repeat(100);
            let biography = unit.repeat(500);
            let text = Text::validate(&format!(" {name} "), &biography).unwrap();
            assert_that!(text.name, eq(&name));
            assert_that!(text.biography, eq(&biography));
            let errors = Text::validate(&unit.repeat(101), &unit.repeat(501)).unwrap_err();
            assert_that!(
                errors.name,
                eq(Some(
                    "Use 100 visible characters or fewer for your display name."
                ))
            );
            assert_that!(
                errors.biography,
                eq(Some(
                    "Use 500 visible characters or fewer for your biography."
                ))
            );
        }
    }
}
