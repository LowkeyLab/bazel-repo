//! The packaged Book Smartz HTTP application.
#![forbid(unsafe_code)]

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use sqlx::{PgPool, postgres::PgPoolOptions};
use topcoat::{
    Result,
    asset::RouterBuilderAssetExt,
    context::{Cx, app_context},
    cookie::RouterBuilderCookieExt,
    font::RouterBuilderFontExt,
    router::{
        content::Form,
        error::{forbidden, see_other, unauthorized},
        module_router, page, request, route,
    },
    runtime::RouterBuilderRuntimeExt,
    session::{self, RouterBuilderSessionExt, SessionConfig},
    view::{View, view},
};
use uuid::Uuid;

use book_smartz_domain::ReaderId;
use book_smartz_storage::{
    Observation, ObservationDeliveryError, Observer, OperationContext, Store,
};

mod identity;
mod package;

struct QuietObserver;
impl Observer for QuietObserver {
    fn observe(&self, _: &Observation) -> std::result::Result<(), ObservationDeliveryError> {
        Ok(())
    }
}

struct Application {
    pool: PgPool,
    store: Store,
    origin: String,
    google: identity::Google,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let command = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    anyhow::ensure!(
        matches!(command.as_str(), "serve" | "migrate"),
        "expected serve or migrate"
    );
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET lock_timeout = '5s'")
                    .execute(&mut *connection)
                    .await?;
                sqlx::query("SET statement_timeout = '15s'")
                    .execute(&mut *connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    let store = Store::new(pool.clone(), Arc::new(QuietObserver));
    if command == "migrate" {
        tokio::time::timeout(Duration::from_secs(120), async {
            store.migrate(OperationContext::default()).await?;
            identity::migrate(&pool).await
        })
        .await??;
        pool.close().await;
        return Ok(());
    }
    let assets = package::load()?;
    let origin = std::env::var("BOOK_SMARTZ_ORIGIN")?;
    let parsed = url::Url::parse(&origin)?;
    anyhow::ensure!(
        parsed.scheme() == "https" && parsed.origin().ascii_serialization() == origin,
        "canonical origin must be an HTTPS origin without a path"
    );
    let google = identity::Google::from_environment(&origin)?;
    identity::ready(&pool).await?;
    let router = module_router!()
        .page(ranking)
        .origin_policy(topcoat::router::OriginPolicy::new().trust_origins([origin.clone()]))
        .layer(PrivateResponses)
        .runtime()
        .font(book_smartz_views::FONT)
        .route(ready)
        .route(identity::login)
        .route(identity::callback)
        .route(logout)
        .assets(assets)
        .cookies()
        .sessions(
            SessionConfig::builder()
                .token_store(identity::BrowserTokens)
                .build(),
        )
        .app_context(Application {
            pool: pool.clone(),
            store,
            origin,
            google,
        })
        .build();
    let service =
        topcoat::router::RouterService::new(router).shutdown_timeout(Duration::from_secs(20));
    topcoat::start(service).await?;
    tokio::time::timeout(Duration::from_secs(5), pool.close()).await?;
    Ok(())
}

struct BrowserSession {
    reader: ReaderId,
    hash: session::TokenHash,
    csrf: String,
}
async fn authorize(cx: &Cx) -> Result<BrowserSession> {
    let hash = session::token_hash(cx).await?.ok_or_else(unauthorized)?;
    let db: &Application = app_context(cx);
    let row: Option<(Uuid, String)> = sqlx::query_as("SELECT reader_id, csrf FROM book_smartz_web.sessions WHERE token_hash = $1 AND expires_at > now()")
        .bind(hash.as_slice()).fetch_optional(&db.pool).await?;
    let (reader, csrf) = row.ok_or_else(unauthorized)?;
    Ok(BrowserSession {
        reader: ReaderId::new(reader),
        hash,
        csrf,
    })
}

#[route(GET "/health/ready")]
async fn ready(cx: &Cx) -> Result<&'static str> {
    let db: &Application = app_context(cx);
    identity::ready(&db.pool)
        .await
        .map_err(|_| topcoat::router::error::service_unavailable(1))?;
    Ok("ready")
}

#[page]
async fn home() -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head><title>"Book Smartz"</title></head>
            <body>
                <main>
                    <h1>"Book Smartz"</h1>
                    <p>"Your books, in your order."</p>
                    <a href="/auth/google">"Sign in with Google"</a>
                </main>
            </body>
        </html>
    })
}

#[page("/ranking")]
async fn ranking(cx: &Cx) -> Result<impl View> {
    let session = authorize(cx).await?;
    let db: &Application = app_context(cx);
    let ranking = db
        .store
        .load_ranking(session.reader, OperationContext::default())
        .await?;
    let mut entries = Vec::new();
    for entry in ranking.projection().entries() {
        if let Some(book) = db
            .store
            .book_by_id(entry.book_id(), OperationContext::default())
            .await?
        {
            let work = book.work_id().as_str().to_owned();
            entries.push((format!("https://openlibrary.org/works/{work}"), work));
        }
    }
    let pending = if let Some(placement) = ranking.projection().pending() {
        let book = db
            .store
            .book_by_id(placement.candidate(), OperationContext::default())
            .await?
            .ok_or_else(topcoat::router::error::not_found)?;
        let work = book.work_id().as_str().to_owned();
        let message = if placement.is_paused() {
            "Your unfinished comparison is paused."
        } else {
            "You have an unfinished comparison."
        };
        Some((
            message,
            format!("https://openlibrary.org/works/{work}"),
            work,
        ))
    } else {
        None
    };
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Your ranking · Book Smartz"</title>
                <link rel="stylesheet" href=(topcoat::asset::asset!("./styles.css"))>
                <script type="module" src=(topcoat::runtime::SCRIPT)></script>
            </head>
            <body class="bg-stone-50 text-stone-900">
                book_smartz_views::heading()
                <main class="mx-auto max-w-5xl p-6">
                    <h1>"Your saved ranking"</h1>
                    if entries.is_empty() {
                        <p>"Your ranking is empty."</p>
                    }
                    <ol>
                        for (link, work) in entries {
                            <li>
                                <img
                                    src=(topcoat::asset::asset!("./cover-placeholder.svg"))
                                    alt="Cover unavailable"
                                    width="48"
                                    height="72"
                                >
                                <a href=(link)>(work)</a>
                                <span>" · Author unavailable"</span>
                            </li>
                        }
                    </ol>
                    if let Some((message, link, work)) = pending {
                        <aside>
                            <h2>"Unfinished comparison"</h2>
                            <p>(message)</p>
                            <a href=(link)>(work)</a>
                            <span>" · Author unavailable"</span>
                        </aside>
                    }
                    <form method="post" action="/logout">
                        <input type="hidden" name="csrf" value=(session.csrf)>
                        <button type="submit">"Sign out of this browser"</button>
                    </form>
                </main>
            </body>
        </html>
    })
}

#[derive(Deserialize)]
struct Logout {
    csrf: String,
}
#[route(POST "/logout")]
async fn logout(cx: &Cx, Form(input): Form<Logout>) -> Result<()> {
    let session = authorize(cx).await?;
    let db: &Application = app_context(cx);
    let origins: Vec<_> = request::headers(cx).get_all("origin").iter().collect();
    if origins.len() != 1
        || origins[0].to_str().ok() != Some(db.origin.as_str())
        || input.csrf != session.csrf
    {
        return Err(forbidden().into());
    }
    sqlx::query("DELETE FROM book_smartz_web.sessions WHERE token_hash = $1")
        .bind(session.hash.as_slice())
        .execute(&db.pool)
        .await?;
    session::stop(cx).await?;
    Err(see_other("/").into())
}

struct PrivateResponses;
impl topcoat::router::Layer for PrivateResponses {
    fn path(&self) -> Option<&topcoat::router::Path> {
        None
    }
    fn handle<'a>(
        &'a self,
        cx: &'a Cx,
        body: topcoat::router::Body,
        next: topcoat::router::Next<'a>,
    ) -> topcoat::router::LayerFuture<'a> {
        Box::pin(async move {
            let headers = topcoat::router::response::response_headers(cx);
            headers.append(
                topcoat::router::header::CACHE_CONTROL,
                topcoat::router::HeaderValue::from_static("no-store"),
            );
            headers.append(
                topcoat::router::header::REFERRER_POLICY,
                topcoat::router::HeaderValue::from_static("no-referrer"),
            );
            next.run(cx, body).await
        })
    }
}
