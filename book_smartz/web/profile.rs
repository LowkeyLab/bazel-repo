//! Private profile reads use the stored session and authoritative ranking projection.
use serde::Deserialize;
use sqlx::{
    PgPool, SqlSafeStr,
    migrate::{Migration, MigrationType, Migrator},
};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        content::Form,
        error::{forbidden, service_unavailable},
        page,
    },
    view::{View, view},
};
use uuid::Uuid;

use book_smartz_storage::OperationContext;

use crate::{Application, authorize};

mod offers;
mod saves;
mod text;

pub use offers::Offers;
use offers::{Offer, SaveId};
use saves::Outcome;
use text::{Errors, Text};

struct Editor {
    draft: Option<Text>,
    offer: Option<Offer>,
    checking: Option<Offer>,
    errors: Errors,
    message: &'static str,
}
impl Editor {
    fn fresh(message: &'static str) -> Self {
        Self {
            draft: None,
            offer: None,
            checking: None,
            errors: Errors::default(),
            message,
        }
    }
}
#[page("/profile")]
pub async fn show(cx: &Cx) -> Result<impl View> {
    let session = authorize(cx).await?;
    let app: &Application = app_context(cx);
    let mut editor = Editor::fresh("");
    if let Some(pending) = saves::pending(&app.pool, session.reader).await? {
        editor.draft = pending.text;
        editor.offer = Some(pending.offer);
        editor.checking = Some(pending.offer);
        editor.message =
            "We cannot confirm your profile save yet. Check your save before you try again.";
    }
    render(cx, editor).await
}
#[derive(Deserialize)]
pub struct Input {
    csrf: String,
    offer: String,
    checking: Option<String>,
    name: String,
    biography: String,
    command: String,
}
#[page(POST "/profile/edit")]
pub async fn edit(cx: &Cx, Form(input): Form<Input>) -> Result<impl View> {
    let session = authorize(cx).await?;
    crate::require_post(cx, &session, &input.csrf)?;
    let app: &Application = app_context(cx);
    let offer = app
        .profile_offers
        .verify(session.reader, &input.offer)
        .map_err(|_| forbidden())?;
    let checking = input
        .checking
        .as_deref()
        .map(|value| app.profile_offers.verify(session.reader, value))
        .transpose()
        .map_err(|_| forbidden())?;
    let raw = Text {
        name: input.name,
        biography: input.biography,
    };
    let mut editor = Editor {
        draft: Some(raw.clone()),
        offer: Some(offer),
        checking,
        errors: Errors::default(),
        message: "",
    };
    let outcome = match input.command.as_str() {
        "discard" => {
            if checking.is_some() {
                editor.message = "Check your save before discarding these changes.";
                return render(cx, editor).await;
            }
            if let Some(pending) = saves::pending(&app.pool, session.reader).await? {
                editor.checking = Some(pending.offer);
                editor.message = "Check your save before discarding these changes.";
                return render(cx, editor).await;
            }
            return render(
                cx,
                Editor::fresh(
                    "We restored your confirmed profile. Your local edits were discarded.",
                ),
            )
            .await;
        }
        "check" => saves::check(&app.pool, session.reader, checking.unwrap_or(offer)).await,
        "save" => {
            if checking.is_some() {
                editor.message = "Check your save before another profile save.";
                return render(cx, editor).await;
            }
            let text = match Text::validate(&raw.name, &raw.biography) {
                Ok(text) => text,
                Err(errors) => {
                    editor.errors = errors;
                    editor.message = "Check the marked profile fields.";
                    return render(cx, editor).await;
                }
            };
            saves::save(&app.pool, session.reader, offer, &text).await
        }
        _ => return Err(forbidden().into()),
    };
    match outcome {
        Outcome::Saved if checking.is_some_and(|original| original.id != offer.id) => {
            editor = Editor::fresh(
                "Another edit changed your profile. We reloaded the saved profile and discarded your local edits.",
            );
        }
        Outcome::Saved => {
            editor = Editor::fresh(if input.command == "check" {
                "We confirmed your profile save."
            } else {
                "Your profile changes were saved."
            });
        }
        Outcome::Conflict => {
            editor = Editor::fresh(
                "Another edit changed your profile. We reloaded the saved profile and discarded your local edits.",
            );
        }
        Outcome::Failed => {
            editor.offer = Some(Offer {
                id: SaveId(Uuid::new_v4()),
                revision: offer.revision,
            });
            editor.checking = None;
            editor.message = "Your profile changes were not saved. Review them and try again.";
        }
        Outcome::Unconfirmed => {
            editor.checking = Some(checking.unwrap_or(offer));
            editor.message =
                "We cannot confirm your profile save yet. Check your save before you try again.";
        }
        Outcome::Blocked(pending) => {
            editor.checking = Some(pending.offer);
            editor.message =
                "An earlier profile save needs checking. Your new submission was not saved.";
        }
        Outcome::ChangedSubmission => return Err(forbidden().into()),
    }
    render(cx, editor).await
}
async fn render(cx: &Cx, editor: Editor) -> Result<impl View> {
    let __cx = cx;
    let session = authorize(cx).await?;
    let app: &Application = app_context(cx);
    let (saved, revision) = saves::current(&app.pool, session.reader).await?;
    let draft = editor.draft.unwrap_or_else(|| saved.clone());
    let offer = editor.offer.unwrap_or_else(|| Offer {
        id: SaveId(Uuid::new_v4()),
        revision,
    });
    let offer = app.profile_offers.sign(session.reader, offer)?;
    let checking = editor
        .checking
        .map(|offer| app.profile_offers.sign(session.reader, offer))
        .transpose()?;
    let blocked = checking.is_some();
    let errors = editor.errors;
    let ranking = app
        .store
        .load_ranking(session.reader, OperationContext::default())
        .await
        .map_err(|_| service_unavailable(1))?;
    let count = ranking.projection().entries().len();
    let count_label = if count == 1 {
        "1 ranked book".to_owned()
    } else {
        format!("{count} ranked books")
    };
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Your profile · Book Smartz"</title>
                book_smartz_views::assets()
                <link rel="stylesheet" href=(topcoat::asset::asset!("./styles.css"))>
                <script type="module" src=(topcoat::asset::asset!("./profile.js"))></script>
                <script type="module" src=(topcoat::runtime::SCRIPT)></script>
            </head>
            <body>
                book_smartz_views::heading()
                <main class="profile-workspace">
                    <h1>"Your profile"</h1>
                    <p>"Only you can see your profile."</p>
                    <article class="profile-bookplate" aria-label="Your saved profile">
                        if !saved.name.is_empty() {
                            <h2 class="profile-name">(saved.name)</h2>
                        }
                        if !saved.biography.is_empty() {
                            <p class="profile-biography">(saved.biography)</p>
                        }
                        <a href="/ranking#saved-ranking">(count_label)</a>
                    </article>
                    <form
                        class="profile-editor"
                        method="post"
                        action="/profile/edit"
                        data-profile-form=""
                    >
                        <h2>"Edit your profile"</h2>
                        <input type="hidden" name="csrf" value=(session.csrf.clone())>
                        <input type="hidden" name="offer" value=(offer)>
                        if let Some(checking) = checking {
                            <input type="hidden" name="checking" value=(checking)>
                        }
                        <label for="profile-name">"Display name"</label>
                        <p id="profile-name-hint">
                            "Optional. Use one line and up to 100 visible characters."
                        </p>
                        <input
                            id="profile-name"
                            name="name"
                            autocomplete="nickname"
                            value=(draft.name)
                            readonly=(blocked)
                            aria-invalid=(errors.name.map(|_| "true"))
                            aria-describedby="profile-name-hint profile-name-error"
                        >
                        <p
                            class="field-error"
                            id="profile-name-error"
                            hidden=(errors.name.is_none())
                        >
                            (errors.name.unwrap_or(""))
                        </p>
                        <label for="profile-biography">"Biography"</label>
                        <p id="profile-biography-hint">
                            "Optional. Use up to 500 visible characters."
                        </p>
                        <textarea
                            id="profile-biography"
                            name="biography"
                            rows="6"
                            readonly=(blocked)
                            aria-invalid=(errors.biography.map(|_| "true"))
                            aria-describedby="profile-biography-hint profile-biography-error"
                        >
                            (format!("\n{}", draft.biography))
                        </textarea>
                        <p
                            class="field-error"
                            id="profile-biography-error"
                            hidden=(errors.biography.is_none())
                        >
                            (errors.biography.unwrap_or(""))
                        </p>
                        <p
                            role="status"
                            aria-live="polite"
                            tabindex="-1"
                            data-profile-message=""
                        >
                            (editor.message)
                        </p>
                        <div class="profile-controls">
                            <button
                                type="submit"
                                name="command"
                                value="save"
                                disabled=(blocked)
                            >
                                "Save changes"
                            </button>
                            <button
                                type="submit"
                                name="command"
                                value="discard"
                                disabled=(blocked)
                            >
                                "Discard changes"
                            </button>
                            if blocked {
                                <button type="submit" name="command" value="check">
                                    "Check your save"
                                </button>
                            }
                        </div>
                    </form>
                    <form method="post" action="/logout">
                        <input type="hidden" name="csrf" value=(session.csrf)>
                        <button type="submit">"Sign out of this browser"</button>
                    </form>
                </main>
            </body>
        </html>
    })
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    let mut connection = pool.acquire().await?;
    connection.close_on_drop();
    sqlx::query("BEGIN").execute(&mut *connection).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(706165241, 2)")
        .execute(&mut *connection)
        .await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS book_smartz_profiles")
        .execute(&mut *connection)
        .await?;
    sqlx::query("COMMIT").execute(&mut *connection).await?;
    sqlx::query("SET search_path TO book_smartz_profiles, pg_catalog")
        .execute(&mut *connection)
        .await?;
    Migrator::with_migrations(vec![Migration::new(
        1,
        "private reader profiles".into(),
        MigrationType::Simple,
        include_str!("profile.sql").into_sql_str(),
        false,
    )])
    .run(&mut *connection)
    .await?;
    drop(connection);
    saves::migrate(pool).await?;
    Ok(())
}

pub async fn ready(pool: &PgPool) -> anyhow::Result<()> {
    saves::ready(pool).await?;
    let version: i64 = sqlx::query_scalar(
        "SELECT max(version) FROM book_smartz_profiles._sqlx_migrations WHERE success",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(version == 1, "incompatible profile schema");
    sqlx::query(
        "SELECT reader_id, display_name, biography FROM book_smartz_profiles.profiles LIMIT 0",
    )
    .execute(pool)
    .await?;
    Ok(())
}
