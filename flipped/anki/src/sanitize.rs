use ammonia::Builder;

use crate::error::{ImportError, ImportErrorCode, Result};

const FORBIDDEN_ELEMENTS: &[&str] = &[
    "audio", "embed", "iframe", "img", "object", "source", "track", "video",
];
const FORBIDDEN_ATTRIBUTES: &[&str] = &["data", "href", "poster", "src"];

pub(crate) fn plain_text_side(source: &str, front: bool, max_bytes: usize) -> Result<String> {
    let decoded = html_escape::decode_html_entities(source).into_owned();
    if contains_sound_reference(&decoded) || contains_forbidden_markup(&decoded) {
        return Err(ImportError::new(ImportErrorCode::MediaRejected));
    }

    let with_breaks = replace_layout_tags(&decoded);
    let sanitized = Builder::default()
        .tags(Default::default())
        .clean(&with_breaks)
        .to_string();
    let normalized = sanitized
        .replace("&nbsp;", " ")
        .replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() {
        return Err(ImportError::new(if front {
            ImportErrorCode::EmptyFront
        } else {
            ImportErrorCode::EmptyBack
        }));
    }
    if normalized.len() > max_bytes {
        return Err(ImportError::new(ImportErrorCode::SqliteLimitExceeded));
    }
    Ok(normalized)
}

fn contains_sound_reference(source: &str) -> bool {
    source
        .as_bytes()
        .windows(b"[sound:".len())
        .any(|window| window.eq_ignore_ascii_case(b"[sound:"))
}

/// Tokenizes HTML start tags sufficiently to distinguish element/attribute syntax from text.
/// Sanitization remains Ammonia's job; this pass only enforces the closed media/reference policy.
fn contains_forbidden_markup(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let Some(relative) = bytes[cursor..].iter().position(|byte| *byte == b'<') else {
            break;
        };
        cursor += relative + 1;
        if bytes
            .get(cursor..)
            .is_some_and(|tail| tail.starts_with(b"!--"))
        {
            cursor += 3;
            if let Some(end) = bytes[cursor..]
                .windows(3)
                .position(|window| window == b"-->")
            {
                cursor += end + 3;
            } else {
                break;
            }
            continue;
        }
        if matches!(bytes.get(cursor), Some(b'!') | Some(b'?')) {
            cursor = skip_to_tag_end(bytes, cursor + 1);
            continue;
        }
        let closing = bytes.get(cursor) == Some(&b'/');
        if closing {
            cursor += 1;
        }
        let name_start = cursor;
        while bytes.get(cursor).is_some_and(|byte| is_name_byte(*byte)) {
            cursor += 1;
        }
        if name_start == cursor {
            continue;
        }
        let element = &source[name_start..cursor];
        if FORBIDDEN_ELEMENTS
            .iter()
            .any(|forbidden| element.eq_ignore_ascii_case(forbidden))
        {
            return true;
        }
        if closing {
            cursor = skip_to_tag_end(bytes, cursor);
            continue;
        }

        loop {
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            match bytes.get(cursor) {
                None => break,
                Some(b'>') => {
                    cursor += 1;
                    break;
                }
                Some(b'/') => {
                    cursor += 1;
                    continue;
                }
                _ => {}
            }
            let attribute_start = cursor;
            while bytes.get(cursor).is_some_and(|byte| {
                !byte.is_ascii_whitespace() && !matches!(byte, b'=' | b'/' | b'>')
            }) {
                cursor += 1;
            }
            if attribute_start == cursor {
                cursor += 1;
                continue;
            }
            let attribute = &source[attribute_start..cursor];
            if FORBIDDEN_ATTRIBUTES
                .iter()
                .any(|forbidden| attribute.eq_ignore_ascii_case(forbidden))
            {
                return true;
            }
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            if bytes.get(cursor) != Some(&b'=') {
                continue;
            }
            cursor += 1;
            while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
            if let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() {
                cursor += 1;
                while bytes.get(cursor).is_some_and(|byte| *byte != quote) {
                    cursor += 1;
                }
                if bytes.get(cursor).is_some() {
                    cursor += 1;
                }
            } else {
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b'/' | b'>'))
                {
                    cursor += 1;
                }
            }
        }
    }
    false
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'-')
}

fn skip_to_tag_end(bytes: &[u8], mut cursor: usize) -> usize {
    let mut quote = None;
    while let Some(byte) = bytes.get(cursor).copied() {
        match (quote, byte) {
            (Some(expected), current) if expected == current => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return cursor + 1,
            _ => {}
        }
        cursor += 1;
    }
    cursor
}

fn replace_layout_tags(source: &str) -> String {
    let mut value = source
        .replace("<br>", " ")
        .replace("<br/>", " ")
        .replace("<br />", " ");
    for tag in ["p", "div", "li", "tr", "h1", "h2", "h3", "h4", "h5", "h6"] {
        value = value.replace(&format!("</{tag}>"), " ");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_entities_and_layout_to_plain_text() {
        let text = plain_text_side("Hello&nbsp;<br><b>world</b>", true, 100)
            .expect("ordinary safe HTML is accepted");
        assert_eq!(text, "Hello world");
    }

    #[test]
    fn rejects_structural_media_with_spacing_and_case_variants() {
        for source in [
            "<IMG SRC = 'card.png'>",
            "<a HREF = https://example.test>text</a>",
            "<video poster = cover.png></video>",
            "<object DATA = deck.bin></object>",
        ] {
            assert_eq!(
                plain_text_side(source, true, 100)
                    .expect_err("media reference is rejected")
                    .code,
                ImportErrorCode::MediaRejected,
                "accepted {source}",
            );
        }
    }

    #[test]
    fn attribute_like_plain_text_and_comments_do_not_false_positive() {
        for source in [
            "metadata=value and src=literal",
            "2 < 3 and href = ordinary text",
            "<!-- <img src=ignored> -->safe",
            "<span data-label='safe'>metadata=value</span>",
        ] {
            plain_text_side(source, true, 200).unwrap_or_else(|error| {
                panic!("safe text {source:?} was rejected as {:?}", error.code)
            });
        }
    }

    #[test]
    fn rejects_media_and_empty_sides() {
        assert_eq!(
            plain_text_side("<img src=x>", true, 100)
                .expect_err("media is rejected")
                .code,
            ImportErrorCode::MediaRejected
        );
        assert_eq!(
            plain_text_side("<script>ignored</script>", false, 100)
                .expect_err("empty sanitized back is rejected")
                .code,
            ImportErrorCode::EmptyBack
        );
    }
}
