//! CloudEvents metadata is immutable; per-server event revision governs replay.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid event metadata: {0}")]
pub struct EventError(pub &'static str);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CloudEvent {
    pub specversion: String,
    pub id: String,
    pub source: String,
    pub r#type: String,
    pub subject: String,
    pub time: String,
    pub datacontenttype: String,
    pub dataschema: String,
    pub guildid: String,
    pub commandid: String,
    pub revision: String,
    pub data: Value,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
}

pub struct Context<'a> {
    pub application: u64,
    pub guild: u64,
    pub revision: i64,
    pub command: &'a str,
    pub accepted_at: i64,
}

impl CloudEvent {
    pub fn new(
        ctx: &Context<'_>,
        name: &str,
        subject: String,
        data: Value,
    ) -> Result<Self, EventError> {
        let time = chrono::DateTime::from_timestamp(ctx.accepted_at, 0)
            .ok_or(EventError("timestamp out of range"))?
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let event = Self {
            specversion: "1.0".into(),
            id: uuid::Uuid::now_v7().to_string(),
            source: format!(
                "urn:lowkeylab:prediction-bot:discord:{}:guild:{}",
                ctx.application, ctx.guild
            ),
            r#type: format!("io.lowkeylab.predictionbot.{name}.v1"),
            subject,
            time,
            datacontenttype: "application/json".into(),
            dataschema: format!(
                "urn:lowkeylab:prediction-bot:schema:{}:v1",
                name.replace('.', "-")
            ),
            guildid: ctx.guild.to_string(),
            commandid: ctx.command.into(),
            revision: ctx.revision.to_string(),
            data,
            extensions: BTreeMap::new(),
        };
        event.validate(ctx, name, &event.subject)?;
        Ok(event)
    }

    pub fn validate(&self, ctx: &Context<'_>, name: &str, subject: &str) -> Result<(), EventError> {
        let id = uuid::Uuid::parse_str(&self.id).map_err(|_| EventError("invalid UUID"))?;
        let time = chrono::DateTime::parse_from_rfc3339(&self.time)
            .map_err(|_| EventError("invalid timestamp"))?;
        if id.get_version_num() != 7 || id.get_variant() != uuid::Variant::RFC4122 {
            return Err(EventError("event ID must be UUID v7"));
        }
        if self.specversion != "1.0"
            || self.datacontenttype != "application/json"
            || !self.data.is_object()
        {
            return Err(EventError("unsupported encoding"));
        }
        if ctx.guild == 0
            || ctx.application == 0
            || ctx.revision < 1
            || self.source
                != format!(
                    "urn:lowkeylab:prediction-bot:discord:{}:guild:{}",
                    ctx.application, ctx.guild
                )
            || self.guildid != ctx.guild.to_string()
            || self.revision != ctx.revision.to_string()
            || self.command_invalid()
            || self.commandid != ctx.command
            || time.timestamp() != ctx.accepted_at
            || time.timestamp_subsec_nanos() != 0
            || time.offset().local_minus_utc() != 0
        {
            return Err(EventError("event does not match stream context"));
        }
        if self.r#type != format!("io.lowkeylab.predictionbot.{name}.v1")
            || self.dataschema
                != format!(
                    "urn:lowkeylab:prediction-bot:schema:{}:v1",
                    name.replace('.', "-")
                )
            || self.subject != subject
        {
            return Err(EventError("unsupported event type, schema, or subject"));
        }
        for (key, value) in &self.extensions {
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                || key == "eventindex"
                || !match value {
                    Value::Bool(_) => true,
                    Value::String(s) => !s.chars().any(char::is_control),
                    Value::Number(n) => n.as_i64().is_some_and(|n| i32::try_from(n).is_ok()),
                    _ => false,
                }
            {
                return Err(EventError("invalid extension attribute"));
            }
        }
        Ok(())
    }

    fn command_invalid(&self) -> bool {
        !(self.commandid.starts_with("discord:") || self.commandid.starts_with("grant:"))
            || self.commandid.chars().any(char::is_control)
            || self.commandid.ends_with(':')
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn historical_fixture_preserves_its_original_identity_and_optional_metadata() {
        let raw = include_str!("../tests/fixtures/member-enrolled.json");
        let stored: CloudEvent = serde_json::from_str(raw).unwrap();
        let ctx = Context {
            application: 1,
            guild: 2,
            revision: 3,
            command: "discord:4",
            accepted_at: 1000,
        };
        stored
            .validate(&ctx, "member.enrolled", "members/5")
            .unwrap();
        let encoded = serde_json::to_value(&stored).unwrap();
        assert_eq!(encoded, serde_json::from_str::<Value>(raw).unwrap());
    }

    #[test]
    fn serialized_event_preserves_identity_payload_and_unknown_extension() {
        let ctx = Context {
            application: 1,
            guild: 2,
            revision: 3,
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
        event.extensions.insert("tracehint".into(), json!("opaque"));
        let encoded = serde_json::to_value(&event).unwrap();
        assert_eq!(encoded["tracehint"], "opaque");
        assert!(encoded.get("extensions").is_none());
        assert!(encoded.get("eventindex").is_none());
        assert_eq!(
            uuid::Uuid::parse_str(&event.id).unwrap().get_version_num(),
            7
        );
        let decoded: CloudEvent = serde_json::from_value(encoded).unwrap();
        decoded
            .validate(&ctx, "member.enrolled", "members/5")
            .unwrap();
        assert_eq!(event, decoded);
    }

    #[test]
    fn rejects_wrong_identity_version_stream_schema_or_extension_type() {
        let ctx = Context {
            application: 1,
            guild: 2,
            revision: 3,
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
        invalid.id = uuid::Uuid::new_v4().to_string();
        assert!(
            invalid
                .validate(&ctx, "member.enrolled", "members/5")
                .is_err()
        );
        let mut invalid = valid.clone();
        invalid.guildid = "9".into();
        assert!(
            invalid
                .validate(&ctx, "member.enrolled", "members/5")
                .is_err()
        );
        let mut invalid = valid.clone();
        invalid.dataschema.push_str("wrong");
        assert!(
            invalid
                .validate(&ctx, "member.enrolled", "members/5")
                .is_err()
        );
        let mut invalid = valid;
        invalid
            .extensions
            .insert("nested".into(), json!({"unsupported": true}));
        assert!(
            invalid
                .validate(&ctx, "member.enrolled", "members/5")
                .is_err()
        );
    }
}
