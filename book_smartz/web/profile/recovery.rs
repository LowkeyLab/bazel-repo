//! Tab-specific draft selection never supplies authentication or save authority.
use std::fmt;

use serde::Deserialize;
use sqlx::{
    PgPool, Postgres, SqlSafeStr, Transaction,
    migrate::{Migration, MigrationType, Migrator},
};
use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookie, Cookies, SameSite, cookies},
    router::error::forbidden,
    session,
    view::{BoxView, ViewExt, view},
};
use uuid::Uuid;

use book_smartz_domain::ReaderId;

use super::{
    Editor, Input,
    offers::{Offer, SaveId},
    saves,
    text::Text,
};
use crate::{Application, BrowserSession, require_origin};

#[derive(Clone, Copy, Deserialize)]
#[serde(transparent)]
pub struct DraftId(Uuid);
impl fmt::Display for DraftId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
#[derive(Clone, Copy)]
pub struct BrowserId(Uuid);

pub async fn expired<'a>(cx: &'a Cx, input: &Input) -> Result<Option<BoxView<'a>>> {
    let app: &Application = app_context(cx);
    if let Some(hash) = session::token_hash(cx).await? {
        let csrf: Option<String> = sqlx::query_scalar(
            "SELECT csrf FROM book_smartz_web.sessions WHERE token_hash=$1 AND expires_at>now()",
        )
        .bind(hash.as_slice())
        .fetch_optional(&app.pool)
        .await?;
        if csrf.as_deref() == Some(input.csrf.as_str()) {
            return Ok(None);
        }
    }
    let Some(id) = input.context else {
        return Ok(None);
    };
    require_origin(cx)?;
    let row: Option<(Uuid, String, String)> = sqlx::query_as(
        "SELECT reader_id,csrf,offer FROM book_smartz_profile_recovery.drafts WHERE id=$1 AND expires_at>now()",
    )
    .bind(id.0)
    .fetch_optional(&app.pool)
    .await?;
    let (reader, csrf, offer) = row.ok_or_else(forbidden)?;
    if csrf != input.csrf || offer != input.offer {
        return Err(forbidden().into());
    }
    let reader = ReaderId::new(reader);
    app.profile_offers
        .verify(reader, &input.offer)
        .map_err(|_| forbidden())?;
    if let Some(checking) = &input.checking {
        app.profile_offers
            .verify(reader, checking)
            .map_err(|_| forbidden())?;
    }
    if !matches!(input.command.as_str(), "save" | "check" | "discard") {
        return Err(forbidden().into());
    }
    let updated=sqlx::query("UPDATE book_smartz_profile_recovery.drafts SET checking=$2,display_name=$3,biography=$4,expires_at=now()+interval '720 hours' WHERE id=$1 AND expires_at>now()")
        .bind(id.0).bind(&input.checking).bind(&input.name).bind(&input.biography).execute(&app.pool).await?;
    if updated.rows_affected() != 1 {
        return Err(forbidden().into());
    }
    let __cx = cx;
    Ok(Some(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <title>"Sign in to restore your profile draft · Book Smartz"</title>
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <meta name="referrer" content="no-referrer">
                book_smartz_views::assets()
                <link rel="stylesheet" href=(topcoat::asset::asset!("../styles.css"))>
            </head>
            <body>
                book_smartz_views::heading()
                <main class="sign-in" data-profile-recovery="">
                    <h1 tabindex="-1">"Sign in to restore your profile draft"</h1>
                    <p>
                        "Your entered text remains available for this sign-in. Sign in with the same account to restore it."
                    </p>
                    <p>"Sign-in will not save your changes."</p>
                    <a class="primary-link" href=(format!("/auth/google?profile={id}"))>
                        "Sign in with Google"
                    </a>
                </main>
            </body>
        </html>
    }.boxed()))
}

pub(super) async fn restore(
    pool: &PgPool,
    reader: ReaderId,
    id: DraftId,
    app: &Application,
) -> Result<Editor> {
    let mut tx = saves::locked(pool, reader).await?;
    let row:Option<(String,Option<String>,String,String)>=sqlx::query_as("SELECT offer,checking,display_name,biography FROM book_smartz_profile_recovery.drafts WHERE id=$1 AND reader_id=$2 AND display_name IS NOT NULL AND expires_at>now() FOR UPDATE")
        .bind(id.0).bind(reader.as_uuid()).fetch_optional(&mut *tx).await?;
    let (offer, checking, name, biography) = row.ok_or_else(forbidden)?;
    let mut editor =
        Editor::fresh("We restored your draft. Review it before you save or check your save.");
    let offer = app
        .profile_offers
        .verify(reader, &offer)
        .map_err(|_| forbidden())?;
    sqlx::query("UPDATE book_smartz_profile_recovery.drafts SET expires_at=now()+interval '720 hours' WHERE id=$1")
        .bind(id.0).execute(&mut *tx).await?;
    let submitted = saves::recovery_requires_check(&mut tx, reader, offer).await?;
    editor.offer = Some(if submitted {
        offer
    } else {
        Offer {
            id: SaveId(Uuid::new_v4()),
            revision: offer.revision,
        }
    });
    editor.checking = checking
        .map(|value| app.profile_offers.verify(reader, &value))
        .transpose()
        .map_err(|_| forbidden())?;
    // The original writer can finish after capture but before sign-in returns.
    if editor.checking.is_none() && submitted {
        editor.checking = Some(offer);
    }
    tx.commit().await?;
    editor.draft = Some(Text { name, biography });
    editor.recovery = Some(id);
    Ok(editor)
}

const BROWSER_COOKIE: &str = "__Host-book-smartz-profile";
fn browser(cx: &Cx) -> Option<BrowserId> {
    cookies(cx)
        .get(BROWSER_COOKIE)
        .and_then(|cookie| Uuid::parse_str(cookie.value()).ok())
        .map(BrowserId)
}
pub fn remember_browser(cx: &Cx, id: BrowserId) {
    // This session cookie groups revocation only. It cannot authorize reads or saves.
    cookies(cx).add(
        Cookie::build((BROWSER_COOKIE, id.0.to_string()))
            .secure(true)
            .http_only(true)
            .same_site(SameSite::Lax)
            .path("/")
            .build(),
    );
}
pub(super) async fn prepare(
    cx: &Cx,
    session: &BrowserSession,
    offer: &str,
) -> std::result::Result<DraftId, sqlx::Error> {
    let app: &Application = app_context(cx);
    let mut tx = app.pool.begin().await?;
    cleanup(&mut tx).await?;
    let browser = browser(cx).unwrap_or_else(|| BrowserId(Uuid::new_v4()));
    let id = DraftId(Uuid::new_v4());
    sqlx::query("INSERT INTO book_smartz_profile_recovery.drafts (id,reader_id,offer,csrf,session_hash,browser_id,expires_at) SELECT $1,$2,$3,$4,$5,$6,expires_at+interval '720 hours' FROM book_smartz_web.sessions WHERE token_hash=$5 AND expires_at>now()")
        .bind(id.0).bind(session.reader.as_uuid()).bind(offer).bind(&session.csrf).bind(session.hash.as_slice()).bind(browser.0).execute(&mut *tx).await?;
    tx.commit().await?;
    remember_browser(cx, browser);
    Ok(id)
}
pub async fn remember(
    tx: &mut Transaction<'_, Postgres>,
    state: &[u8],
    draft: DraftId,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO book_smartz_profile_recovery.login_returns (state_hash,draft_id) SELECT $1,id FROM book_smartz_profile_recovery.drafts WHERE id=$2 AND display_name IS NOT NULL AND expires_at>now()")
        .bind(state).bind(draft.0).execute(&mut **tx).await?;
    Ok(())
}
pub async fn destination(
    tx: &mut Transaction<'_, Postgres>,
    state: &[u8],
    reader: ReaderId,
    session_hash: &[u8],
) -> std::result::Result<Option<(String, BrowserId)>, sqlx::Error> {
    let row:Option<(Uuid,Uuid,Uuid)>=sqlx::query_as("DELETE FROM book_smartz_profile_recovery.login_returns l USING book_smartz_profile_recovery.drafts d WHERE l.state_hash=$1 AND l.expires_at>now() AND d.id=l.draft_id AND d.expires_at>now() RETURNING d.id,d.reader_id,d.browser_id")
        .bind(state).fetch_optional(&mut **tx).await?;
    let Some((id, owner, browser)) = row else {
        return Ok(None);
    };
    let id = DraftId(id);
    let owner = ReaderId::new(owner);
    let browser = BrowserId(browser);
    if owner != reader {
        sqlx::query(
            "DELETE FROM book_smartz_profile_recovery.drafts WHERE reader_id=$1 AND browser_id=$2",
        )
        .bind(owner.as_uuid())
        .bind(browser.0)
        .execute(&mut **tx)
        .await?;
        return Ok(None);
    }
    sqlx::query("UPDATE book_smartz_profile_recovery.drafts SET session_hash=$1 WHERE reader_id=$2 AND browser_id=$3")
        .bind(session_hash).bind(reader.as_uuid()).bind(browser.0).execute(&mut **tx).await?;
    Ok(Some((format!("/profile?recovery={id}"), browser)))
}
pub async fn clear_session(
    tx: &mut Transaction<'_, Postgres>,
    cx: &Cx,
    reader: ReaderId,
    session_hash: &[u8],
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM book_smartz_profile_recovery.drafts WHERE (reader_id=$1 AND session_hash=$2) OR browser_id=$3")
        .bind(reader.as_uuid()).bind(session_hash).bind(browser(cx).map(|id| id.0)).execute(&mut **tx).await?;
    Ok(())
}
pub fn forget_browser(cx: &Cx) {
    cookies(cx).remove(
        Cookie::build((BROWSER_COOKIE, ""))
            .secure(true)
            .http_only(true)
            .same_site(SameSite::Lax)
            .path("/")
            .build(),
    );
}
pub async fn clear_draft(
    pool: &PgPool,
    reader: ReaderId,
    id: DraftId,
) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM book_smartz_profile_recovery.drafts WHERE reader_id=$1 AND id=$2")
        .bind(reader.as_uuid())
        .bind(id.0)
        .execute(pool)
        .await?;
    Ok(())
}
pub async fn cleanup(tx: &mut Transaction<'_, Postgres>) -> std::result::Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM book_smartz_profile_recovery.login_returns WHERE state_hash IN (SELECT state_hash FROM book_smartz_profile_recovery.login_returns WHERE expires_at<=now() ORDER BY expires_at LIMIT 256)").execute(&mut **tx).await?;
    sqlx::query("DELETE FROM book_smartz_profile_recovery.drafts WHERE id IN (SELECT id FROM book_smartz_profile_recovery.drafts WHERE expires_at<=now() ORDER BY expires_at LIMIT 256)").execute(&mut **tx).await?;
    Ok(())
}
pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    let mut c = pool.acquire().await?;
    c.close_on_drop();
    sqlx::query("BEGIN").execute(&mut *c).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(706165241,2)")
        .execute(&mut *c)
        .await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS book_smartz_profile_recovery")
        .execute(&mut *c)
        .await?;
    sqlx::query("COMMIT").execute(&mut *c).await?;
    sqlx::query("SET search_path TO book_smartz_profile_recovery,pg_catalog")
        .execute(&mut *c)
        .await?;
    Migrator::with_migrations(vec![
        Migration::new(
            1,
            "profile draft recovery".into(),
            MigrationType::Simple,
            include_str!("recovery.sql").into_sql_str(),
            false,
        ),
        Migration::new(
            2,
            "profile recovery lifecycle".into(),
            MigrationType::Simple,
            include_str!("recovery_lifecycle.sql").into_sql_str(),
            false,
        ),
    ])
    .run(&mut *c)
    .await?;
    Ok(())
}
pub async fn ready(pool: &PgPool) -> anyhow::Result<()> {
    let version: i64 = sqlx::query_scalar(
        "SELECT max(version) FROM book_smartz_profile_recovery._sqlx_migrations WHERE success",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(version == 2, "incompatible profile recovery schema");
    sqlx::query("SELECT d.id,d.reader_id,d.offer,d.csrf,d.session_hash,d.browser_id,d.checking,d.display_name,d.biography,d.expires_at,l.state_hash,l.draft_id,l.expires_at FROM book_smartz_profile_recovery.drafts d CROSS JOIN book_smartz_profile_recovery.login_returns l LIMIT 0").execute(pool).await?;
    Ok(())
}
