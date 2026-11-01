//! Private profile reads use the stored session and authoritative ranking projection.
use sqlx::{
    PgPool, SqlSafeStr,
    migrate::{Migration, MigrationType, Migrator},
};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{error::service_unavailable, page},
    view::{View, view},
};

use book_smartz_storage::OperationContext;

use crate::{Application, authorize};

#[page("/profile")]
pub async fn show(cx: &Cx) -> Result<impl View> {
    let session = authorize(cx).await?;
    let app: &Application = app_context(cx);
    let fields: Option<(String, String)> = sqlx::query_as(
        "SELECT display_name, biography FROM book_smartz_profiles.profiles WHERE reader_id = $1",
    )
    .bind(session.reader.as_uuid())
    .fetch_optional(&app.pool)
    .await?;
    let (name, biography) = fields.unwrap_or_default();
    // A failed ranking read cannot establish an empty ranking.
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
                <script type="module" src=(topcoat::runtime::SCRIPT)></script>
            </head>
            <body>
                book_smartz_views::heading()
                <main class="profile-workspace">
                    <h1>"Your profile"</h1>
                    <p>"Only you can see your profile."</p>
                    <article class="profile-bookplate" aria-label="Your saved profile">
                        if !name.is_empty() {
                            <h2 class="profile-name">(name)</h2>
                        }
                        if !biography.is_empty() {
                            <p class="profile-biography">(biography)</p>
                        }
                        <a href="/ranking#saved-ranking">(count_label)</a>
                    </article>
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
    Ok(())
}

pub async fn ready(pool: &PgPool) -> anyhow::Result<()> {
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
