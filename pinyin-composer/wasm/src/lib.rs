mod annotator;
mod converter;
mod error;
mod model;

pub use annotator::annotate_phrase;
pub use converter::convert_pinyin;
pub use error::EngineError;
pub use model::{AnnotationResult, Candidate, ConversionResult};

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn engine_version() -> String {
    install_panic_hook();
    "pinyin-composer-wasm/0.1.0".to_string()
}

#[wasm_bindgen]
pub fn convert_pinyin_js(source_pinyin: &str, limit: usize) -> Result<JsValue, JsValue> {
    install_panic_hook();
    let result = convert_pinyin(source_pinyin, limit)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    serde_wasm_bindgen::to_value(&result).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn annotate_phrase_js(hanzi: &str) -> Result<JsValue, JsValue> {
    install_panic_hook();
    let result = annotate_phrase(hanzi).map_err(|error| JsValue::from_str(&error.to_string()))?;
    serde_wasm_bindgen::to_value(&result).map_err(|error| JsValue::from_str(&error.to_string()))
}

fn install_panic_hook() {
    console_error_panic_hook::set_once();
}
