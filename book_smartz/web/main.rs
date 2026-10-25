//! The packaged Book Smartz HTTP application.
#![forbid(unsafe_code)]

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use sqlx::{
    ConnectOptions, PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
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

use book_smartz_domain::{PlacementSession, RankingEntry, ReaderId};
use book_smartz_storage::{
    Observation, ObservationDeliveryError, Observer, OperationContext, Store,
};

mod actions;
mod catalog;
mod diagnostics;
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
    catalog: catalog::Catalog,
    diagnostics: diagnostics::Diagnostics,
    integrity: package::IntegrityMonitor,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if run().await.is_ok() {
        return std::process::ExitCode::SUCCESS;
    }
    // Never format dependency errors: they may contain private SQL or credentials.
    // Exit without synchronous diagnostic I/O so a broken sink cannot delay shutdown.
    std::process::ExitCode::FAILURE
}

async fn run() -> anyhow::Result<()> {
    let command = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    anyhow::ensure!(
        matches!(command.as_str(), "serve" | "migrate"),
        "expected serve or migrate"
    );
    if command == "migrate" {
        tokio::time::timeout(Duration::from_secs(120), async {
            let pool = connect(1).await?;
            let store = Store::new(pool.clone(), Arc::new(QuietObserver));
            let result = async {
                store.migrate(OperationContext::default()).await?;
                identity::migrate(&pool).await
            }
            .await;
            pool.close().await;
            result
        })
        .await??;
        return Ok(());
    }
    let pool = connect(5).await?;
    let store = Store::new(pool.clone(), Arc::new(QuietObserver));
    let assets = tokio::task::spawn_blocking(package::load).await??;
    let integrity = package::IntegrityMonitor::start()?;
    let diagnostics = diagnostics::Diagnostics::start(package::release()?);
    let origin = std::env::var("BOOK_SMARTZ_ORIGIN")?;
    let parsed = url::Url::parse(&origin)?;
    anyhow::ensure!(
        parsed.scheme() == "https" && parsed.origin().ascii_serialization() == origin,
        "canonical origin must be an HTTPS origin without a path"
    );
    let google = identity::Google::from_environment(&origin)?;
    let catalog = catalog::Catalog::from_environment()?;
    identity::ready(&pool).await?;
    let router = module_router!()
        .page(ranking)
        .page(catalog::search)
        .page(catalog::select)
        .page(actions::add)
        .page(actions::recover)
        .page(actions::submit_preference)
        .page(actions::pause)
        .page(actions::resume)
        .origin_policy(topcoat::router::OriginPolicy::new().trust_origins([origin.clone()]))
        .layer(PrivateResponses)
        .runtime()
        .font(book_smartz_views::FONT)
        .font(book_smartz_views::HEADING_FONT)
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
            catalog,
            diagnostics,
            integrity,
        })
        .build();
    let service =
        topcoat::router::RouterService::new(router).shutdown_timeout(Duration::from_secs(20));
    topcoat::start(service).await?;
    tokio::time::timeout(Duration::from_secs(5), pool.close()).await?;
    Ok(())
}

async fn connect(maximum: u32) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(maximum)
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
        .connect_with(
            std::env::var("DATABASE_URL")?
                .parse::<PgConnectOptions>()?
                .application_name("book-smartz")
                .disable_statement_logging(),
        )
        .await?;
    Ok(pool)
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
    if !db.integrity.is_ready() {
        return Err(topcoat::router::error::service_unavailable(1).into());
    }
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
            <head>
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Book Smartz"</title>
                book_smartz_views::assets()
                <link rel="stylesheet" href=(topcoat::asset::asset!("./styles.css"))>
                <script type="module" src=(topcoat::asset::asset!("./submitting.js"))></script>
            </head>
            <body>
                book_smartz_views::heading()
                <main class="sign-in">
                    <h1 tabindex="-1" data-response-focus="task">"Book Smartz"</h1>
                    <p>"Your books, in your order."</p>
                    <p>
                        "Sign in to build your private ranking. You choose what to save after sign-in."
                    </p>
                    <a class="primary-link" href="/auth/google">
                        "Sign in with Google"
                    </a>
                </main>
            </body>
        </html>
    })
}

#[page("/ranking")]
async fn ranking(cx: &Cx) -> Result<impl View> {
    let session = authorize(cx).await?;
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Your ranking · Book Smartz"</title>
                book_smartz_views::assets()
                <link rel="stylesheet" href=(topcoat::asset::asset!("./styles.css"))>
                <script type="module" src=(topcoat::asset::asset!("./submitting.js"))></script>
                <script type="module" src=(topcoat::runtime::SCRIPT)></script>
            </head>
            <body>
                book_smartz_views::heading()
                <main class="library-workspace">
                    <section class="active-task" aria-label="Book workspace">
                        actions::comparison(
                            reader: session.reader,
                            csrf: session.csrf.clone()
                        )
                        <h1 tabindex="-1" data-response-focus="task">
                            "Find your next book"
                        </h1>
                        <form method="post" action="/search">
                            <input type="hidden" name="csrf" value=(&session.csrf)>
                            <label>
                                "Title or author"
                                <input name="query" maxlength="256" required="">
                            </label>
                            <input type="hidden" name="page" value="1">
                            <button type="submit">"Search books"</button>
                        </form>
                        actions::recovery_controls(
                            reader: session.reader,
                            csrf: session.csrf.clone()
                        )
                        <form method="post" action="/logout">
                            <input type="hidden" name="csrf" value=(&session.csrf)>
                            <button type="submit">"Sign out of this browser"</button>
                        </form>
                    </section>
                    saved_ranking(reader: session.reader)
                </main>
            </body>
        </html>
    })
}

#[topcoat::view::component]
async fn saved_ranking(cx: &Cx, reader: ReaderId) -> Result<impl View> {
    let db: &Application = app_context(cx);
    let stored_ranking = db
        .store
        .load_ranking(reader, OperationContext::default())
        .await?;
    let projection = stored_ranking.projection();
    let ids: Vec<_> = projection
        .entries()
        .iter()
        .map(RankingEntry::book_id)
        .chain(projection.pending().map(PlacementSession::candidate))
        .collect();
    let books: std::collections::HashMap<_, _> = db
        .store
        .books_by_ids(&ids, OperationContext::default())
        .await?
        .into_iter()
        .map(|book| (book.id(), book))
        .collect();
    let display_ids: Vec<_> = ids.iter().map(|id| *id.as_uuid()).collect();
    let display_rows: Vec<(Uuid, String, Vec<String>, Option<i64>)> = sqlx::query_as(
        "SELECT book_id,title,authors,cover FROM book_smartz_actions.display WHERE book_id=ANY($1)",
    )
    .bind(&display_ids)
    .fetch_all(&db.pool)
    .await?;
    let displays: std::collections::HashMap<_, _> = display_rows
        .into_iter()
        .map(|(id, title, authors, cover)| (id, (title, authors, cover)))
        .collect();
    let mut entries = Vec::new();
    for entry in projection.entries() {
        if let Some(book) = books.get(&entry.book_id()) {
            let work = book.work_id().as_str().to_owned();
            let (title, authors, cover) = displays
                .get(entry.book_id().as_uuid())
                .cloned()
                .unwrap_or_else(|| (work.clone(), Vec::new(), None));
            let author = if authors.is_empty() {
                "Author unavailable".to_owned()
            } else {
                authors.join(", ")
            };
            entries.push((
                entry.book_id(),
                book_smartz_views::BookDisplay {
                    source: format!("https://openlibrary.org/works/{work}"),
                    title,
                    author,
                    cover: cover.map(|cover| {
                        format!("https://covers.openlibrary.org/b/id/{cover}-M.jpg?default=false")
                    }),
                },
            ));
        }
    }
    Ok(view! {
        <section
            class="saved-ranking"
            id="saved-ranking"
            aria-labelledby="saved-ranking-heading"
        >
            <h2 id="saved-ranking-heading">"Your saved ranking"</h2>
            <p>"Saved positions"</p>
            if entries.is_empty() {
                <p>"Your ranking is empty."</p>
                <p>"Find a book to begin your private ranking."</p>
            }
            <ol>
                for (id, display) in entries {
                    <li data-book-id=(id.as_uuid().to_string())>
                        book_smartz_views::book(display: display, compact: true)
                    </li>
                }
            </ol>
            <p>"Pending books receive a position only after placement finishes."</p>
        </section>
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
    require_post(cx, &session, &input.csrf)?;
    sqlx::query("DELETE FROM book_smartz_web.sessions WHERE token_hash = $1")
        .bind(session.hash.as_slice())
        .execute(&db.pool)
        .await?;
    session::stop(cx).await?;
    Err(see_other("/").into())
}

fn require_post(cx: &Cx, session: &BrowserSession, csrf: &str) -> Result<()> {
    let db: &Application = app_context(cx);
    let origins: Vec<_> = request::headers(cx).get_all("origin").iter().collect();
    if origins.len() != 1
        || origins[0].to_str().ok() != Some(db.origin.as_str())
        || csrf != session.csrf
    {
        return Err(forbidden().into());
    }
    Ok(())
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
            // Registered fingerprinted assets retain the framework's immutable
            // policy. Private HTML/auth responses remain non-cacheable.
            if !["/_topcoat/assets/", "/_topcoat/fonts/"]
                .iter()
                .any(|prefix| request::uri(cx).path().starts_with(prefix))
            {
                headers.append(
                    topcoat::router::header::CACHE_CONTROL,
                    topcoat::router::HeaderValue::from_static("no-store"),
                );
            }
            // Native form POSTs need a non-null Origin; disclose no path or query.
            headers.append(
                topcoat::router::header::REFERRER_POLICY,
                topcoat::router::HeaderValue::from_static("strict-origin"),
            );
            next.run(cx, body).await
        })
    }
}
