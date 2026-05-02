use pinyin_composer_wasm::{annotate_phrase, convert_pinyin};

#[test]
fn convert_pinyin_returns_ranked_phrase_candidates() {
    let result = convert_pinyin("wo xiang qu beijing", 3).expect("conversion succeeds");

    assert!(!result.candidates.is_empty());
    assert!(result.candidates.len() <= 3);
    assert_eq!(result.source_pinyin, "wo xiang qu beijing");
    assert!(
        result
            .candidates
            .iter()
            .all(|candidate| candidate.score.is_finite())
    );
    assert!(
        result
            .candidates
            .iter()
            .any(|candidate| candidate.hanzi.contains('我'))
    );
}

#[test]
fn convert_pinyin_clamps_large_candidate_limit() {
    let result = convert_pinyin("chi fan", 10_000).expect("conversion succeeds");

    assert!(!result.candidates.is_empty());
    assert!(result.candidates.len() <= 20);
}

#[test]
fn convert_pinyin_normalizes_whitespace_and_case() {
    let result = convert_pinyin("  BEI   JING  ", 3).expect("conversion succeeds");

    assert_eq!(result.source_pinyin, "bei jing");
    assert!(
        result
            .candidates
            .iter()
            .all(|candidate| candidate.source_pinyin == "bei jing")
    );
}

#[test]
fn zero_candidate_limit_is_rejected() {
    let error = convert_pinyin("beijing", 0).expect_err("zero limit fails");

    assert_eq!(
        error.to_string(),
        "candidate limit must be greater than zero"
    );
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

#[test]
fn blank_hanzi_input_is_rejected() {
    let error = annotate_phrase("   ").expect_err("blank input fails");

    assert_eq!(
        error.to_string(),
        "hanzi input must contain at least one character"
    );
}

#[test]
fn unannotatable_phrase_is_rejected() {
    let error = annotate_phrase("abc").expect_err("latin text has no pinyin annotation");

    assert_eq!(
        error.to_string(),
        "conversion unavailable: no pinyin annotation found for `abc`"
    );
}
