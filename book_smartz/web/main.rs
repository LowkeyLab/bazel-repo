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
    view::{View, ViewExt, view},
};
use uuid::Uuid;

use book_smartz_domain::{PlacementSession, RankingEntry, ReaderId};
use book_smartz_storage::{
    Observation, ObservationDeliveryError, Observer, OperationContext, Store,
};

mod actions;
mod catalog;
mod diagnostic_events;
mod diagnostics;
mod feedback;
mod identity;
mod initialization;
mod package;

use crate::diagnostic_events::{
    EventIdentity, InitializationOutcome, InitializationReason, InitializationStage, OperationKind,
    OperationOutcome, ReadinessReason, ReadinessResult, ReadinessStage, ReadinessState, Reason,
    ShutdownReason, Stage,
};

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
    feedback: feedback::Feedback,
    diagnostics: diagnostics::Diagnostics,
    integrity: package::IntegrityMonitor,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    if run().await.is_ok() {
        return std::process::ExitCode::SUCCESS;
    }
    // Never format dependency errors: they may contain private SQL or credentials.
    // Early diagnostic output has a bounded wait and cannot replace this failure status.
    std::process::ExitCode::FAILURE
}

async fn run() -> anyhow::Result<()> {
    let command = std::env::args().nth(1).unwrap_or_else(|| "serve".into());
    let event = if command == "migrate" {
        EventIdentity::MigrationFinished
    } else {
        EventIdentity::StartupFinished
    };
    let mut observation = initialization::Initialization::new(event);
    let result = match command.as_str() {
        "migrate" => migrate(&mut observation).await,
        "serve" => serve(&mut observation).await,
        _ => Err(anyhow::anyhow!("expected serve or migrate")),
    };
    if let Err(error) = &result {
        observation.failure(error);
        if let Some(fact) = observation.finish(InitializationOutcome::Failed) {
            initialization::fallback(fact);
        }
    }
    result
}

async fn migrate(observation: &mut initialization::Initialization) -> anyhow::Result<()> {
    initialization::migrate(
        observation,
        Duration::from_secs(120),
        async |observation| {
            let pool = connect(1, observation).await?;
            let store = Store::new(pool.clone(), Arc::new(QuietObserver));
            observation.phase(InitializationStage::Schema, InitializationReason::Schema);
            let result = async {
                store.migrate(OperationContext::default()).await?;
                identity::migrate(&pool).await
            }
            .await;
            pool.close().await;
            result
        },
        initialization::fallback,
    )
    .await
}

async fn serve(observation: &mut initialization::Initialization) -> anyhow::Result<()> {
    let pool = connect(5, observation).await?;
    let store = Store::new(pool.clone(), Arc::new(QuietObserver));
    observation.phase(InitializationStage::Package, InitializationReason::Package);
    let assets = tokio::task::spawn_blocking(package::load).await??;
    let release = package::release()?;
    observation.verified_release(release.clone());
    let integrity = package::IntegrityMonitor::start()?;
    observation.phase(
        InitializationStage::Configuration,
        InitializationReason::Configuration,
    );
    let origin = std::env::var("BOOK_SMARTZ_ORIGIN")?;
    let parsed = url::Url::parse(&origin)?;
    anyhow::ensure!(
        parsed.scheme() == "https" && parsed.origin().ascii_serialization() == origin,
        "canonical origin must be an HTTPS origin without a path"
    );
    observation.phase(
        InitializationStage::Provider,
        InitializationReason::Provider,
    );
    let google = identity::Google::from_environment(&origin)?;
    let catalog = catalog::Catalog::from_environment(pool.clone())?;
    observation.phase(InitializationStage::Schema, InitializationReason::Schema);
    identity::ready(&pool).await?;
    observation.phase(
        InitializationStage::Configuration,
        InitializationReason::Configuration,
    );
    let feedback = feedback::Feedback::from_environment()?;
    let host = environment_or_default("HOST", "127.0.0.1")?;
    let port = environment_or_default("PORT", "3000")?.parse::<u16>()?;
    observation.phase(
        InitializationStage::Listener,
        InitializationReason::Unavailable,
    );
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await?;
    let diagnostics = diagnostics::Diagnostics::start(release, observation.instance());
    let startup_delivery = diagnostics.clone();
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
            feedback,
            diagnostics,
            integrity,
        })
        .build();
    let service =
        topcoat::router::RouterService::new(router).shutdown_timeout(Duration::from_secs(20));
    if let Some(fact) = observation.finish(InitializationOutcome::Succeeded)
        && let Err(fact) = startup_delivery.observe_initialization(fact)
    {
        initialization::fallback(fact);
    }
    let mut shutdown_started = None;
    let shutdown = async {
        tokio::select! { _ = terminate.recv() => {}, _ = interrupt.recv() => {} }
        shutdown_started = Some(startup_delivery.begin_shutdown(ShutdownReason::Signal));
    };
    let serving = topcoat::serve_until(listener, service, shutdown).await;
    let started = shutdown_started
        .unwrap_or_else(|| startup_delivery.begin_shutdown(ShutdownReason::Listener));
    let reason = if serving.is_ok() {
        ShutdownReason::Signal
    } else {
        ShutdownReason::Listener
    };
    let cleanup = startup_delivery
        .finish_shutdown(started, reason, pool.close(), Duration::from_secs(5))
        .await;
    serving?;
    cleanup?;
    Ok(())
}

fn environment_or_default(key: &str, default: &str) -> anyhow::Result<String> {
    match std::env::var(key) {
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Ok(default.into()),
        Err(error) => Err(error.into()),
    }
}

async fn connect(
    maximum: u32,
    observation: &mut initialization::Initialization,
) -> anyhow::Result<PgPool> {
    observation.phase(
        InitializationStage::Configuration,
        InitializationReason::Configuration,
    );
    let options = std::env::var("DATABASE_URL")?
        .parse::<PgConnectOptions>()?
        .application_name("book-smartz")
        .disable_statement_logging();
    observation.phase(
        InitializationStage::Database,
        InitializationReason::Database,
    );
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
        .connect_with(options)
        .await?;
    Ok(pool)
}

struct BrowserSession {
    reader: ReaderId,
    hash: session::TokenHash,
    csrf: String,
}
async fn authorize(cx: &Cx) -> Result<BrowserSession> {
    authorize_observed(cx, None).await
}

async fn authorize_observed(
    cx: &Cx,
    mut diagnostic: Option<&mut diagnostics::Operation>,
) -> Result<BrowserSession> {
    if let Some(operation) = diagnostic.as_mut() {
        operation.stage(Stage::Authentication);
    }
    let hash = match session::token_hash(cx).await {
        Ok(Some(hash)) => hash,
        Ok(None) => {
            if let Some(operation) = diagnostic.as_mut() {
                operation.finish(OperationOutcome::Rejected, Reason::IdentityRejected);
            }
            return Err(unauthorized().into());
        }
        Err(error) => {
            if let Some(operation) = diagnostic.as_mut() {
                operation.stage(Stage::Session);
                operation.finish(
                    OperationOutcome::DependencyFailure,
                    Reason::SessionOperation,
                );
            }
            return Err(error);
        }
    };
    let db: &Application = app_context(cx);
    let row: Option<(Uuid, String)> = sqlx::query_as("SELECT reader_id, csrf FROM book_smartz_web.sessions WHERE token_hash = $1 AND expires_at > now()")
        .bind(hash.as_slice()).fetch_optional(&db.pool).await.inspect_err(|error| { if let Some(operation) = diagnostic.as_mut() { operation.database_failure(error); } })?;
    let Some((reader, csrf)) = row else {
        if let Some(operation) = diagnostic.as_mut() {
            operation.finish(OperationOutcome::Rejected, Reason::IdentityRejected);
        }
        return Err(unauthorized().into());
    };
    Ok(BrowserSession {
        reader: ReaderId::new(reader),
        hash,
        csrf,
    })
}

#[route(GET "/health/ready")]
async fn ready(cx: &Cx) -> Result<&'static str> {
    let db: &Application = app_context(cx);
    let started = std::time::Instant::now();
    let result = if !db.integrity.is_ready() {
        ReadinessResult {
            state: ReadinessState::Unavailable,
            stage: ReadinessStage::Package,
            reason: ReadinessReason::Package,
        }
    } else if let Err(error) = identity::ready(&db.pool).await {
        let database = error
            .chain()
            .find_map(|source| source.downcast_ref::<sqlx::Error>())
            .is_some_and(|error| match error {
                sqlx::Error::Database(error) => !error
                    .code()
                    .is_some_and(|code| code.starts_with("42") || code.starts_with("3F")),
                sqlx::Error::ColumnNotFound(_)
                | sqlx::Error::ColumnDecode { .. }
                | sqlx::Error::Decode(_)
                | sqlx::Error::TypeNotFound { .. }
                | sqlx::Error::Migrate(_) => false,
                _ => true,
            });
        if database {
            ReadinessResult {
                state: ReadinessState::Unavailable,
                stage: ReadinessStage::Database,
                reason: ReadinessReason::Database,
            }
        } else {
            ReadinessResult {
                state: ReadinessState::Unavailable,
                stage: ReadinessStage::Schema,
                reason: ReadinessReason::Schema,
            }
        }
    } else {
        ReadinessResult {
            state: ReadinessState::Ready,
            stage: ReadinessStage::Complete,
            reason: ReadinessReason::None,
        }
    };
    db.diagnostics.observe_readiness(result, started);
    if result.state == ReadinessState::Unavailable {
        return Err(topcoat::router::error::service_unavailable(1).into());
    }
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
    let app: &Application = app_context(cx);
    let mut diagnostic = app.diagnostics.operation(OperationKind::RankingRead);
    let session = authorize_observed(cx, Some(&mut diagnostic)).await?;
    let receipt = app.feedback.take(cx, session.reader);
    let completion = receipt.as_ref().and_then(feedback::Receipt::completion);
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
                        actions::recovery_controls(
                            reader: session.reader,
                            csrf: session.csrf.clone()
                        )
                        if let Some(receipt) = receipt {
                            book_smartz_views::feedback(
                                message: receipt.message(),
                                kind: receipt.focus()
                            )
                        }
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
                        <form method="post" action="/logout">
                            <input type="hidden" name="csrf" value=(&session.csrf)>
                            <button type="submit">"Sign out of this browser"</button>
                        </form>
                    </section>
                    saved_ranking(
                        reader: session.reader,
                        completion: completion,
                        diagnostic: Some(diagnostic)
                    )
                </main>
            </body>
        </html>
    })
}

#[topcoat::view::component]
async fn saved_ranking(
    cx: &Cx,
    reader: ReaderId,
    completion: Option<(Uuid, String)>,
    diagnostic: Option<diagnostics::Operation>,
) -> Result<impl View> {
    let db: &Application = app_context(cx);
    let mut diagnostic =
        diagnostic.unwrap_or_else(|| db.diagnostics.operation(OperationKind::RankingRead));
    diagnostic.stage(Stage::Persistence);
    let stored_ranking = db
        .store
        .load_ranking(reader, OperationContext::default())
        .await
        .inspect_err(|error| diagnostic.storage_failure(error))?;
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
        .await
        .inspect_err(|error| diagnostic.storage_failure(error))?
        .into_iter()
        .map(|book| (book.id(), book))
        .collect();
    let display_ids: Vec<_> = ids.iter().map(|id| *id.as_uuid()).collect();
    let display_rows: Vec<(Uuid, String, Vec<String>, Option<i64>)> = sqlx::query_as(
        "SELECT book_id,title,authors,cover FROM book_smartz_actions.display WHERE book_id=ANY($1)",
    )
    .bind(&display_ids)
    .fetch_all(&db.pool)
    .await
    .inspect_err(|error| diagnostic.database_failure(error))?;
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
            let completion_action = completion
                .as_ref()
                .filter(|(book, _)| book == entry.book_id().as_uuid())
                .map(|(_, action)| action.clone());
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
                completion_action,
            ));
        }
    }
    diagnostic.finish(OperationOutcome::Confirmed, Reason::None);
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
                for (id, display, completion_action) in entries {
                    <li
                        data-book-id=(id.as_uuid().to_string())
                        data-completion-action=(completion_action)
                    >
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
    let db: &Application = app_context(cx);
    let mut diagnostic = db.diagnostics.operation(OperationKind::SignOut);
    let session = authorize_observed(cx, Some(&mut diagnostic)).await?;
    diagnostic.stage(Stage::Validation);
    require_post(cx, &session, &input.csrf)
        .inspect_err(|_| diagnostic.finish(OperationOutcome::Rejected, Reason::InvalidInput))?;
    diagnostic.stage(Stage::SessionRevocation);
    sqlx::query("DELETE FROM book_smartz_web.sessions WHERE token_hash = $1")
        .bind(session.hash.as_slice())
        .execute(&db.pool)
        .await
        .inspect_err(|error| diagnostic.database_failure(error))?;
    diagnostic.stage(Stage::Session);
    session::stop(cx).await.inspect_err(|_| {
        diagnostic.finish(
            OperationOutcome::DependencyFailure,
            Reason::SessionOperation,
        );
    })?;
    diagnostic.finish(OperationOutcome::Confirmed, Reason::None);
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

fn access_error(cx: &Cx, status: topcoat::router::StatusCode) -> topcoat::view::BoxView<'_> {
    let __cx = cx;
    let sign_in = status == topcoat::router::StatusCode::UNAUTHORIZED;
    let title = if sign_in {
        "Sign in to continue"
    } else {
        "Action unavailable"
    };
    let message = if sign_in {
        "Sign in to view your private ranking. Then choose the action you want to submit."
    } else {
        "Return to your ranking. Review the current state before you submit another action."
    };
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                book_smartz_views::assets()
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(format!("{title} · Book Smartz"))</title>
                <link rel="stylesheet" href=(topcoat::asset::asset!("./styles.css"))>
                <script type="module" src=(topcoat::asset::asset!("./submitting.js"))></script>
            </head>
            <body>
                book_smartz_views::heading()
                <main class="sign-in">
                    <h1 tabindex="-1" data-response-focus="error">(title)</h1>
                    book_smartz_views::feedback(
                        message: message.to_owned(),
                        kind: book_smartz_views::FeedbackKind::Status
                    )
                    if sign_in {
                        <a class="primary-link" href="/auth/google">
                            "Sign in with Google"
                        </a>
                    } else {
                        <a href="/ranking">"Return to your ranking"</a>
                    }
                </main>
            </body>
        </html>
    }
    .boxed()
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
            use topcoat::router::{
                StatusCode,
                error::{ForbiddenError, UnauthorizedError},
                response::IntoResponse,
            };
            let headers = topcoat::router::response::response_headers(cx);
            // Registered fingerprinted assets retain the framework's immutable
            // policy. Private HTML/auth responses remain non-cacheable.
            let private_html = !["/_topcoat/assets/", "/_topcoat/fonts/"]
                .iter()
                .any(|prefix| request::uri(cx).path().starts_with(prefix));
            if private_html {
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
            let result = next.run(cx, body).await;
            let status = match &result {
                Ok(response) => Some(response.status()),
                Err(error) if error.downcast_ref::<UnauthorizedError>().is_some() => {
                    Some(StatusCode::UNAUTHORIZED)
                }
                Err(error) if error.downcast_ref::<ForbiddenError>().is_some() => {
                    Some(StatusCode::FORBIDDEN)
                }
                Err(_) => None,
            };
            let Some(status) =
                status.filter(|status| private_html && matches!(status.as_u16(), 401 | 403))
            else {
                return result;
            };
            let mut response = access_error(cx, status).single().await?.into_response(cx)?;
            *response.status_mut() = status;
            Ok(response)
        })
    }
}
