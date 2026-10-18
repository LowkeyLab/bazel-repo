//! Frozen ranking commands and recovery derived solely from immutable histories.
//!
//! Correlation extension: cloudevents/spec at
//! 472f0e1d856e335c30fa204f36fd6428c2bfd732 (experimental, explicitly accepted).
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{
    PgPool, SqlSafeStr,
    migrate::{Migration, MigrationType, Migrator},
};
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        content::Form,
        error::{forbidden, see_other},
        page,
    },
    view::{BoxView, View, ViewExt, view},
};
use uuid::Uuid;

use book_smartz_domain::{
    Book, BookId, BookRegistry, Command, CommandContext, ComparisonChoice, EventId,
    OpenLibraryWorkId, ReaderId, encode_event,
};
use book_smartz_storage::{ApplicationEvent, CommandTransaction, OperationContext};

use crate::{Application, authorize, catalog::CatalogWork};

mod evidence;

use evidence::{
    facts, prepared, ranking_document, registration_prepared, rejected, saved, unresolved,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(Uuid);
impl ActionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn text(self) -> String {
        self.0.to_string()
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    action: ActionId,
    selection: String,
    work: Option<CatalogWork>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    answer: Option<Preference>,
    revision: u64,
    event: Uuid,
    time: DateTime<Utc>,
}
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Choice {
    Candidate,
    Opponent,
    Pause,
    Resume,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    candidate: Uuid,
    opponent: Uuid,
    choice: Choice,
}
impl Preference {
    fn binding(&self) -> String {
        let kind = match self.choice {
            Choice::Candidate | Choice::Opponent => "answer",
            Choice::Pause => "pause",
            Choice::Resume => "resume",
        };
        format!("{kind}:{}:{}", self.candidate, self.opponent)
    }
}
#[derive(Deserialize)]
pub struct AnswerInput {
    csrf: String,
    action: ActionId,
    candidate: Uuid,
    opponent: Uuid,
    choice: Choice,
    revision: u64,
    offer: String,
}
#[derive(Deserialize)]
pub struct AddInput {
    csrf: String,
    action: ActionId,
    selection: String,
    revision: u64,
    offer: String,
}
#[derive(Deserialize)]
pub struct RecoveryInput {
    csrf: String,
    action: ActionId,
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    let mut connection = pool.acquire().await?;
    connection.close_on_drop();
    sqlx::query("BEGIN").execute(&mut *connection).await?;
    sqlx::query("SELECT pg_advisory_xact_lock(706165241, 2)")
        .execute(&mut *connection)
        .await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS book_smartz_actions")
        .execute(&mut *connection)
        .await?;
    sqlx::query("COMMIT").execute(&mut *connection).await?;
    sqlx::query("SET search_path TO book_smartz_actions, pg_catalog")
        .execute(&mut *connection)
        .await?;
    Migrator::with_migrations(vec![Migration::new(
        1,
        "retained display".into(),
        MigrationType::Simple,
        include_str!("actions.sql").into_sql_str(),
        false,
    )])
    .run(&mut *connection)
    .await?;
    Ok(())
}
pub async fn ready(pool: &PgPool) -> anyhow::Result<()> {
    let version: Option<i64> = sqlx::query_scalar(
        "SELECT max(version) FROM book_smartz_actions._sqlx_migrations WHERE success",
    )
    .fetch_one(pool)
    .await?;
    anyhow::ensure!(version == Some(1), "incompatible action schema");
    sqlx::query(
        "SELECT book_id, title, authors, cover, source FROM book_smartz_actions.display LIMIT 0",
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn intent(tx: &CommandTransaction, action: ActionId) -> anyhow::Result<Option<Intent>> {
    evidence::intent(tx.application_history(), action)
}
fn application_fact(
    tx: &CommandTransaction,
    reader: ReaderId,
    action: ActionId,
    kind: &str,
    cause: Option<&str>,
    data: &Value,
    id: Uuid,
) -> anyhow::Result<ApplicationEvent> {
    let sequence = tx
        .application_history()
        .last()
        .map_or(0, ApplicationEvent::sequence)
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("Application history exhausted"))?;
    let family = if data.get("answer").is_some_and(|a| !a.is_null())
        || intent(tx, action)?.is_some_and(|i| i.answer.is_some())
    {
        "preference"
    } else {
        "add"
    };
    let mut envelope = json!({"specversion":"1.0","id":id.to_string(),"source":format!("urn:book-smartz:application:{}",reader.as_uuid()),"type":format!("booksmartz.{family}.{kind}.v1"),"time":Utc::now().to_rfc3339(),"datacontenttype":"application/json","sequence":format!("{sequence:020}"),"correlationid":action.text(),"data":data});
    if let Some(cause) = cause {
        envelope["causationid"] = json!(cause);
    }
    Ok(ApplicationEvent::decode(reader, &envelope.to_string())?)
}

async fn fact(
    tx: &mut CommandTransaction,
    reader: ReaderId,
    action: ActionId,
    kind: &str,
    cause: Option<&str>,
    data: Value,
) -> anyhow::Result<()> {
    let event = application_fact(tx, reader, action, kind, cause, &data, Uuid::new_v4())?;
    tx.append_application(&event).await?;
    Ok(())
}

#[page(POST "/add")]
pub async fn add(cx: &Cx, Form(input): Form<AddInput>) -> Result<impl View> {
    let session = authorize(cx).await?;
    crate::require_post(cx, &session, &input.csrf)?;
    let app: &Application = app_context(cx);
    let started = std::time::Instant::now();
    app.catalog
        .verify_offer(
            session.reader,
            input.action,
            &input.selection,
            input.revision,
            &input.offer,
        )
        .map_err(|_| forbidden())?;
    let Ok(mut tx) = app.store.begin_command(session.reader).await else {
        app.diagnostics.unconfirmed_save(started, input.action);
        return Ok(outcome(
            cx,
            "Checking an earlier save. Reload before trying the original action again.",
        ));
    };
    if let Some(original) =
        intent(&tx, input.action).map_err(|_| topcoat::Error::msg("Action data unavailable"))?
    {
        if original.answer.is_some()
            || original.selection != input.selection
            || original.revision != input.revision
        {
            return Err(forbidden().into());
        }
    } else {
        let work = app
            .catalog
            .verify(session.reader, &input.selection)
            .map_err(|_| forbidden())?;
        if !unresolved(tx.application_history(), tx.ranking_documents())
            .map_err(|_| topcoat::Error::msg("Action data unavailable"))?
            .is_empty()
        {
            return Ok(outcome(
                cx,
                "Checking an earlier save. Resolve it before adding another book.",
            ));
        }
        let original = Intent {
            action: input.action,
            selection: input.selection,
            work: Some(work),
            answer: None,
            revision: input.revision,
            event: Uuid::new_v4(),
            time: Utc::now(),
        };
        fact(
            &mut tx,
            session.reader,
            input.action,
            "intent",
            None,
            serde_json::to_value(original)?,
        )
        .await
        .map_err(|_| topcoat::Error::msg("Action data unavailable"))?;
        if tx.commit().await.is_err() {
            app.diagnostics.unconfirmed_save(started, input.action);
            return Ok(outcome(
                cx,
                "Checking your save. Reload your ranking to recover the original action.",
            ));
        }
        return finish(cx, session.reader, input.action).await;
    }
    drop(tx);
    finish(cx, session.reader, input.action).await
}
#[page(POST "/answer")]
pub async fn submit_preference(cx: &Cx, Form(input): Form<AnswerInput>) -> Result<impl View> {
    if !matches!(input.choice, Choice::Candidate | Choice::Opponent) {
        return Err(forbidden().into());
    }
    submit_ranking_command(cx, input).await
}
#[page(POST "/pause")]
pub async fn pause(cx: &Cx, Form(input): Form<AnswerInput>) -> Result<impl View> {
    if input.choice != Choice::Pause {
        return Err(forbidden().into());
    }
    submit_ranking_command(cx, input).await
}
#[page(POST "/resume")]
pub async fn resume(cx: &Cx, Form(input): Form<AnswerInput>) -> Result<impl View> {
    if input.choice != Choice::Resume {
        return Err(forbidden().into());
    }
    submit_ranking_command(cx, input).await
}
async fn submit_ranking_command(cx: &Cx, input: AnswerInput) -> Result<BoxView<'_>> {
    let session = authorize(cx).await?;
    crate::require_post(cx, &session, &input.csrf)?;
    let app: &Application = app_context(cx);
    let started = std::time::Instant::now();
    let answer = Preference {
        candidate: input.candidate,
        opponent: input.opponent,
        choice: input.choice,
    };
    app.catalog
        .verify_offer(
            session.reader,
            input.action,
            &answer.binding(),
            input.revision,
            &input.offer,
        )
        .map_err(|_| forbidden())?;
    let Ok(mut tx) = app.store.begin_command(session.reader).await else {
        app.diagnostics.unconfirmed_save(started, input.action);
        return Ok(outcome(
            cx,
            "Checking an earlier save. Reload your ranking.",
        ));
    };
    if let Some(original) =
        intent(&tx, input.action).map_err(|_| topcoat::Error::msg("Action data unavailable"))?
    {
        if original.answer.as_ref() != Some(&answer) || original.revision != input.revision {
            return Err(forbidden().into());
        }
    } else {
        if !unresolved(tx.application_history(), tx.ranking_documents())
            .map_err(|_| topcoat::Error::msg("Action data unavailable"))?
            .is_empty()
        {
            return Ok(outcome(
                cx,
                "Checking an earlier save. Resolve it before answering another comparison.",
            ));
        }
        let original = Intent {
            action: input.action,
            selection: String::new(),
            work: None,
            answer: Some(answer),
            revision: input.revision,
            event: Uuid::new_v4(),
            time: Utc::now(),
        };
        fact(
            &mut tx,
            session.reader,
            input.action,
            "intent",
            None,
            serde_json::to_value(original)?,
        )
        .await
        .map_err(|_| topcoat::Error::msg("Action data unavailable"))?;
        if tx.commit().await.is_err() {
            app.diagnostics.unconfirmed_save(started, input.action);
            return Ok(outcome(
                cx,
                "Checking your save. Reload your ranking to recover the original action.",
            ));
        }
        return finish(cx, session.reader, input.action).await;
    }
    drop(tx);
    finish(cx, session.reader, input.action).await
}
#[page(POST "/recover")]
pub async fn recover(cx: &Cx, Form(input): Form<RecoveryInput>) -> Result<impl View> {
    let session = authorize(cx).await?;
    crate::require_post(cx, &session, &input.csrf)?;
    finish(cx, session.reader, input.action).await
}
fn outcome<'a>(cx: &'a Cx, message: &str) -> BoxView<'a> {
    let __cx = cx;
    let message = message.to_owned();
    view! {
        <main>
            <h1>"Book Smartz"</h1>
            <p role="status">(message)</p>
            <a href="/ranking">"Return to your ranking"</a>
        </main>
    }
    .boxed()
}
async fn finish(cx: &Cx, reader: ReaderId, action: ActionId) -> Result<BoxView<'_>> {
    let app: &Application = app_context(cx);
    // Any failure after intent persists is unresolved, including cancellation.
    let started = std::time::Instant::now();
    match execute(app, reader, action).await {
        Ok(Outcome::Saved) => Err(see_other("/ranking").into()),
        Ok(Outcome::Rejected(reason)) => {
            let session = authorize(cx).await?;
            let __cx = cx;
            Ok(view! {
                <!DOCTYPE html>
                <html lang="en">
                    <head>
                        <meta
                            name="viewport"
                            content="width=device-width, initial-scale=1"
                        >
                        <title>"Confirm your ranking · Book Smartz"</title>
                        <link
                            rel="stylesheet"
                            href=(topcoat::asset::asset!("./styles.css"))
                        >
                        <script
                            type="module"
                            src=(topcoat::asset::asset!("./submitting.js"))
                        ></script>
                    </head>
                    <body>
                        book_smartz_views::heading()
                        <main class="mx-auto grid max-w-5xl gap-8 p-6 md:grid-cols-2">
                            <section>
                                <h1>"Confirm your current ranking"</h1>
                                <p role="status">(reason)</p>
                                comparison(reader: reader, csrf: session.csrf)
                                <a href="/ranking">"Return to your ranking"</a>
                            </section>
                            crate::saved_ranking(reader: reader)
                        </main>
                    </body>
                </html>
            }
            .boxed())
        }
        Ok(Outcome::Missing) => Err(forbidden().into()),
        Err(_) => {
            app.diagnostics.unconfirmed_save(started, action);
            Ok(outcome(
                cx,
                "Checking your save. Reload your ranking to recover the original action.",
            ))
        }
    }
}
enum Outcome {
    Saved,
    Rejected(String),
    Missing,
}
async fn execute(app: &Application, reader: ReaderId, action: ActionId) -> anyhow::Result<Outcome> {
    let started = std::time::Instant::now();
    loop {
        // This lock synchronizes with in-flight commits before any safe original retry.
        // A missing unlocked read alone is never used as evidence of rollback.
        let mut tx = app.store.begin_command(reader).await?;
        let Some(original) = intent(&tx, action)? else {
            return Ok(Outcome::Missing);
        };
        if saved(tx.application_history(), tx.ranking_documents(), action)? {
            return Ok(Outcome::Saved);
        }
        if let Some(reason) = rejected(tx.application_history(), action) {
            return Ok(Outcome::Rejected(reason));
        }
        if let Some(event) = prepared(tx.application_history(), action) {
            let document = ranking_document(event, tx.ranking_documents())?;
            if tx.ranking().projection().revision() != original.revision {
                let cause = event["id"].as_str().unwrap().to_owned();
                fact(
                    &mut tx,
                    reader,
                    action,
                    "rejected",
                    Some(&cause),
                    json!({"reason":"Your ranking changed. Reload and confirm a fresh action."}),
                )
                .await?;
                tx.commit().await?;
                continue;
            }
            tx.append_ranking(&document, original.revision).await?;
            tx.commit().await?;
            app.diagnostics.record(
                crate::diagnostics::Category::Persistence,
                crate::diagnostics::Outcome::Confirmed,
                started,
                Some(action),
            );
            return Ok(Outcome::Saved);
        }
        if let Some(answer) = &original.answer {
            drop(tx);
            prepare_preference(app, reader, &original, answer).await?;
            continue;
        }
        let allocation = registration_prepared(tx.application_history(), action).cloned();
        let Some(allocation) = allocation else {
            drop(tx);
            prepare_registration(app, reader, &original).await?;
            continue;
        };
        let proposed = Uuid::parse_str(allocation["data"]["bookId"].as_str().unwrap())?;
        drop(tx);
        let id = register(
            app,
            original
                .work
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Missing work"))?,
            proposed,
        )
        .await?;
        let mut tx = app.store.begin_command(reader).await?;
        // Another retry may have advanced this immutable action while registration ran.
        if prepared(tx.application_history(), action).is_some()
            || rejected(tx.application_history(), action).is_some()
        {
            continue;
        }
        let book = Book::new(
            BookId::new(id),
            OpenLibraryWorkId::try_from(
                original
                    .work
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Missing work"))?
                    .work_id
                    .as_str(),
            )?,
        );
        prepare_ranking(
            &mut tx,
            reader,
            &original,
            &book,
            allocation["id"].as_str().unwrap(),
        )
        .await?;
        tx.commit().await?;
    }
}
async fn prepare_preference(
    app: &Application,
    reader: ReaderId,
    original: &Intent,
    answer: &Preference,
) -> anyhow::Result<()> {
    let action = original.action;
    let books = app
        .store
        .books_by_ids(
            &[BookId::new(answer.candidate), BookId::new(answer.opponent)],
            OperationContext::default(),
        )
        .await?;
    let mut registry = BookRegistry::new();
    for book in books {
        registry.register(book)?;
    }
    let mut tx = app.store.begin_command(reader).await?;
    if prepared(tx.application_history(), action).is_some()
        || rejected(tx.application_history(), action).is_some()
    {
        return Ok(());
    }
    let projection = tx.ranking().projection();
    let decision = if projection
        .pending()
        .is_some_and(|pending| *pending.candidate().as_uuid() == answer.candidate)
    {
        book_smartz_domain::decide(
            projection,
            &registry,
            CommandContext {
                expected_revision: original.revision,
                event_id: EventId::new(original.event),
                time: original.time,
            },
            match answer.choice {
                Choice::Candidate | Choice::Opponent => Command::Answer {
                    opponent: BookId::new(answer.opponent),
                    choice: if answer.choice == Choice::Candidate {
                        ComparisonChoice::PreferCandidate
                    } else {
                        ComparisonChoice::PreferOpponent
                    },
                },
                Choice::Pause => Command::Pause,
                Choice::Resume => Command::Resume,
            },
        )
    } else {
        Err(book_smartz_domain::DomainError::UnknownBook)
    };
    let cause = facts(tx.application_history(), action)[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    prepare_decision(
        &mut tx,
        reader,
        original,
        &cause,
        decision,
        "Your comparison changed. Reload your ranking and confirm a fresh preference.",
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
async fn prepare_registration(
    app: &Application,
    reader: ReaderId,
    original: &Intent,
) -> anyhow::Result<()> {
    // Catalog reads/effects own a connection independently of reader composition.
    let work_id = OpenLibraryWorkId::try_from(
        original
            .work
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing work"))?
            .work_id
            .as_str(),
    )?;
    let existing = app
        .store
        .book_by_work_id(&work_id, OperationContext::default())
        .await?;
    let id = existing.map_or_else(Uuid::new_v4, |book| *book.id().as_uuid());
    let mut tx = app.store.begin_command(reader).await?;
    if registration_prepared(tx.application_history(), original.action).is_some() {
        return Ok(());
    }
    let cause = facts(tx.application_history(), original.action)[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    fact(
        &mut tx,
        reader,
        original.action,
        "registrationprepared",
        Some(&cause),
        json!({"bookId":id}),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
async fn prepare_ranking(
    tx: &mut CommandTransaction,
    reader: ReaderId,
    original: &Intent,
    book: &Book,
    cause: &str,
) -> anyhow::Result<()> {
    let mut registry = BookRegistry::new();
    registry.register(book.clone())?;
    let decision = book_smartz_domain::decide(
        tx.ranking().projection(),
        &registry,
        CommandContext {
            expected_revision: original.revision,
            event_id: EventId::new(original.event),
            time: original.time,
        },
        Command::Start { book_id: book.id() },
    );
    let reason = if tx
        .ranking()
        .projection()
        .entries()
        .iter()
        .any(|e| e.book_id() == book.id())
    {
        "Already ranked."
    } else if tx.ranking().projection().pending().is_some() {
        "Finish your unfinished comparison before adding another book."
    } else {
        "Your ranking changed. Reload and confirm a fresh action."
    };
    prepare_decision(tx, reader, original, cause, decision, reason).await
}
async fn prepare_decision(
    tx: &mut CommandTransaction,
    reader: ReaderId,
    original: &Intent,
    cause: &str,
    decision: std::result::Result<
        book_smartz_domain::RankingEvent,
        book_smartz_domain::DomainError,
    >,
    reason: &str,
) -> anyhow::Result<()> {
    if let Ok(event) = decision {
        let mut envelope: Value = serde_json::from_str(&encode_event(&event)?)?;
        envelope["correlationid"] = json!(original.action.text());
        let prepared_id = Uuid::new_v4();
        envelope["causationid"] = json!(prepared_id.to_string());
        let event = application_fact(
            tx,
            reader,
            original.action,
            "rankingprepared",
            Some(cause),
            &json!({"document":envelope}),
            prepared_id,
        )?;
        tx.append_application(&event).await?;
    } else {
        fact(
            tx,
            reader,
            original.action,
            "rejected",
            Some(cause),
            json!({"reason":reason}),
        )
        .await?;
    }
    Ok(())
}
async fn register(app: &Application, work: &CatalogWork, proposed: Uuid) -> anyhow::Result<Uuid> {
    let mut tx = app.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO book_smartz.books (id, work_id) VALUES ($1,$2) ON CONFLICT DO NOTHING",
    )
    .bind(proposed)
    .bind(&work.work_id)
    .execute(&mut *tx)
    .await?;
    let winner: Uuid = sqlx::query_scalar("SELECT id FROM book_smartz.books WHERE work_id=$1")
        .bind(&work.work_id)
        .fetch_one(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO book_smartz_actions.display (book_id,title,authors,cover,source) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (book_id) DO UPDATE SET title=EXCLUDED.title, authors=CASE WHEN cardinality(EXCLUDED.authors)>0 THEN EXCLUDED.authors ELSE display.authors END, cover=COALESCE(EXCLUDED.cover,display.cover), source=EXCLUDED.source")
        .bind(winner).bind(&work.title).bind(&work.authors).bind(work.cover.map(i64::try_from).transpose()?).bind(work.source()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(winner)
}
#[topcoat::view::component]
pub async fn recovery_controls(cx: &Cx, reader: ReaderId, csrf: String) -> Result<impl View> {
    let app: &Application = app_context(cx);
    let history = app.store.load_application_history(reader).await?;
    let ranking = app.store.load_ranking_documents(reader).await?;
    let actions = unresolved(&history, &ranking)
        .map_err(|_| topcoat::Error::msg("Action data unavailable"))?;
    Ok(view! {
        for action in actions {
            <aside>
                <p role="status">
                    "Checking an earlier save. Further ranking changes are blocked."
                </p>
                <form method="post" action="/recover">
                    <input type="hidden" name="csrf" value=(&csrf)>
                    <input type="hidden" name="action" value=(action.text())>
                    <button type="submit">"Check original save"</button>
                </form>
            </aside>
        }
    })
}

pub async fn checking(app: &Application, reader: ReaderId) -> anyhow::Result<bool> {
    let history = app.store.load_application_history(reader).await?;
    let ranking = app.store.load_ranking_documents(reader).await?;
    Ok(!unresolved(&history, &ranking)?.is_empty())
}
struct ComparisonState {
    candidate: BookId,
    candidate_title: String,
    candidate_link: String,
    revision: u64,
    preferences: usize,
    disabled: bool,
    action: ActionId,
    offered: Option<(BookId, String, String)>,
    placement_control: PlacementControl,
}
struct PlacementControl {
    route: &'static str,
    choice: &'static str,
    label: &'static str,
    offer: String,
}
fn placement_control(
    app: &Application,
    reader: ReaderId,
    action: ActionId,
    candidate: BookId,
    revision: u64,
    paused: bool,
) -> Result<PlacementControl> {
    let (route, choice, value, label) = if paused {
        ("/resume", Choice::Resume, "resume", "Resume")
    } else {
        ("/pause", Choice::Pause, "pause", "Pause for now")
    };
    // Placement transitions bind only the pending identity, never a new opponent.
    let binding = Preference {
        candidate: *candidate.as_uuid(),
        opponent: *candidate.as_uuid(),
        choice,
    }
    .binding();
    Ok(PlacementControl {
        route,
        choice: value,
        label,
        offer: app
            .catalog
            .offer(reader, action, &binding, revision)
            .map_err(|_| topcoat::Error::msg("Action data unavailable"))?,
    })
}
async fn comparison_state(app: &Application, reader: ReaderId) -> Result<Option<ComparisonState>> {
    let ranking = app
        .store
        .load_ranking(reader, OperationContext::default())
        .await?;
    let projection = ranking.projection();
    let Some(pending) = projection.pending() else {
        return Ok(None);
    };
    let candidate = pending.candidate();
    let opponent = projection.next_opponent();
    let ids: Vec<_> = std::iter::once(candidate).chain(opponent).collect();
    let books = app
        .store
        .books_by_ids(&ids, OperationContext::default())
        .await?;
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT book_id,title FROM book_smartz_actions.display WHERE book_id=ANY($1)",
    )
    .bind(ids.iter().map(|id| *id.as_uuid()).collect::<Vec<_>>())
    .fetch_all(&app.pool)
    .await?;
    let titles: HashMap<_, _> = rows.into_iter().collect();
    let title = |id: BookId| {
        titles
            .get(id.as_uuid())
            .cloned()
            .or_else(|| {
                books
                    .iter()
                    .find(|book| book.id() == id)
                    .map(|book| book.work_id().as_str().to_owned())
            })
            .unwrap_or_else(|| "Book unavailable".into())
    };
    let revision = projection.revision();
    let candidate_title = title(candidate);
    let candidate_link = books
        .iter()
        .find(|book| book.id() == candidate)
        .map_or_else(
            || "/ranking".into(),
            |book| format!("https://openlibrary.org/works/{}", book.work_id().as_str()),
        );
    let mut preferences = 0;
    for event in ranking.history().iter().rev() {
        match event.kind() {
            book_smartz_domain::EventKind::PlacementStarted => break,
            book_smartz_domain::EventKind::ComparisonAnswered {
                choice: ComparisonChoice::PreferCandidate | ComparisonChoice::PreferOpponent,
                ..
            } => preferences += 1,
            _ => {}
        }
    }
    let disabled = checking(app, reader)
        .await
        .map_err(|_| topcoat::Error::msg("Action data unavailable"))?;
    let action = ActionId::new();
    let offered = if let Some(opponent) = opponent {
        let binding = Preference {
            candidate: *candidate.as_uuid(),
            opponent: *opponent.as_uuid(),
            choice: Choice::Candidate,
        }
        .binding();
        Some((
            opponent,
            title(opponent),
            app.catalog
                .offer(reader, action, &binding, projection.revision())
                .map_err(|_| topcoat::Error::msg("Action data unavailable"))?,
        ))
    } else {
        None
    };
    let placement_control = placement_control(
        app,
        reader,
        action,
        candidate,
        revision,
        pending.is_paused(),
    )?;
    Ok(Some(ComparisonState {
        candidate,
        candidate_title,
        candidate_link,
        revision,
        preferences,
        disabled,
        action,
        offered,
        placement_control,
    }))
}
#[topcoat::view::component]
pub async fn comparison(cx: &Cx, reader: ReaderId, csrf: String) -> Result<impl View> {
    let app: &Application = app_context(cx);
    let Some(ComparisonState {
        candidate,
        candidate_title,
        candidate_link,
        revision,
        preferences,
        disabled,
        action,
        offered,
        placement_control:
            PlacementControl {
                route,
                choice,
                label,
                offer: placement_offer,
            },
    }) = comparison_state(app, reader).await?
    else {
        return Ok(view! {}.boxed());
    };
    Ok(view! {
        <aside>
            <h2>"Unfinished comparison"</h2>
            <p>
                "Pending book: "
                <a href=(candidate_link)>(candidate_title.clone())</a>
            </p>
            <p role="status">
                (format!("{preferences} preferences saved for this placement."))
            </p>
            if let Some((opponent, opponent_title, offer)) = offered {
                <h3>"Which book do you prefer?"</h3>
                <form method="post" action="/answer" data-ranking-action="">
                    <input type="hidden" name="csrf" value=(&csrf)>
                    <input type="hidden" name="action" value=(action.text())>
                    <input
                        type="hidden"
                        name="candidate"
                        value=(candidate.as_uuid().to_string())
                    >
                    <input
                        type="hidden"
                        name="opponent"
                        value=(opponent.as_uuid().to_string())
                    >
                    <input type="hidden" name="revision" value=(revision.to_string())>
                    <input type="hidden" name="offer" value=(offer)>
                    <fieldset disabled=(disabled)>
                        <legend>"Choose your preferred book"</legend>
                        <button type="submit" name="choice" value="candidate">
                            (format!("Prefer {candidate_title}"))
                        </button>
                        <button type="submit" name="choice" value="opponent">
                            (format!("Prefer {opponent_title}"))
                        </button>
                    </fieldset>
                    <p role="status" data-submitting="" hidden="">
                        "Saving your preference…"
                    </p>
                </form>
            } else {
                <p>"Your unfinished comparison is paused."</p>
            }
            <form method="post" action=(route) data-ranking-action="">
                <input type="hidden" name="csrf" value=(&csrf)>
                <input type="hidden" name="action" value=(action.text())>
                <input
                    type="hidden"
                    name="candidate"
                    value=(candidate.as_uuid().to_string())
                >
                <input
                    type="hidden"
                    name="opponent"
                    value=(candidate.as_uuid().to_string())
                >
                <input type="hidden" name="choice" value=(choice)>
                <input type="hidden" name="revision" value=(revision.to_string())>
                <input type="hidden" name="offer" value=(placement_offer)>
                <button type="submit" disabled=(disabled)>(label)</button>
                <p role="status" data-submitting="" hidden="">
                    "Saving your placement…"
                </p>
            </form>
        </aside>
    }
    .boxed())
}
