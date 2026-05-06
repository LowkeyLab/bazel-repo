use pinyin::ToPinyin;

use crate::error::EngineError;
use crate::model::AnnotationResult;

pub fn annotate_phrase(hanzi: &str) -> Result<AnnotationResult, EngineError> {
    let trimmed = hanzi.trim();
    if trimmed.is_empty() {
        return Err(EngineError::BlankHanziInput);
    }

    let pinyin_syllables = trimmed
        .chars()
        .filter_map(|character| character.to_pinyin())
        .map(|pinyin| pinyin.with_tone().to_string())
        .collect::<Vec<_>>();

    if pinyin_syllables.is_empty() {
        return Err(EngineError::ConversionUnavailable(format!(
            "no pinyin annotation found for `{trimmed}`"
        )));
    }

    let display_pinyin_syllables = title_case_first_syllable(&pinyin_syllables);

    Ok(AnnotationResult {
        hanzi: trimmed.to_string(),
        pinyin: display_pinyin_syllables.join(""),
        pinyin_syllables: display_pinyin_syllables,
    })
}

fn title_case_first_syllable(syllables: &[String]) -> Vec<String> {
    syllables
        .iter()
        .enumerate()
        .map(|(index, syllable)| {
            if index == 0 {
                title_case_first(syllable)
            } else {
                syllable.clone()
            }
        })
        .collect()
}

fn title_case_first(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}
