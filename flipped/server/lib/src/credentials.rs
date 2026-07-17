use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use hmac::{Hmac, Mac};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, decode_header, encode,
};
use rand::TryRngCore;
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::config::Config;
use crate::error::{Result, ServerError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AccessRole {
    TestTaker,
    Examiner,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccessClaims {
    pub iss: String,
    pub aud: String,
    pub sub: String,
    pub role: AccessRole,
    pub token_use: String,
    pub credential_version: u64,
    pub jti: String,
    pub iat: u64,
    pub nbf: u64,
    pub exp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub issued_token_type: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitationStatus {
    Available,
    Consumed,
    Revoked,
}

#[derive(Debug, Clone)]
pub struct InvitationRecord {
    pub session_id: String,
    pub digest: [u8; 32],
    pub expires_at: SystemTime,
    pub bound_client_id: String,
    pub bound_audience: String,
    pub status: InvitationStatus,
    pub consumed_at: Option<SystemTime>,
    pub redemption_id: Option<Uuid>,
    pub request_hash: Option<[u8; 32]>,
    pub cached_token_response: Option<Vec<u8>>,
    pub cached_until: Option<SystemTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Jwk {
    pub kty: &'static str,
    #[serde(rename = "use")]
    pub key_use: &'static str,
    pub alg: &'static str,
    pub kid: String,
    pub n: String,
    pub e: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Jwks {
    pub keys: Vec<Jwk>,
}

struct VerificationKey {
    kid: String,
    decoding: DecodingKey,
    public: RsaPublicKey,
}

pub struct CredentialService {
    issuer: String,
    audience: String,
    active_kid: String,
    encoding: EncodingKey,
    verification: Vec<VerificationKey>,
    invitation_hmac_key: [u8; 32],
    jwt_ttl: Duration,
    invitation_ttl: Duration,
}

impl CredentialService {
    pub fn new(config: &Config) -> Result<Self> {
        let private_pem = std::str::from_utf8(&config.jwt_active_private_key).map_err(|_| {
            ServerError::Configuration("JWT private key is not UTF-8 PEM".to_owned())
        })?;
        let private = RsaPrivateKey::from_pkcs8_pem(private_pem).map_err(|_| {
            ServerError::Configuration("JWT private key must be PKCS#8 RSA".to_owned())
        })?;
        validate_rsa(&private.to_public_key())?;
        let encoding = EncodingKey::from_rsa_pem(&config.jwt_active_private_key)
            .map_err(|_| ServerError::Configuration("JWT private key is invalid".to_owned()))?;
        let active_public = private.to_public_key();
        let mut verification = vec![VerificationKey {
            kid: config.jwt_active_kid.clone(),
            decoding: DecodingKey::from_rsa_components(
                &b64(&active_public.n().to_bytes_be()),
                &b64(&active_public.e().to_bytes_be()),
            )
            .map_err(|_| ServerError::Configuration("JWT public key is invalid".to_owned()))?,
            public: active_public,
        }];
        if let (Some(public_pem), Some(kid)) = (
            config.jwt_previous_public_key.as_ref(),
            config.jwt_previous_kid.as_ref(),
        ) {
            let pem = std::str::from_utf8(public_pem).map_err(|_| {
                ServerError::Configuration("previous JWT public key is not UTF-8 PEM".to_owned())
            })?;
            let public = RsaPublicKey::from_public_key_pem(pem).map_err(|_| {
                ServerError::Configuration("previous JWT key must be SPKI RSA".to_owned())
            })?;
            validate_rsa(&public)?;
            verification.push(VerificationKey {
                kid: kid.clone(),
                decoding: DecodingKey::from_rsa_components(
                    &b64(&public.n().to_bytes_be()),
                    &b64(&public.e().to_bytes_be()),
                )
                .map_err(|_| {
                    ServerError::Configuration("previous JWT key is invalid".to_owned())
                })?,
                public,
            });
        }
        Ok(Self {
            issuer: config.oauth_issuer.clone(),
            audience: config.oauth_audience.clone(),
            active_kid: config.jwt_active_kid.clone(),
            encoding,
            verification,
            invitation_hmac_key: config.invitation_hmac_key,
            jwt_ttl: config.jwt_ttl,
            invitation_ttl: config.invitation_ttl,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self {
            issuer: "https://issuer.example".to_owned(),
            audience: "flipped".to_owned(),
            active_kid: "unused-test-key".to_owned(),
            encoding: EncodingKey::from_secret(b"unused-test-only-encoding-key"),
            verification: Vec::new(),
            invitation_hmac_key: [3; 32],
            jwt_ttl: Duration::from_secs(3_600),
            invitation_ttl: Duration::from_secs(900),
        }
    }

    pub fn issue_access_token(
        &self,
        session_id: &str,
        role: AccessRole,
        credential_version: u64,
        now: SystemTime,
    ) -> Result<(String, AccessClaims)> {
        let now_seconds = epoch_seconds(now)?;
        let claims = AccessClaims {
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            sub: session_id.to_owned(),
            role,
            token_use: "access".to_owned(),
            credential_version,
            jti: Uuid::now_v7().to_string(),
            iat: now_seconds,
            nbf: now_seconds,
            exp: now_seconds.saturating_add(self.jwt_ttl.as_secs()),
        };
        let mut header = Header::new(Algorithm::RS256);
        header.typ = Some("JWT".to_owned());
        header.kid = Some(self.active_kid.clone());
        let token = encode(&header, &claims, &self.encoding).map_err(|_| ServerError::Internal)?;
        Ok((token, claims))
    }

    pub fn validate_access_token(&self, token: &str, now: SystemTime) -> Result<AccessClaims> {
        let header = decode_header(token).map_err(|_| ServerError::Credential)?;
        if header.alg != Algorithm::RS256 || header.typ.as_deref() != Some("JWT") {
            return Err(ServerError::Credential);
        }
        let kid = header.kid.ok_or(ServerError::Credential)?;
        let key = self
            .verification
            .iter()
            .find(|candidate| candidate.kid == kid)
            .ok_or(ServerError::Credential)?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);
        validation.validate_nbf = true;
        validation.required_spec_claims = ["exp", "nbf", "iss", "aud", "sub"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        let claims = decode::<AccessClaims>(token, &key.decoding, &validation)
            .map_err(|_| ServerError::Credential)?
            .claims;
        let now = epoch_seconds(now)?;
        if claims.token_use != "access"
            || claims.jti.parse::<Uuid>().is_err()
            || claims.nbf > now
            || claims.exp <= now
        {
            return Err(ServerError::Credential);
        }
        Ok(claims)
    }

    pub fn issue_invitation(
        &self,
        session_id: &str,
        client_id: &str,
        audience: &str,
        now: SystemTime,
    ) -> Result<(String, InvitationRecord)> {
        let mut random = [0_u8; 32];
        rand::rngs::OsRng
            .try_fill_bytes(&mut random)
            .map_err(|_| ServerError::Internal)?;
        let token = b64(&random);
        let digest = self.hash_invitation(&token);
        Ok((
            token,
            InvitationRecord {
                session_id: session_id.to_owned(),
                digest,
                expires_at: now + self.invitation_ttl,
                bound_client_id: client_id.to_owned(),
                bound_audience: audience.to_owned(),
                status: InvitationStatus::Available,
                consumed_at: None,
                redemption_id: None,
                request_hash: None,
                cached_token_response: None,
                cached_until: None,
            },
        ))
    }

    pub fn hash_invitation(&self, token: &str) -> [u8; 32] {
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.invitation_hmac_key)
            .expect("HMAC accepts a 32-byte key");
        mac.update(token.as_bytes());
        mac.finalize().into_bytes().into()
    }

    pub fn invitation_matches(&self, token: &str, expected: &[u8; 32]) -> bool {
        self.hash_invitation(token).ct_eq(expected).into()
    }

    pub fn jwks(&self) -> Jwks {
        Jwks {
            keys: self
                .verification
                .iter()
                .map(|key| Jwk {
                    kty: "RSA",
                    key_use: "sig",
                    alg: "RS256",
                    kid: key.kid.clone(),
                    n: b64(&key.public.n().to_bytes_be()),
                    e: b64(&key.public.e().to_bytes_be()),
                })
                .collect(),
        }
    }
}

pub fn parse_canonical_uuid_v7(value: &str) -> std::result::Result<Uuid, ()> {
    let parsed = Uuid::parse_str(value).map_err(|_| ())?;
    if parsed.get_version_num() != 7 || parsed.hyphenated().to_string() != value {
        return Err(());
    }
    Ok(parsed)
}

pub fn token_exchange_request_hash(fields: &[&str]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for field in fields {
        digest.update((field.len() as u64).to_be_bytes());
        digest.update(field.as_bytes());
    }
    digest.finalize().into()
}

fn validate_rsa(public: &RsaPublicKey) -> Result<()> {
    if public.n().bits() < 2_048 || public.e().to_bytes_be() != [1, 0, 1] {
        return Err(ServerError::Configuration(
            "JWT RSA key must be at least 2048 bits with exponent 65537".to_owned(),
        ));
    }
    Ok(())
}

fn epoch_seconds(time: SystemTime) -> Result<u64> {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| ServerError::Internal)
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::parse_canonical_uuid_v7;

    #[test]
    fn accepts_only_canonical_lowercase_uuid_v7() {
        let valid = "018f3d2e-7b4c-7abc-8def-0123456789ab";
        assert!(parse_canonical_uuid_v7(valid).is_ok());
        for invalid in [
            "018F3D2E-7B4C-7ABC-8DEF-0123456789AB",
            "{018f3d2e-7b4c-7abc-8def-0123456789ab}",
            "018f3d2e7b4c7abc8def0123456789ab",
            "018f3d2e-6b4c-6abc-8def-0123456789ab",
        ] {
            assert!(
                parse_canonical_uuid_v7(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }
}
