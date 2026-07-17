//! Bounded importer for the ordinary text-only subset of Anki APKG exports.

#![deny(clippy::disallowed_macros)]

mod archive;
mod cancellation;
mod collection;
mod error;
mod importer;
mod sanitize;

use std::time::Duration;

pub use cancellation::ImportCancellation;
pub use error::{ImportError, ImportErrorCode};
pub use flipped_anki_template::{
    AnkiCardTemplate, AnkiNoteFields, RenderedCard, TemplateRenderError, render_template,
};
pub use importer::{
    import_apkg, import_apkg_file, import_apkg_file_with_cancellation,
    import_apkg_with_cancellation,
};

#[derive(Debug, Clone)]
pub struct ImportLimits {
    pub max_upload_bytes: u64,
    pub max_extracted_bytes: u64,
    pub max_archive_entries: usize,
    pub max_entry_bytes: u64,
    pub max_compression_ratio: u64,
    pub max_cards: usize,
    pub max_models: usize,
    pub max_models_bytes: usize,
    pub card_side_max_bytes: usize,
    pub sqlite_timeout: Duration,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_upload_bytes: 20_971_520,
            max_extracted_bytes: 104_857_600,
            max_archive_entries: 16,
            max_entry_bytes: 104_857_600,
            max_compression_ratio: 100,
            max_cards: 10_000,
            max_models: 128,
            max_models_bytes: 1_048_576,
            card_side_max_bytes: 65_536,
            sqlite_timeout: Duration::from_millis(5_000),
        }
    }
}
