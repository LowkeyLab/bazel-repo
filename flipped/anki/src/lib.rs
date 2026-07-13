//! Anki import helpers for the `flipped` domain model.
//!
//! This crate currently provides a small Anki card-template renderer. It is
//! intentionally scoped to common exported-deck templates and is not a complete
//! reimplementation of Anki's rendering engine.

pub use flipped_anki_template::{
    AnkiCardTemplate, AnkiNoteFields, RenderedCard, TemplateRenderError, render_template,
};
