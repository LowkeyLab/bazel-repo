//! Frozen Add commands and recovery derived solely from immutable histories.
//!
//! Correlation extension: cloudevents/spec at
//! 472f0e1d856e335c30fa204f36fd6428c2bfd732 (experimental, explicitly accepted).
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
    Book, BookId, BookRegistry, CloudEventDocument, Command, CommandContext, EventId,
    OpenLibraryWorkId, ReaderId, decode_event, encode_document, encode_event,
};
use book_smartz_storage::{ApplicationEvent, CommandTransaction, OperationContext};

use crate::{Application, authorize, catalog::CatalogWork};

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    work: CatalogWork,
    revision: u64,
    event: Uuid,
    time: DateTime<Utc>,
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

fn facts(history: &[ApplicationEvent], action: ActionId) -> Vec<&Value> {
    history
        .iter()
        .map(ApplicationEvent::envelope)
        .filter(|event| event["correlationid"] == action.text())
        .collect()
}
fn intent(tx: &CommandTransaction, action: ActionId) -> anyhow::Result<Option<Intent>> {
    facts(tx.application_history(), action)
        .iter()
        .find(|e| e["type"] == "booksmartz.add.intent.v1")
        .map(|e| serde_json::from_value(e["data"].clone()).map_err(Into::into))
        .transpose()
}
fn prepared(history: &[ApplicationEvent], action: ActionId) -> Option<&Value> {
    facts(history, action)
        .into_iter()
        .find(|e| e["type"] == "booksmartz.add.rankingprepared.v1")
}
fn saved(
    history: &[ApplicationEvent],
    ranking: &[CloudEventDocument],
    action: ActionId,
) -> anyhow::Result<bool> {
    let Some(expected) = prepared(history, action) else {
        return Ok(false);
    };
    for doc in ranking {
        if doc.event().metadata().id.as_uuid().to_string()
            == expected["data"]["document"]["id"].as_str().unwrap_or("")
        {
            let actual: Value = serde_json::from_str(&encode_document(doc)?)?;
            return Ok(actual == expected["data"]["document"]);
        }
    }
    Ok(false)
}
fn rejected(history: &[ApplicationEvent], action: ActionId) -> Option<String> {
    facts(history, action)
        .into_iter()
        .find(|e| e["type"] == "booksmartz.add.rejected.v1")
        .and_then(|e| e["data"]["reason"].as_str())
        .map(str::to_owned)
}
/// A read only reconstruction. Absence is unresolved, never a rollback verdict.
fn unresolved(
    history: &[ApplicationEvent],
    ranking: &[CloudEventDocument],
) -> anyhow::Result<Vec<ActionId>> {
    let mut result = Vec::new();
    for event in history {
        let e = event.envelope();
        if e["type"] == "booksmartz.add.intent.v1" {
            let recorded: Intent = serde_json::from_value(e["data"].clone())?;
            if rejected(history, recorded.action).is_none()
                && !saved(history, ranking, recorded.action)?
            {
                result.push(recorded.action);
            }
        }
    }
    Ok(result)
}
async fn fact(
    tx: &mut CommandTransaction,
    reader: ReaderId,
    action: ActionId,
    kind: &str,
    cause: Option<&str>,
    data: Value,
) -> anyhow::Result<()> {
    let sequence = tx
        .application_history()
        .last()
        .map_or(1, |e| e.sequence() + 1);
    let mut envelope = json!({"specversion":"1.0","id":Uuid::new_v4().to_string(),"source":format!("urn:book-smartz:application:{}",reader.as_uuid()),"type":format!("booksmartz.add.{kind}.v1"),"time":Utc::now().to_rfc3339(),"datacontenttype":"application/json","sequence":format!("{sequence:020}"),"correlationid":action.text(),"data":data});
    if let Some(cause) = cause {
        envelope["causationid"] = json!(cause);
    }
    tx.append_application(&ApplicationEvent::decode(reader, &envelope.to_string())?)
        .await?;
    Ok(())
}

#[page(POST "/add")]
pub async fn add(cx: &Cx, Form(input): Form<AddInput>) -> Result<impl View> {
    let session = authorize(cx).await?;
    crate::require_post(cx, &session, &input.csrf)?;
    let app: &Application = app_context(cx);
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
        return Ok(outcome(
            cx,
            "Checking an earlier save. Reload before trying the original action again.",
        ));
    };
    if let Some(original) =
        intent(&tx, input.action).map_err(|_| topcoat::Error::msg("Action data unavailable"))?
    {
        if original.selection != input.selection || original.revision != input.revision {
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
            work,
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
    match execute(app, reader, action).await {
        Ok(Outcome::Saved) => Err(see_other("/ranking").into()),
        Ok(Outcome::Rejected(reason)) => Ok(outcome(cx, &reason)),
        Ok(Outcome::Missing) => Err(forbidden().into()),
        Err(_) => Ok(outcome(
            cx,
            "Checking your save. Reload your ranking to recover the original action.",
        )),
    }
}
enum Outcome {
    Saved,
    Rejected(String),
    Missing,
}
async fn execute(app: &Application, reader: ReaderId, action: ActionId) -> anyhow::Result<Outcome> {
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
            let document = decode_event(&event["data"]["document"].to_string())?;
            // A matching identity with different contents is never success or rejection.
            anyhow::ensure!(
                !tx.ranking_documents()
                    .iter()
                    .any(|d| d.event().metadata().id == document.event().metadata().id),
                "conflicting durable evidence"
            );
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
            return Ok(Outcome::Saved);
        }
        let allocation = facts(tx.application_history(), action)
            .into_iter()
            .find(|e| e["type"] == "booksmartz.add.registrationprepared.v1")
            .cloned();
        let Some(allocation) = allocation else {
            let work_id = OpenLibraryWorkId::try_from(original.work.work_id.as_str())?;
            let existing = app
                .store
                .book_by_work_id(&work_id, OperationContext::default())
                .await?;
            let id = existing.map_or_else(Uuid::new_v4, |b| *b.id().as_uuid());
            let cause = facts(tx.application_history(), action)[0]["id"]
                .as_str()
                .unwrap()
                .to_owned();
            fact(
                &mut tx,
                reader,
                action,
                "registrationprepared",
                Some(&cause),
                json!({"bookId":id}),
            )
            .await?;
            tx.commit().await?;
            continue;
        };
        let proposed = Uuid::parse_str(allocation["data"]["bookId"].as_str().unwrap())?;
        let id = register(app, &original.work, proposed).await?;
        let book = Book::new(
            BookId::new(id),
            OpenLibraryWorkId::try_from(original.work.work_id.as_str())?,
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
    if let Ok(event) = decision {
        let mut envelope: Value = serde_json::from_str(&encode_event(&event)?)?;
        envelope["correlationid"] = json!(original.action.text());
        // The recorded preparation directly causes the ranking append.
        let prepared_id = Uuid::new_v4();
        envelope["causationid"] = json!(prepared_id.to_string());
        let sequence = tx
            .application_history()
            .last()
            .map_or(1, |e| e.sequence() + 1);
        let prepared = json!({"specversion":"1.0","id":prepared_id.to_string(),"source":format!("urn:book-smartz:application:{}",reader.as_uuid()),"type":"booksmartz.add.rankingprepared.v1","time":Utc::now().to_rfc3339(),"datacontenttype":"application/json","sequence":format!("{sequence:020}"),"correlationid":original.action.text(),"causationid":cause,"data":{"document":envelope}});
        tx.append_application(&ApplicationEvent::decode(reader, &prepared.to_string())?)
            .await?;
    } else {
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
