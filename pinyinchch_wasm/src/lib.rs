use pinyinchch::dag::dispatch;
use pinyinchch::pinyin::pinyin_split_by_trie_tokenizer;
use pinyinchch_model_dag::DefaultDag;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
pub fn guess_hanzi(pinyin: &str, limit: usize) -> String {
    let normalized_pinyin = pinyin_split_by_trie_tokenizer(pinyin);
    let pinyin_sequence = normalized_pinyin.split(' ').collect::<Vec<_>>();
    let dag = DefaultDag::default();
    let candidates = dispatch(&dag, &pinyin_sequence, limit.max(1), true);

    let json_candidates = candidates
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "text": candidate.path().join(""),
                "score": candidate.score(),
            })
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&json_candidates).unwrap_or_else(|_| "[]".to_owned())
}
