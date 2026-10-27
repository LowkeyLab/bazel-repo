//! Short-lived, reader-bound display receipts follow confirmed command outcomes.
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use topcoat::{
    context::Cx,
    cookie::{Cookie, Cookies, SameSite, cookies},
};

use book_smartz_domain::ReaderId;

use crate::actions::ActionId;

const COOKIE: &str = "__Host-book-smartz-feedback";
const AUDIENCE: &str = "book-smartz-confirmed-display";

pub struct Feedback {
    key: Vec<u8>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum SaveKind {
    Add,
    Preference,
    Pause,
    Resume,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Source {
    Fresh,
    Replay,
    Recovery,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Placement {
    pub book: uuid::Uuid,
    pub position: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub action: ActionId,
    pub kind: SaveKind,
    pub source: Source,
    pub placement: Option<Placement>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Claims {
    aud: String,
    reader: uuid::Uuid,
    exp: i64,
    receipt: Receipt,
}
impl Receipt {
    pub fn completion(&self) -> Option<(uuid::Uuid, String)> {
        if !matches!(self.source, Source::Fresh)
            || !matches!(self.kind, SaveKind::Add | SaveKind::Preference)
        {
            return None;
        }
        self.placement
            .as_ref()
            .map(|placement| (placement.book, self.action.text()))
    }
    pub fn message(&self) -> String {
        match self.source {
            Source::Replay => "This action was already saved.".into(),
            Source::Recovery => "The original save is confirmed.".into(),
            Source::Fresh => self.placement.as_ref().map_or_else(
                || {
                    match self.kind {
                        SaveKind::Add => "Book added. Compare it with books in your ranking to find its position.",
                        SaveKind::Preference => "Your preference is saved.",
                        SaveKind::Pause => "Placement paused. Your preferences are saved.",
                        SaveKind::Resume => "Placement resumed. Your preferences are saved.",
                    }
                    .into()
                },
                |placement| format!("We saved this book at position {}.", placement.position),
            ),
        }
    }
    pub const fn focus(&self) -> book_smartz_views::FeedbackKind {
        if matches!(self.source, Source::Fresh)
            && self.placement.is_none()
            && matches!(self.kind, SaveKind::Add | SaveKind::Preference)
        {
            book_smartz_views::FeedbackKind::Status
        } else {
            book_smartz_views::FeedbackKind::Result
        }
    }
}
impl Feedback {
    pub fn from_environment() -> anyhow::Result<Self> {
        let key = std::env::var("BOOK_SMARTZ_SELECTION_KEY")?.into_bytes();
        anyhow::ensure!(
            key.len() >= 32,
            "display signing key must contain at least 32 bytes"
        );
        Ok(Self { key })
    }
    // Presentation failures cannot change an already confirmed command outcome.
    pub fn issue(&self, cx: &Cx, reader: ReaderId, receipt: Receipt) {
        let claims = Claims {
            aud: AUDIENCE.into(),
            reader: *reader.as_uuid(),
            exp: chrono::Utc::now().timestamp() + 60,
            receipt,
        };
        if let Ok(token) = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(&self.key),
        ) {
            cookies(cx).add(
                Cookie::build((COOKIE, token))
                    .path("/")
                    .secure(true)
                    .http_only(true)
                    .same_site(SameSite::Lax)
                    .max_age(topcoat::cookie::time::Duration::seconds(60))
                    .build(),
            );
        }
    }
    pub fn take(&self, cx: &Cx, reader: ReaderId) -> Option<Receipt> {
        let cookie = cookies(cx).get(COOKIE)?;
        cookies(cx).remove(
            Cookie::build((COOKIE, ""))
                .path("/")
                .secure(true)
                .http_only(true)
                .same_site(SameSite::Lax)
                .build(),
        );
        if cookie.value().len() > 4096 {
            return None;
        }
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        validation.set_audience(&[AUDIENCE]);
        let claims = decode::<Claims>(
            cookie.value(),
            &DecodingKey::from_secret(&self.key),
            &validation,
        )
        .ok()?
        .claims;
        (claims.reader == *reader.as_uuid()).then_some(claims.receipt)
    }
}
