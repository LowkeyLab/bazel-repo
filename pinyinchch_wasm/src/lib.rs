use pinyinchch::hmm::viterbi;
use pinyinchch::pinyin::pinyin_split_by_trie_tokenizer;
use pinyinchch_model_hmm::DefaultHmm;
use wasm_bindgen::prelude::wasm_bindgen;

const MIN_PROBABILITY: f64 = 3.14e-200;

#[wasm_bindgen]
pub fn guess_hanzi(pinyin: &str, limit: usize) -> String {
    let normalized_pinyin = pinyin_split_by_trie_tokenizer(pinyin);
    let pinyin_sequence = normalized_pinyin.split(' ').collect::<Vec<_>>();
    let hmm = DefaultHmm::default();
    let candidates = viterbi(&hmm, &pinyin_sequence, limit.max(1), true, MIN_PROBABILITY);

    let json_candidates = candidates
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "text": candidate.path().iter().collect::<String>(),
                "score": candidate.score(),
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&json_candidates).unwrap_or_else(|_| "[]".to_owned())
}
