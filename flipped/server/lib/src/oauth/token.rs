use std::collections::HashMap;
use std::time::SystemTime;

use crate::application::TokenExchangeRequest;
use crate::credentials::parse_canonical_uuid_v7;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use serde::Serialize;
use subtle::ConstantTimeEq;

use super::OAuthState;

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
}

pub async fn exchange(
    State(state): State<OAuthState>,
    headers: HeaderMap,
    body: String,
) -> Response {
    let _request_event = state
        .application
        .request_event(crate::observability::ServiceEventName::OAuthRequestCompleted);
    let Some((client_id, client_secret)) = basic_credentials(&headers) else {
        return oauth_error("invalid_client", StatusCode::UNAUTHORIZED, true);
    };
    if !bool::from(client_id.as_bytes().ct_eq(state.client_id.as_bytes()))
        || !bool::from(
            client_secret
                .as_bytes()
                .ct_eq(state.client_secret.as_bytes()),
        )
    {
        return oauth_error("invalid_client", StatusCode::UNAUTHORIZED, true);
    }
    let fields = match unique_form(&body) {
        Some(fields) => fields,
        None => return oauth_error("invalid_request", StatusCode::BAD_REQUEST, false),
    };
    let required = |name: &str| fields.get(name).map(String::as_str);
    let (
        Some(grant_type),
        Some(subject_token),
        Some(subject_token_type),
        Some(requested_token_type),
        Some(audience),
        Some(scope),
        Some(redemption_id),
    ) = (
        required("grant_type"),
        required("subject_token"),
        required("subject_token_type"),
        required("requested_token_type"),
        required("audience"),
        required("scope"),
        required("flipped_redemption_id"),
    )
    else {
        return oauth_error("invalid_request", StatusCode::BAD_REQUEST, false);
    };
    let Some(redemption_id) = parse_canonical_uuid_v7(redemption_id) else {
        return oauth_error("invalid_request", StatusCode::BAD_REQUEST, false);
    };
    let result = state
        .application
        .redeem_invitation(
            TokenExchangeRequest {
                client_id: &client_id,
                grant_type,
                subject_token,
                subject_token_type,
                requested_token_type,
                audience,
                scope,
                redemption_id,
            },
            SystemTime::now(),
        )
        .await;
    match result {
        Ok(body) => cached_json(StatusCode::OK, body),
        Err(code) => oauth_error(
            code,
            if code == "server_error" {
                StatusCode::INTERNAL_SERVER_ERROR
            } else {
                StatusCode::BAD_REQUEST
            },
            false,
        ),
    }
}

fn unique_form(body: &str) -> Option<HashMap<String, String>> {
    let pairs: Vec<(String, String)> = serde_urlencoded::from_str(body).ok()?;
    let mut fields = HashMap::new();
    for (key, value) in pairs {
        if fields.insert(key, value).is_some() {
            return None;
        }
    }
    Some(fields)
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (client_id, secret) = decoded.split_once(':')?;
    let client_id = decode_form_component(client_id)?;
    let secret = decode_form_component(secret)?;
    if client_id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((client_id, secret))
}

fn decode_form_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return None;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    let encoded = format!("value={value}");
    let mut pairs = url::form_urlencoded::parse(encoded.as_bytes());
    let (key, decoded) = pairs.next()?;
    if key != "value" || pairs.next().is_some() {
        return None;
    }
    Some(decoded.into_owned())
}

fn oauth_error(code: &'static str, status: StatusCode, authenticate: bool) -> Response {
    let mut response = (status, axum::Json(ErrorBody { error: code })).into_response();
    cache_headers(response.headers_mut());
    if authenticate {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static("Basic realm=\"flipped-oauth\""),
        );
    }
    response
}

fn cached_json(status: StatusCode, body: Vec<u8>) -> Response {
    let mut response = (status, [(header::CONTENT_TYPE, "application/json")], body).into_response();
    cache_headers(response.headers_mut());
    response
}

fn cache_headers(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let encoded = base64::engine::general_purpose::STANDARD.encode(value);
        headers.insert(
            header::AUTHORIZATION,
            format!("Basic {encoded}")
                .parse()
                .expect("authorization header"),
        );
        headers
    }

    #[test]
    fn client_secret_basic_form_decodes_reserved_characters() {
        assert_eq!(
            basic_credentials(&basic("client%3Aid+with+space:s%25cret%3Avalue%2B")),
            Some((
                "client:id with space".to_owned(),
                "s%cret:value+".to_owned()
            ))
        );
    }

    #[test]
    fn client_secret_basic_rejects_malformed_encoding_and_ambiguous_headers() {
        assert!(basic_credentials(&basic("client:bad%2")).is_none());
        assert!(basic_credentials(&basic("client:bad%GG")).is_none());
        assert!(basic_credentials(&basic("client%26other:value")).is_some());
        assert!(basic_credentials(&basic("client&other:value")).is_none());

        let mut headers = basic("client:secret");
        headers.append(
            header::AUTHORIZATION,
            "Basic Y2xpZW50OnNlY3JldA==".parse().expect("header"),
        );
        assert!(basic_credentials(&headers).is_none());
    }
}
