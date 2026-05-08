use pinyinchch::hmm::viterbi;
use pinyinchch::pinyin::pinyin_split_by_trie_tokenizer;
use pinyinchch_model_hmm::DefaultHmm;

use crate::annotator::annotate_phrase;
use crate::error::EngineError;
use crate::model::{Candidate, ConversionResult};

const MAX_CANDIDATE_LIMIT: usize = 20;

pub fn convert_pinyin(source_pinyin: &str, limit: usize) -> Result<ConversionResult, EngineError> {
    let normalized = normalize_pinyin_input(source_pinyin)?;
    if limit == 0 {
        return Err(EngineError::CandidateLimitMustBePositive);
    }

    let source_pinyin_syllables = syllables_from_normalized_pinyin(&normalized)?;
    let decoded = decode_ranked_candidates(
        &normalized,
        &source_pinyin_syllables,
        limit.min(MAX_CANDIDATE_LIMIT),
    )?;
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
    let syllable_refs = syllables.iter().map(String::as_str).collect::<Vec<_>>();

    let hmm = DefaultHmm::default();
    let candidates = viterbi(&hmm, &syllable_refs, limit, false, 3.14e-200)
        .into_iter()
        .map(|candidate| DecodedCandidate {
            hanzi: candidate.path().iter().collect(),
            score: candidate.score(),
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        return Err(EngineError::ConversionUnavailable(format!(
            "no candidate found for `{normalized}`"
        )));
    }

    Ok(candidates)
}
