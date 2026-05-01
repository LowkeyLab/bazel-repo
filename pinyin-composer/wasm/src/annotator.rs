use pinyin::ToPinyin;

use crate::error::EngineError;
use crate::model::AnnotationResult;

pub fn annotate_phrase(hanzi: &str) -> Result<AnnotationResult, EngineError> {
    let trimmed = hanzi.trim();
    if trimmed.is_empty() {
        return Err(EngineError::BlankHanziInput);
    }

    let pinyin_parts = trimmed
        .chars()
        .filter_map(|character| character.to_pinyin())
        .map(|pinyin| pinyin.with_tone().to_string())
        .collect::<Vec<_>>();

    if pinyin_parts.is_empty() {
        return Err(EngineError::ConversionUnavailable(format!(
            "no pinyin annotation found for `{trimmed}`"
        )));
    }

    Ok(AnnotationResult {
        hanzi: trimmed.to_string(),
        pinyin: title_case_first(&pinyin_parts.join("")),
    })
}

fn title_case_first(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}
