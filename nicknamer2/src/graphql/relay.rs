use anyhow::{Context as _, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use juniper::ID;
use serde::{Deserialize, Serialize};
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
    /// Server is identified by server_id
    Server { server_id: u64 },
}

impl RelayId {
    /// Encode a Name's composite key into a Relay global ID
    pub fn encode_name(user_id: Uuid, server_id: u64) -> ID {
        let plain = format!("Name:{}:{}", user_id, server_id);
        ID::from(BASE64.encode(plain.as_bytes()))
    }

    /// Encode a Server's ID into a Relay global ID
    pub fn encode_server(server_id: u64) -> ID {
        let plain = format!("Server:{}", server_id);
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
            ["Server", server_id_str] => {
                let server_id = server_id_str
                    .parse::<u64>()
                    .context("Invalid server ID format")?;

                Ok(RelayId {
                    type_name: "Server".to_string(),
                    raw_id: RawId::Server { server_id },
                })
            }
            _ => Err(anyhow!("Unknown type in global ID: {}", plain)),
        }
    }

    /// Extract Name components or return error
    pub fn as_name(&self) -> Result<(Uuid, u64)> {
        match &self.raw_id {
            RawId::Name { user_id, server_id } => Ok((*user_id, *server_id)),
            _ => Err(anyhow!("ID is not a Name type")),
        }
    }

    /// Extract Server components or return error
    pub fn as_server(&self) -> Result<u64> {
        match &self.raw_id {
            RawId::Server { server_id } => Ok(*server_id),
            _ => Err(anyhow!("ID is not a Server type")),
        }
    }
}

/// Cursor for pagination in Relay connections
/// Contains the user_id used for cursor-based pagination
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// The user_id used for pagination (unique ordering)
    pub user_id: Uuid,
}

impl Cursor {
    /// Create a new cursor from a user_id value
    pub fn new(user_id: Uuid) -> Self {
        Self { user_id }
    }

    /// Encode cursor to base64 JSON string
    pub fn encode(&self) -> String {
        let json = serde_json::to_string(self)
            .expect("Cursor serialization should never fail with valid data");
        BASE64.encode(json.as_bytes())
    }

    /// Decode cursor from base64 JSON string
    pub fn decode(encoded: &str) -> Result<Self> {
        let bytes = BASE64
            .decode(encoded)
            .context("Invalid base64 encoding in cursor")?;

        let json_str = String::from_utf8(bytes).context("Invalid UTF-8 in cursor")?;

        let cursor: Cursor = serde_json::from_str(&json_str).context("Invalid JSON in cursor")?;

        Ok(cursor)
    }

    /// Get the user_id value for use in SQL queries
    pub fn user_id_value(&self) -> Uuid {
        self.user_id
    }
}

/// Cursor for pagination in Server connections
/// Contains the server_id used for cursor-based pagination
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerCursor {
    /// The server_id used for pagination
    pub server_id: u64,
}

impl ServerCursor {
    /// Create a new server cursor from a server_id value
    pub fn new(server_id: u64) -> Self {
        Self { server_id }
    }

    /// Encode cursor to base64 JSON string
    pub fn encode(&self) -> String {
        let json = serde_json::to_string(self)
            .expect("ServerCursor serialization should never fail with valid data");
        BASE64.encode(json.as_bytes())
    }

    /// Decode cursor from base64 JSON string
    pub fn decode(encoded: &str) -> Result<Self> {
        let bytes = BASE64
            .decode(encoded)
            .context("Invalid base64 encoding in server cursor")?;

        let json_str = String::from_utf8(bytes).context("Invalid UTF-8 in server cursor")?;

        let cursor: ServerCursor =
            serde_json::from_str(&json_str).context("Invalid JSON in server cursor")?;

        Ok(cursor)
    }

    /// Get the server_id value for use in SQL queries
    pub fn server_id_value(&self) -> u64 {
        self.server_id
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

    #[test]
    fn test_cursor_encode() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let cursor = Cursor::new(user_id);
        let encoded = cursor.encode();

        // Should be base64 encoded
        assert!(!encoded.is_empty());
        // Should be decodeable
        assert!(BASE64.decode(&encoded).is_ok());
    }

    #[test]
    fn test_cursor_round_trip() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let original = Cursor::new(user_id);
        let encoded = original.encode();
        let decoded = Cursor::decode(&encoded).unwrap();

        assert_eq!(decoded.user_id, user_id);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_cursor_with_different_uuids() {
        let user_id = Uuid::parse_str("f47ac10b-58cc-4372-a567-0e02b2c3d479").unwrap();
        let cursor = Cursor::new(user_id);
        let encoded = cursor.encode();
        let decoded = Cursor::decode(&encoded).unwrap();

        assert_eq!(decoded.user_id, user_id);
    }

    #[test]
    fn test_cursor_decode_invalid_base64() {
        let result = Cursor::decode("not-valid-base64!!!");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid base64 encoding")
        );
    }

    #[test]
    fn test_cursor_decode_invalid_json() {
        let invalid_json = BASE64.encode(b"not valid json");
        let result = Cursor::decode(&invalid_json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_cursor_user_id_value() {
        let user_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let cursor = Cursor::new(user_id);
        assert_eq!(cursor.user_id_value(), user_id);
    }

    #[test]
    fn test_server_cursor_encode() {
        let cursor = ServerCursor::new(12345);
        let encoded = cursor.encode();
        assert!(!encoded.is_empty());
        assert!(BASE64.decode(&encoded).is_ok());
    }

    #[test]
    fn test_server_cursor_round_trip() {
        let original = ServerCursor::new(987654321);
        let encoded = original.encode();
        let decoded = ServerCursor::decode(&encoded).unwrap();
        assert_eq!(decoded.server_id, 987654321);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_server_cursor_decode_invalid_base64() {
        let result = ServerCursor::decode("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_server_cursor_decode_invalid_json() {
        let invalid_json = BASE64.encode(b"not valid json");
        let result = ServerCursor::decode(&invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_server_cursor_value() {
        let cursor = ServerCursor::new(42);
        assert_eq!(cursor.server_id_value(), 42);
    }
}
