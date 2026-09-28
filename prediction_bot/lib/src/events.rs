//! `CloudEvents` metadata is immutable; per-server event revision governs replay.
use chrono::Datelike;
use cloudevents::event::{ExtensionValue, SpecVersion};
use cloudevents::{AttributesReader, Data, Event, EventBuilder, EventBuilderV10};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::types::{ApplicationId, EventRevision, GuildId};

#[derive(Debug, Error)]
#[error("invalid event metadata: {0}")]
pub struct EventError(pub &'static str);

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(transparent)]
pub struct CloudEvent(Event);

// Check wire values before the SDK normalizes timestamps and schema URIs or
// drops null extensions. Persisted history must retain the bot's stricter profile.
impl<'de> Deserialize<'de> for CloudEvent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let fields = serde_json::Map::<String, Value>::deserialize(deserializer)?;
        if fields.values().any(Value::is_null)
            || fields.contains_key("data_base64")
            || !fields.get("data").is_some_and(Value::is_object)
        {
            return Err(D::Error::custom("unsupported event encoding"));
        }
        let time = fields
            .get("time")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::custom("missing event timestamp"))?;
        let time = chrono::DateTime::parse_from_rfc3339(time).map_err(D::Error::custom)?;
        if time.offset().local_minus_utc() != 0 || time.timestamp_subsec_nanos() != 0 {
            return Err(D::Error::custom(
                "event timestamp must be whole seconds in UTC",
            ));
        }
        let schema = fields
            .get("dataschema")
            .and_then(Value::as_str)
            .ok_or_else(|| D::Error::custom("missing event schema"))?
            .to_owned();
        let event: Event =
            serde_json::from_value(Value::Object(fields)).map_err(D::Error::custom)?;
        if event.dataschema().map(AsRef::as_ref) != Some(schema.as_str()) {
            return Err(D::Error::custom(
                "event schema must not require normalization",
            ));
        }
        Ok(Self(event))
    }
}

pub struct Context<'a> {
    pub application: ApplicationId,
    pub guild: GuildId,
    pub revision: EventRevision,
    pub command: &'a str,
    pub accepted_at: i64,
}

impl CloudEvent {
    /// Construct an event with a fresh UUID v7 identity.
    ///
    /// # Errors
    /// Returns an error for an out-of-range timestamp or invalid stream metadata.
    pub fn new(
        ctx: &Context<'_>,
        name: &str,
        subject: String,
        data: Value,
    ) -> Result<Self, EventError> {
        let time = chrono::DateTime::from_timestamp(ctx.accepted_at, 0)
            .ok_or(EventError("timestamp out of range"))?;
        let event = Self(
            EventBuilderV10::new()
                .id(uuid::Uuid::now_v7().to_string())
                .source(format!(
                    "urn:lowkeylab:prediction-bot:discord:{}:guild:{}",
                    ctx.application, ctx.guild
                ))
                .ty(format!("io.lowkeylab.predictionbot.{name}.v1"))
                .subject(&subject)
                .time(time)
                .data_with_schema(
                    "application/json",
                    format!(
                        "urn:lowkeylab:prediction-bot:schema:{}:v1",
                        name.replace('.', "-")
                    ),
                    data,
                )
                .extension("guildid", ctx.guild.to_string())
                .extension("commandid", ctx.command)
                .extension("revision", ctx.revision.to_string())
                .build()
                .map_err(|_| EventError("invalid CloudEvents envelope"))?,
        );
        event.validate(ctx, name, &subject)?;
        Ok(event)
    }

    /// The persisted event identity, used to reject duplicate events during replay.
    #[must_use]
    pub fn identity(&self) -> (String, String) {
        (self.0.source().clone(), self.0.id().to_owned())
    }

    /// The domain payload to replay.
    ///
    /// # Errors
    /// Returns an error unless the payload is a JSON object.
    pub fn data(&self) -> Result<&Value, EventError> {
        match self.0.data() {
            Some(Data::Json(data)) if data.is_object() => Ok(data),
            _ => Err(EventError("unsupported encoding")),
        }
    }

    /// Validate metadata against its persisted stream context and domain event.
    ///
    /// # Errors
    /// Returns an error for unsupported encoding, identity, extensions, or mismatched metadata.
    pub fn validate(&self, ctx: &Context<'_>, name: &str, subject: &str) -> Result<(), EventError> {
        let event = &self.0;
        let id = uuid::Uuid::parse_str(event.id()).map_err(|_| EventError("invalid UUID"))?;
        let time = event.time().ok_or(EventError("invalid timestamp"))?;
        if !(0..=9999).contains(&time.year()) {
            return Err(EventError("timestamp out of RFC 3339 range"));
        }
        if id.get_version_num() != 7 || id.get_variant() != uuid::Variant::RFC4122 {
            return Err(EventError("event ID must be UUID v7"));
        }
        if event.specversion() != SpecVersion::V10
            || event.datacontenttype() != Some("application/json")
        {
            return Err(EventError("unsupported encoding"));
        }
        self.data()?;
        if ctx.guild.0 == 0
            || ctx.application.0 == 0
            || ctx.revision.0 < 1
            || event.source()
                != &format!(
                    "urn:lowkeylab:prediction-bot:discord:{}:guild:{}",
                    ctx.application, ctx.guild
                )
            || event.extension("guildid") != Some(&ExtensionValue::String(ctx.guild.to_string()))
            || event.extension("revision")
                != Some(&ExtensionValue::String(ctx.revision.to_string()))
            || !(ctx.command.starts_with("discord:") || ctx.command.starts_with("grant:"))
            || ctx.command.chars().any(char::is_control)
            || ctx.command.ends_with(':')
            || event.extension("commandid") != Some(&ExtensionValue::String(ctx.command.into()))
            || time.timestamp() != ctx.accepted_at
            || time.timestamp_subsec_nanos() != 0
        {
            return Err(EventError("event does not match stream context"));
        }
        if event.ty() != format!("io.lowkeylab.predictionbot.{name}.v1")
            || event.dataschema().map(AsRef::as_ref)
                != Some(
                    format!(
                        "urn:lowkeylab:prediction-bot:schema:{}:v1",
                        name.replace('.', "-")
                    )
                    .as_str(),
                )
            || event.subject() != Some(subject)
        {
            return Err(EventError("unsupported event type, schema, or subject"));
        }
        for (key, value) in event.iter_extensions() {
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                || key == "eventindex"
                || !match value {
                    ExtensionValue::Boolean(_) => true,
                    ExtensionValue::String(s) => !s.chars().any(char::is_control),
                    ExtensionValue::Integer(n) => i32::try_from(*n).is_ok(),
                }
            {
                return Err(EventError("invalid extension attribute"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cloudevents::AttributesWriter;
    use googletest::{
        assert_that,
        matchers::{anything, eq, err, none},
    };
    use serde_json::json;

    #[googletest::test]
    fn historical_fixture_preserves_its_original_identity_and_optional_metadata() {
        let raw = include_str!("../tests/fixtures/member-enrolled.json");
        let stored: CloudEvent = serde_json::from_str(raw).unwrap();
        let ctx = Context {
            application: ApplicationId(1),
            guild: GuildId(2),
            revision: EventRevision(3),
            command: "discord:4",
            accepted_at: 1000,
        };
        stored
            .validate(&ctx, "member.enrolled", "members/5")
            .unwrap();
        let encoded = serde_json::to_value(&stored).unwrap();
        assert_that!(encoded, eq(&serde_json::from_str::<Value>(raw).unwrap()));
    }

    #[googletest::test]
    fn serialized_event_preserves_identity_payload_and_unknown_extension() {
        let ctx = Context {
            application: ApplicationId(1),
            guild: GuildId(2),
            revision: EventRevision(3),
            command: "discord:4",
            accepted_at: 1000,
        };
        let mut event = CloudEvent::new(
            &ctx,
            "member.enrolled",
            "members/5".into(),
            json!({"user_id": 5}),
        )
        .unwrap();
        event.0.set_extension("tracehint", "opaque");
        event.0.set_extension("flag", true);
        event.0.set_extension("count", i64::from(i32::MIN));
        let encoded = serde_json::to_value(&event).unwrap();
        assert_that!(encoded["tracehint"], eq("opaque"));
        assert_that!(encoded["flag"], eq(&json!(true)));
        assert_that!(encoded["count"], eq(&json!(i32::MIN)));
        assert_that!(encoded["guildid"], eq("2"));
        assert_that!(encoded["revision"], eq("3"));
        assert_that!(encoded.get("extensions"), none());
        assert_that!(encoded.get("eventindex"), none());
        assert_that!(
            uuid::Uuid::parse_str(event.0.id())
                .unwrap()
                .get_version_num(),
            eq(7)
        );
        let decoded: CloudEvent = serde_json::from_value(encoded).unwrap();
        decoded
            .validate(&ctx, "member.enrolled", "members/5")
            .unwrap();
        assert_that!(event.identity(), eq(&decoded.identity()));
        assert_that!(event, eq(&decoded));
    }

    #[googletest::test]
    fn rejects_wrong_identity_version_stream_schema_or_extension_type() {
        let ctx = Context {
            application: ApplicationId(1),
            guild: GuildId(2),
            revision: EventRevision(3),
            command: "discord:4",
            accepted_at: 1000,
        };
        let valid = CloudEvent::new(
            &ctx,
            "member.enrolled",
            "members/5".into(),
            json!({"user_id": 5}),
        )
        .unwrap();
        let mut invalid = valid.clone();
        invalid.0.set_id(uuid::Uuid::new_v4().to_string());
        assert_that!(
            invalid.validate(&ctx, "member.enrolled", "members/5"),
            err(anything())
        );
        let mut invalid = valid.clone();
        invalid.0.set_extension("guildid", "9");
        assert_that!(
            invalid.validate(&ctx, "member.enrolled", "members/5"),
            err(anything())
        );
        let mut invalid = valid.clone();
        let mut schema = invalid.0.dataschema().unwrap().clone();
        schema.set_fragment(Some("wrong"));
        invalid.0.set_dataschema(Some(schema));
        assert_that!(
            invalid.validate(&ctx, "member.enrolled", "members/5"),
            err(anything())
        );
        let mut invalid = valid;
        invalid
            .0
            .set_extension("oversized", i64::from(i32::MAX) + 1);
        assert_that!(
            invalid.validate(&ctx, "member.enrolled", "members/5"),
            err(anything())
        );
    }

    #[googletest::test]
    fn replay_rejects_encodings_that_sdk_may_normalize() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/member-enrolled.json")).unwrap();
        let ctx = Context {
            application: ApplicationId(1),
            guild: GuildId(2),
            revision: EventRevision(3),
            command: "discord:4",
            accepted_at: 1000,
        };
        for (field, value) in [
            ("time", json!("1970-01-01T01:16:40+01:00")),
            ("time", json!("1970-01-01T00:16:40.001Z")),
            (
                "dataschema",
                json!(" urn:lowkeylab:prediction-bot:schema:member-enrolled:v1"),
            ),
            ("guildid", json!(2)),
            ("revision", json!(3)),
            ("tracehint", json!(2147483648_i64)),
            ("tracehint", json!({"nested": true})),
            ("tracehint", Value::Null),
            ("tracehint", json!("control\n")),
            ("eventindex", json!(1)),
            ("data_base64", json!("e30=")),
            ("specversion", json!("0.3")),
        ] {
            let mut raw = fixture.clone();
            raw[field] = value.clone();
            let accepted = serde_json::from_value::<CloudEvent>(raw)
                .ok()
                .is_some_and(|event| event.validate(&ctx, "member.enrolled", "members/5").is_ok());
            assert_that!(accepted, eq(false), "accepted {field}: {value}");
        }
    }

    #[googletest::test]
    fn rejects_timestamps_outside_the_persisted_rfc3339_range() {
        for accepted_at in [253_402_300_800, -62_167_219_201, i64::MAX] {
            let ctx = Context {
                application: ApplicationId(1),
                guild: GuildId(2),
                revision: EventRevision(3),
                command: "discord:4",
                accepted_at,
            };
            assert_that!(
                CloudEvent::new(
                    &ctx,
                    "member.enrolled",
                    "members/5".into(),
                    json!({"user_id": 5})
                ),
                err(anything())
            );
        }
    }
}
