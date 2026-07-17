use std::path::Path;

use flipped::{Deck, Flashcard};

use crate::ImportLimits;
use crate::archive::{CollectionArchive, extract_collection, extract_collection_file};
use crate::cancellation::ImportCancellation;
use crate::collection::{Model, read_collection};
use crate::error::{ImportError, ImportErrorCode, Result};
use crate::sanitize::plain_text_side;

pub fn import_apkg(bytes: &[u8], extension: &str, limits: &ImportLimits) -> Result<Deck> {
    import_apkg_with_cancellation(bytes, extension, limits, &ImportCancellation::new())
}

pub fn import_apkg_with_cancellation(
    bytes: &[u8],
    extension: &str,
    limits: &ImportLimits,
    cancellation: &ImportCancellation,
) -> Result<Deck> {
    import_collection(
        extract_collection(bytes, extension, limits, cancellation)?,
        limits,
        cancellation,
    )
}

pub fn import_apkg_file(path: &Path, extension: &str, limits: &ImportLimits) -> Result<Deck> {
    import_apkg_file_with_cancellation(path, extension, limits, &ImportCancellation::new())
}

pub fn import_apkg_file_with_cancellation(
    path: &Path,
    extension: &str,
    limits: &ImportLimits,
    cancellation: &ImportCancellation,
) -> Result<Deck> {
    import_collection(
        extract_collection_file(path, extension, limits, cancellation)?,
        limits,
        cancellation,
    )
}

fn import_collection(
    archive: CollectionArchive,
    limits: &ImportLimits,
    cancellation: &ImportCancellation,
) -> Result<Deck> {
    cancellation.check()?;
    if archive.database_name != "collection.anki2" && archive.database_name != "collection.anki21" {
        return Err(ImportError::new(ImportErrorCode::MissingCollectionDatabase));
    }
    let cards = read_collection(
        archive.database.path(),
        limits,
        cancellation,
        validate_model,
        |_card_id, fields, _model| {
            let mut fields = fields.split('\u{1f}');
            let front = fields
                .next()
                .ok_or_else(|| ImportError::new(ImportErrorCode::SqliteSchemaInvalid))?;
            let back = fields
                .next()
                .ok_or_else(|| ImportError::new(ImportErrorCode::SqliteSchemaInvalid))?;
            if fields.next().is_some() {
                return Err(ImportError::new(ImportErrorCode::SqliteSchemaInvalid));
            }
            cancellation.check()?;
            let front = plain_text_side(front, true, limits.card_side_max_bytes)?;
            let back = plain_text_side(back, false, limits.card_side_max_bytes)?;
            Flashcard::new(front, back)
                .map_err(|_| ImportError::new(ImportErrorCode::SqliteSchemaInvalid))
        },
    )?;
    cancellation.check()?;
    if cards.is_empty() {
        return Err(ImportError::new(ImportErrorCode::NoSupportedNotes));
    }
    Deck::new(None, cards).map_err(|_| ImportError::new(ImportErrorCode::NoSupportedNotes))
}

fn validate_model(model: &Model) -> Result<()> {
    if model.model_type != 0 {
        return Err(ImportError::new(ImportErrorCode::ClozeRejected));
    }
    if model.flds.len() != 2
        || model.flds[0].name != "Front"
        || model.flds[0].ord != 0
        || model.flds[1].name != "Back"
        || model.flds[1].ord != 1
    {
        return Err(ImportError::new(ImportErrorCode::CustomTemplateRejected));
    }
    if model.tmpls.len() != 1 || model.tmpls[0].ord != 0 || model.tmpls[0].name.trim().is_empty() {
        return Err(ImportError::new(ImportErrorCode::CustomTemplateRejected));
    }
    let template = &model.tmpls[0];
    if normalize_template(&template.qfmt) != "{{Front}}" {
        return Err(ImportError::new(ImportErrorCode::CustomTemplateRejected));
    }
    let answer = normalize_template(&template.afmt)
        .replace("<hrid=answer>", "<hrid=answer/>")
        .replace("<hrid=\"answer\">", "<hrid=answer/>")
        .replace("<hrid='answer'>", "<hrid=answer/>");
    if answer != "{{FrontSide}}<hrid=answer/>{{Back}}" {
        return Err(ImportError::new(ImportErrorCode::CustomTemplateRejected));
    }
    let css = model.css.to_ascii_lowercase();
    if [
        "url(",
        "@import",
        "expression(",
        "javascript:",
        "data:",
        "-moz-binding",
    ]
    .iter()
    .any(|marker| css.contains(marker))
    {
        return Err(ImportError::new(ImportErrorCode::CustomTemplateRejected));
    }
    Ok(())
}

fn normalize_template(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split_whitespace()
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use rusqlite::{Connection, params};
    use tempfile::NamedTempFile;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn ordinary_package(database_name: &str) -> Vec<u8> {
        package(database_name, 1, "")
    }

    fn package(database_name: &str, card_count: usize, css: &str) -> Vec<u8> {
        let database = NamedTempFile::new().expect("temporary database");
        let connection = Connection::open(database.path()).expect("open database");
        connection
            .execute_batch(
                "CREATE TABLE col (ver INTEGER NOT NULL, models TEXT NOT NULL);\n\
                 CREATE TABLE notes (id INTEGER PRIMARY KEY, mid INTEGER NOT NULL, flds BLOB NOT NULL);\n\
                 CREATE TABLE cards (id INTEGER PRIMARY KEY, nid INTEGER NOT NULL, ord INTEGER NOT NULL, type INTEGER NOT NULL, queue INTEGER NOT NULL, due INTEGER NOT NULL, ivl INTEGER NOT NULL, factor INTEGER NOT NULL, reps INTEGER NOT NULL, lapses INTEGER NOT NULL);",
            )
            .expect("schema");
        let models = serde_json::json!({
            "1": {
                "type": 0,
                "flds": [{"name": "Front", "ord": 0}, {"name": "Back", "ord": 1}],
                "tmpls": [{
                    "name": "Card 1",
                    "ord": 0,
                    "qfmt": "{{Front}}",
                    "afmt": "{{FrontSide}}<hr id=answer>{{Back}}"
                }],
                "css": css
            }
        })
        .to_string();
        connection
            .execute("INSERT INTO col(ver, models) VALUES (11, ?1)", [&models])
            .expect("collection row");
        for id in 1..=card_count as i64 {
            connection
                .execute(
                    "INSERT INTO notes(id, mid, flds) VALUES (?1, 1, ?2)",
                    params![id, b"question\x1fanswer".as_slice()],
                )
                .expect("note row");
            connection
                .execute(
                    "INSERT INTO cards(id, nid, ord, type, queue, due, ivl, factor, reps, lapses) VALUES (?1, ?1, 0, 0, 0, 0, 0, 0, 0, 0)",
                    [id],
                )
                .expect("card row");
        }
        drop(connection);

        let cursor = Cursor::new(Vec::new());
        let mut archive = ZipWriter::new(cursor);
        archive
            .start_file(database_name, SimpleFileOptions::default())
            .expect("database entry");
        archive
            .write_all(&std::fs::read(database.path()).expect("database bytes"))
            .expect("database content");
        archive.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn accepts_documented_anki2_and_anki21_ordinary_exports() {
        for database_name in ["collection.anki2", "collection.anki21"] {
            let deck = import_apkg(
                &ordinary_package(database_name),
                ".apkg",
                &ImportLimits::default(),
            )
            .expect("ordinary APKG is accepted");
            assert_eq!(deck.len(), 1);
            assert_eq!(deck.cards()[0].front().as_str(), "question");
            assert_eq!(deck.cards()[0].back().as_str(), "answer");
        }
    }

    #[test]
    fn large_shared_model_is_bounded_once_instead_of_amplified_per_card() {
        let css = "x".repeat(64 * 1024);
        let mut limits = ImportLimits::default();
        limits.max_models_bytes = 128 * 1024;
        limits.max_compression_ratio = u64::MAX;
        let deck = import_apkg(&package("collection.anki21", 128, &css), ".apkg", &limits)
            .expect("one bounded shared model is accepted");
        assert_eq!(deck.len(), 128);

        limits.max_models_bytes = 1024;
        assert_eq!(
            import_apkg(&package("collection.anki21", 1, &css), ".apkg", &limits)
                .expect_err("the model table has one aggregate bound")
                .code,
            ImportErrorCode::SqliteLimitExceeded
        );
    }

    #[test]
    fn cancellation_rejects_before_archive_or_sqlite_work() {
        let cancellation = ImportCancellation::new();
        cancellation.cancel();
        assert_eq!(
            import_apkg_with_cancellation(
                &ordinary_package("collection.anki21"),
                ".apkg",
                &ImportLimits::default(),
                &cancellation,
            )
            .expect_err("cancelled import is rejected")
            .code,
            ImportErrorCode::Cancelled
        );
    }

    #[test]
    fn malformed_bytes_never_create_a_deck() {
        assert_eq!(
            import_apkg(b"not a zip", ".apkg", &ImportLimits::default())
                .expect_err("malformed package is rejected")
                .code,
            ImportErrorCode::InvalidZip
        );
    }
}
