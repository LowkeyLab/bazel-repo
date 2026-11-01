//! Signed submission identities keep profile evidence private to the authenticated reader.
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use book_smartz_domain::ReaderId;

const AUDIENCE: &str = "book-smartz-profile-submission";
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SaveId(pub Uuid);
#[derive(Clone, Copy, Deserialize, Serialize)]
pub struct Offer {
    pub id: SaveId,
    pub revision: i64,
}
#[derive(Deserialize, Serialize)]
struct Claims {
    aud: String,
    reader: Uuid,
    offer: Offer,
}
pub struct Offers {
    key: Vec<u8>,
}
impl Offers {
    pub fn from_environment() -> anyhow::Result<Self> {
        let key = std::env::var("BOOK_SMARTZ_SELECTION_KEY")?.into_bytes();
        anyhow::ensure!(
            key.len() >= 32,
            "profile signing key must contain at least 32 bytes"
        );
        Ok(Self { key })
    }
    pub fn sign(&self, reader: ReaderId, offer: Offer) -> jsonwebtoken::errors::Result<String> {
        encode(
            &Header::new(Algorithm::HS256),
            &Claims {
                aud: AUDIENCE.into(),
                reader: *reader.as_uuid(),
                offer,
            },
            &EncodingKey::from_secret(&self.key),
        )
    }
    pub fn verify(&self, reader: ReaderId, token: &str) -> anyhow::Result<Offer> {
        anyhow::ensure!(token.len() <= 4096, "invalid profile submission");
        let mut validation = Validation::new(Algorithm::HS256);
        // Evidence survives restart and elapsed time. Current session authority remains mandatory.
        validation.validate_exp = false;
        validation.set_required_spec_claims(&["aud"]);
        validation.set_audience(&[AUDIENCE]);
        let claims =
            decode::<Claims>(token, &DecodingKey::from_secret(&self.key), &validation)?.claims;
        anyhow::ensure!(
            claims.reader == *reader.as_uuid() && claims.offer.revision >= 0,
            "invalid profile submission"
        );
        Ok(claims.offer)
    }
}
