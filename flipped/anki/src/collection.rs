use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

use crate::ImportLimits;
use crate::cancellation::ImportCancellation;
use crate::error::{ImportError, ImportErrorCode, Result};

#[derive(Debug, Deserialize)]
pub(crate) struct Model {
    #[serde(rename = "type")]
    pub model_type: i64,
    pub flds: Vec<ModelField>,
    pub tmpls: Vec<ModelTemplate>,
    #[serde(default)]
    pub css: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelField {
    pub name: String,
    pub ord: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModelTemplate {
    pub name: String,
    pub ord: i64,
    pub qfmt: String,
    pub afmt: String,
}

pub(crate) fn read_collection<T, V, F>(
    path: &Path,
    limits: &ImportLimits,
    cancellation: &ImportCancellation,
    validate_model: V,
    mut map_card: F,
) -> Result<Vec<T>>
where
    V: Fn(&Model) -> Result<()>,
    F: FnMut(i64, &str, &Model) -> Result<T>,
{
    cancellation.check()?;
    // `immutable=1` prevents SQLite from creating journals or trusting mutable sidecars.
    // Named temporary paths contain no URI metacharacters in supported deployments.
    let uri = format!("file:{}?immutable=1", path.display());
    let connection = Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|error| map_sqlite(error, cancellation))?;
    connection
        .busy_timeout(limits.sqlite_timeout)
        .map_err(|error| map_sqlite(error, cancellation))?;
    connection.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH,
        20 * 1024 * 1024,
    );
    connection.set_limit(
        rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH,
        1024 * 1024,
    );
    connection.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_COLUMN, 64);
    connection.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_COMPOUND_SELECT, 8);
    connection.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 64);
    connection.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_EXPR_DEPTH, 64);
    connection.set_limit(rusqlite::limits::Limit::SQLITE_LIMIT_ATTACHED, 0);

    // `busy_timeout` does not bound CPU spent evaluating hostile SQLite input. A short-lived
    // watcher interrupts at the configured deadline or as soon as request cancellation arrives.
    let interrupt = connection.get_interrupt_handle();
    let watcher_cancellation = cancellation.clone();
    let (complete, watcher) = mpsc::channel();
    let timeout = limits.sqlite_timeout;
    std::thread::spawn(move || {
        let started = Instant::now();
        loop {
            if watcher_cancellation.is_cancelled() {
                interrupt.interrupt();
                break;
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                interrupt.interrupt();
                break;
            }
            match watcher.recv_timeout(remaining.min(Duration::from_millis(10))) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    });

    let result = read_bounded_rows(
        &connection,
        limits,
        cancellation,
        validate_model,
        &mut map_card,
    );
    let _ = complete.send(());
    result
}

fn read_bounded_rows<T, V, F>(
    connection: &Connection,
    limits: &ImportLimits,
    cancellation: &ImportCancellation,
    validate_model: V,
    map_card: &mut F,
) -> Result<Vec<T>>
where
    V: Fn(&Model) -> Result<()>,
    F: FnMut(i64, &str, &Model) -> Result<T>,
{
    cancellation.check()?;
    let col_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM col", [], |row| row.get(0))
        .map_err(|error| map_sqlite(error, cancellation))?;
    if col_count != 1 {
        return Err(ImportError::new(ImportErrorCode::SqliteSchemaInvalid));
    }
    let (version, models_json): (i64, String) = connection
        .query_row("SELECT ver, models FROM col", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|error| map_sqlite(error, cancellation))?;
    if version != 11 {
        return Err(ImportError::new(ImportErrorCode::UnsupportedPackageVersion));
    }
    if models_json.len() > limits.max_models_bytes {
        return Err(ImportError::new(ImportErrorCode::SqliteLimitExceeded));
    }
    cancellation.check()?;
    let raw_models: HashMap<String, Model> = serde_json::from_str(&models_json)
        .map_err(|_| ImportError::new(ImportErrorCode::SqliteSchemaInvalid))?;
    if raw_models.is_empty() || raw_models.len() > limits.max_models {
        return Err(ImportError::new(ImportErrorCode::SqliteLimitExceeded));
    }
    let mut models = HashMap::with_capacity(raw_models.len());
    for (id, model) in raw_models {
        cancellation.check()?;
        let numeric_id = id
            .parse::<i64>()
            .ok()
            .filter(|numeric| numeric.to_string() == id)
            .ok_or_else(|| ImportError::new(ImportErrorCode::SqliteSchemaInvalid))?;
        validate_model(&model)?;
        if models.insert(numeric_id, model).is_some() {
            return Err(ImportError::new(ImportErrorCode::SqliteSchemaInvalid));
        }
    }

    let dangling_note: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM cards LEFT JOIN notes ON notes.id = cards.nid WHERE notes.id IS NULL LIMIT 1)",
            [],
            |row| row.get(0),
        )
        .map_err(|error| map_sqlite(error, cancellation))?;
    if dangling_note {
        return Err(ImportError::new(ImportErrorCode::SqliteSchemaInvalid));
    }

    let mut statement = connection
        .prepare(
            "SELECT cards.id, cards.nid, cards.ord, cards.type, cards.queue, cards.due, \
             cards.ivl, cards.factor, cards.reps, cards.lapses, notes.mid, notes.flds \
             FROM cards JOIN notes ON notes.id = cards.nid ORDER BY cards.id",
        )
        .map_err(|error| map_sqlite(error, cancellation))?;
    let mut rows = statement
        .query([])
        .map_err(|error| map_sqlite(error, cancellation))?;

    let mut cards = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| map_sqlite(error, cancellation))?
    {
        cancellation.check()?;
        if cards.len() >= limits.max_cards {
            return Err(ImportError::new(ImportErrorCode::TooManyCards));
        }
        let card_id = row
            .get::<_, i64>(0)
            .map_err(|error| map_sqlite(error, cancellation))?;
        let _note_id = row
            .get::<_, i64>(1)
            .map_err(|error| map_sqlite(error, cancellation))?;
        let ord = row
            .get::<_, i64>(2)
            .map_err(|error| map_sqlite(error, cancellation))?;
        let card_type = row
            .get::<_, i64>(3)
            .map_err(|error| map_sqlite(error, cancellation))?;
        for index in 4..=9 {
            let _scheduling_value = row
                .get::<_, i64>(index)
                .map_err(|error| map_sqlite(error, cancellation))?;
        }
        let model_id = row
            .get::<_, i64>(10)
            .map_err(|error| map_sqlite(error, cancellation))?;
        let fields = row
            .get::<_, Vec<u8>>(11)
            .map_err(|error| map_sqlite(error, cancellation))?;
        if ord != 0 || !(0..=3).contains(&card_type) {
            return Err(ImportError::new(ImportErrorCode::SqliteSchemaInvalid));
        }
        let fields = String::from_utf8(fields)
            .map_err(|_| ImportError::new(ImportErrorCode::InvalidUtf8))?;
        let model = models
            .get(&model_id)
            .ok_or_else(|| ImportError::new(ImportErrorCode::SqliteSchemaInvalid))?;
        cards.push(map_card(card_id, &fields, model)?);
    }
    Ok(cards)
}

fn map_sqlite(error: rusqlite::Error, cancellation: &ImportCancellation) -> ImportError {
    if cancellation.is_cancelled() {
        return ImportError::new(ImportErrorCode::Cancelled);
    }
    match error {
        rusqlite::Error::SqliteFailure(inner, _)
            if matches!(
                inner.code,
                rusqlite::ErrorCode::TooBig | rusqlite::ErrorCode::OperationInterrupted
            ) =>
        {
            ImportError::new(ImportErrorCode::SqliteLimitExceeded)
        }
        _ => ImportError::new(ImportErrorCode::SqliteSchemaInvalid),
    }
}
