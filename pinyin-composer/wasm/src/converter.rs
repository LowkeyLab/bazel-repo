use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

use pinyinchch::hmm::viterbi;
use pinyinchch::pinyin::{pinyin_split, pinyin_split_by_trie_tokenizer};
use pinyinchch_model_hmm::DefaultHmm;

use crate::annotator::annotate_phrase;
use crate::error::EngineError;
use crate::model::{Candidate, ConversionResult};

const MAX_CANDIDATE_LIMIT: usize = 20;
const MAX_EXHAUSTIVE_SPLIT_INPUT_LEN: usize = 32;
const MAX_EXHAUSTIVE_SPLIT_VARIANTS: usize = 64;

struct MistakableWord {
    syllables: &'static [&'static str],
    hanzi: &'static str,
}

static MISTAKABLE_WORDS: &[MistakableWord] = &[MistakableWord {
    syllables: &["jue", "de"],
    hanzi: "觉得",
}];

pub fn convert_pinyin(source_pinyin: &str, limit: usize) -> Result<ConversionResult, EngineError> {
    let normalized = normalize_pinyin_input(source_pinyin)?;
    if limit == 0 {
        return Err(EngineError::CandidateLimitMustBePositive);
    }

    let source_pinyin_syllables = syllables_from_normalized_pinyin(&normalized)?;
    let capped_limit = limit.min(MAX_CANDIDATE_LIMIT);
    let decoded = decode_ranked_candidates(&normalized, &source_pinyin_syllables, capped_limit)?;
    let candidates = decoded
        .into_iter()
        .enumerate()
        .map(|(index, decoded)| {
            let annotation = annotate_phrase(&decoded.hanzi)?;
            Ok(Candidate {
                id: format!("candidate-{index}"),
                source_pinyin: normalized.clone(),
                source_pinyin_syllables: source_pinyin_syllables.clone(),
                hanzi: decoded.hanzi,
                display_pinyin: annotation.pinyin,
                display_pinyin_syllables: annotation.pinyin_syllables,
                score: decoded.score,
            })
        })
        .collect::<Result<Vec<_>, EngineError>>()?;

    Ok(ConversionResult {
        source_pinyin: normalized,
        candidates,
    })
}

struct DecodedCandidate {
    hanzi: String,
    score: f64,
    variant_order: usize,
}

fn normalize_pinyin_input(source_pinyin: &str) -> Result<String, EngineError> {
    let normalized = source_pinyin
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    if normalized.is_empty() {
        return Err(EngineError::BlankPinyinInput);
    }

    Ok(normalized)
}

fn syllables_from_normalized_pinyin(normalized: &str) -> Result<Vec<String>, EngineError> {
    let tokenized = pinyin_split_by_trie_tokenizer(normalized);
    let syllables = tokenized
        .split_whitespace()
        .map(|syllable| syllable.to_string())
        .collect::<Vec<_>>();

    if syllables.is_empty() {
        return Err(EngineError::BlankPinyinInput);
    }

    Ok(syllables)
}

fn decode_ranked_candidates(
    normalized: &str,
    syllables: &[String],
    limit: usize,
) -> Result<Vec<DecodedCandidate>, EngineError> {
    let hmm = DefaultHmm::default();
    let fast_candidates = decode_syllable_candidates(&hmm, syllables, limit, 0);
    let candidates = if fast_candidates.is_empty() {
        exhaustive_split_candidates(&hmm, normalized, limit)
    } else {
        fast_candidates
    };
    let candidates = dedupe_and_rank_candidates(candidates, limit);

    if candidates.is_empty() {
        return Err(EngineError::ConversionUnavailable(format!(
            "no candidate found for `{normalized}`"
        )));
    }

    Ok(candidates)
}

fn exhaustive_split_candidates(
    hmm: &DefaultHmm,
    normalized: &str,
    limit: usize,
) -> Vec<DecodedCandidate> {
    let compact = normalized.split_whitespace().collect::<String>();
    if compact.len() > MAX_EXHAUSTIVE_SPLIT_INPUT_LEN {
        return Vec::new();
    }

    let variants = pinyin_split(&compact);
    let variants = variants
        .into_iter()
        .take(MAX_EXHAUSTIVE_SPLIT_VARIANTS)
        .enumerate()
        .map(|(variant_order, variant)| {
            let syllables = variant
                .split_whitespace()
                .map(|syllable| syllable.to_string())
                .collect::<Vec<_>>();
            (variant_order, syllables)
        })
        .collect::<Vec<_>>();

    let mut candidates = variants
        .iter()
        .flat_map(|(variant_order, syllables)| {
            decode_syllable_candidates(hmm, syllables, limit, *variant_order)
        })
        .collect::<Vec<_>>();

    let mistakable_candidates = MISTAKABLE_WORDS
        .iter()
        .filter(|entry| !candidates_contain_mistakable_word(&candidates, entry))
        .flat_map(|entry| {
            variants.iter().flat_map(move |(variant_order, syllables)| {
                mistakable_word_candidates(hmm, entry, syllables, *variant_order)
            })
        })
        .collect::<Vec<_>>();

    candidates.extend(mistakable_candidates);

    candidates
}

fn decode_syllable_candidates(
    hmm: &DefaultHmm,
    syllables: &[String],
    limit: usize,
    variant_order: usize,
) -> Vec<DecodedCandidate> {
    let syllable_refs = syllables.iter().map(String::as_str).collect::<Vec<_>>();

    viterbi(hmm, &syllable_refs, limit, false, 3.14e-200)
        .into_iter()
        .map(|candidate| DecodedCandidate {
            hanzi: candidate.path().iter().collect(),
            score: candidate.score(),
            variant_order,
        })
        .collect::<Vec<_>>()
}

fn candidates_contain_mistakable_word(
    candidates: &[DecodedCandidate],
    entry: &MistakableWord,
) -> bool {
    candidates
        .iter()
        .any(|candidate| candidate.hanzi.contains(entry.hanzi))
}

fn mistakable_word_candidates(
    hmm: &DefaultHmm,
    entry: &MistakableWord,
    syllables: &[String],
    variant_order: usize,
) -> Vec<DecodedCandidate> {
    assert!(
        !entry.syllables.is_empty(),
        "mistakable word entry must contain at least one syllable"
    );

    syllables
        .windows(entry.syllables.len())
        .enumerate()
        .filter(|(_, window)| mistakable_syllable_window_matches(window, entry.syllables))
        .filter_map(|(index, _)| {
            let (prefix_hanzi, prefix_score) = decode_best_hanzi(hmm, &syllables[..index])?;
            let suffix_start = index + entry.syllables.len();
            let (suffix_hanzi, suffix_score) = decode_best_hanzi(hmm, &syllables[suffix_start..])?;
            let score = if prefix_hanzi.is_empty() && suffix_hanzi.is_empty() {
                1.0
            } else {
                prefix_score * suffix_score
            };

            Some(DecodedCandidate {
                hanzi: format!("{prefix_hanzi}{}{suffix_hanzi}", entry.hanzi),
                score,
                variant_order,
            })
        })
        .collect()
}

fn mistakable_syllable_window_matches(window: &[String], entry_syllables: &[&str]) -> bool {
    window
        .iter()
        .map(String::as_str)
        .eq(entry_syllables.iter().copied())
}

fn decode_best_hanzi(hmm: &DefaultHmm, syllables: &[String]) -> Option<(String, f64)> {
    if syllables.is_empty() {
        return Some((String::new(), 1.0));
    }

    decode_syllable_candidates(hmm, syllables, 1, 0)
        .into_iter()
        .next()
        .map(|candidate| (candidate.hanzi, candidate.score))
}

fn dedupe_and_rank_candidates(
    candidates: Vec<DecodedCandidate>,
    limit: usize,
) -> Vec<DecodedCandidate> {
    let mut best_by_hanzi = HashMap::new();

    for candidate in candidates {
        match best_by_hanzi.entry(candidate.hanzi.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(candidate);
            }
            Entry::Occupied(mut entry) => {
                if is_better_candidate(&candidate, entry.get()) {
                    entry.insert(candidate);
                }
            }
        }
    }

    let mut candidates = best_by_hanzi.into_values().collect::<Vec<_>>();
    candidates.sort_by(compare_candidates);
    candidates.truncate(limit);
    candidates
}

fn is_better_candidate(candidate: &DecodedCandidate, current: &DecodedCandidate) -> bool {
    compare_candidates(candidate, current).is_lt()
}

fn compare_candidates(left: &DecodedCandidate, right: &DecodedCandidate) -> Ordering {
    right
        .score
        .partial_cmp(&left.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.variant_order.cmp(&right.variant_order))
        .then_with(|| left.hanzi.cmp(&right.hanzi))
}

#[cfg(test)]
mod tests {
    use super::{DecodedCandidate, MistakableWord, candidates_contain_mistakable_word};

    #[test]
    fn candidates_contain_mistakable_word_checks_one_entry() {
        let present_entry = MistakableWord {
            syllables: &["jue", "de"],
            hanzi: "觉得",
        };
        let missing_entry = MistakableWord {
            syllables: &["yi", "wei"],
            hanzi: "以为",
        };
        let candidates = [DecodedCandidate {
            hanzi: "我觉得".to_string(),
            score: 1.0,
            variant_order: 0,
        }];

        assert!(candidates_contain_mistakable_word(
            &candidates,
            &present_entry
        ));
        assert!(!candidates_contain_mistakable_word(
            &candidates,
            &missing_entry
        ));
    }
}
