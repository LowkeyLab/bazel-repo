use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine;
use flipped_anki::ImportLimits;
use url::Url;

use crate::error::{Result, ServerError};

#[derive(Debug, Clone)]
pub struct Config {
    pub grpc_addr: SocketAddr,
    pub http_addr: SocketAddr,
    pub oauth_issuer: String,
    pub oauth_audience: String,
    pub oauth_client_id: String,
    pub oauth_client_secret: String,
    pub jwt_active_private_key: Vec<u8>,
    pub jwt_active_kid: String,
    pub jwt_previous_public_key: Option<Vec<u8>>,
    pub jwt_previous_kid: Option<String>,
    pub invitation_hmac_key: [u8; 32],
    pub observability_hmac_key: [u8; 32],
    pub environment: String,
    pub instance_id: String,
    pub service_version: String,
    pub otlp_endpoint: Option<String>,
    pub otel_resource_attributes: Option<String>,
    pub otel_traces_sampler: Option<String>,
    pub otel_traces_sampler_arg: Option<String>,
    pub import_limits: ImportLimits,
    pub invitation_ttl: Duration,
    pub session_ttl: Duration,
    pub jwt_ttl: Duration,
    pub redemption_retry: Duration,
    pub command_result_capacity: usize,
    pub event_queue_capacity: usize,
    pub event_stream_capacity: usize,
    pub max_concurrent_imports: usize,
    pub max_global_watches: usize,
    pub max_watches_per_session: usize,
    pub max_sessions: usize,
    pub tombstone_retention: Duration,
    pub observability_flush_timeout: Duration,
    pub cleanup_interval: Duration,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let max_cards = unsigned("FLIPPED_MAX_CARDS", 10_000)?;
        let command_result_capacity = command_capacity(
            max_cards,
            optional_unsigned("FLIPPED_COMMAND_RESULT_CAPACITY")?,
        )?;

        let environment = required("FLIPPED_ENVIRONMENT")?;
        let oauth_issuer = required("FLIPPED_OAUTH_ISSUER")?;
        validate_issuer(&oauth_issuer, &environment)?;
        let active_kid = required("FLIPPED_JWT_ACTIVE_KID")?;
        let previous_public = optional_file("FLIPPED_JWT_PREVIOUS_PUBLIC_KEY_FILE")?;
        let previous_kid = env::var("FLIPPED_JWT_PREVIOUS_KID").ok();
        if previous_public.is_some() != previous_kid.is_some() {
            return Err(ServerError::Configuration(
                "previous JWT key and kid must be configured together".to_owned(),
            ));
        }
        if previous_kid.as_deref() == Some(active_kid.as_str()) {
            return Err(ServerError::Configuration(
                "active and previous JWT kids must differ".to_owned(),
            ));
        }

        Ok(Self {
            grpc_addr: parse_required("FLIPPED_GRPC_ADDR")?,
            http_addr: parse_required("FLIPPED_HTTP_ADDR")?,
            oauth_issuer,
            oauth_audience: required("FLIPPED_OAUTH_AUDIENCE")?,
            oauth_client_id: required("FLIPPED_OAUTH_CLIENT_ID")?,
            oauth_client_secret: read_text_secret("FLIPPED_OAUTH_CLIENT_SECRET_FILE")?,
            jwt_active_private_key: required_file("FLIPPED_JWT_ACTIVE_PRIVATE_KEY_FILE")?,
            jwt_active_kid: active_kid,
            jwt_previous_public_key: previous_public,
            jwt_previous_kid: previous_kid,
            invitation_hmac_key: read_hmac_key("FLIPPED_INVITATION_HMAC_KEY_FILE")?,
            observability_hmac_key: read_hmac_key("FLIPPED_OBSERVABILITY_HMAC_KEY_FILE")?,
            environment,
            instance_id: required("FLIPPED_INSTANCE_ID")?,
            service_version: required("FLIPPED_SERVICE_VERSION")?,
            otlp_endpoint: env::var("FLIPPED_OTLP_ENDPOINT").ok(),
            otel_resource_attributes: env::var("OTEL_RESOURCE_ATTRIBUTES").ok(),
            otel_traces_sampler: env::var("OTEL_TRACES_SAMPLER").ok(),
            otel_traces_sampler_arg: env::var("OTEL_TRACES_SAMPLER_ARG").ok(),
            import_limits: ImportLimits {
                max_upload_bytes: unsigned("FLIPPED_MAX_UPLOAD_BYTES", 20_971_520)? as u64,
                max_extracted_bytes: unsigned("FLIPPED_MAX_EXTRACTED_BYTES", 104_857_600)? as u64,
                max_archive_entries: unsigned("FLIPPED_MAX_ARCHIVE_ENTRIES", 16)?,
                max_entry_bytes: 104_857_600,
                max_compression_ratio: unsigned("FLIPPED_MAX_COMPRESSION_RATIO", 100)? as u64,
                max_cards,
                max_models: nonzero_unsigned("FLIPPED_MAX_MODELS", 128)?,
                max_models_bytes: nonzero_unsigned("FLIPPED_MAX_MODELS_BYTES", 1_048_576)?,
                card_side_max_bytes: unsigned("FLIPPED_CARD_SIDE_MAX_BYTES", 65_536)?,
                sqlite_timeout: Duration::from_millis(unsigned(
                    "FLIPPED_SQLITE_TIMEOUT_MILLISECONDS",
                    5_000,
                )? as u64),
            },
            invitation_ttl: seconds("FLIPPED_INVITATION_TTL_SECONDS", 900)?,
            session_ttl: seconds("FLIPPED_SESSION_TTL_SECONDS", 14_400)?,
            jwt_ttl: seconds("FLIPPED_JWT_TTL_SECONDS", 14_400)?,
            redemption_retry: seconds("FLIPPED_REDEMPTION_RETRY_SECONDS", 60)?,
            command_result_capacity,
            event_queue_capacity: nonzero_unsigned("FLIPPED_EVENT_QUEUE_CAPACITY", 1_024)?,
            event_stream_capacity: nonzero_unsigned("FLIPPED_EVENT_STREAM_CAPACITY", 64)?,
            max_concurrent_imports: nonzero_unsigned("FLIPPED_MAX_CONCURRENT_IMPORTS", 4)?,
            max_global_watches: nonzero_unsigned("FLIPPED_MAX_GLOBAL_WATCHES", 4_096)?,
            max_watches_per_session: nonzero_unsigned("FLIPPED_MAX_WATCHES_PER_SESSION", 8)?,
            max_sessions: nonzero_unsigned("FLIPPED_MAX_SESSIONS", 1_024)?,
            tombstone_retention: seconds("FLIPPED_TOMBSTONE_RETENTION_SECONDS", 300)?,
            observability_flush_timeout: seconds("FLIPPED_OBSERVABILITY_FLUSH_TIMEOUT_SECONDS", 5)?,
            cleanup_interval: nonzero_seconds("FLIPPED_CLEANUP_INTERVAL_SECONDS", 60)?,
        })
    }
}

fn command_capacity(max_cards: usize, explicit: Option<usize>) -> Result<usize> {
    let minimum = max_cards
        .checked_add(2)
        .ok_or_else(|| ServerError::Configuration("FLIPPED_MAX_CARDS + 2 overflows".to_owned()))?;
    let capacity = explicit.unwrap_or(minimum);
    if capacity < minimum {
        return Err(ServerError::Configuration(
            "FLIPPED_COMMAND_RESULT_CAPACITY is smaller than FLIPPED_MAX_CARDS + 2".to_owned(),
        ));
    }
    Ok(capacity)
}

fn validate_issuer(value: &str, environment: &str) -> Result<()> {
    let issuer = Url::parse(value)
        .map_err(|_| ServerError::Configuration("invalid OAuth issuer".to_owned()))?;
    if issuer.query().is_some() || issuer.fragment().is_some() {
        return Err(ServerError::Configuration(
            "OAuth issuer cannot contain a query or fragment".to_owned(),
        ));
    }
    if issuer.scheme() == "https" {
        return Ok(());
    }
    let host = issuer.host_str().unwrap_or_default();
    if issuer.scheme() == "http"
        && (matches!(host, "127.0.0.1" | "::1" | "[::1]" | "localhost")
            || (environment == "test" && host == "flipped-server"))
    {
        return Ok(());
    }
    Err(ServerError::Configuration(
        "non-HTTPS OAuth issuer is not allowed".to_owned(),
    ))
}

fn required(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ServerError::Configuration(format!("{name} is required")))
}

fn parse_required<T>(name: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    required(name)?
        .parse()
        .map_err(|_| ServerError::Configuration(format!("{name} is invalid")))
}

fn optional_unsigned(name: &str) -> Result<Option<usize>> {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| ServerError::Configuration(format!("{name} must be unsigned decimal")))
        })
        .transpose()
}

fn unsigned(name: &str, default: usize) -> Result<usize> {
    optional_unsigned(name).map(|value| value.unwrap_or(default))
}

fn seconds(name: &str, default: usize) -> Result<Duration> {
    Ok(Duration::from_secs(unsigned(name, default)? as u64))
}

fn nonzero_unsigned(name: &str, default: usize) -> Result<usize> {
    let value = unsigned(name, default)?;
    if value == 0 {
        return Err(ServerError::Configuration(format!(
            "{name} must be nonzero"
        )));
    }
    Ok(value)
}

fn nonzero_seconds(name: &str, default: usize) -> Result<Duration> {
    Ok(Duration::from_secs(nonzero_unsigned(name, default)? as u64))
}

fn secret_path(name: &str) -> Result<PathBuf> {
    let path = PathBuf::from(required(name)?);
    if !path.is_absolute() {
        return Err(ServerError::Configuration(format!(
            "{name} must be an absolute path"
        )));
    }
    Ok(path)
}

fn read_file(path: &Path, name: &str) -> Result<Vec<u8>> {
    fs::read(path).map_err(|_| ServerError::Configuration(format!("cannot read {name}")))
}

fn required_file(name: &str) -> Result<Vec<u8>> {
    read_file(&secret_path(name)?, name)
}

fn optional_file(name: &str) -> Result<Option<Vec<u8>>> {
    match env::var(name) {
        Ok(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(ServerError::Configuration(format!(
                    "{name} must be an absolute path"
                )));
            }
            read_file(&path, name).map(Some)
        }
        Err(_) => Ok(None),
    }
}

fn strip_one_line_ending(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.ends_with(b"\r\n") {
        bytes.truncate(bytes.len() - 2);
    } else if bytes.ends_with(b"\n") {
        bytes.truncate(bytes.len() - 1);
    }
    bytes
}

fn read_text_secret(name: &str) -> Result<String> {
    let bytes = strip_one_line_ending(required_file(name)?);
    let value = String::from_utf8(bytes)
        .map_err(|_| ServerError::Configuration(format!("{name} must be UTF-8")))?;
    if value.is_empty() || value.contains(['\0', '\r', '\n']) {
        return Err(ServerError::Configuration(format!(
            "{name} has invalid content"
        )));
    }
    Ok(value)
}

fn read_hmac_key(name: &str) -> Result<[u8; 32]> {
    let encoded = String::from_utf8(strip_one_line_ending(required_file(name)?))
        .map_err(|_| ServerError::Configuration(format!("{name} must be UTF-8")))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| ServerError::Configuration(format!("{name} must be base64url")))?;
    bytes
        .try_into()
        .map_err(|_| ServerError::Configuration(format!("{name} must decode to 32 bytes")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_and_validates_command_capacity_with_checked_arithmetic() {
        assert_eq!(command_capacity(10, None).expect("derived capacity"), 12);
        assert_eq!(command_capacity(10, Some(20)).expect("larger capacity"), 20);
        assert!(command_capacity(10, Some(11)).is_err());
        assert!(command_capacity(usize::MAX, None).is_err());
    }

    #[test]
    fn issuer_guard_allows_only_https_loopback_and_test_alias_http() {
        assert!(validate_issuer("https://auth.example", "production").is_ok());
        assert!(validate_issuer("http://127.0.0.1:8080", "production").is_ok());
        assert!(validate_issuer("http://[::1]:8080", "production").is_ok());
        assert!(validate_issuer("http://localhost:8080", "production").is_ok());
        assert!(validate_issuer("http://flipped-server:8080", "test").is_ok());
        assert!(validate_issuer("http://flipped-server:8080", "production").is_err());
        assert!(validate_issuer("http://auth.example", "test").is_err());
    }
}
