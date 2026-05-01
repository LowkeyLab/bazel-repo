use pinyin_composer_wasm::{annotate_phrase, convert_pinyin};

#[test]
fn convert_pinyin_returns_ranked_phrase_candidates() {
    let result = convert_pinyin("wo xiang qu beijing", 3).expect("conversion succeeds");

    assert!(!result.candidates.is_empty());
    assert!(result.candidates.len() <= 3);
    assert!(result.candidates[0].hanzi.contains('我'));
    assert!(result.candidates[0].score.is_finite());
}

#[test]
fn annotate_phrase_returns_phrase_level_pinyin() {
    let result = annotate_phrase("北京").expect("annotation succeeds");

    assert_eq!(result.hanzi, "北京");
    assert_eq!(result.pinyin, "Běijīng");
}

#[test]
fn blank_pinyin_input_is_rejected() {
    let error = convert_pinyin("   ", 3).expect_err("blank input fails");

    assert_eq!(
        error.to_string(),
        "pinyin input must contain at least one syllable"
    );
}
