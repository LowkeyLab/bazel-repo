use anyhow::{Context as _, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use juniper::ID;
use uuid::Uuid;

/// Relay Global Object Identification ID
/// Format: base64("Type:identifier")
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayId {
    /// The GraphQL type name (e.g., "Name")
    pub type_name: String,
    /// The raw ID components
    pub raw_id: RawId,
}

/// Raw identifier components for different types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawId {
    /// Name is identified by (user_id, server_id)
    Name { user_id: Uuid, server_id: u64 },
}

impl RelayId {
    /// Encode a Name's composite key into a Relay global ID
    pub fn encode_name(user_id: Uuid, server_id: u64) -> ID {
        let plain = format!("Name:{}:{}", user_id, server_id);
        ID::from(BASE64.encode(plain.as_bytes()))
    }

    /// Decode a Relay global ID and extract components
    pub fn decode(encoded: &ID) -> Result<RelayId> {
        let encoded_str: &str = encoded;
        let bytes = BASE64
            .decode(encoded_str)
            .context("Invalid base64 encoding")?;

        let plain = String::from_utf8(bytes).context("Invalid UTF-8 in decoded ID")?;

        let parts: Vec<&str> = plain.split(':').collect();

        match parts.as_slice() {
            ["Name", user_id_str, server_id_str] => {
                let user_id = Uuid::parse_str(user_id_str).context("Invalid UUID format")?;
                let server_id = server_id_str
                    .parse::<u64>()
                    .context("Invalid server ID format")?;

                Ok(RelayId {
                    type_name: "Name".to_string(),
                    raw_id: RawId::Name { user_id, server_id },
                })
            }
            _ => Err(anyhow!("Unknown type in global ID: {}", plain)),
        }
    }

    /// Extract Name components or return error
    pub fn as_name(&self) -> Result<(Uuid, u64)> {
        match &self.raw_id {
            RawId::Name { user_id, server_id } => Ok((*user_id, *server_id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_name_id() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let server_id = 987654321_u64;

        let id = RelayId::encode_name(user_id, server_id);

        // Should be base64 encoded
        let id_str: &str = &id;
        assert!(!id_str.is_empty());
        // Should be decodeable
        let decoded = RelayId::decode(&id).unwrap();
        assert_eq!(decoded.type_name, "Name");
    }

    #[test]
    fn test_decode_name_id_round_trip() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let server_id = 123456789_u64;

        let encoded = RelayId::encode_name(user_id, server_id);
        let decoded = RelayId::decode(&encoded).unwrap();
        let (decoded_user_id, decoded_server_id) = decoded.as_name().unwrap();

        assert_eq!(decoded_user_id, user_id);
        assert_eq!(decoded_server_id, server_id);
    }

    #[test]
    fn test_decode_invalid_base64() {
        let invalid_id = ID::from("not-valid-base64!!!".to_string());
        let result = RelayId::decode(&invalid_id);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid base64 encoding")
        );
    }

    #[test]
    fn test_decode_unknown_type() {
        let plain = "UnknownType:some-id";
        let encoded = ID::from(BASE64.encode(plain.as_bytes()));
        let result = RelayId::decode(&encoded);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unknown type in global ID")
        );
    }

    #[test]
    fn test_decode_malformed_uuid() {
        let plain = "Name:not-a-uuid:123";
        let encoded = ID::from(BASE64.encode(plain.as_bytes()));
        let result = RelayId::decode(&encoded);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid UUID format")
        );
    }

    #[test]
    fn test_decode_malformed_server_id() {
        let plain = "Name:550e8400-e29b-41d4-a716-446655440000:not-a-number";
        let encoded = ID::from(BASE64.encode(plain.as_bytes()));
        let result = RelayId::decode(&encoded);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid server ID format")
        );
    }
}
