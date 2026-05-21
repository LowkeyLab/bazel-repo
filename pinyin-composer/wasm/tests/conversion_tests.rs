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
fn convert_pinyin_accepts_joined_juede() {
    let result = convert_pinyin("juede", 5).expect("juede conversion succeeds");

    assert_eq!(result.source_pinyin, "juede");
    assert!(
        result
            .candidates
            .iter()
            .any(|candidate| candidate.hanzi == "觉得")
    );
}

#[test]
fn convert_pinyin_accepts_spaced_jue_de() {
    let result = convert_pinyin("jue de", 5).expect("jue de conversion succeeds");

    assert_eq!(result.source_pinyin, "jue de");
    assert!(
        result
            .candidates
            .iter()
            .any(|candidate| candidate.hanzi == "觉得")
    );
}

#[test]
fn convert_pinyin_accepts_sentence_context_wo_juede() {
    let result = convert_pinyin("wo juede", 5).expect("wo juede conversion succeeds");

    assert_eq!(result.source_pinyin, "wo juede");
    assert!(
        result
            .candidates
            .iter()
            .any(|candidate| candidate.hanzi.contains("觉得"))
    );
}

#[test]
fn convert_pinyin_accepts_sentence_context_wo_jue_de() {
    let result = convert_pinyin("wo jue de", 5).expect("wo jue de conversion succeeds");

    assert_eq!(result.source_pinyin, "wo jue de");
    assert!(
        result
            .candidates
            .iter()
            .any(|candidate| candidate.hanzi.contains("觉得"))
    );
}

#[test]
fn convert_pinyin_does_not_inject_juede_for_unrelated_input() {
    let result = convert_pinyin("wo qu le", 5).expect("wo qu le conversion succeeds");

    assert_eq!(result.source_pinyin, "wo qu le");
    assert!(
        result
            .candidates
            .iter()
            .all(|candidate| !candidate.hanzi.contains("觉得"))
    );
}

#[test]
fn convert_pinyin_deduplicates_juede_candidate() {
    let result = convert_pinyin("wo jue de", 20).expect("wo jue de conversion succeeds");
    let juede_count = result
        .candidates
        .iter()
        .filter(|candidate| candidate.hanzi.contains("觉得"))
        .count();

    assert!(juede_count <= 1);
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
fn convert_pinyin_exposes_candidate_syllables() {
    let result = convert_pinyin("bei jing", 3).expect("conversion succeeds");
    let candidate = result
        .candidates
        .iter()
        .find(|candidate| candidate.hanzi == "北京")
        .expect("beijing candidate exists");

    assert_eq!(candidate.source_pinyin_syllables, ["bei", "jing"]);
    assert_eq!(candidate.display_pinyin_syllables, ["Běi", "jīng"]);
    assert_eq!(candidate.display_pinyin, "Běijīng");
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
    assert_eq!(result.pinyin_syllables, ["Běi", "jīng"]);
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
