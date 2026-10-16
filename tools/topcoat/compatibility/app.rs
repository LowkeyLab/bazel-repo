//! Throwaway application; controlled fixture identity replaces a live provider.
use sqlx::{PgPool, postgres::PgPoolOptions};
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt, asset},
    context::{Cx, app_context},
    cookie::RouterBuilderCookieExt,
    font::RouterBuilderFontExt,
    router::{error::unauthorized, module_router, page, route},
    runtime::{RouterBuilderRuntimeExt, procedure, shard},
    session::{self, RouterBuilderSessionExt, SessionConfig},
    view::{View, view},
};
use uuid::Uuid;

use book_smartz_domain::ReaderId;
use book_smartz_storage::{OperationContext, Store, tracing_observer};

struct Database {
    pool: PgPool,
    store: Store,
    fixture_reader: ReaderId,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;
    let store = Store::new(pool.clone(), tracing_observer());
    store.migrate(OperationContext::default()).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS topcoat_fixture_sessions (token_hash bytea PRIMARY KEY, reader_id uuid NOT NULL, expires_at timestamptz NOT NULL)")
        .execute(&pool).await?;
    let fixture_reader = ReaderId::new(std::env::var("FIXTURE_READER_ID")?.parse()?);
    let router = module_router!()
        .runtime()
        .font(compatibility_views::FIXTURE_FONT)
        .route(fixture_login)
        .route(protected_procedure)
        .route(protected_shard)
        .cookies()
        .sessions(SessionConfig::default())
        .assets(AssetBundle::load()?)
        .app_context(Database {
            pool,
            store,
            fixture_reader,
        })
        .build();
    topcoat::start(router).await?;
    Ok(())
}

async fn reader(cx: &Cx) -> Result<ReaderId> {
    let hash = session::token_hash(cx).await?.ok_or_else(unauthorized)?;
    let db: &Database = app_context(cx);
    let id: Option<Uuid> = sqlx::query_scalar("SELECT reader_id FROM topcoat_fixture_sessions WHERE token_hash = $1 AND expires_at > now()")
        .bind(hash.as_slice()).fetch_optional(&db.pool).await?;
    id.map(ReaderId::new).ok_or_else(|| unauthorized().into())
}

// Isolated experiment only: no credential or production account provisioning.
#[route(POST "/fixture-login")]
async fn fixture_login(cx: &Cx) -> Result<String> {
    let db: &Database = app_context(cx);
    let issued = session::start(cx).await?;
    let expires_at: chrono::DateTime<chrono::Utc> = issued.expires_at.into();
    sqlx::query("INSERT INTO topcoat_fixture_sessions (token_hash, reader_id, expires_at) VALUES ($1, $2, $3)")
        .bind(issued.token_hash.as_slice()).bind(db.fixture_reader.as_uuid()).bind(expires_at)
        .execute(&db.pool).await?;
    Ok(db.fixture_reader.as_uuid().to_string())
}

#[page]
async fn home(cx: &Cx) -> Result<impl View> {
    let reader = reader(cx).await?;
    let db: &Database = app_context(cx);
    db.store
        .load_ranking(reader, OperationContext::default())
        .await?;
    Ok(view! {
        <!DOCTYPE html>
        <html>
            <head>
                <title>"Compatibility gate"</title>
                <link rel="stylesheet" href=(asset!("./styles.css"))>
                <script type="module" src=(topcoat::runtime::SCRIPT)></script>
            </head>
            <body>
                compatibility_views::library_panel()
                <p>(reader.as_uuid().to_string())</p>
            </body>
        </html>
    })
}

#[procedure("/protected-procedure")]
async fn protected_procedure(cx: &Cx) -> Result<String> {
    Ok(reader(cx).await?.as_uuid().to_string())
}

#[shard("/protected-shard")]
async fn protected_shard(cx: &Cx) -> Result<impl View> {
    let id = reader(cx).await?;
    Ok(view! { <p>(id.as_uuid().to_string())</p> })
}
