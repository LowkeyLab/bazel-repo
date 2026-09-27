use std::collections::BTreeMap;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    AutomaticPauseReason, BookId, ComparisonChoice, DomainNotice, EventId, EventKind,
    EventMetadata, RankingEvent, ReaderId, Sequence,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    #[error("malformed JSON: {0}")]
    MalformedJson(String),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("invalid field: {0}")]
    InvalidField(&'static str),
    #[error("unsupported CloudEvents specversion")]
    UnsupportedSpecVersion,
    #[error("unsupported authoritative event type: {0}")]
    UnsupportedEventType(String),
    #[error("invalid CloudEvents extension: {0}")]
    InvalidExtension(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudEventDocument {
    event: RankingEvent,
    extensions: BTreeMap<String, Value>,
}

impl CloudEventDocument {
    pub fn event(&self) -> &RankingEvent {
        &self.event
    }
}

#[derive(Serialize, Deserialize)]
struct WireEnvelope {
    specversion: Option<String>,
    id: Option<String>,
    source: Option<String>,
    #[serde(rename = "type")]
    event_type: Option<String>,
    subject: Option<String>,
    time: Option<String>,
    datacontenttype: Option<String>,
    data: Option<Value>,
    #[serde(flatten)]
    extensions: BTreeMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
struct WireData {
    #[serde(rename = "readerId")]
    reader_id: Option<String>,
    #[serde(rename = "candidateBookId")]
    candidate_book_id: Option<String>,
    sequence: Option<String>,
    #[serde(rename = "opponentBookId")]
    opponent_book_id: Option<String>,
    choice: Option<String>,
}

fn required(value: Option<String>, name: &'static str) -> Result<String, CodecError> {
    value
        .filter(|value| !value.is_empty())
        .ok_or(CodecError::MissingField(name))
}

fn parse_uuid(value: &str, field: &'static str) -> Result<Uuid, CodecError> {
    let uuid = Uuid::parse_str(value).map_err(|_| CodecError::InvalidField(field))?;
    if uuid.to_string() != value {
        return Err(CodecError::InvalidField(field));
    }
    Ok(uuid)
}

fn parse_sequence(value: &str) -> Result<Sequence, CodecError> {
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(CodecError::InvalidField("sequence"));
    }
    let parsed = value
        .parse::<u64>()
        .map_err(|_| CodecError::InvalidField("sequence"))?;
    Sequence::new(parsed).map_err(|_| CodecError::InvalidField("sequence"))
}

fn valid_extension_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn validate_extensions(extensions: &BTreeMap<String, Value>) -> Result<(), CodecError> {
    for (name, value) in extensions {
        if name == "data_base64"
            || !valid_extension_name(name)
            || !matches!(value, Value::String(_) | Value::Bool(_) | Value::Number(_))
        {
            return Err(CodecError::InvalidExtension(name.clone()));
        }
        if let Value::Number(number) = value {
            if number
                .as_i64()
                .and_then(|number| i32::try_from(number).ok())
                .is_none()
            {
                return Err(CodecError::InvalidExtension(name.clone()));
            }
        }
        if let Value::String(value) = value {
            if value
                .chars()
                .any(|character| matches!(character as u32, 0x00..=0x1f | 0x7f..=0x9f))
            {
                return Err(CodecError::InvalidExtension(name.clone()));
            }
        }
    }
    Ok(())
}

fn event_type(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::PlacementStarted => "bookranking.placement.started.v1",
        EventKind::ComparisonAnswered { .. } => "bookranking.comparison.answered.v1",
        EventKind::PlacementPaused => "bookranking.placement.paused.v1",
        EventKind::PlacementResumed => "bookranking.placement.resumed.v1",
    }
}

fn time_string(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::AutoSi, true)
}

fn base_envelope(
    id: String,
    metadata: &EventMetadata,
    candidate: BookId,
    event_type: &str,
    data: Value,
) -> WireEnvelope {
    WireEnvelope {
        specversion: Some("1.0".into()),
        id: Some(id),
        source: Some(format!("urn:uuid:{}", metadata.reader_id.as_uuid())),
        event_type: Some(event_type.into()),
        subject: Some(format!("books/{}", candidate.as_uuid())),
        time: Some(time_string(metadata.time)),
        datacontenttype: Some("application/json".into()),
        data: Some(data),
        extensions: BTreeMap::new(),
    }
}

fn common_data(metadata: &EventMetadata, candidate: BookId) -> Map<String, Value> {
    let mut data = Map::new();
    data.insert(
        "readerId".into(),
        Value::String(metadata.reader_id.as_uuid().to_string()),
    );
    data.insert(
        "candidateBookId".into(),
        Value::String(candidate.as_uuid().to_string()),
    );
    data.insert(
        "sequence".into(),
        Value::String(metadata.sequence.value().to_string()),
    );
    data
}

fn encode_wire(wire: &WireEnvelope) -> Result<String, CodecError> {
    serde_json::to_string(wire).map_err(|error| CodecError::MalformedJson(error.to_string()))
}

pub fn encode_event(event: &RankingEvent) -> Result<String, CodecError> {
    let mut data = common_data(event.metadata(), event.candidate());
    if let EventKind::ComparisonAnswered { opponent, choice } = event.kind() {
        data.insert(
            "opponentBookId".into(),
            Value::String(opponent.as_uuid().to_string()),
        );
        data.insert(
            "choice".into(),
            Value::String(
                match choice {
                    ComparisonChoice::PreferCandidate => "prefer_candidate",
                    ComparisonChoice::PreferOpponent => "prefer_opponent",
                    ComparisonChoice::Skip => "skip",
                }
                .into(),
            ),
        );
    }
    encode_wire(&base_envelope(
        event.metadata().id.as_uuid().to_string(),
        event.metadata(),
        event.candidate(),
        event_type(event.kind()),
        Value::Object(data),
    ))
}

pub fn encode_document(document: &CloudEventDocument) -> Result<String, CodecError> {
    let mut wire: WireEnvelope = serde_json::from_str(&encode_event(&document.event)?)
        .map_err(|error| CodecError::MalformedJson(error.to_string()))?;
    wire.extensions = document.extensions.clone();
    encode_wire(&wire)
}

pub fn encode_notice(notice: &DomainNotice) -> Result<String, CodecError> {
    let metadata = notice.metadata();
    let candidate = notice.candidate();
    let mut data = common_data(metadata, candidate);
    data.insert(
        "causationId".into(),
        Value::String(metadata.id.as_uuid().to_string()),
    );
    let (suffix, event_type) = match notice {
        DomainNotice::BookRanked {
            position,
            entry_count,
            ..
        } => {
            data.insert("position".into(), Value::from(*position as u64));
            data.insert("entryCount".into(), Value::from(*entry_count as u64));
            ("bookranked", "bookranking.book.ranked.v1")
        }
        DomainNotice::PlacementAutomaticallyPaused { reason, .. } => {
            let reason = match reason {
                AutomaticPauseReason::NoEligibleOpponents => "no_eligible_opponents",
            };
            data.insert("reason".into(), Value::String(reason.into()));
            (
                "automaticallypaused",
                "bookranking.placement.automaticallypaused.v1",
            )
        }
    };
    encode_wire(&base_envelope(
        format!("{}:{suffix}", metadata.id.as_uuid()),
        metadata,
        candidate,
        event_type,
        Value::Object(data),
    ))
}

pub fn decode_event(text: &str) -> Result<CloudEventDocument, CodecError> {
    let StrictValue(value) = serde_json::from_str::<StrictValue>(text)
        .map_err(|error| CodecError::MalformedJson(error.to_string()))?;
    if !value.is_object() {
        return Err(CodecError::InvalidField("envelope"));
    }
    let wire: WireEnvelope = serde_json::from_value(value)
        .map_err(|error| CodecError::MalformedJson(error.to_string()))?;
    if required(wire.specversion, "specversion")? != "1.0" {
        return Err(CodecError::UnsupportedSpecVersion);
    }
    let id = EventId::new(parse_uuid(&required(wire.id, "id")?, "id")?);
    let source = required(wire.source, "source")?;
    let event_type = required(wire.event_type, "type")?;
    let subject = required(wire.subject, "subject")?;
    let time = DateTime::parse_from_rfc3339(&required(wire.time, "time")?)
        .map_err(|_| CodecError::InvalidField("time"))?
        .with_timezone(&Utc);
    if required(wire.datacontenttype, "datacontenttype")? != "application/json" {
        return Err(CodecError::InvalidField("datacontenttype"));
    }
    let data = wire.data.ok_or(CodecError::MissingField("data"))?;
    if !data.is_object() {
        return Err(CodecError::InvalidField("data"));
    }
    validate_extensions(&wire.extensions)?;
    let payload: WireData =
        serde_json::from_value(data).map_err(|_| CodecError::InvalidField("data"))?;
    let reader_id = ReaderId::new(parse_uuid(
        &required(payload.reader_id, "readerId")?,
        "readerId",
    )?);
    let candidate = BookId::new(parse_uuid(
        &required(payload.candidate_book_id, "candidateBookId")?,
        "candidateBookId",
    )?);
    let sequence = parse_sequence(&required(payload.sequence, "sequence")?)?;
    if source != format!("urn:uuid:{}", reader_id.as_uuid()) {
        return Err(CodecError::InvalidField("source"));
    }
    if subject != format!("books/{}", candidate.as_uuid()) {
        return Err(CodecError::InvalidField("subject"));
    }
    let kind = match event_type.as_str() {
        "bookranking.placement.started.v1" => EventKind::PlacementStarted,
        "bookranking.comparison.answered.v1" => {
            let opponent = BookId::new(parse_uuid(
                &required(payload.opponent_book_id, "opponentBookId")?,
                "opponentBookId",
            )?);
            let choice = match required(payload.choice, "choice")?.as_str() {
                "prefer_candidate" => ComparisonChoice::PreferCandidate,
                "prefer_opponent" => ComparisonChoice::PreferOpponent,
                "skip" => ComparisonChoice::Skip,
                _ => return Err(CodecError::InvalidField("choice")),
            };
            EventKind::ComparisonAnswered { opponent, choice }
        }
        "bookranking.placement.paused.v1" => EventKind::PlacementPaused,
        "bookranking.placement.resumed.v1" => EventKind::PlacementResumed,
        _ => return Err(CodecError::UnsupportedEventType(event_type)),
    };
    Ok(CloudEventDocument {
        event: RankingEvent::new(
            EventMetadata {
                id,
                reader_id,
                sequence,
                time,
            },
            candidate,
            kind,
        ),
        extensions: wire.extensions,
    })
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
