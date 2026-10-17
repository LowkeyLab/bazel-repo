//! Bounded live discovery, deployment-shared admission and reader-bound selections.
use std::{collections::HashSet, time::Duration};

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        content::Form,
        error::{bad_request, forbidden},
        page,
    },
    view::{View, view},
};

use book_smartz_domain::{OpenLibraryWorkId, ReaderId};

use crate::{Application, authorize};

pub struct Catalog {
    client: reqwest::Client,
    endpoint: url::Url,
    signing_key: Vec<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogWork {
    pub work_id: String,
    pub title: String,
    pub authors: Vec<String>,
    year: Option<i32>,
    pub cover: Option<u64>,
}
impl CatalogWork {
    fn valid(&self) -> bool {
        self.work_id.len() <= 64
            && OpenLibraryWorkId::try_from(self.work_id.as_str()).is_ok()
            && valid_text(&self.title, 1024)
            && self.authors.len() <= 20
            && self.authors.iter().all(|author| valid_text(author, 256))
            && self.year.is_none_or(|year| (-5000..=10000).contains(&year))
            && self
                .cover
                .is_none_or(|cover| cover > 0 && i64::try_from(cover).is_ok())
    }
    pub fn source(&self) -> String {
        format!("https://openlibrary.org/works/{}", self.work_id)
    }
    pub fn cover_url(&self) -> Option<String> {
        self.cover
            .map(|id| format!("https://covers.openlibrary.org/b/id/{id}-M.jpg?default=false"))
    }
    pub fn author_label(&self) -> String {
        if self.authors.is_empty() {
            "Author unavailable".into()
        } else {
            self.authors.join(", ")
        }
    }
}
fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= maximum
        && !value.chars().any(char::is_control)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    iss: String,
    aud: String,
    reader: uuid::Uuid,
    iat: i64,
    exp: i64,
    work: CatalogWork,
}

#[derive(Serialize, Deserialize)]
struct Offer {
    reader: uuid::Uuid,
    action: crate::actions::ActionId,
    selection: String,
    revision: u64,
    aud: String,
}
impl Catalog {
    pub fn offer(
        &self,
        reader: ReaderId,
        action: crate::actions::ActionId,
        selection: &str,
        revision: u64,
    ) -> anyhow::Result<String> {
        Ok(encode(
            &Header::new(Algorithm::HS256),
            &Offer {
                reader: *reader.as_uuid(),
                action,
                selection: selection.to_owned(),
                revision,
                aud: "book-smartz-offer".into(),
            },
            &EncodingKey::from_secret(&self.signing_key),
        )?)
    }
    pub fn verify_offer(
        &self,
        reader: ReaderId,
        action: crate::actions::ActionId,
        selection: &str,
        revision: u64,
        token: &str,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(token.len() <= 131_072, "invalid offer");
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        validation.required_spec_claims.clear();
        validation.set_audience(&["book-smartz-offer"]);
        let offer = decode::<Offer>(
            token,
            &DecodingKey::from_secret(&self.signing_key),
            &validation,
        )?
        .claims;
        anyhow::ensure!(
            offer.reader == *reader.as_uuid()
                && offer.action == action
                && offer.selection == selection
                && offer.revision == revision,
            "invalid offer"
        );
        Ok(())
    }

    pub fn from_environment() -> anyhow::Result<Self> {
        let endpoint = url::Url::parse(
            &std::env::var("OPEN_LIBRARY_SEARCH_ENDPOINT")
                .unwrap_or_else(|_| "https://openlibrary.org/search.json".into()),
        )?;
        anyhow::ensure!(
            endpoint.scheme() == "https"
                || (endpoint.scheme() == "http"
                    && matches!(endpoint.host_str(), Some("127.0.0.1" | "[::1]"))),
            "catalog endpoint must use HTTPS or explicit loopback"
        );
        anyhow::ensure!(
            endpoint.username().is_empty()
                && endpoint.password().is_none()
                && endpoint.fragment().is_none()
                && endpoint.query().is_none(),
            "invalid catalog endpoint"
        );
        let contact = std::env::var("BOOK_SMARTZ_MAINTAINER_CONTACT")?;
        anyhow::ensure!(valid_text(&contact, 256), "maintainer contact is required");
        let signing_key = std::env::var("BOOK_SMARTZ_SELECTION_KEY")?.into_bytes();
        anyhow::ensure!(
            signing_key.len() >= 32,
            "selection key must contain at least 32 bytes"
        );
        Ok(Self {
            endpoint,
            signing_key,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .retry(reqwest::retry::never())
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(format!("Book Smartz ({contact})"))
                .build()?,
        })
    }
    fn sign(&self, reader: ReaderId, work: CatalogWork) -> jsonwebtoken::errors::Result<String> {
        let now = chrono::Utc::now().timestamp();
        encode(
            &Header::new(Algorithm::HS256),
            &Selection {
                iss: "book-smartz-selection".into(),
                aud: "book-smartz-add".into(),
                reader: *reader.as_uuid(),
                iat: now,
                exp: now + 900,
                work,
            },
            &EncodingKey::from_secret(&self.signing_key),
        )
    }
    /// Verifies the complete retained selection locally; never consults the provider.
    pub fn verify(&self, reader: ReaderId, token: &str) -> anyhow::Result<CatalogWork> {
        anyhow::ensure!(token.len() <= 65536, "invalid selection");
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 0;
        validation.set_issuer(&["book-smartz-selection"]);
        validation.set_audience(&["book-smartz-add"]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        let selection = decode::<Selection>(
            token,
            &DecodingKey::from_secret(&self.signing_key),
            &validation,
        )?
        .claims;
        let now = chrono::Utc::now().timestamp();
        anyhow::ensure!(
            selection.reader == *reader.as_uuid()
                && selection.iat <= now
                && selection.exp > now
                && selection.exp.checked_sub(selection.iat) == Some(900)
                && selection.work.valid(),
            "invalid selection"
        );
        Ok(selection.work)
    }
    async fn discover(
        &self,
        pool: &PgPool,
        reader: ReaderId,
        query: &str,
        page: u32,
    ) -> Result<Discovery> {
        let result = if admit(pool, reader).await? {
            self.search(query, page).await
        } else {
            Err(Failure::Limited)
        };
        let mut results = Vec::new();
        let mut message = None;
        let mut more = false;
        match result {
            Ok(found) => {
                more = found.more && page < 1_000_000;
                if found.works.is_empty() {
                    message = Some(if found.no_matches {
                        "No matches found."
                    } else {
                        "No selectable works returned."
                    });
                }
                for work in found.works {
                    let token = self.sign(reader, work.clone())?;
                    results.push((work, token));
                }
            }
            Err(error) => {
                message = Some(error.message());
            }
        }
        Ok(Discovery {
            results,
            message,
            more,
        })
    }
    async fn search(&self, query: &str, page: u32) -> std::result::Result<SearchPage, Failure> {
        let page = page.to_string();
        let mut endpoint = self.endpoint.clone();
        endpoint.query_pairs_mut().extend_pairs([
            ("q", query),
            ("page", page.as_str()),
            ("limit", "20"),
            ("fields", "key,title,author_name,first_publish_year,cover_i"),
        ]);
        let mut response = self
            .client
            .get(endpoint)
            .send()
            .await
            .map_err(|error| transport(&error))?;
        if response.status().as_u16() == 429 {
            return Err(Failure::Limited);
        }
        if !response.status().is_success() {
            return Err(Failure::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|size| size > 1_048_576)
        {
            return Err(Failure::Oversized);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| transport(&error))? {
            if bytes.len() + chunk.len() > 1_048_576 {
                return Err(Failure::Oversized);
            }
            bytes.extend_from_slice(&chunk);
        }
        let raw: ProviderPage = serde_json::from_slice(&bytes).map_err(|_| Failure::Malformed)?;
        if raw.docs.len() > 20 {
            return Err(Failure::Malformed);
        }
        let mut seen = HashSet::new();
        let mut works = Vec::new();
        for doc in raw.docs {
            let Ok(doc) = serde_json::from_value::<ProviderWork>(doc) else {
                continue;
            };
            let id = doc
                .key
                .strip_prefix("/works/")
                .unwrap_or(&doc.key)
                .to_owned();
            let work = CatalogWork {
                work_id: id,
                title: doc.title,
                authors: doc.author_name,
                year: doc.first_publish_year,
                cover: doc.cover_i,
            };
            if work.valid() && seen.insert(work.work_id.clone()) {
                works.push(work);
            }
        }
        Ok(SearchPage {
            works,
            more: raw
                .start
                .checked_add(20)
                .is_some_and(|end| end < raw.num_found),
            no_matches: raw.num_found == 0,
        })
    }
}
#[derive(Deserialize)]
struct ProviderPage {
    #[serde(alias = "numFound")]
    num_found: u64,
    start: u64,
    docs: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
struct ProviderWork {
    key: String,
    title: String,
    #[serde(default)]
    author_name: Vec<String>,
    first_publish_year: Option<i32>,
    cover_i: Option<u64>,
}
struct Discovery {
    results: Vec<(CatalogWork, String)>,
    message: Option<&'static str>,
    more: bool,
}
struct SearchPage {
    works: Vec<CatalogWork>,
    more: bool,
    no_matches: bool,
}
enum Failure {
    Limited,
    Unavailable,
    Timeout,
    Malformed,
    Oversized,
}
impl Failure {
    const fn message(&self) -> &'static str {
        match self {
            Self::Limited => "Search rate limit reached. Please retry shortly.",
            Self::Unavailable => "Search is unavailable. Please retry.",
            Self::Timeout => "Search timed out. Please retry.",
            Self::Malformed => "Search returned a malformed response. Please retry.",
            Self::Oversized => "Search response was too large. Please retry.",
        }
    }
}
fn transport(error: &reqwest::Error) -> Failure {
    if error.is_timeout() {
        Failure::Timeout
    } else {
        Failure::Unavailable
    }
}

/// Try admission without waiting on another instance. A committed permit is
/// consumed even on provider failure; uncertain commits never dispatch a request.
async fn admit(pool: &PgPool, reader: ReaderId) -> std::result::Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(706165241, 3)")
        .fetch_one(&mut *transaction)
        .await?;
    if !locked {
        return Ok(false);
    }
    let available: bool = sqlx::query_scalar("SELECT next_search_at <= clock_timestamp() FROM book_smartz_catalog.search_gate WHERE singleton").fetch_one(&mut *transaction).await?;
    if !available {
        return Ok(false);
    }
    sqlx::query("DELETE FROM book_smartz_catalog.search_admissions WHERE admitted_at <= clock_timestamp() - interval '1 minute'").execute(&mut *transaction).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM book_smartz_catalog.search_admissions WHERE reader_id = $1",
    )
    .bind(reader.as_uuid())
    .fetch_one(&mut *transaction)
    .await?;
    if count >= 10 {
        return Ok(false);
    }
    sqlx::query("INSERT INTO book_smartz_catalog.search_admissions (reader_id, admitted_at) VALUES ($1, clock_timestamp())").bind(reader.as_uuid()).execute(&mut *transaction).await?;
    sqlx::query("UPDATE book_smartz_catalog.search_gate SET next_search_at = clock_timestamp() + interval '1 second' WHERE singleton").execute(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(true)
}
#[derive(Deserialize)]
pub struct SearchInput {
    csrf: String,
    query: String,
    page: u32,
}
#[page(POST "/search")]
pub async fn search(cx: &Cx, Form(input): Form<SearchInput>) -> Result<impl View> {
    let session = authorize(cx).await?;
    let app: &Application = app_context(cx);
    crate::require_post(cx, &session, &input.csrf)?;
    if !valid_text(&input.query, 256) || !(1..=1_000_000).contains(&input.page) {
        return Err(bad_request("Enter 1–256 search characters and a valid page").into());
    }
    let Discovery {
        results,
        message,
        more,
    } = app
        .catalog
        .discover(&app.pool, session.reader, &input.query, input.page)
        .await?;
    let next_page = input.page + 1;
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Search · Book Smartz"</title>
                <link rel="stylesheet" href=(topcoat::asset::asset!("./styles.css"))>
                <script type="module" src=(topcoat::asset::asset!("./submitting.js"))></script>
            </head>
            <body>
                book_smartz_views::heading()
                <main class="mx-auto grid max-w-5xl gap-8 p-6 md:grid-cols-2">
                    <section>
                        <h1>"Search books"</h1>
                        <a href="/ranking">"Your saved ranking"</a>
                        if let Some(message) = message {
                            <p role="status">(message)</p>
                        }
                        <form method="post" action="/search">
                            <input type="hidden" name="csrf" value=(&session.csrf)>
                            <label>
                                "Title or author"
                                <input
                                    name="query"
                                    maxlength="256"
                                    required=""
                                    value=(&input.query)
                                >
                            </label>
                            <input
                                type="hidden"
                                name="page"
                                value=(input.page.to_string())
                            >
                            <button type="submit">"Retry search"</button>
                        </form>
                        <ul>
                            for (work, token) in results {
                                result_card(
                                    work: work,
                                    token: token,
                                    csrf: session.csrf.clone()
                                )
                            }
                        </ul>
                        if input.page > 1 {
                            <form method="post" action="/search">
                                <input type="hidden" name="csrf" value=(&session.csrf)>
                                <input type="hidden" name="query" value=(&input.query)>
                                <input
                                    type="hidden"
                                    name="page"
                                    value=((input.page - 1).to_string())
                                >
                                <button type="submit">"Previous page"</button>
                            </form>
                        }
                        if more {
                            <form method="post" action="/search">
                                <input type="hidden" name="csrf" value=(&session.csrf)>
                                <input type="hidden" name="query" value=(&input.query)>
                                <input
                                    type="hidden"
                                    name="page"
                                    value=(next_page.to_string())
                                >
                                <button type="submit">"Next page"</button>
                            </form>
                        }
                    </section>
                    crate::saved_ranking(reader: session.reader)
                </main>
            </body>
        </html>
    })
}
#[derive(Deserialize)]
pub struct SelectInput {
    csrf: String,
    selection: String,
}
#[page(POST "/select")]
pub async fn select(cx: &Cx, Form(input): Form<SelectInput>) -> Result<impl View> {
    let session = authorize(cx).await?;
    crate::require_post(cx, &session, &input.csrf)?;
    let app: &Application = app_context(cx);
    let work = app
        .catalog
        .verify(session.reader, &input.selection)
        .map_err(|_| forbidden())?;
    let ranking = app
        .store
        .load_ranking(
            session.reader,
            book_smartz_storage::OperationContext::default(),
        )
        .await?;
    let revision = ranking.projection().revision();
    let blocked = ranking.projection().pending().is_some()
        || crate::actions::checking(app, session.reader)
            .await
            .map_err(|_| topcoat::Error::msg("Action data unavailable"))?;
    let gate = if blocked {
        Some(
            "Finish your unfinished comparison or check your earlier save before adding another book.",
        )
    } else {
        None
    };
    let action = crate::actions::ActionId::new();
    let offer = app
        .catalog
        .offer(session.reader, action, &input.selection, revision)
        .map_err(|_| topcoat::Error::msg("Action data unavailable"))?;
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Selected work · Book Smartz"</title>
                <link rel="stylesheet" href=(topcoat::asset::asset!("./styles.css"))>
                <script type="module" src=(topcoat::asset::asset!("./submitting.js"))></script>
            </head>
            <body>
                book_smartz_views::heading()
                <main>
                    <h1>"Selected work"</h1>
                    <a href=(work.source())>(work.title)</a>
                    <p>"This selection expires 15 minutes after search."</p>
                    if let Some(message) = gate {
                        <p role="status">(message)</p>
                    }
                    <form method="post" action="/add" data-ranking-action="">
                        <input type="hidden" name="csrf" value=(&session.csrf)>
                        <input type="hidden" name="selection" value=(&input.selection)>
                        <input type="hidden" name="action" value=(action.text())>
                        <input
                            type="hidden"
                            name="revision"
                            value=(revision.to_string())
                        >
                        <input type="hidden" name="offer" value=(offer)>
                        <button type="submit" disabled=(blocked)>
                            "Add to your ranking"
                        </button>
                    </form>
                    <a href="/ranking">"Return to your ranking"</a>
                </main>
            </body>
        </html>
    })
}

#[topcoat::view::component]
async fn result_card(work: CatalogWork, token: String, csrf: String) -> Result<impl View> {
    Ok(view! {
        <li>
            <div style="width:96px;height:144px;position:relative">
                <img
                    src=(topcoat::asset::asset!("./cover-placeholder.svg"))
                    alt="Cover unavailable"
                    width="96"
                    height="144"
                >
                if let Some(cover) = work.cover_url() {
                    <object
                        data=(cover)
                        type="image/jpeg"
                        aria-label="Representative book cover"
                        style="position:absolute;inset:0;width:96px;height:144px"
                    >
                        <img
                            src=(topcoat::asset::asset!("./cover-placeholder.svg"))
                            alt="Cover unavailable"
                            width="96"
                            height="144"
                        >
                    </object>
                }
            </div>
            <a href=(work.source())>(work.title.clone())</a>
            <p>(work.author_label())</p>
            if let Some(year) = work.year {
                <p>(year.to_string())</p>
            }
            <form method="post" action="/select">
                <input type="hidden" name="csrf" value=(csrf)>
                <input type="hidden" name="selection" value=(token)>
                <button type="submit">"Select this work"</button>
            </form>
        </li>
    })
}
