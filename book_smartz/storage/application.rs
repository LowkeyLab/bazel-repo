use book_smartz_domain::ReaderId;
use chrono::Datelike;
use cloudevents::{AttributesReader, Event};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use sqlx::{Postgres, Transaction, types::Json};
use uuid::Uuid;

use crate::{Store, StoreError, wire};

/// A validated immutable application fact, separate from authoritative ranking events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationEvent {
    reader: ReaderId,
    sequence: u64,
    id: Uuid,
    envelope: Value,
}

impl ApplicationEvent {
    /// Validates a complete application `CloudEvent` with independent padded sequencing.
    ///
    /// The source is `urn:book-smartz:application:<reader UUID>`. Event data is an
    /// application-owned JSON object; loading it never executes recorded commands.
    ///
    /// # Errors
    /// Rejects invalid `CloudEvents`, identities, context, or sequence values.
    pub fn decode(reader: ReaderId, text: &str) -> Result<Self, StoreError> {
        let invalid = || StoreError::InvalidEnvelope;
        // Validate duplicate keys separately; rebuilding visitor numbers can lose precision.
        let StrictValue(_) = serde_json::from_str::<StrictValue>(text).map_err(|_| invalid())?;
        let envelope: Value = serde_json::from_str(text).map_err(|_| invalid())?;
        if !numbers_preserved(text, &envelope).map_err(|_| invalid())? {
            return Err(invalid());
        }
        let sdk: Event = serde_json::from_str(text).map_err(|_| invalid())?;
        if sdk
            .time()
            .is_none_or(|time| !(0..=9999).contains(&time.year()))
        {
            return Err(invalid());
        }
        let fields = envelope.as_object().ok_or_else(invalid)?;
        for name in ["id", "source", "type", "time", "datacontenttype"] {
            if fields
                .get(name)
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(invalid());
            }
        }
        let id_text = envelope["id"].as_str().ok_or_else(invalid)?;
        let id = Uuid::parse_str(id_text).map_err(|_| invalid())?;
        if id.to_string() != id_text
            || envelope["specversion"] != "1.0"
            || envelope["source"] != format!("urn:book-smartz:application:{}", reader.as_uuid())
            || envelope["datacontenttype"] != "application/json"
            || !envelope["data"].is_object()
            || fields.contains_key("data_base64")
        {
            return Err(invalid());
        }
        for name in ["traceparent", "tracestate"] {
            if fields
                .get(name)
                .is_some_and(|value| value.as_str().is_none_or(str::is_empty))
            {
                return Err(invalid());
            }
        }
        if fields.contains_key("tracestate") && !fields.contains_key("traceparent") {
            return Err(invalid());
        }
        for (name, value) in fields {
            if value.as_str().is_some_and(|value| {
                value.chars().any(|character| {
                    let codepoint = character as u32;
                    character.is_control()
                        || matches!(codepoint, 0xfdd0..=0xfdef)
                        || codepoint & 0xffff >= 0xfffe
                })
            }) {
                return Err(invalid());
            }
            if matches!(
                name.as_str(),
                "specversion"
                    | "id"
                    | "source"
                    | "type"
                    | "time"
                    | "datacontenttype"
                    | "data"
                    | "subject"
                    | "dataschema"
            ) {
                continue;
            }
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                || !(value.is_string()
                    || value.is_boolean()
                    || value
                        .as_i64()
                        .is_some_and(|number| i32::try_from(number).is_ok()))
            {
                return Err(invalid());
            }
        }
        let sequence = wire::parse_sequence_key(envelope["sequence"].as_str().ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
        Ok(Self {
            reader,
            sequence,
            id,
            envelope,
        })
    }

    /// Returns the complete validated envelope, including original command data.
    #[must_use]
    pub const fn envelope(&self) -> &Value {
        &self.envelope
    }

    /// Returns the independent application-history sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub(crate) const fn reader(&self) -> ReaderId {
        self.reader
    }
}

type Row = (Uuid, String, Uuid, Json<Value>);

fn decode_rows(reader: ReaderId, rows: Vec<Row>) -> Result<Vec<ApplicationEvent>, StoreError> {
    let mut history = Vec::with_capacity(rows.len());
    let mut expected = 1_u64;
    for (row_reader, sequence, id, Json(envelope)) in rows {
        let event = ApplicationEvent::decode(reader, &envelope.to_string())
            .map_err(|_| StoreError::CorruptHistory("invalid application envelope"))?;
        if row_reader != *reader.as_uuid()
            || id != event.id
            || wire::parse_sequence_key(&sequence)? != event.sequence
            || event.sequence != expected
        {
            return Err(StoreError::CorruptHistory(
                "invalid application event index or sequence",
            ));
        }
        expected = expected
            .checked_add(1)
            .ok_or(StoreError::CorruptHistory("application sequence overflow"))?;
        history.push(event);
    }
    Ok(history)
}

pub async fn in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    reader: ReaderId,
) -> Result<Vec<ApplicationEvent>, StoreError> {
    let rows: Vec<Row> = sqlx::query_as("SELECT reader_id, sequence::text, event_id, envelope FROM book_smartz.application_events WHERE reader_id = $1 ORDER BY sequence")
        .bind(reader.as_uuid()).fetch_all(&mut **transaction).await?;
    decode_rows(reader, rows)
}

pub async fn insert(
    transaction: &mut Transaction<'_, Postgres>,
    event: &ApplicationEvent,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO book_smartz.application_events (reader_id, sequence, event_id, envelope) VALUES ($1, $2, $3, $4)")
        .bind(event.reader.as_uuid()).bind(wire::sequence_key(event.sequence)?).bind(event.id)
        .bind(Json(&event.envelope)).execute(&mut **transaction).await?;
    Ok(())
}

impl Store {
    /// Loads immutable facts in their independent stream order without performing effects.
    ///
    /// # Errors
    /// Returns database failure or corrupt application history.
    pub async fn load_application_history(
        &self,
        reader: ReaderId,
    ) -> Result<Vec<ApplicationEvent>, StoreError> {
        let rows: Vec<Row> = sqlx::query_as("SELECT reader_id, sequence::text, event_id, envelope FROM book_smartz.application_events WHERE reader_id = $1 ORDER BY sequence")
            .bind(reader.as_uuid()).fetch_all(&self.pool).await?;
        decode_rows(reader, rows)
    }
}

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictValue;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("JSON value without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(value)))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::from(value)))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::from(value)))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|number| StrictValue(Value::Number(number)))
                    .ok_or_else(|| E::custom("invalid JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(value.into())))
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::String(value)))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(StrictValue(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(StrictValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some((key, StrictValue(value))) =
                    map.next_entry::<String, StrictValue>()?
                {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON key: {key}"
                        )));
                    }
                }
                Ok(StrictValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(StrictVisitor)
    }
}

// Compare decimal values, rather than binary floating-point approximations.
// JSONB may normalize spelling; every accepted number must keep its numeric value.
fn normalized_number(text: &str) -> Option<(bool, String, i64)> {
    let (mantissa, exponent) = text.split_once(['e', 'E']).unwrap_or((text, "0"));
    let fraction = mantissa
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    let digits = mantissa.trim_start_matches('-').replace('.', "");
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        return Some((false, "0".into(), 0));
    }
    let trimmed = significant.trim_end_matches('0');
    let exponent = exponent
        .parse::<i64>()
        .ok()?
        .checked_sub(i64::try_from(fraction).ok()?)?
        .checked_add(i64::try_from(significant.len() - trimmed.len()).ok()?)?;
    Some((mantissa.starts_with('-'), trimmed.into(), exponent))
}

fn numbers_preserved(text: &str, value: &Value) -> Result<bool, serde_json::Error> {
    match value {
        Value::Number(number) => Ok(normalized_number(text)
            .zip(normalized_number(&number.to_string()))
            .is_some_and(|(original, stored)| original == stored)),
        Value::Array(values) => {
            let originals: Vec<&serde_json::value::RawValue> = serde_json::from_str(text)?;
            for (original, stored) in originals.iter().zip(values) {
                if !numbers_preserved(original.get(), stored)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Value::Object(values) => {
            let originals: std::collections::BTreeMap<String, &serde_json::value::RawValue> =
                serde_json::from_str(text)?;
            for (name, original) in originals {
                if !numbers_preserved(original.get(), &values[&name])? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(true),
    }
}
